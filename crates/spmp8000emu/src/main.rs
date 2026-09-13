// SPMP8000 Emulator - standalone front-end (minifb window + CLI).
//
// This binary reuses the shared emulator core from the `spmp8000emu-core` library
// crate and only adds the platform layer: window management, command-line
// argument parsing, and keyboard/gamepad input.

use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use minifb::{Key, Window, WindowOptions};
use spmp8000emu_core::config::CoreConfig;
use spmp8000emu_core::emulator::Emulator;

mod audio_output;
mod standalone;

use audio_output::AudioOutput;
use standalone::cli::Cli;
use standalone::gamepad::GamepadMapper;
use standalone::gamepad_overlay::GamepadOverlay;
use standalone::input::KeyboardMapper;
use standalone::scaler::{rgba_to_xrgb, DisplayScaler};

#[cfg(target_os = "windows")]
mod screen {
    extern "system" {
        fn GetSystemMetrics(index: i32) -> i32;
    }

    const SM_CXSCREEN: i32 = 0;
    const SM_CYSCREEN: i32 = 1;

    pub fn get_screen_size() -> (usize, usize) {
        unsafe {
            (
                GetSystemMetrics(SM_CXSCREEN) as usize,
                GetSystemMetrics(SM_CYSCREEN) as usize,
            )
        }
    }
}

#[cfg(target_os = "linux")]
mod screen {
    type Display = *mut core::ffi::c_void;

    #[link(name = "X11")]
    extern "system" {
        fn XOpenDisplay(display_name: *const u8) -> Display;
        fn XCloseDisplay(display: Display) -> i32;
        fn XDisplayWidth(display: Display, screen_number: i32) -> i32;
        fn XDisplayHeight(display: Display, screen_number: i32) -> i32;
    }

    pub fn get_screen_size() -> (usize, usize) {
        unsafe {
            let display = XOpenDisplay(std::ptr::null());
            if display.is_null() {
                return (800, 600);
            }
            let width = XDisplayWidth(display, 0) as usize;
            let height = XDisplayHeight(display, 0) as usize;
            let _ = XCloseDisplay(display);
            (width, height)
        }
    }
}

#[cfg(target_os = "macos")]
mod screen {
    type CGDirectDisplayId = u32;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGMainDisplayID() -> CGDirectDisplayId;
        fn CGDisplayPixelsWide(display: CGDirectDisplayId) -> usize;
        fn CGDisplayPixelsHigh(display: CGDirectDisplayId) -> usize;
    }

    pub fn get_screen_size() -> (usize, usize) {
        unsafe {
            let display = CGMainDisplayID();
            (CGDisplayPixelsWide(display), CGDisplayPixelsHigh(display))
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse_args();

    // Initialize logging
    let default_log_filter = if cli.debug_logging { "debug" } else { "info" };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(default_log_filter))
        .format_timestamp_millis()
        .init();

    // Validate game path
    if !cli.game_path.exists() {
        eprintln!("Error: Game file not found: {}", cli.game_path.display());
        std::process::exit(1);
    }

    log::info!("Loading game: {}", cli.game_path.display());

    // Create the emulator
    let config = CoreConfig {
        volume: cli.volume,
        swap_o_x: cli.swap_o_x,
        debug_logging: cli.debug_logging,
        unknown_instruction_policy: cli.unknown_instruction_policy.into(),
    };
    let mut emu = Emulator::from_path_with_config(cli.game_path.clone(), config)
        .context("Failed to create emulator")?;
    for cheat in &cli.cheats {
        if let Err(error) = emu.add_cheat(cheat) {
            log::warn!("Ignoring invalid cheat '{}': {}", cheat, error);
        }
    }

    let (width, height) = emu.get_resolution();
    let display_width = width * cli.scale;
    let display_height = height * cli.scale;

    log::info!(
        "Resolution: {}x{} (display: {}x{})",
        width,
        height,
        display_width,
        display_height
    );

    if cli.headless || cli.screenshot.is_some() {
        emu.start();
        let frames = cli
            .screenshot
            .as_ref()
            .map_or(cli.frames, |_| cli.screenshot_frames);
        for frame in 0..frames {
            emu.tick();
            if !emu.is_running() && !emu.should_exit() {
                anyhow::bail!("Emulation stopped before frame {}", frame + 1);
            }
        }
        if let Some(path) = &cli.screenshot {
            emu.renderer
                .save_screenshot(path)
                .context("Failed to save screenshot")?;
            log::info!("Screenshot saved to: {}", path.display());
        } else {
            log::info!("Headless run completed: {} frames", frames);
        }
        return Ok(());
    }

    let (window_width, window_height) = if cli.fullscreen {
        screen::get_screen_size()
    } else {
        (display_width as usize, display_height as usize)
    };

    // Create window
    let mut window = Window::new(
        &format!("SPMP8000 Emulator - {}", cli.game_path.display()),
        window_width,
        window_height,
        WindowOptions {
            resize: !cli.fullscreen,
            borderless: cli.fullscreen,
            scale_mode: minifb::ScaleMode::Stretch,
            ..Default::default()
        },
    )
    .context("Failed to create window")?;

    if cli.fullscreen {
        window.topmost(true);
        window.set_position(0, 0);
    }

    // Limit to ~30fps
    window.set_target_fps(30);

    // Start emulation
    emu.start();

    let audio_output = match AudioOutput::new(emu.get_audio_sample_rate() as u32) {
        Ok(output) => Some(output),
        Err(error) => {
            log::warn!("Audio output is unavailable: {}", error);
            None
        }
    };

    // Main loop
    let frame_duration = Duration::from_secs_f64(1.0 / 30.0);
    let mut source_buffer = Vec::with_capacity((width * height) as usize);
    let mut display_scaler = DisplayScaler::new(cli.filter);
    let keyboard = KeyboardMapper::new(&cli.remappings);
    let mut gamepad = GamepadMapper::new(!cli.no_gamepad);

    while window.is_open() && !window.is_key_down(Key::Escape) && !emu.should_exit() {
        let start = Instant::now();

        let buttons = keyboard.pressed_buttons(&window) | gamepad.pressed_buttons();
        emu.set_buttons(buttons);

        // Execute one frame
        emu.tick();
        if let Some(output) = &audio_output {
            output.submit(emu.get_audio_samples());
        }

        // Update window with framebuffer
        rgba_to_xrgb(emu.get_framebuffer(), &mut source_buffer);
        let (frame_width, frame_height) = emu.get_resolution();
        if cli.show_gamepad {
            GamepadOverlay::draw(
                &mut source_buffer,
                frame_width,
                frame_height,
                emu.input.get_buttons(),
            );
        }
        let (window_width, window_height) = window.get_size();
        let buffer = display_scaler.render(
            &source_buffer,
            frame_width,
            frame_height,
            window_width,
            window_height,
        );

        window
            .update_with_buffer(buffer, window_width.max(1), window_height.max(1))
            .context("Failed to update window")?;

        // Frame rate control
        let elapsed = start.elapsed();
        if elapsed < frame_duration {
            std::thread::sleep(frame_duration - elapsed);
        }
    }

    log::info!("Emulator shutdown");
    Ok(())
}
