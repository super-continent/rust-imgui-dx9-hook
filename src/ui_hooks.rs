use crate::winapi_helpers::*;

use std::sync::Arc;
use std::error::Error;

use parking_lot::Mutex;
use detour::static_detour;
use winapi::shared::{
    windef::HWND,
    minwindef::*,
    d3d9::*,
};
use winapi::um::winuser::{ WNDPROC, GWLP_WNDPROC };
use imgui::Context;
use d3d9_device_grabber::get_d3d9_device_with_hwnd;
use imgui_dx9_renderer::Renderer;
use imgui_impl_win32_rs::*;

lazy_static! {
    static ref UI_STATE: Arc<Mutex<Option<ImState>>> = {
        Arc::new(Mutex::new(None))
    };
    static ref ORIG_WNDPROC: Arc<Mutex<WNDPROC>> = {
        Arc::new(Mutex::new(None))
    };
}

// Static Detour for EndScene
static_detour! {
    static EndSceneHook: unsafe extern "system" fn(*mut IDirect3DDevice9) -> i32;
}

pub unsafe fn init_ui() -> Result<(), Box<dyn Error>> {
    trace!("Beginning init_ui");
    let (device, window_handle) = match get_d3d9_device_with_hwnd() {
        Ok((dev, window)) => (dev, window),
        Err(e) => {
            return Err(e)
        }
    };
    let endscene = (*device.lpVtbl).EndScene;
    trace!("Got device VTable");

    let mut im_ctx = imgui::Context::create();
    im_ctx.style_mut().use_dark_colors();
    im_ctx.fonts();
    im_ctx.set_ini_filename(Some(std::path::PathBuf::from("imgui.ini")));
    
    let wind_impl = Win32Impl::init(&mut im_ctx, window_handle)?;
    info!("Set up imgui context and window impl");

    *ORIG_WNDPROC.lock() = get_wndproc(window_handle);

    let program_state = 
        ImState {
            renderer: None,
            window: wind_impl,
            im_ctx,
        };
    
    {
        let mut lock = UI_STATE.lock();
        *lock = Some(program_state);
    }

    EndSceneHook.initialize(endscene, endscene_hook)?.enable()?;
    set_window_long_ptr(window_handle, GWLP_WNDPROC, wnd_proc as i32);
    Ok(())
}

fn endscene_hook(device: *mut IDirect3DDevice9) -> i32 {
    unsafe {
        trace!("endscene called");
        let mut state_lock = UI_STATE.lock();
        trace!("acquired state lock");
        let state: &mut ImState = match *state_lock {
            Some(ref mut s) => s,
            None => {
                return EndSceneHook.call(device);
            }
        };

        if let None = state.renderer {
            let new_renderer = match Renderer::new_raw(&mut state.im_ctx, device) {
                Ok(r) => r,
                Err(e) => {
                    error!("Error creating new renderer: {:#X}", e);
                    return EndSceneHook.call(device);
                }
            };

            state.renderer = Some(new_renderer);
        }

        if let Err(e) = state.window.prepare_frame(&mut state.im_ctx) {
            error!("Error calling Win32Impl::prepare_frame: {}", e);
            return EndSceneHook.call(device)
        };

        let ui = state.im_ctx.frame();
        ui.show_demo_window(&mut true);
        
        let draw_data = ui.render();

        let renderer = match state.renderer.as_mut() {
            Some(r) => r,
            None => {
                error!("no renderer in state");
                return EndSceneHook.call(device)
            },
        };
        if let Err(e) = renderer.render(draw_data) {
            error!("could not render draw data: {}", e);
        };

        EndSceneHook.call(device)
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: UINT, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    trace!("wndproc called");
    let orig_wndproc = *ORIG_WNDPROC.lock();

    if let Err(e) = imgui_win32_window_proc(hwnd, msg, wparam, lparam) {
        error!("Error calling imgui window proc: {}", e);
    };

    return call_wndproc(orig_wndproc, hwnd, msg, wparam, lparam);
}

struct ImState {
    renderer: Option<Renderer>,
    im_ctx: Context,
    window: Win32Impl,
}
unsafe impl Send for ImState {}