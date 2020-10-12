use std::thread;
use std::sync::{
    Arc,
    Mutex
};

#[macro_use]
extern crate lazy_static;
use imgui::*;
use imgui_impl_win32_rs::Win32Impl;
use d3d9_device_grabber::get_d3d9_device_with_hwnd;
use imgui_dx9_renderer::Renderer;
use detour::static_detour;
use winapi::{
    shared::{
        minwindef::*,
        d3d9::*,
        d3d9types::*,
    },
    um::winnt::{ DLL_PROCESS_ATTACH },
    ctypes::{ c_void },
    um::{ consoleapi::AllocConsole, libloaderapi }
};

#[no_mangle]
#[allow(non_snake_case)]
pub extern "stdcall" fn DllMain(
    hinst_dll: HINSTANCE,
    attach_reason: DWORD,
    _: c_void
) -> BOOL
{
    unsafe {
        libloaderapi::DisableThreadLibraryCalls(hinst_dll);
    }

    match attach_reason {
        DLL_PROCESS_ATTACH => { thread::spawn(|| unsafe { initialize() } ); },
        _ => {},
    };

    return TRUE
}

// Lazy static Arc<Mutex<T>> to hold state
lazy_static! {
static ref STATE: Arc<Mutex<Option<ImHook>>> = {
    Arc::new(Mutex::new(None))
};
}
// Static Detour for EndScene
static_detour! {
    static EndSceneHook: unsafe extern "system" fn(*mut IDirect3DDevice9) -> i32;
}

unsafe fn initialize() {
    let (device, window_handle) = match get_d3d9_device_with_hwnd() {
        Ok((dev, window)) => (dev, window),
        Err(e) => {
            AllocConsole();
            println!("could not get device: {}", e.to_string());
            return
        }
    };
    AllocConsole();
    let endscene = (*device.lpVtbl).EndScene;

    let mut im_ctx = imgui::Context::create();
    im_ctx.style_mut().use_dark_colors();
    im_ctx.fonts();

    let renderer = match Renderer::new_raw(&mut im_ctx, device) {
        Ok(r) => r,
        Err(e) => {
            println!("could not initialize renderer! code `{:#X}`", e);
            return
        }
    };

    let wind_impl = Win32Impl::init(&mut im_ctx, window_handle).unwrap();
    im_ctx.set_ini_filename(Some(std::path::PathBuf::from("imgui.ini")));

    let program_state = ImHook::new(renderer, im_ctx, device, wind_impl);

    match STATE.lock() {
        Ok(mut lock) => {
            *lock = Some(program_state);
        },
        Err(e) => {
            println!("couldnt acquire lock `{}`", e);
            return
        }
    };

    EndSceneHook.initialize(endscene, |x| {
        endscene_hook(x)
    }).unwrap().enable().unwrap();

    println!("hook success");
}

fn endscene_hook(device: *mut IDirect3DDevice9) -> i32 {
    unsafe {

        println!("render time");
        let mut state_lock = STATE.lock().unwrap();

        let state: &mut ImHook = match *state_lock {
            Some(ref mut s) => s,
            None => {
                println!("no ImHook?");
                return EndSceneHook.call(device);
            }
        };

        if let Err(e) = state.window.prepare_frame(&mut state.im_ctx) {
            println!("error: {}", e);
            return EndSceneHook.call(device)
        };

        let frame: Ui = state.im_ctx.frame();
        Window::new(im_str!("test window")).build(&frame, || {});
        frame.show_demo_window(&mut true);
        
        let draw_data = frame.render();
        
        println!("draw_data display_size: {:?}", draw_data.display_size);

        if let Err(e) = state.renderer.render(draw_data) {
            println!("error rendering: {:#X}", e);
        };

        return EndSceneHook.call(device);
    }
}

struct ImHook {
    renderer: Renderer,
    pub im_ctx: Context,
    pub d3d_device: &'static mut IDirect3DDevice9,
    pub window: Win32Impl,
}

impl ImHook {
    pub fn new(renderer: Renderer, im_ctx: Context, d3d_device: &'static mut IDirect3DDevice9, window: Win32Impl) -> ImHook {
        ImHook {
            renderer,
            im_ctx,
            d3d_device,
            window,
        }
    }
}

unsafe impl Send for ImHook {}