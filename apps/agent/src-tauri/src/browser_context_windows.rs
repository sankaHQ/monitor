use super::Browser;
use std::path::Path;
#[cfg(target_os = "windows")]
pub(super) fn register_windows(
    browser: Browser,
    name: &str,
    manifest: &Path,
) -> Result<(), String> {
    use windows_sys::Win32::System::Registry::*;
    let path = format!(
        "Software\\{}\\NativeMessagingHosts\\{}",
        match browser {
            Browser::Chrome => "Google\\Chrome",
            Browser::Edge => "Microsoft\\Edge",
        },
        name
    );
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let key = wide(&path);
    let value = wide(&manifest.to_string_lossy());
    unsafe {
        let mut handle = std::ptr::null_mut();
        let status = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            0,
            std::ptr::null(),
            0,
            KEY_READ | KEY_WRITE,
            std::ptr::null(),
            &mut handle,
            std::ptr::null_mut(),
        );
        if status != 0 {
            return Err("Cannot register per-user browser host".into());
        }
        let mut old = [0u16; 2048];
        let mut bytes = 4096;
        let mut ty = 0;
        let read = RegQueryValueExW(
            handle,
            std::ptr::null(),
            std::ptr::null_mut(),
            &mut ty,
            old.as_mut_ptr().cast(),
            &mut bytes,
        );
        if read == 0 && (ty != REG_SZ || old[..(bytes as usize / 2).min(old.len())] != value) {
            RegCloseKey(handle);
            return Err("Preserving an existing browser host registration".into());
        }
        if read != 0 && read != 2 {
            RegCloseKey(handle);
            return Err("Existing browser registration unavailable".into());
        }
        let result = RegSetValueExW(
            handle,
            std::ptr::null(),
            0,
            REG_SZ,
            value.as_ptr().cast(),
            (value.len() * 2) as u32,
        );
        RegCloseKey(handle);
        if result != 0 {
            return Err("Cannot save per-user registration".into());
        }
    }
    Ok(())
}
