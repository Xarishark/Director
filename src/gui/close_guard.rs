//! Protect in-flight transactions when the Director window is closed.
use std::sync::atomic::{AtomicBool, Ordering};
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    System::Threading::GetCurrentThreadId,
    UI::{
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            EnumThreadWindows, GWL_STYLE, GetWindowLongPtrW, WM_CLOSE, WM_NCDESTROY, WS_CAPTION,
        },
    },
};
use windows::core::BOOL;
static BUSY: AtomicBool = AtomicBool::new(false);
pub fn set_busy(value: bool) {
    BUSY.store(value, Ordering::Release);
}
unsafe extern "system" fn find_window(hwnd: HWND, data: LPARAM) -> BOOL {
    if unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) } as u32 & WS_CAPTION.0 != 0 {
        unsafe {
            *(data.0 as *mut HWND) = hwnd;
        }
        return BOOL(0);
    }
    BOOL(1)
}
unsafe extern "system" fn guard(
    hwnd: HWND,
    message: u32,
    w: WPARAM,
    l: LPARAM,
    id: usize,
    _: usize,
) -> LRESULT {
    if message == WM_CLOSE && BUSY.load(Ordering::Acquire) {
        return LRESULT(0);
    }
    if message == WM_NCDESTROY {
        let _ = unsafe { RemoveWindowSubclass(hwnd, Some(guard), id) };
    }
    unsafe { DefSubclassProc(hwnd, message, w, l) }
}
pub fn window() -> Result<HWND, String> {
    let mut hwnd = HWND::default();
    let _ = unsafe {
        EnumThreadWindows(
            GetCurrentThreadId(),
            Some(find_window),
            LPARAM((&mut hwnd as *mut HWND) as isize),
        )
    };
    if hwnd == HWND::default() {
        return Err("Could not find the Director window for transaction protection.".into());
    }
    Ok(hwnd)
}
pub fn install() -> Result<(), String> {
    let hwnd = window()?;
    unsafe { SetWindowSubclass(hwnd, Some(guard), 0x444952, 0) }
        .ok()
        .map_err(|e| format!("Could not protect an in-progress operation: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::{
        Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, IsWindow, SendMessageW, WINDOW_EX_STYLE,
            WS_OVERLAPPEDWINDOW,
        },
        core::w,
    };
    #[test]
    fn closing_waits_for_an_operation_then_works_normally() {
        // Hidden native window: no audio changes and no visible test window.
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!("Director close guard test"),
                WS_OVERLAPPEDWINDOW,
                0,
                0,
                200,
                100,
                None,
                None,
                None,
                None,
            )
        }
        .unwrap();
        struct Cleanup(HWND);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                set_busy(false);
                if unsafe { IsWindow(Some(self.0)) }.as_bool() {
                    let _ = unsafe { DestroyWindow(self.0) };
                }
            }
        }
        let _cleanup = Cleanup(hwnd);
        install().unwrap();
        set_busy(true);
        unsafe {
            SendMessageW(hwnd, WM_CLOSE, Some(WPARAM(0)), Some(LPARAM(0)));
        }
        assert!(
            unsafe { IsWindow(Some(hwnd)) }.as_bool(),
            "busy window must stay open"
        );
        set_busy(false);
        unsafe {
            SendMessageW(hwnd, WM_CLOSE, Some(WPARAM(0)), Some(LPARAM(0)));
        }
        assert!(
            !unsafe { IsWindow(Some(hwnd)) }.as_bool(),
            "idle window must close"
        );
    }
}
