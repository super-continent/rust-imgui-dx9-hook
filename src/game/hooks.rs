use super::{get_script_file, ScriptFile, ScriptType, names, offset, types};
use crate::{global, make_fn};

use std::ptr;
use std::ffi::CStr;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use detour::static_detour;
use parking_lot::Mutex;

static_detour! {
    static LoadBBScriptHook: unsafe extern "thiscall" fn (*mut u8, *mut u8, u32);
    //static GameLoopHook: unsafe extern "thiscall" fn (*mut u8);
    //static ProcessEventHook: unsafe extern "stdcall" fn (*mut usize, *mut usize, *mut usize);
}

lazy_static! {
    static ref MATCH_SCRIPTS: Arc<Mutex<BBScriptStorage>> = Arc::new(Mutex::new(BBScriptStorage {
        common: None,
        common_ef: None,
        player_1: None,
        player_2: None,
        player_1_ef: None,
        player_2_ef: None
    }));
    static ref SCRIPT_LOAD_CALL_COUNTER: AtomicUsize = AtomicUsize::new(0);
    static ref SCRIPT_LAST_CHARACTER: Arc<Mutex<ScriptFile>> = Arc::new(Mutex::new(ScriptFile::Sol));
}

pub unsafe fn init_game_hooks() -> Result<(), detour::Error> {
    let base = global::BASE_ADDRESS.load(Ordering::SeqCst);

    // let game_loop_fn =
    //     make_fn!(base + offset::FN_LOOP_ROOT => unsafe extern "thiscall" fn (*mut u8));

    // debug!("game loop address: {:#X}", game_loop_fn as usize);

    // GameLoopHook
    //     .initialize(game_loop_fn, |x| {
    //         trace!("game loop called!");

    //         GameLoopHook.call(x)
    //     })?
    //     .enable()?;

    let load_bbscript_fn = make_fn!(base + offset::FN_LOAD_BBSCRIPT => types::FnLoadBBScript);

    LoadBBScriptHook.initialize(load_bbscript_fn, load_script_hook)?.enable()?;

    Ok(())
}

// Hook for the fn that transfers script pointers.
// This implementation currently will break any modes that load in more than 6 scripts (e.g. MOM mode)
fn load_script_hook(this: *mut u8, script_ptr: *mut u8, script_size: u32) {
    // TODO: figure out how to detect which character and script
    // is being loaded in a non-hacky way, should be a UE3 script function

    trace!("this: {:#X}, script_ptr: {:#X}, script_size: {:#X}", this as usize, script_ptr as usize, script_size);
    
    unsafe {
        let mut last_script = SCRIPT_LAST_CHARACTER.lock();
        let count = SCRIPT_LOAD_CALL_COUNTER.fetch_add(1, Ordering::SeqCst) % 6;
        
        let script = if count == 0 || count == 2 { // player 1/2 main file, we find the char names here
            let state_count = ptr::read_unaligned::<u32>(script_ptr as *const _);
            // calculate offset to a specific bbscript function that seems to always contains the characters name
            let character_str = script_ptr.add((0x8 + ((state_count + 1) * 0x24)) as usize);

            let character_shortname = CStr::from_ptr(character_str as *mut i8);

            debug!("character_shortname: {:?}", character_shortname);

            *last_script = match character_shortname.to_bytes() {
                names::ANSWER => ScriptFile::Answer,
                names::AXL => ScriptFile::Axl,
                names::BAIKEN => ScriptFile::Baiken,
                names::BEDMAN => ScriptFile::Bedman,
                names::CHIPP => ScriptFile::Chipp,
                names::DIZZY => ScriptFile::Dizzy,
                names::ELPHELT => ScriptFile::Elphelt,
                names::FAUST => ScriptFile::Faust,
                names::INO => ScriptFile::Ino,
                names::JACKO => ScriptFile::Jacko,
                names::JAM => ScriptFile::Jam,
                names::JOHNNY => ScriptFile::Johnny,
                names::KUM => ScriptFile::Kum,
                names::KY => ScriptFile::Ky,
                names::LEO => ScriptFile::Leo,
                names::MAY => ScriptFile::May,
                names::MILLIA => ScriptFile::Millia,
                names::POTEMKIN => ScriptFile::Potemkin,
                names::RAMLETHAL => ScriptFile::Ramlethal,
                names::RAVEN => ScriptFile::Raven,
                names::SIN => ScriptFile::Sin,
                names::SLAYER => ScriptFile::Slayer,
                names::SOL => ScriptFile::Sol,
                names::VENOM => ScriptFile::Venom,
                names::ZATO => ScriptFile::Zato,
                _ => ScriptFile::Sol,
            };

            get_script_file(*last_script, ScriptType::Main)
        } else if count == 1 || count == 3 { // player 1/2 effect file
            get_script_file(*last_script, ScriptType::Effect)
        } else if count == 4 { // cmn
            get_script_file(ScriptFile::Common, ScriptType::Main)
        } else { // cmn effect
            get_script_file(ScriptFile::Common, ScriptType::Effect)
        };

        let mut script_storage = MATCH_SCRIPTS.lock();

        match count {
            0 => script_storage.player_1 = script,
            1 => script_storage.player_1_ef = script,
            2 => script_storage.player_2 = script,
            3 => script_storage.player_2_ef = script,
            4 => script_storage.common = script,
            5 => script_storage.common_ef = script,
            _ => {},
        }

        let mods_enabled = global::MODS_ENABLED.load(Ordering::SeqCst);
        debug!("Mods enabled: {}", mods_enabled);
        if mods_enabled {
            if let Some((mod_pointer, mod_size)) = script_storage.get_script_ptr(count) {
                return LoadBBScriptHook.call(this, mod_pointer, mod_size);
            }
        }
        
        LoadBBScriptHook.call(this, script_ptr, script_size)
    }
}

struct BBScriptStorage {
    pub common: Option<Vec<u8>>,
    pub common_ef: Option<Vec<u8>>,
    pub player_1: Option<Vec<u8>>,
    pub player_2: Option<Vec<u8>>,
    pub player_1_ef: Option<Vec<u8>>,
    pub player_2_ef: Option<Vec<u8>>,
}

impl BBScriptStorage {
    /// Index is the number of times the script function has been called during the current match loading cycle
    pub fn get_script_ptr(&mut self, index: usize) -> Option<(*mut u8, u32)> {
        match index {
            0 => {
                if let Some(script) = self.player_1.as_mut() {
                    return Some((script.as_mut_ptr(), script.len() as u32))
                }
            }
            1 => {
                if let Some(script) = self.player_1_ef.as_mut() {
                    return Some((script.as_mut_ptr(), script.len() as u32))
                }
            }
            2 => {
                if let Some(script) = self.player_2.as_mut() {
                    return Some((script.as_mut_ptr(), script.len() as u32))
                }
            }
            3 => {
                if let Some(script) = self.player_2_ef.as_mut() {
                    return Some((script.as_mut_ptr(), script.len() as u32))
                }
            }
            4 => {
                if let Some(script) = self.common.as_mut() {
                    return Some((script.as_mut_ptr(), script.len() as u32))
                }
            }
            5 => {
                if let Some(script) = self.common_ef.as_mut() {
                    return Some((script.as_mut_ptr(), script.len() as u32))
                }
            }
            _ => {}
        }

        None
    }
}