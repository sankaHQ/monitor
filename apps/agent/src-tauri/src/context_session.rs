use super::ContextState;
#[cfg(target_os = "macos")]
pub(super) fn session_available() -> bool {
    use std::{
        ffi::{c_void, CString},
        ptr,
    };
    type Ref = *const c_void;
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn CGSessionCopyCurrentDictionary() -> Ref;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(v: Ref);
        fn CFStringCreateWithCString(a: Ref, s: *const i8, e: u32) -> Ref;
        fn CFDictionaryGetValue(d: Ref, k: Ref) -> Ref;
        fn CFGetTypeID(v: Ref) -> usize;
        fn CFBooleanGetTypeID() -> usize;
        fn CFBooleanGetValue(v: Ref) -> bool;
    }
    unsafe {
        let d = CGSessionCopyCurrentDictionary();
        if d.is_null() {
            return false;
        }
        let result = [
            ("kCGSSessionOnConsoleKey", true),
            ("kCGSessionLoginDoneKey", true),
            ("CGSSessionScreenIsLocked", false),
        ]
        .iter()
        .all(|(name, expected)| {
            let name = CString::new(*name).unwrap();
            let k = CFStringCreateWithCString(ptr::null(), name.as_ptr(), 0x08000100);
            if k.is_null() {
                return false;
            }
            let v = CFDictionaryGetValue(d, k);
            CFRelease(k);
            if v.is_null() {
                return !expected;
            }
            CFGetTypeID(v) == CFBooleanGetTypeID() && CFBooleanGetValue(v) == *expected
        });
        CFRelease(d);
        result
    }
}
#[cfg(target_os = "windows")]
pub(super) fn session_available() -> bool {
    use windows_sys::Win32::System::StationsAndDesktops::*;
    unsafe {
        let desktop = OpenInputDesktop(0, 0, DESKTOP_READOBJECTS);
        if desktop.is_null() {
            return false;
        }
        let mut name = [0u16; 64];
        let mut size = 0;
        let ok =
            GetUserObjectInformationW(desktop, UOI_NAME, name.as_mut_ptr().cast(), 128, &mut size)
                != 0;
        CloseDesktop(desktop);
        ok && String::from_utf16_lossy(&name[..name.iter().position(|c| *c == 0).unwrap_or(0)])
            .eq_ignore_ascii_case("default")
    }
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(super) fn session_available() -> bool {
    false
}

#[cfg(target_os = "macos")]
pub(super) fn install_session_notifications(state: ContextState) {
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{NSNotification, NSString};
    let center = NSWorkspace::sharedWorkspace().notificationCenter();
    for name in [
        "NSWorkspaceWillSleepNotification",
        "NSWorkspaceSessionDidResignActiveNotification",
        "NSWorkspaceScreensDidSleepNotification",
    ] {
        let state = state.clone();
        let block = block2::RcBlock::new(move |_: std::ptr::NonNull<NSNotification>| state.pause());
        let token = unsafe {
            center.addObserverForName_object_queue_usingBlock(
                Some(&NSString::from_str(name)),
                None,
                None,
                &block,
            )
        };
        // Process-lifetime callbacks retain a pause-only handle, never any captured content.
        std::mem::forget(token);
    }
    // Distributed notifications are delivered on AppKit's main run loop.
    {
        use std::ffi::{c_void, CString};
        type Ref = *const c_void;
        #[link(name = "CoreFoundation", kind = "framework")]
        extern "C" {
            fn CFNotificationCenterGetDistributedCenter() -> Ref;
            fn CFNotificationCenterAddObserver(
                c: Ref,
                o: Ref,
                f: unsafe extern "C" fn(Ref, *mut c_void, Ref, Ref, Ref),
                n: Ref,
                obj: Ref,
                behavior: isize,
            );
            fn CFStringCreateWithCString(a: Ref, s: *const i8, e: u32) -> Ref;
            fn CFRunLoopGetCurrent() -> Ref;
            fn CFRunLoopGetMain() -> Ref;
            fn CFRelease(value: Ref);
        }
        unsafe extern "C" fn locked(_: Ref, observer: *mut c_void, _: Ref, _: Ref, _: Ref) {
            if let Some(state) = unsafe { (observer as *const ContextState).as_ref() } {
                state.pause();
            }
        }
        unsafe {
            let center = CFNotificationCenterGetDistributedCenter();
            if center.is_null() || CFRunLoopGetCurrent() != CFRunLoopGetMain() {
                state.pause();
                return;
            }
            let name = CString::new("com.apple.screenIsLocked").unwrap();
            let name = CFStringCreateWithCString(std::ptr::null(), name.as_ptr(), 0x08000100);
            if name.is_null() {
                state.pause();
                return;
            }
            // The process-lifetime observer owns this pause-only state handle.
            let state = Box::into_raw(Box::new(state));
            CFNotificationCenterAddObserver(
                center,
                state.cast(),
                locked,
                name,
                std::ptr::null(),
                4,
            );
            CFRelease(name);
            (*state)
                .session_ready
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }
}
#[cfg(target_os = "windows")]
pub(super) fn install_session_notifications(state: ContextState) {
    std::thread::spawn(move || unsafe {
        use windows_sys::Win32::{
            Foundation::*,
            System::{LibraryLoader::GetModuleHandleW, RemoteDesktop::*},
            UI::WindowsAndMessaging::*,
        };
        unsafe extern "system" fn callback(
            window: HWND,
            message: u32,
            wparam: WPARAM,
            lparam: LPARAM,
        ) -> LRESULT {
            let ptr = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as *const ContextState;
            if message == WM_WTSSESSION_CHANGE || message == WM_POWERBROADCAST {
                if let Some(state) = unsafe { ptr.as_ref() } {
                    state.pause();
                }
            }
            unsafe { DefWindowProcW(window, message, wparam, lparam) }
        }
        let name: Vec<u16> = "SankaContextSessionGate\0".encode_utf16().collect();
        let instance = GetModuleHandleW(std::ptr::null());
        let class = WNDCLASSW {
            lpfnWndProc: Some(callback),
            hInstance: instance,
            lpszClassName: name.as_ptr(),
            ..std::mem::zeroed()
        };
        if RegisterClassW(&class) == 0 {
            return;
        }
        // Hidden top-level window receives power broadcasts; message-only windows do not.
        let window = CreateWindowExW(
            0,
            name.as_ptr(),
            name.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        );
        if window.is_null() {
            return;
        }
        let ptr = Box::into_raw(Box::new(state));
        SetWindowLongPtrW(window, GWLP_USERDATA, ptr as isize);
        if WTSRegisterSessionNotification(window, NOTIFY_FOR_THIS_SESSION) == 0 {
            (*ptr).pause();
            DestroyWindow(window);
            drop(Box::from_raw(ptr));
            return;
        }
        (*ptr)
            .session_ready
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let mut message = std::mem::zeroed();
        while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        (*ptr)
            .session_ready
            .store(false, std::sync::atomic::Ordering::SeqCst);
        (*ptr).pause();
        WTSUnRegisterSessionNotification(window);
        DestroyWindow(window);
        drop(Box::from_raw(ptr));
    });
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(super) fn install_session_notifications(state: ContextState) {
    state.pause();
}
