//! Background keychain work never opens a password dialog. Only the explicit
//! foreground recovery command may ask macOS for access.
#[cfg(target_os = "macos")]
static KEYCHAIN_ACCESS: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn access<T>(
    interactive: bool,
    operation: impl FnOnce() -> Result<T, keyring::Error>,
) -> Result<T, keyring::Error> {
    #[cfg(target_os = "macos")]
    {
        #[link(name = "Security", kind = "framework")]
        extern "C" {
            fn SecKeychainSetUserInteractionAllowed(allowed: u8) -> i32;
        }
        let _lock = KEYCHAIN_ACCESS.lock().map_err(|_| {
            keyring::Error::PlatformFailure(Box::new(std::io::Error::other(
                "Secure storage unavailable",
            )))
        })?;
        struct SilentOnExit;
        impl Drop for SilentOnExit {
            fn drop(&mut self) {
                unsafe {
                    SecKeychainSetUserInteractionAllowed(0);
                }
            }
        }
        let _reset = SilentOnExit;
        let status = unsafe { SecKeychainSetUserInteractionAllowed(u8::from(interactive)) };
        if status != 0 {
            return Err(keyring::Error::PlatformFailure(Box::new(
                std::io::Error::other("Secure storage interaction policy unavailable"),
            )));
        }
        operation()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = interactive;
        operation()
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    #[link(name = "Security", kind = "framework")]
    extern "C" {
        fn SecKeychainGetUserInteractionAllowed(allowed: *mut u8) -> i32;
    }
    fn allowed() -> u8 {
        let mut value = 0;
        assert_eq!(
            unsafe { SecKeychainGetUserInteractionAllowed(&mut value) },
            0
        );
        value
    }
    #[test]
    fn background_access_is_silent_and_foreground_failure_restores_silence() {
        access(false, || {
            assert_eq!(allowed(), 0);
            Ok(())
        })
        .unwrap();
        let result: Result<(), _> = access(true, || {
            assert_eq!(allowed(), 1);
            Err(keyring::Error::NoEntry)
        });
        assert!(matches!(result, Err(keyring::Error::NoEntry)));
        let _lock = KEYCHAIN_ACCESS.lock().unwrap();
        assert_eq!(allowed(), 0);
    }
}
