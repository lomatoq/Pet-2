//! Exactly one menu owns the control session for a data directory.
#[cfg(windows)]
pub struct Guard(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
impl Guard {
    pub fn acquire(root: &std::path::Path) -> std::io::Result<Option<Self>> {
        use std::hash::{Hash, Hasher};
        use windows_sys::Win32::{
            Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError},
            System::Threading::CreateMutexW,
        };
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        std::fs::canonicalize(root)
            .unwrap_or_else(|_| root.to_path_buf())
            .to_string_lossy()
            .to_lowercase()
            .hash(&mut hash);
        let name: Vec<u16> = format!("Local\\Pet2.CareMenu.{:016x}\0", hash.finish())
            .encode_utf16()
            .collect();
        // A named kernel handle releases ownership even if the menu crashes.
        let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        if handle.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            unsafe {
                CloseHandle(handle);
            }
            Ok(None)
        } else {
            Ok(Some(Self(handle)))
        }
    }
}
#[cfg(windows)]
impl Drop for Guard {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}
#[cfg(not(windows))]
pub struct Guard;
#[cfg(not(windows))]
impl Guard {
    pub fn acquire(_: &std::path::Path) -> std::io::Result<Option<Self>> {
        Ok(Some(Self))
    }
}
#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn duplicate_menu_is_rejected_and_exit_releases_ownership() {
        let root = tempfile::tempdir().unwrap();
        let first = super::Guard::acquire(root.path()).unwrap().unwrap();
        assert!(super::Guard::acquire(root.path()).unwrap().is_none());
        drop(first);
        assert!(super::Guard::acquire(root.path()).unwrap().is_some());
    }
}
