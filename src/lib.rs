mod ui_hooks;
mod winapi_helpers;

use std::thread;

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
use simplelog::*;
use winapi::{
    ctypes::c_void, shared::minwindef::*, um::libloaderapi, um::winnt::DLL_PROCESS_ATTACH,
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
        std::fs::File::create("rust_imgui_hook.log").unwrap(),
    )
    .unwrap();
    info!("Initializing hooks!");
    let mut ui_result = ui_hooks::init_ui();
    while let Err(e) = ui_result {
        ui_result = ui_hooks::init_ui();
        error!("error initializing UI: {}", e);
    }

    info!("hook success");
}
