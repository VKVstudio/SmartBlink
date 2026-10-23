//! Global named mutex single-instance guard (ARCHITECTURE.md §1).

use windows::core::w;
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;

pub struct SingleInstance {
    handle: HANDLE,
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        if !self.handle.is_invalid() {
            unsafe { CloseHandle(self.handle).ok(); }
        }
    }
}

/// Returns Some(guard) when this process is the first instance; None otherwise.
pub fn acquire() -> Option<SingleInstance> {
    let name = w!("Local\\SmartPunto.SingleInstance.Mutex");
    // SAFETY: plain Win32 call, no aliasing; handle owned by guard.
    let handle = unsafe { CreateMutexW(None, false, name) };
    match handle {
        Err(_) => None,
        Ok(h) => {
            let already = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
            if already {
                unsafe { CloseHandle(h).ok(); }
                None
            } else {
                Some(SingleInstance { handle: h })
            }
        }
    }
}
