//! Bounded focused-button/menu title normalization; never retain labels or read values/documents.
use crate::{
    collector::{native_profile, BrowserContext, Observation, Role},
    Action, ContextSettings, Gap, SessionGate,
};
use objc2_app_kit::NSWorkspace;
use std::{
    ffi::{c_void, CString},
    ptr,
    time::Instant,
};
type Ref = *const c_void;
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXUIElementCreateApplication(pid: i32) -> Ref;
    fn AXUIElementCopyAttributeValue(element: Ref, name: Ref, value: *mut Ref) -> i32;
    fn AXUIElementSetMessagingTimeout(element: Ref, seconds: f32) -> i32;
    fn AXUIElementGetPid(element: Ref, pid: *mut i32) -> i32;
    fn CGSessionCopyCurrentDictionary() -> Ref;
}
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(value: Ref);
    fn CFStringCreateWithCString(allocator: Ref, value: *const i8, encoding: u32) -> Ref;
    fn CFStringGetCString(value: Ref, buffer: *mut i8, size: isize, encoding: u32) -> bool;
    fn CFGetTypeID(value: Ref) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFBooleanGetTypeID() -> usize;
    fn CFBooleanGetValue(value: Ref) -> bool;
    fn CFDictionaryGetValue(dictionary: Ref, key: Ref) -> Ref;
    fn CFEqual(left: Ref, right: Ref) -> bool;
}
struct Owned(Ref);
impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CFRelease(self.0) }
        }
    }
}
fn string(value: &str) -> std::result::Result<Owned, Gap> {
    let value = CString::new(value).map_err(|_| Gap::Unsupported)?;
    let value = unsafe { CFStringCreateWithCString(ptr::null(), value.as_ptr(), 0x08000100) };
    if value.is_null() {
        Err(Gap::Unsupported)
    } else {
        Ok(Owned(value))
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
fn attribute(
    element: Ref,
    name: &str,
    deadline: Instant,
    active: &SessionGate,
) -> std::result::Result<Owned, Gap> {
    check(deadline, active)?;
    let name = string(name)?;
    let remaining = deadline
        .saturating_duration_since(Instant::now())
        .as_secs_f32();
    if remaining <= 0.0 || unsafe { AXUIElementSetMessagingTimeout(element, remaining) } != 0 {
        return Err(Gap::Unsupported);
    }
    let mut value = ptr::null();
    let code = unsafe { AXUIElementCopyAttributeValue(element, name.0, &mut value) };
    let value = Owned(value);
    check(deadline, active)?;
    if code != 0 || value.0.is_null() {
        Err(Gap::Unsupported)
    } else {
        Ok(value)
    }
}
fn role_string(
    element: Ref,
    name: &str,
    deadline: Instant,
    active: &SessionGate,
) -> std::result::Result<String, Gap> {
    let value = attribute(element, name, deadline, active)?;
    if unsafe { CFGetTypeID(value.0) != CFStringGetTypeID() } {
        return Err(Gap::Unsupported);
    }
    let mut bytes = [0i8; 128];
    if !unsafe {
        CFStringGetCString(
            value.0,
            bytes.as_mut_ptr(),
            bytes.len() as isize,
            0x08000100,
        )
    } {
        return Err(Gap::Unsupported);
    }
    Ok(unsafe { std::ffi::CStr::from_ptr(bytes.as_ptr()) }
        .to_string_lossy()
        .into_owned())
}
fn session_available() -> bool {
    let dictionary = Owned(unsafe { CGSessionCopyCurrentDictionary() });
    if dictionary.0.is_null() {
        return false;
    }
    ["kCGSSessionOnConsoleKey", "kCGSessionLoginDoneKey"]
        .into_iter()
        .all(|key| {
            let Ok(key) = string(key) else {
                return false;
            };
            let value = unsafe { CFDictionaryGetValue(dictionary.0, key.0) };
            !value.is_null()
                && unsafe { CFGetTypeID(value) == CFBooleanGetTypeID() && CFBooleanGetValue(value) }
        })
}
fn native_ancestors(
    focus: Ref,
    window: Ref,
    pid: i32,
    deadline: Instant,
    active: &SessionGate,
) -> std::result::Result<Vec<String>, Gap> {
    let mut parent = attribute(focus, "AXParent", deadline, active)?;
    let mut roles = Vec::new();
    for _ in 0..8 {
        let role = role_string(parent.0, "AXRole", deadline, active)?;
        roles.push(role.clone());
        if role == "AXWindow" {
            return if unsafe { CFEqual(parent.0, window) } {
                Ok(roles)
            } else {
                Err(Gap::Unsupported)
            };
        }
        if role == "AXApplication" {
            let mut parent_pid = 0;
            return if unsafe { AXUIElementGetPid(parent.0, &mut parent_pid) } == 0
                && parent_pid == pid
            {
                Ok(roles)
            } else {
                Err(Gap::Unsupported)
            };
        }
        if !matches!(
            role.as_str(),
            "AXToolbar" | "AXGroup" | "AXMenu" | "AXMenuBar" | "AXMenuBarItem"
        ) {
            return Err(Gap::Unsupported);
        }
        parent = attribute(parent.0, "AXParent", deadline, active)?;
    }
    Err(Gap::Unsupported)
}
pub(crate) fn observe(
    settings: &ContextSettings,
    deadline: Instant,
    active: &SessionGate,
) -> std::result::Result<Observation, Gap> {
    check(deadline, active)?;
    // Console/login keys are public availability checks, not a lock-state guarantee.
    // The application additionally latches lock/session/sleep notifications; unknown is paused.
    if !session_available() {
        return Err(Gap::Locked);
    }
    if !unsafe { AXIsProcessTrusted() } {
        return Err(Gap::PermissionDenied);
    }
    let workspace = NSWorkspace::sharedWorkspace();
    let app = workspace.frontmostApplication().ok_or(Gap::Unsupported)?;
    let app_id = app.bundleIdentifier().ok_or(Gap::Unsupported)?.to_string();
    let location = app
        .bundleURL()
        .and_then(|url| url.path())
        .ok_or(Gap::Unsupported)?
        .to_string();
    let profile = native_profile(settings, &app_id, &location, None)?;
    let app_name = app
        .localizedName()
        .map(|n| n.to_string())
        .unwrap_or_else(|| app_id.clone());
    let pid = app.processIdentifier();
    let application = Owned(unsafe { AXUIElementCreateApplication(pid) });
    if application.0.is_null() {
        return Err(Gap::Unsupported);
    }
    let window = attribute(application.0, "AXFocusedWindow", deadline, active)?;
    if role_string(window.0, "AXSubrole", deadline, active)? != "AXStandardWindow" {
        return Err(Gap::Excluded);
    }
    let focus = attribute(application.0, "AXFocusedUIElement", deadline, active)?;
    let mut focus_pid = 0;
    if unsafe { AXUIElementGetPid(focus.0, &mut focus_pid) } != 0 || focus_pid != pid {
        return Err(Gap::Unsupported);
    }
    let role = match role_string(focus.0, "AXRole", deadline, active)?.as_str() {
        "AXButton" => Role::Button,
        "AXMenuItem" => Role::MenuItem,
        "AXCheckBox" | "AXRadioButton" => Role::Toggle,
        "AXStaticText" => Role::StaticText,
        "AXTextField" | "AXTextArea" | "AXComboBox" => Role::Editable,
        "AXSecureTextField" => Role::Password,
        // Web areas, documents, arbitrary custom controls and remote surfaces have no safe adapter.
        _ => Role::Unknown,
    };
    let action = if matches!(role, Role::Button | Role::MenuItem) {
        let ancestors = native_ancestors(focus.0, window.0, pid, deadline, active)?;
        // The 128-byte CFString buffer rejects longer labels instead of truncating a prefix.
        profile.read_action(
            role,
            &ancestors.iter().map(String::as_str).collect::<Vec<_>>(),
            || role_string(focus.0, "AXTitle", deadline, active),
        )?
    } else {
        Action::Unknown
    };
    check(deadline, active)?;
    if !unsafe { AXIsProcessTrusted() } {
        return Err(Gap::PermissionDenied);
    }
    if !session_available()
        || workspace
            .frontmostApplication()
            .is_none_or(|front| front.processIdentifier() != pid)
    {
        return Err(Gap::Locked);
    }
    Ok(Observation {
        app_id,
        app_name,
        role,
        action,
        browser: BrowserContext::NotBrowser,
        permission: true,
        unlocked: true,
    })
}
