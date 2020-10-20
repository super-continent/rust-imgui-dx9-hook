use std::mem;
use winapi::ctypes::c_int;

use winapi::shared::{
    windef::HWND,
    minwindef::*,
};
use winapi::um::winuser::{ IsWindowUnicode, CallWindowProcA, CallWindowProcW, SetWindowLongPtrA, SetWindowLongPtrW, GetWindowLongPtrA, GetWindowLongPtrW, GWLP_WNDPROC, WNDPROC };

pub unsafe fn set_window_long_ptr(hwnd: HWND, index: c_int, new_long: i32) -> i32 {
    match IsWindowUnicode(hwnd) {
        0 => SetWindowLongPtrA(hwnd, index, new_long),
        _ => SetWindowLongPtrW(hwnd, index, new_long)
    }
}

pub unsafe fn get_wndproc(hwnd: HWND) -> WNDPROC {
    // make the transmute cleaner
    type WndProcfn = unsafe extern "system" fn(HWND, UINT, WPARAM, LPARAM) -> isize;

    let wndproc_i = match IsWindowUnicode(hwnd) {
        0 => GetWindowLongPtrA(hwnd, GWLP_WNDPROC),
        _ => GetWindowLongPtrW(hwnd, GWLP_WNDPROC)
    };

    if wndproc_i != 0 {
        return Some(mem::transmute::<i32, WndProcfn>(wndproc_i))
    } else {
        return None
    }
}

pub unsafe fn call_wndproc(prev_wnd_func: WNDPROC, hwnd: HWND, msg: UINT, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match IsWindowUnicode(hwnd) {
        0 => CallWindowProcA(prev_wnd_func, hwnd, msg, wparam, lparam),
        _ => CallWindowProcW(prev_wnd_func, hwnd, msg, wparam, lparam)
    }
}