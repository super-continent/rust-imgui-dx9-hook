use super::gui;

use crate::helpers::*;

use std::error::Error;
use std::mem;
use std::sync::Arc;

use d3d9_device_grabber::get_d3d9_device;
use detour::static_detour;
use imgui_dx9_renderer::Renderer;
use imgui_impl_win32_rs::*;
use parking_lot::Mutex;
use winapi::shared::{
    d3d9::*,
    d3d9types::{D3DDEVICE_CREATION_PARAMETERS, D3DPRESENT_PARAMETERS},
    minwindef::*,
    windef::HWND,
};
use winapi::um::winuser::{GWLP_WNDPROC, WNDPROC};

lazy_static! {
    static ref IMHOOK_STATE: Arc<Mutex<Option<ImState>>> = Arc::new(Mutex::new(None));
    static ref ORIG_WNDPROC: Arc<Mutex<WNDPROC>> = Arc::new(Mutex::new(None));
}

// Static Detour for EndScene and Reset
static_detour! {
    static EndSceneHook: unsafe extern "system" fn(*mut IDirect3DDevice9) -> i32;
    static ResetHook: unsafe extern "system" fn(*mut IDirect3DDevice9, *mut D3DPRESENT_PARAMETERS) -> i32;
}

pub unsafe fn init_ui() -> Result<(), Box<dyn Error>> {
    info!("Initializing UI");
    let device = get_d3d9_device()?;

    debug!("Got device VTable");

    let endscene = (*device.lpVtbl).EndScene;
    let reset = (*device.lpVtbl).Reset;

    let mut im_ctx = imgui::Context::create();
    im_ctx.style_mut().use_dark_colors();
    im_ctx.fonts();
    im_ctx.set_ini_filename(Some(std::path::PathBuf::from("imgui.ini")));

    debug!("Set up imgui context");

    let program_state = ImState {
        renderer: None,
        window: None,
        im_ctx,
    };

    {
        // initialize global ui hooks state
        *IMHOOK_STATE.lock() = Some(program_state);
    }

    ResetHook.initialize(reset, reset_hook)?.enable()?;
    EndSceneHook.initialize(endscene, endscene_hook)?.enable()?;
    Ok(())
}

fn endscene_hook(device: *mut IDirect3DDevice9) -> i32 {
    unsafe {
        trace!("endscene called");
        let mut state_lock = IMHOOK_STATE.lock();
        //trace!("acquired state lock");
        let state: &mut ImState = match *state_lock {
            Some(ref mut s) => s,
            None => {
                return EndSceneHook.call(device);
            }
        };

        if state.renderer.is_none() {
            let new_renderer = match Renderer::new_raw(&mut state.im_ctx, device) {
                Ok(r) => r,
                Err(e) => {
                    error!("Error creating new renderer: {:#X}", e);
                    return EndSceneHook.call(device);
                }
            };

            state.renderer = Some(new_renderer);
        }

        if state.window.is_none() {
            let mut creation_params: D3DDEVICE_CREATION_PARAMETERS = mem::zeroed();

            if (*device).GetCreationParameters(&mut creation_params) != 0 {
                return EndSceneHook.call(device);
            };

            let new_window = match Win32Impl::init(&mut state.im_ctx, creation_params.hFocusWindow)
            {
                Ok(r) => r,
                Err(e) => {
                    error!("Error creating new Win32Impl: {}", e);
                    return EndSceneHook.call(device);
                }
            };

            state.window = Some(new_window);

            {
                *ORIG_WNDPROC.lock() = get_wndproc(creation_params.hFocusWindow);
                set_window_long_ptr(creation_params.hFocusWindow, GWLP_WNDPROC, wnd_proc as i32);
            }
        }

        // Should always be Some
        if let Some(wind) = state.window.as_mut() {
            if let Err(e) = wind.prepare_frame(&mut state.im_ctx) {
                // Handles error of possibly setting wndproc for wrong window, should never happen.
                error!("Error calling Win32Impl::prepare_frame: {}", e);
                let mut creation_params: D3DDEVICE_CREATION_PARAMETERS = mem::zeroed();

                if (*device).GetCreationParameters(&mut creation_params) != 0 {
                    return EndSceneHook.call(device);
                };

                {
                    *ORIG_WNDPROC.lock() = get_wndproc(creation_params.hFocusWindow);
                    set_window_long_ptr(
                        creation_params.hFocusWindow,
                        GWLP_WNDPROC,
                        wnd_proc as i32,
                    );
                }

                match Win32Impl::init(&mut state.im_ctx, creation_params.hFocusWindow) {
                    Ok(i) => {
                        state.window = Some(i);

                        // Will always be Some, unwrapping is safe
                        let window = state.window.as_mut().unwrap();
                        if let Err(e) = window.prepare_frame(&mut state.im_ctx) {
                            error!("Error calling Win32Impl::prepare_frame: {}", e);
                            return EndSceneHook.call(device);
                        };
                    }
                    Err(e) => {
                        error!("Tried to create new window impl, failed with error: {}", e);
                        return EndSceneHook.call(device);
                    }
                };
            }
        }

        let ui = gui::ui_loop(state.im_ctx.frame());
        let draw_data = ui.render();

        let renderer = match state.renderer.as_mut() {
            Some(r) => r,
            None => {
                error!("no renderer in state");
                return EndSceneHook.call(device);
            }
        };
        if let Err(e) = renderer.render(draw_data) {
            error!("could not render draw data: {}", e);
        };

        EndSceneHook.call(device)
    }
}

fn reset_hook(device: *mut IDirect3DDevice9, present_params: *mut D3DPRESENT_PARAMETERS) -> i32 {
    unsafe {
        trace!("Reset called");
        let mut state_lock = IMHOOK_STATE.lock();
        //trace!("acquired state lock");
        let state: &mut ImState = match *state_lock {
            Some(ref mut s) => s,
            None => {
                return ResetHook.call(device, present_params);
            }
        };

        state.renderer = None;

        ResetHook.call(device, present_params)
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: UINT,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    //trace!("wndproc called");
    let orig_wndproc = *ORIG_WNDPROC.lock();

    if let Err(e) = imgui_win32_window_proc(hwnd, msg, wparam, lparam) {
        error!("Error calling imgui window proc: {}", e);
    };

    return call_wndproc(orig_wndproc, hwnd, msg, wparam, lparam);
}

struct ImState {
    renderer: Option<Renderer>,
    im_ctx: imgui::Context,
    window: Option<Win32Impl>,
}
unsafe impl Send for ImState {}
