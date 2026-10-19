//! Windows collectors: the foreground window resolved to its executable (the
//! Windows app id), the executable's `FileDescription` as the display name,
//! and seconds since the last input.
//!
//! Nothing here reads window titles or contents, and no accessibility or
//! admin permission is involved: `GetForegroundWindow`, a limited-rights
//! process handle for the image path, and `GetLastInputInfo`.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{Mutex, OnceLock};

use windows_sys::Win32::Foundation::{CloseHandle, BOOL, HANDLE, HWND, LPARAM};
use windows_sys::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
};
use windows_sys::Win32::System::SystemInformation::GetTickCount;
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, GetClassNameW, GetForegroundWindow, GetWindowThreadProcessId,
};

/// The foreground window, resolved to the process that owns it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForegroundWindow {
    pub app_id: String,
    pub app_name: String,
}

pub fn foreground_window() -> Option<ForegroundWindow> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_null() {
        return None;
    }
    let host_path = process_image_path(hwnd)?;
    // Store apps run inside ApplicationFrameHost; the real app owns the
    // CoreWindow child.
    let path = if is_app_frame_host(&host_path) {
        uwp_child(hwnd)
            .and_then(process_image_path)
            .unwrap_or(host_path)
    } else {
        host_path
    };
    let app_id = crate::app_id_from_path(&path)?;
    let app_name = display_name(&path, &app_id);
    Some(ForegroundWindow { app_id, app_name })
}

/// Seconds since the last keyboard/mouse input — *that* input happened,
/// never what. Tick counts wrap every 49.7 days; `wrapping_sub` covers it.
pub fn idle_seconds() -> f64 {
    let mut info = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    if unsafe { GetLastInputInfo(&mut info) } == 0 {
        return 0.0;
    }
    let now = unsafe { GetTickCount() };
    f64::from(now.wrapping_sub(info.dwTime)) / 1000.0
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn is_app_frame_host(path: &str) -> bool {
    crate::app_id_from_path(path).as_deref() == Some("applicationframehost.exe")
}

fn process_image_path(hwnd: HWND) -> Option<String> {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    if pid == 0 {
        return None;
    }
    let handle: HANDLE = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let mut buffer = vec![0u16; 1024];
    let mut len = buffer.len() as u32;
    let ok = unsafe { QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut len) };
    unsafe { CloseHandle(handle) };
    if ok == 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..len as usize]))
}

struct ChildSearch {
    host_pid: u32,
    found: HWND,
}

unsafe extern "system" fn find_core_window(child: HWND, lparam: LPARAM) -> BOOL {
    let search = &mut *(lparam as *mut ChildSearch);
    let mut class = [0u16; 64];
    let len = GetClassNameW(child, class.as_mut_ptr(), class.len() as i32);
    if len > 0 && String::from_utf16_lossy(&class[..len as usize]) == "Windows.UI.Core.CoreWindow" {
        let mut pid = 0u32;
        GetWindowThreadProcessId(child, &mut pid);
        if pid != 0 && pid != search.host_pid {
            search.found = child;
            return 0;
        }
    }
    1
}

fn uwp_child(host: HWND) -> Option<HWND> {
    let mut host_pid = 0u32;
    unsafe { GetWindowThreadProcessId(host, &mut host_pid) };
    let mut search = ChildSearch {
        host_pid,
        found: std::ptr::null_mut(),
    };
    unsafe {
        EnumChildWindows(
            host,
            Some(find_core_window),
            &mut search as *mut ChildSearch as LPARAM,
        )
    };
    (!search.found.is_null()).then_some(search.found)
}

/// `FileDescription` from the executable's version resource ("Microsoft
/// Edge" for msedge.exe), cached per path since the collector polls.
fn display_name(path: &str, app_id: &str) -> String {
    static CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(name) = cache.lock().ok().and_then(|map| map.get(path).cloned()) {
        return name;
    }
    let name = file_description(path).unwrap_or_else(|| crate::friendly_name_from_app_id(app_id));
    if let Ok(mut map) = cache.lock() {
        if map.len() > 256 {
            map.clear();
        }
        map.insert(path.to_string(), name.clone());
    }
    name
}

fn file_description(path: &str) -> Option<String> {
    let path_w = wide(path);
    unsafe {
        let mut handle = 0u32;
        let size = GetFileVersionInfoSizeW(path_w.as_ptr(), &mut handle);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        if GetFileVersionInfoW(path_w.as_ptr(), 0, size, data.as_mut_ptr().cast::<c_void>()) == 0 {
            return None;
        }
        let mut translation: *mut c_void = std::ptr::null_mut();
        let mut len = 0u32;
        let key = wide("\\VarFileInfo\\Translation");
        if VerQueryValueW(
            data.as_ptr().cast::<c_void>(),
            key.as_ptr(),
            &mut translation,
            &mut len,
        ) == 0
            || len < 4
            || translation.is_null()
        {
            return None;
        }
        let pairs = std::slice::from_raw_parts(translation.cast::<u16>(), (len / 2) as usize);
        for pair in pairs.as_chunks::<2>().0 {
            let sub = wide(&format!(
                "\\StringFileInfo\\{:04x}{:04x}\\FileDescription",
                pair[0], pair[1]
            ));
            let mut value: *mut c_void = std::ptr::null_mut();
            let mut value_len = 0u32;
            if VerQueryValueW(
                data.as_ptr().cast::<c_void>(),
                sub.as_ptr(),
                &mut value,
                &mut value_len,
            ) != 0
                && value_len > 1
                && !value.is_null()
            {
                let text = std::slice::from_raw_parts(value.cast::<u16>(), value_len as usize);
                let text = String::from_utf16_lossy(text)
                    .trim_end_matches('\0')
                    .trim()
                    .to_string();
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
        None
    }
}
