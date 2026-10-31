// libretro API implementation.

#![allow(static_mut_refs)]
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use super::callbacks;
use super::constants::*;
use super::types::*;
use spmp8000emu_core::config::{CoreConfig, UnknownInstructionPolicy};
use spmp8000emu_core::emulator::Emulator;
use spmp8000emu_core::input_handler::Button;
use spmp8000emu_core::memory::{
    Memory, PERIPHERAL_BASE, PERIPHERAL_SIZE, RAM_BASE, RAM_SIZE, VRAM_BASE, VRAM_SIZE,
};
use std::ffi::{c_void, CStr};
use std::ptr;

const PERFORMANCE_LEVEL: u32 = 4;

/// Global emulator instance
static mut EMULATOR: Option<Emulator> = None;

/// Get a reference to the emulator
unsafe fn get_emulator() -> &'static Emulator {
    EMULATOR.as_ref().expect("Emulator not initialized")
}

/// Get a mutable reference to the emulator
unsafe fn get_emulator_mut() -> &'static mut Emulator {
    EMULATOR.as_mut().expect("Emulator not initialized")
}

// ============================================================
// Startup functions
// ============================================================

#[no_mangle]
pub extern "C" fn retro_set_environment(cb: retro_environment_t) {
    callbacks::set_environment(cb);
    set_core_options();
}

#[no_mangle]
pub extern "C" fn retro_set_video_refresh(cb: retro_video_refresh_t) {
    callbacks::set_video_refresh(cb);
}

#[no_mangle]
pub extern "C" fn retro_set_audio_sample(cb: retro_audio_sample_t) {
    callbacks::set_audio_sample(cb);
}

#[no_mangle]
pub extern "C" fn retro_set_audio_sample_batch(cb: retro_audio_sample_batch_t) {
    callbacks::set_audio_sample_batch(cb);
}

#[no_mangle]
pub extern "C" fn retro_set_input_poll(cb: retro_input_poll_t) {
    callbacks::set_input_poll(cb);
}

#[no_mangle]
pub extern "C" fn retro_set_input_state(cb: retro_input_state_t) {
    callbacks::set_input_state(cb);
}

#[no_mangle]
pub extern "C" fn retro_api_version() -> u32 {
    RETRO_API_VERSION
}

#[no_mangle]
pub extern "C" fn retro_init() {
    callbacks::init_log();
    super::logger::init();
    log::info!("SPMP8000Emu libretro core initialized");
}

#[no_mangle]
pub extern "C" fn retro_deinit() {
    unsafe {
        EMULATOR = None;
    }
    super::logger::set_debug_logging(false);
    log::info!("SPMP8000Emu libretro core deinitialized");
}

#[no_mangle]
pub extern "C" fn retro_get_system_info(info: *mut retro_system_info) {
    unsafe {
        (*info) = retro_system_info {
            library_name: c"SPMP8000Emu".as_ptr(),
            library_version: c"1.0.0".as_ptr(),
            valid_extensions: c"bin".as_ptr(),
            need_fullpath: true,
            block_extract: false,
        };
    }
}

#[no_mangle]
pub extern "C" fn retro_set_controller_port_device(_port: u32, _device: u32) {
    // SPMP8000 only supports basic joypad
}

// ============================================================
// Running functions
// ============================================================

#[no_mangle]
pub extern "C" fn retro_load_game(info: *const retro_game_info) -> bool {
    unsafe {
        let game_info = &*info;

        if game_info.path.is_null() {
            // need_fullpath is true, so a file path is the only supported input.
            // Memory buffers cannot be mapped into the NGame loader, which reads
            // the file, parses its header and derives the game directory.
            return fail(
                "SPMP8000Emu: no content path was provided.\n\n\
                 This core loads games from a file (need_fullpath); in-memory content \
                 is not supported. Select a .bin game file in the frontend.",
            );
        }
        if !game_info.data.is_null() || game_info.size != 0 {
            log::warn!(
                "Ignoring the in-memory buffer supplied for content; loading from the path instead"
            );
        }

        let path = match CStr::from_ptr(game_info.path).to_str() {
            Ok(p) => p,
            Err(e) => {
                return fail(&format!(
                    "SPMP8000Emu: the content path is not valid UTF-8 ({e}).\n\n\
                     Move the game to a path the frontend can represent and load it again."
                ));
            }
        };

        // Set pixel format to XRGB8888
        let pixel_format = retro_pixel_format::RETRO_PIXEL_FORMAT_XRGB8888;
        let success = callbacks::environment(
            RETRO_ENVIRONMENT_SET_PIXEL_FORMAT,
            &pixel_format as *const _ as *mut c_void,
        );
        if !success {
            return fail(
                "SPMP8000Emu: the frontend refused the XRGB8888 pixel format.\n\n\
                 RetroDroid supports RGB565, RGB1555 and XRGB8888; update the \
                 frontend or use a compatible core build.",
            );
        }

        register_input_descriptors();

        callbacks::environment(
            RETRO_ENVIRONMENT_SET_PERFORMANCE_LEVEL,
            &PERFORMANCE_LEVEL as *const _ as *mut c_void,
        );

        let content_path = std::path::Path::new(path);
        if !content_path.is_file() {
            return fail(&format!(
                "SPMP8000Emu: content file not found:\n{path}\n\n\
                 The core loads .bin content from that exact path. RetroDroid \
                 copies the SAF selection into its app-private cache before \
                 loading, so the file must still exist and be readable there."
            ));
        }

        let config = read_core_config();
        super::logger::set_debug_logging(config.debug_logging);
        log_core_config("loaded", &config);

        // Create emulator instance
        match Emulator::from_path_with_config(std::path::PathBuf::from(path), config) {
            Ok(mut emu) => {
                let (width, height) = emu.get_resolution();
                log::info!("Game loaded: {} ({}x{})", path, width, height);
                emu.start();
                EMULATOR = Some(emu);
                register_memory_maps(get_emulator_mut());
                true
            }
            Err(e) => fail(&format!(
                "SPMP8000Emu: failed to load content:\n{path}\n{e:#}\n\n\
                 Only NGame .bin games are supported."
            )),
        }
    }
}

/// Report a load failure to both the log and the frontend's on-screen message.
fn fail(message: &str) -> bool {
    log::error!("{message}");
    callbacks::show_message(message, 240);
    false
}

#[no_mangle]
pub extern "C" fn retro_unload_game() {
    unsafe {
        EMULATOR = None;
    }
    log::info!("Game unloaded");
}

#[no_mangle]
pub extern "C" fn retro_get_system_av_info(info: *mut retro_system_av_info) {
    unsafe {
        let emu = get_emulator();
        let (width, height) = emu.get_resolution();
        let sample_rate = emu.get_audio_sample_rate();

        (*info) = retro_system_av_info {
            geometry: retro_game_geometry {
                base_width: width,
                base_height: height,
                max_width: width,
                max_height: height,
                aspect_ratio: width as f32 / height as f32,
            },
            timing: retro_system_timing {
                fps: 30.0,
                sample_rate,
            },
        };
    }
}

#[no_mangle]
pub extern "C" fn retro_run() {
    unsafe {
        if core_options_changed() {
            let config = read_core_config();
            super::logger::set_debug_logging(config.debug_logging);
            log_core_config("updated", &config);
            get_emulator_mut().set_config(config);
        }

        let emu = get_emulator_mut();

        // Poll input
        callbacks::input_poll();

        // Read button states
        let mut buttons: u32 = 0;
        if callbacks::input_state(0, RETRO_DEVICE_JOYPAD, 0, RETRO_DEVICE_ID_JOYPAD_UP) != 0 {
            buttons |= Button::Up.mask();
        }
        if callbacks::input_state(0, RETRO_DEVICE_JOYPAD, 0, RETRO_DEVICE_ID_JOYPAD_DOWN) != 0 {
            buttons |= Button::Down.mask();
        }
        if callbacks::input_state(0, RETRO_DEVICE_JOYPAD, 0, RETRO_DEVICE_ID_JOYPAD_LEFT) != 0 {
            buttons |= Button::Left.mask();
        }
        if callbacks::input_state(0, RETRO_DEVICE_JOYPAD, 0, RETRO_DEVICE_ID_JOYPAD_RIGHT) != 0 {
            buttons |= Button::Right.mask();
        }
        if callbacks::input_state(0, RETRO_DEVICE_JOYPAD, 0, RETRO_DEVICE_ID_JOYPAD_A) != 0 {
            buttons |= Button::O.mask();
        }
        if callbacks::input_state(0, RETRO_DEVICE_JOYPAD, 0, RETRO_DEVICE_ID_JOYPAD_B) != 0 {
            buttons |= Button::X.mask();
        }
        if callbacks::input_state(0, RETRO_DEVICE_JOYPAD, 0, RETRO_DEVICE_ID_JOYPAD_START) != 0 {
            buttons |= Button::Start.mask();
        }
        if callbacks::input_state(0, RETRO_DEVICE_JOYPAD, 0, RETRO_DEVICE_ID_JOYPAD_SELECT) != 0 {
            buttons |= Button::Select.mask();
        }

        emu.set_buttons(buttons);

        // Execute one frame
        emu.tick();

        // Submit framebuffer
        let (width, height) = emu.get_resolution();
        let framebuffer = emu.get_framebuffer();
        callbacks::video_refresh(
            framebuffer.as_ptr() as *const c_void,
            width,
            height,
            (width * 4) as usize, // XRGB8888
        );

        // Submit audio samples
        let samples = emu.get_audio_samples();
        if !samples.is_empty() {
            callbacks::audio_sample_batch(samples.as_ptr(), samples.len() / 2);
        }
    }
}

fn log_core_config(action: &str, config: &CoreConfig) {
    log::info!(
        "Core options {}: volume={} swap_o_x={} debug_logging={} unknown_instruction_policy={:?}",
        action,
        config.volume,
        config.swap_o_x,
        config.debug_logging,
        config.unknown_instruction_policy
    );
}

// ============================================================
// Stub functions
// ============================================================

#[no_mangle]
pub extern "C" fn retro_load_game_special(
    _type: u32,
    _info: *const retro_game_info,
    _num: usize,
) -> bool {
    fail(
        "SPMP8000Emu: special content loading is not supported.\n\n\
         Load the NGame .bin game directly.",
    )
}

#[no_mangle]
pub extern "C" fn retro_serialize_size() -> usize {
    unsafe {
        match EMULATOR.as_ref() {
            Some(emu) => emu.serialize_size(),
            None => 0,
        }
    }
}

#[no_mangle]
pub extern "C" fn retro_serialize(data: *mut c_void, size: usize) -> bool {
    unsafe {
        let Some(emu) = EMULATOR.as_ref() else {
            return false;
        };
        let required_size = emu.serialize_size();
        if data.is_null() || size < required_size {
            return false;
        }

        let buffer = std::slice::from_raw_parts_mut(data as *mut u8, required_size);
        match emu.serialize(buffer) {
            Ok(()) => true,
            Err(error) => {
                log::error!("Failed to serialize game state: {}", error);
                false
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn retro_unserialize(data: *const c_void, size: usize) -> bool {
    unsafe {
        let Some(emu) = EMULATOR.as_mut() else {
            return false;
        };
        if data.is_null() || size == 0 {
            return false;
        }

        let buffer = std::slice::from_raw_parts(data as *const u8, size);
        match emu.deserialize(buffer) {
            Ok(()) => true,
            Err(error) => {
                log::error!("Failed to restore game state: {}", error);
                false
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn retro_get_region() -> u32 {
    RETRO_REGION_NTSC
}

#[no_mangle]
pub extern "C" fn retro_cheat_reset() {
    unsafe {
        if let Some(emu) = EMULATOR.as_mut() {
            emu.clear_cheats();
        }
    }
}

#[no_mangle]
pub extern "C" fn retro_cheat_set(index: u32, enabled: bool, code: *const std::ffi::c_char) {
    unsafe {
        let Some(emu) = EMULATOR.as_mut() else {
            return;
        };
        let code = if code.is_null() {
            ""
        } else {
            match CStr::from_ptr(code).to_str() {
                Ok(code) => code,
                Err(error) => {
                    log::warn!("Ignoring invalid UTF-8 cheat at slot {}: {}", index, error);
                    return;
                }
            }
        };

        if let Err(error) = emu.set_cheat_slot(index, enabled, code) {
            log::warn!(
                "Ignoring invalid cheat at slot {} ('{}'): {}",
                index,
                code,
                error
            );
        }
    }
}

#[no_mangle]
pub extern "C" fn retro_reset() {
    unsafe {
        if let Some(emu) = EMULATOR.as_mut() {
            match emu.reset() {
                Ok(()) => {
                    emu.start();
                    log::info!("Game reset");
                }
                Err(error) => log::error!("Failed to reset game: {}", error),
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn retro_get_memory_data(id: u32) -> *mut c_void {
    unsafe {
        let Some(emu) = EMULATOR.as_mut() else {
            return ptr::null_mut();
        };
        match id & RETRO_MEMORY_MASK {
            RETRO_MEMORY_SYSTEM_RAM => emu
                .memory
                .system_ram_mut()
                .map_or(ptr::null_mut(), |ram| ram.as_mut_ptr().cast()),
            RETRO_MEMORY_VIDEO_RAM => emu
                .memory
                .video_ram_mut()
                .map_or(ptr::null_mut(), |vram| vram.as_mut_ptr().cast()),
            _ => ptr::null_mut(),
        }
    }
}

#[no_mangle]
pub extern "C" fn retro_get_memory_size(id: u32) -> usize {
    unsafe {
        let Some(emu) = EMULATOR.as_ref() else {
            return 0;
        };
        match id & RETRO_MEMORY_MASK {
            RETRO_MEMORY_SYSTEM_RAM => emu.memory.system_ram().map_or(0, <[u8]>::len),
            RETRO_MEMORY_VIDEO_RAM => emu.memory.video_ram().map_or(0, <[u8]>::len),
            _ => 0,
        }
    }
}

fn memory_descriptors(memory: &mut Memory) -> Option<[retro_memory_descriptor; 3]> {
    let ram = memory.system_ram_mut()?.as_mut_ptr().cast();
    let vram = memory.video_ram_mut()?.as_mut_ptr().cast();
    let peripherals = memory.peripheral_memory_mut()?.as_mut_ptr().cast();
    Some([
        retro_memory_descriptor {
            flags: RETRO_MEMDESC_SYSTEM_RAM,
            ptr: ram,
            offset: 0,
            start: RAM_BASE as usize,
            select: 0,
            disconnect: 0,
            len: RAM_SIZE as usize,
            addrspace: c"SPMP".as_ptr(),
        },
        retro_memory_descriptor {
            flags: RETRO_MEMDESC_VIDEO_RAM,
            ptr: vram,
            offset: 0,
            start: VRAM_BASE as usize,
            select: 0,
            disconnect: 0,
            len: VRAM_SIZE as usize,
            addrspace: c"SPMP".as_ptr(),
        },
        retro_memory_descriptor {
            flags: 0,
            ptr: peripherals,
            offset: 0,
            start: PERIPHERAL_BASE as usize,
            select: 0,
            disconnect: 0,
            len: PERIPHERAL_SIZE as usize,
            addrspace: c"SPMP".as_ptr(),
        },
    ])
}

fn register_memory_maps(emu: &mut Emulator) {
    let Some(descriptors) = memory_descriptors(&mut emu.memory) else {
        log::error!("Failed to expose the SPMP8000 memory map");
        return;
    };
    let memory_map = retro_memory_map {
        descriptors: descriptors.as_ptr(),
        num_descriptors: descriptors.len() as u32,
    };
    if callbacks::environment(
        RETRO_ENVIRONMENT_SET_MEMORY_MAPS,
        &memory_map as *const _ as *mut c_void,
    ) {
        log::info!("Registered RAM, VRAM, and peripheral memory descriptors");
    } else {
        log::warn!("Frontend did not accept SPMP8000 memory descriptors");
    }
}

fn input_descriptors() -> [retro_input_descriptor; 9] {
    [
        retro_input_descriptor {
            port: 0,
            device: RETRO_DEVICE_JOYPAD,
            index: 0,
            id: RETRO_DEVICE_ID_JOYPAD_UP,
            description: c"D-Pad Up".as_ptr(),
        },
        retro_input_descriptor {
            port: 0,
            device: RETRO_DEVICE_JOYPAD,
            index: 0,
            id: RETRO_DEVICE_ID_JOYPAD_DOWN,
            description: c"D-Pad Down".as_ptr(),
        },
        retro_input_descriptor {
            port: 0,
            device: RETRO_DEVICE_JOYPAD,
            index: 0,
            id: RETRO_DEVICE_ID_JOYPAD_LEFT,
            description: c"D-Pad Left".as_ptr(),
        },
        retro_input_descriptor {
            port: 0,
            device: RETRO_DEVICE_JOYPAD,
            index: 0,
            id: RETRO_DEVICE_ID_JOYPAD_RIGHT,
            description: c"D-Pad Right".as_ptr(),
        },
        retro_input_descriptor {
            port: 0,
            device: RETRO_DEVICE_JOYPAD,
            index: 0,
            id: RETRO_DEVICE_ID_JOYPAD_A,
            description: c"O Button".as_ptr(),
        },
        retro_input_descriptor {
            port: 0,
            device: RETRO_DEVICE_JOYPAD,
            index: 0,
            id: RETRO_DEVICE_ID_JOYPAD_B,
            description: c"X Button".as_ptr(),
        },
        retro_input_descriptor {
            port: 0,
            device: RETRO_DEVICE_JOYPAD,
            index: 0,
            id: RETRO_DEVICE_ID_JOYPAD_START,
            description: c"Start".as_ptr(),
        },
        retro_input_descriptor {
            port: 0,
            device: RETRO_DEVICE_JOYPAD,
            index: 0,
            id: RETRO_DEVICE_ID_JOYPAD_SELECT,
            description: c"Select".as_ptr(),
        },
        retro_input_descriptor {
            port: 0,
            device: RETRO_DEVICE_NONE,
            index: 0,
            id: 0,
            description: ptr::null(),
        },
    ]
}

fn register_input_descriptors() {
    let descriptors = input_descriptors();
    callbacks::environment(
        RETRO_ENVIRONMENT_SET_INPUT_DESCRIPTORS,
        descriptors.as_ptr() as *mut c_void,
    );
}

fn core_option_variables() -> [retro_variable; 5] {
    [
        retro_variable {
            key: c"spmp8000emu_volume".as_ptr(),
            value: c"Audio Volume (%); 100|90|80|70|60|50|40|30|20|10|0".as_ptr(),
        },
        retro_variable {
            key: c"spmp8000emu_swap_ox".as_ptr(),
            value: c"Swap O/X Buttons; disabled|enabled".as_ptr(),
        },
        retro_variable {
            key: c"spmp8000emu_debug_logging".as_ptr(),
            value: c"CPU/HLE Debug Logging; disabled|enabled".as_ptr(),
        },
        retro_variable {
            key: c"spmp8000emu_unknown_instruction".as_ptr(),
            value: c"Unknown ARM Instruction Policy; stop|skip".as_ptr(),
        },
        retro_variable {
            key: ptr::null(),
            value: ptr::null(),
        },
    ]
}

fn set_core_options() {
    let variables = core_option_variables();
    callbacks::environment(
        RETRO_ENVIRONMENT_SET_VARIABLES,
        variables.as_ptr() as *mut c_void,
    );
}

fn get_core_option(key: &CStr) -> Option<String> {
    let mut variable = retro_variable {
        key: key.as_ptr(),
        value: ptr::null(),
    };
    let success = callbacks::environment(
        RETRO_ENVIRONMENT_GET_VARIABLE,
        &mut variable as *mut _ as *mut c_void,
    );
    if success && !variable.value.is_null() {
        unsafe {
            CStr::from_ptr(variable.value)
                .to_str()
                .ok()
                .map(str::to_owned)
        }
    } else {
        None
    }
}

fn core_options_changed() -> bool {
    let mut updated = false;
    let success = callbacks::environment(
        RETRO_ENVIRONMENT_GET_VARIABLE_UPDATE,
        &mut updated as *mut _ as *mut c_void,
    );
    success && updated
}

fn read_core_config() -> CoreConfig {
    parse_core_config(get_core_option)
}

fn parse_core_config(mut get: impl FnMut(&CStr) -> Option<String>) -> CoreConfig {
    let mut config = CoreConfig::default();
    if let Some(volume) = get(c"spmp8000emu_volume").and_then(|value| value.parse().ok()) {
        config.volume = volume;
    }
    if let Some(swap) = get(c"spmp8000emu_swap_ox") {
        config.swap_o_x = swap == "enabled";
    }
    if let Some(debug) = get(c"spmp8000emu_debug_logging") {
        config.debug_logging = debug == "enabled";
    }
    if let Some(policy) = get(c"spmp8000emu_unknown_instruction") {
        config.unknown_instruction_policy = if policy == "skip" {
            UnknownInstructionPolicy::Skip
        } else {
            UnknownInstructionPolicy::Stop
        };
    }
    config.normalized()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn memory_descriptor_list_matches_the_spmp_address_space() {
        let mut memory = Memory::new();
        memory.init_default().unwrap();
        let descriptors = memory_descriptors(&mut memory).unwrap();

        assert_eq!(descriptors[0].flags, RETRO_MEMDESC_SYSTEM_RAM);
        assert_eq!(descriptors[0].start, RAM_BASE as usize);
        assert_eq!(descriptors[0].len, RAM_SIZE as usize);
        assert_eq!(descriptors[1].flags, RETRO_MEMDESC_VIDEO_RAM);
        assert_eq!(descriptors[1].start, VRAM_BASE as usize);
        assert_eq!(descriptors[1].len, VRAM_SIZE as usize);
        assert_eq!(descriptors[2].flags, 0);
        assert_eq!(descriptors[2].start, PERIPHERAL_BASE as usize);
        assert_eq!(descriptors[2].len, PERIPHERAL_SIZE as usize);
        assert!(descriptors
            .iter()
            .all(|descriptor| !descriptor.ptr.is_null()));
        assert!(descriptors
            .iter()
            .all(|descriptor| unsafe { CStr::from_ptr(descriptor.addrspace) == c"SPMP" }));
    }

    #[test]
    fn input_descriptor_list_covers_all_supported_buttons() {
        let descriptors = input_descriptors();
        let ids: Vec<u32> = descriptors[..8]
            .iter()
            .map(|descriptor| descriptor.id)
            .collect();

        assert_eq!(
            ids,
            [
                RETRO_DEVICE_ID_JOYPAD_UP,
                RETRO_DEVICE_ID_JOYPAD_DOWN,
                RETRO_DEVICE_ID_JOYPAD_LEFT,
                RETRO_DEVICE_ID_JOYPAD_RIGHT,
                RETRO_DEVICE_ID_JOYPAD_A,
                RETRO_DEVICE_ID_JOYPAD_B,
                RETRO_DEVICE_ID_JOYPAD_START,
                RETRO_DEVICE_ID_JOYPAD_SELECT,
            ]
        );
        assert_eq!(descriptors[8].device, RETRO_DEVICE_NONE);
        assert!(descriptors[8].description.is_null());
    }

    #[test]
    fn core_options_have_stable_keys_and_defaults() {
        let variables = core_option_variables();
        let keys: Vec<&CStr> = variables[..4]
            .iter()
            .map(|variable| unsafe { CStr::from_ptr(variable.key) })
            .collect();
        assert_eq!(
            keys,
            [
                c"spmp8000emu_volume",
                c"spmp8000emu_swap_ox",
                c"spmp8000emu_debug_logging",
                c"spmp8000emu_unknown_instruction",
            ]
        );
        assert!(variables[4].key.is_null());

        let config = parse_core_config(|key| match key.to_bytes() {
            b"spmp8000emu_volume" => Some("40".to_string()),
            b"spmp8000emu_swap_ox" => Some("enabled".to_string()),
            b"spmp8000emu_debug_logging" => Some("enabled".to_string()),
            b"spmp8000emu_unknown_instruction" => Some("skip".to_string()),
            _ => None,
        });
        assert_eq!(config.volume, 40);
        assert!(config.swap_o_x);
        assert!(config.debug_logging);
        assert_eq!(
            config.unknown_instruction_policy,
            UnknownInstructionPolicy::Skip
        );
    }

    #[test]
    fn core_naming_and_info_match_implemented_features() {
        let core_info = include_str!("../../spmp8000emu_libretro.info");
        let libretro_manifest = include_str!("../../Cargo.toml");
        let core_manifest = include_str!("../../../spmp8000emu-core/Cargo.toml");
        let standalone_manifest = include_str!("../../../spmp8000emu/Cargo.toml");
        let workspace_manifest = include_str!("../../../../Cargo.toml");
        let buildbot_config = include_str!("../../../../.gitlab-ci.yml");

        assert!(libretro_manifest.contains("name = \"spmp8000emu-libretro\""));
        assert!(libretro_manifest.contains("name = \"spmp8000emu\""));
        assert!(core_manifest.contains("name = \"spmp8000emu-core\""));
        assert!(core_manifest.contains("name = \"spmp8000emu_core\""));
        assert!(standalone_manifest.contains("name = \"spmp8000emu\""));
        assert!(standalone_manifest.contains("name = \"spmp8000-emu\""));
        assert!(workspace_manifest.contains("\"crates/spmp8000emu-core\""));
        assert!(workspace_manifest.contains("\"crates/spmp8000emu\""));
        assert!(workspace_manifest.contains("\"crates/spmp8000emu-libretro\""));
        assert!(buildbot_config.contains("CORENAME: spmp8000emu"));
        assert!(core_info.contains("corename = \"spmp8000emu\""));
        assert!(core_info.contains("savestate = \"true\""));
        assert!(core_info.contains("cheats = \"true\""));
        assert!(core_info.contains("input_descriptors = \"true\""));
        assert!(core_info.contains("memory_descriptors = \"true\""));
        assert!(core_info.contains("core_options = \"true\""));
    }

    unsafe extern "C" fn integration_environment(cmd: u32, _data: *mut c_void) -> bool {
        !matches!(
            cmd,
            RETRO_ENVIRONMENT_GET_VARIABLE | RETRO_ENVIRONMENT_GET_VARIABLE_UPDATE
        )
    }

    #[test]
    #[ignore = "requires local SmartBlocks game assets (set SPMP8000_GAME_DIR)"]
    fn real_content_exposes_memory_and_applies_libretro_cheat_slots() {
        let game_dir = std::env::var_os("SPMP8000_GAME_DIR").expect("SPMP8000_GAME_DIR is not set");
        let game_path = std::path::PathBuf::from(game_dir).join("SmartBlocks-1.4.2_P_new.bin");
        assert!(game_path.is_file(), "missing {}", game_path.display());
        let game_path = CString::new(game_path.to_string_lossy().as_bytes()).unwrap();

        retro_set_environment(integration_environment);
        let game_info = retro_game_info {
            path: game_path.as_ptr(),
            data: ptr::null(),
            size: 0,
            meta: ptr::null(),
        };
        assert!(retro_load_game(&game_info));
        assert_eq!(
            retro_get_memory_size(RETRO_MEMORY_SYSTEM_RAM),
            RAM_SIZE as usize
        );
        assert_eq!(
            retro_get_memory_size(RETRO_MEMORY_VIDEO_RAM),
            VRAM_SIZE as usize
        );
        let ram = retro_get_memory_data(RETRO_MEMORY_SYSTEM_RAM).cast::<u8>();
        let vram = retro_get_memory_data(RETRO_MEMORY_VIDEO_RAM);
        assert!(!ram.is_null());
        assert!(!vram.is_null());

        let freeze = CString::new("mem32:0x00001000=0x12345678").unwrap();
        let disabled = CString::new("mem32:0x00001000=0").unwrap();
        retro_cheat_set(0, true, freeze.as_ptr());
        retro_cheat_set(1, false, disabled.as_ptr());
        retro_run();
        assert_eq!(
            unsafe { std::ptr::read_unaligned(ram.add(0x1000).cast::<u32>()) },
            0x1234_5678
        );

        unsafe {
            std::ptr::write_unaligned(ram.add(0x1000).cast::<u32>(), 0);
        }
        let invalid = CString::new("mem32:0x02000000=1").unwrap();
        retro_cheat_set(0, true, invalid.as_ptr());
        retro_run();
        assert_eq!(
            unsafe { std::ptr::read_unaligned(ram.add(0x1000).cast::<u32>()) },
            0x1234_5678
        );

        let original_ram = ram;
        retro_reset();
        assert_eq!(
            retro_get_memory_data(RETRO_MEMORY_SYSTEM_RAM).cast::<u8>(),
            original_ram
        );
        retro_run();
        assert_eq!(
            unsafe { std::ptr::read_unaligned(ram.add(0x1000).cast::<u32>()) },
            0x1234_5678
        );

        retro_cheat_reset();
        unsafe {
            std::ptr::write_unaligned(ram.add(0x1000).cast::<u32>(), 0);
        }
        retro_run();
        assert_eq!(
            unsafe { std::ptr::read_unaligned(ram.add(0x1000).cast::<u32>()) },
            0
        );

        retro_unload_game();
        assert!(retro_get_memory_data(RETRO_MEMORY_SYSTEM_RAM).is_null());
        assert_eq!(retro_get_memory_size(RETRO_MEMORY_SYSTEM_RAM), 0);
    }
}
