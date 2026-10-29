//! UIA normalizes bounded noneditable button/menu names; never retain labels or read values/documents.
use crate::{
    collector::{browser_app, normalize_action, sensitive_app, BrowserContext, Observation, Role},
    Action, ContextSettings, Gap, SessionGate,
};
use std::time::Instant;
use windows::{
    core::PWSTR,
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        System::{
            Com::{
                CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
                COINIT_MULTITHREADED,
            },
            StationsAndDesktops::{
                CloseDesktop, GetUserObjectInformationW, OpenInputDesktop, DESKTOP_CONTROL_FLAGS,
                DESKTOP_READOBJECTS, UOI_NAME,
            },
            Threading::{
                OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
                PROCESS_QUERY_LIMITED_INFORMATION,
            },
        },
        UI::{
            Accessibility::{
                CUIAutomation8, IUIAutomation2, UIA_ButtonControlTypeId, UIA_CheckBoxControlTypeId,
                UIA_ComboBoxControlTypeId, UIA_EditControlTypeId, UIA_MenuItemControlTypeId,
                UIA_RadioButtonControlTypeId, UIA_TextControlTypeId,
            },
            WindowsAndMessaging::{GetClassNameW, GetForegroundWindow, GetWindowThreadProcessId},
        },
    },
};
struct Com;
impl Drop for Com {
    fn drop(&mut self) {
        unsafe { CoUninitialize() }
    }
}
struct Process(HANDLE);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}
fn check(deadline: Instant, active: &SessionGate) -> std::result::Result<(), Gap> {
    if !active.is_active() {
        return Err(Gap::Locked);
    }
    if Instant::now() >= deadline {
        return Err(Gap::Unsupported);
    }
    Ok(())
}
fn unlocked_desktop() -> bool {
    let Ok(desktop) =
        (unsafe { OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_READOBJECTS) })
    else {
        return false;
    };
    let mut name = [0u16; 64];
    let mut needed = 0;
    let result = unsafe {
        GetUserObjectInformationW(
            HANDLE(desktop.0),
            UOI_NAME,
            Some(name.as_mut_ptr().cast()),
            std::mem::size_of_val(&name) as u32,
            Some(&mut needed),
        )
    };
    let _ = unsafe { CloseDesktop(desktop) };
    result.is_ok()
        && String::from_utf16_lossy(&name[..name.iter().position(|c| *c == 0).unwrap_or(0)])
            .eq_ignore_ascii_case("default")
}
fn timeout(
    automation: &IUIAutomation2,
    deadline: Instant,
    active: &SessionGate,
) -> std::result::Result<(), Gap> {
    check(deadline, active)?;
    let ms = deadline
        .saturating_duration_since(Instant::now())
        .as_millis()
        .clamp(1, 250) as u32;
    unsafe {
        automation
            .SetConnectionTimeout(ms)
            .and_then(|_| automation.SetTransactionTimeout(ms))
    }
    .map_err(|_| Gap::Unsupported)
}
pub(crate) fn observe(
    settings: &ContextSettings,
    deadline: Instant,
    active: &SessionGate,
) -> std::result::Result<Observation, Gap> {
    check(deadline, active)?;
    if !unlocked_desktop() {
        return Err(Gap::Locked);
    }
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        return Err(Gap::Unsupported);
    }
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 {
        return Err(Gap::Unsupported);
    }
    let process = Process(
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
            .map_err(|_| Gap::PermissionDenied)?,
    );
    let mut path = [0u16; 1024];
    let mut len = path.len() as u32;
    unsafe {
        QueryFullProcessImageNameW(
            process.0,
            PROCESS_NAME_WIN32,
            PWSTR(path.as_mut_ptr()),
            &mut len,
        )
    }
    .map_err(|_| Gap::PermissionDenied)?;
    let path = String::from_utf16_lossy(&path[..len as usize]);
    let app_id = path
        .rsplit(['\\', '/'])
        .next()
        .ok_or(Gap::Unsupported)?
        .to_ascii_lowercase();
    if !settings.allows_app(&app_id) || sensitive_app(&app_id) {
        return Err(Gap::Excluded);
    }
    if browser_app(&app_id) {
        return Err(Gap::PrivateOrUnknownBrowser);
    }
    let mut class = [0u16; 64];
    let class_len = unsafe { GetClassNameW(hwnd, &mut class) };
    if class_len <= 0 || String::from_utf16_lossy(&class[..class_len as usize]) == "#32770" {
        return Err(Gap::Excluded);
    }
    check(deadline, active)?;
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
        .ok()
        .map_err(|_| Gap::Unsupported)?;
    let _com = Com;
    let automation: IUIAutomation2 =
        unsafe { CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER) }
            .map_err(|_| Gap::Unsupported)?;
    timeout(&automation, deadline, active)?;
    let focused = unsafe { automation.GetFocusedElement() }.map_err(|_| Gap::PermissionDenied)?;
    timeout(&automation, deadline, active)?;
    if unsafe { focused.CurrentProcessId() }.map_err(|_| Gap::Unsupported)? != pid as i32 {
        return Err(Gap::Unsupported);
    }
    timeout(&automation, deadline, active)?;
    let password = unsafe { focused.CurrentIsPassword() }
        .map_err(|_| Gap::Unsupported)?
        .as_bool();
    timeout(&automation, deadline, active)?;
    let control = unsafe { focused.CurrentControlType() }.map_err(|_| Gap::Unsupported)?;
    let role = if password {
        Role::Password
    } else if control == UIA_ButtonControlTypeId {
        Role::Button
    } else if control == UIA_MenuItemControlTypeId {
        Role::MenuItem
    } else if [UIA_CheckBoxControlTypeId, UIA_RadioButtonControlTypeId].contains(&control) {
        Role::Toggle
    } else if control == UIA_TextControlTypeId {
        Role::StaticText
    } else if [UIA_EditControlTypeId, UIA_ComboBoxControlTypeId].contains(&control) {
        Role::Editable
    } else {
        Role::Unknown
    };
    let action = if matches!(role, Role::Button | Role::MenuItem) {
        timeout(&automation, deadline, active)?;
        // UIA returns a BSTR; reject oversized UTF-16 before decoding/copying its content.
        match unsafe { focused.CurrentName() } {
            Ok(label) if label.len() <= 127 => String::from_utf16(&label)
                .map(|label| normalize_action(role, &label))
                .unwrap_or(Action::Unknown),
            _ => Action::Unknown,
        }
    } else {
        Action::Unknown
    };
    check(deadline, active)?;
    if !unlocked_desktop() || unsafe { GetForegroundWindow() } != hwnd {
        return Err(Gap::Locked);
    }
    Ok(Observation {
        app_name: app_id.clone(),
        app_id,
        role,
        action,
        browser: BrowserContext::NotBrowser,
        permission: true,
        unlocked: true,
    })
}
