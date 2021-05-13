#![feature(once_cell, abi_thiscall, const_fn_trait_bound)]

mod game;
mod global;
mod helpers;
mod ui;

use std::ffi::{CString, OsString};
use std::fs;
use std::mem;
use std::os::windows::ffi::OsStringExt;
use std::ptr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
use simplelog::*;
use winapi::{
    ctypes::c_void,
    shared::guiddef::REFIID,
    shared::minwindef::*,
    shared::ntdef::HRESULT,
    shared::winerror,
    um::libloaderapi,
    um::sysinfoapi::GetSystemDirectoryW,
    um::{unknwnbase::LPUNKNOWN, winnt::DLL_PROCESS_ATTACH},
};

const LOG_LEVEL: LevelFilter = LevelFilter::Debug;

#[no_mangle]
#[allow(non_snake_case)]
pub extern "stdcall" fn DllMain(hinst_dll: HINSTANCE, attach_reason: DWORD, _: c_void) -> BOOL {
    unsafe {
        libloaderapi::DisableThreadLibraryCalls(hinst_dll);
    }

    match attach_reason {
        DLL_PROCESS_ATTACH => {
            thread::spawn(|| unsafe { initialize() });
        }
        _ => {}
    };

    return TRUE;
}

unsafe fn initialize() {
    WriteLogger::init(
        LOG_LEVEL,
        Config::default(),
        std::fs::File::create("rev2mod.log").unwrap(),
    )
    .unwrap();

    info!("Initializing!");

    info!(
        "Mods folder created: {}",
        fs::create_dir(global::MODS_FOLDER).is_ok()
    );

    let base_addr = libloaderapi::GetModuleHandleA(ptr::null_mut());

    global::BASE_ADDRESS.store(base_addr as u32, Ordering::SeqCst);

    let mut ui_result = ui::ui_hooks::init_ui();
    while let Err(e) = ui_result {
        error!("Initializing UI failed: {}", e);
        thread::sleep(std::time::Duration::from_secs(1));
        ui_result = ui::ui_hooks::init_ui();
    }
    info!("UI hook success!");

    let game_result = game::hooks::init_game_hooks();

    info!("Game hooks ok?: {}", game_result.is_ok());
}

// type alias to make transmute cleaner
type DInput8Create =
    extern "stdcall" fn(HINSTANCE, DWORD, REFIID, *mut LPVOID, LPUNKNOWN) -> HRESULT;

const SYSTEM32_DEFAULT: &str = r"C:\Windows\System32";

lazy_static! {
    static ref REAL_DINPUT8_HANDLE: AtomicU32 = AtomicU32::new(0);
}

// Used by GuiltyGearXrd.exe, lets you rename the DLL to dinput8.dll and have it load on startup
#[no_mangle]
pub unsafe extern "stdcall" fn DirectInput8Create(
    inst_handle: HINSTANCE,
    version: DWORD,
    r_iid: REFIID,
    ppv_out: *mut LPVOID,
    p_unk_outer: LPUNKNOWN,
) -> HRESULT {
    debug!("DirectInput8Create called");

    // Load real dinput8.dll if not already loaded
    if REAL_DINPUT8_HANDLE.load(Ordering::SeqCst) == 0 {
        let mut buffer = [0u16; MAX_PATH];
        let written_wchars = GetSystemDirectoryW(buffer.as_mut_ptr(), MAX_PATH as u32);

        let system_directory = if written_wchars == 0 {
            SYSTEM32_DEFAULT.into()
        } else {
            let str_with_nulls = OsString::from_wide(&buffer)
                .into_string()
                .unwrap_or(SYSTEM32_DEFAULT.into());
            str_with_nulls.trim_matches('\0').to_string()
        };

        let dinput_path = system_directory + r"\dinput8.dll";
        debug!("Got real dinput8.dll path: `{}`", dinput_path);

        let real_dinput_handle =
            libloaderapi::LoadLibraryW(helpers::win32_wstring(&dinput_path).as_mut_ptr());

        if !real_dinput_handle.is_null() {
            debug!(
                "Storing pointer to real DInput8: `{:#X}`",
                real_dinput_handle as u32
            );
            REAL_DINPUT8_HANDLE.store(real_dinput_handle as u32, Ordering::SeqCst);
        }
    }

    let real_dinput8 = REAL_DINPUT8_HANDLE.load(Ordering::SeqCst) as HINSTANCE;
    let dinput8create_fn_name =
        CString::new("DirectInput8Create").expect("CString::new(`DirectInput8Create`) failed");

    let dinput8_create = libloaderapi::GetProcAddress(real_dinput8, dinput8create_fn_name.as_ptr());

    if !real_dinput8.is_null() && !dinput8_create.is_null() {
        let dinput8create_fn = mem::transmute::<_, DInput8Create>(dinput8_create);
        return dinput8create_fn(inst_handle, version, r_iid, ppv_out, p_unk_outer);
    }

    winerror::E_FAIL // Unspecified failure
}
