// Command-line interface for the standalone emulator.

use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use spmp8000emu_core::config::UnknownInstructionPolicy;

use super::input::RemapSpec;
use super::scaler::ScaleFilter;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum UnknownInstructionMode {
    Stop,
    Skip,
}

impl From<UnknownInstructionMode> for UnknownInstructionPolicy {
    fn from(value: UnknownInstructionMode) -> Self {
        match value {
            UnknownInstructionMode::Stop => Self::Stop,
            UnknownInstructionMode::Skip => Self::Skip,
        }
    }
}

/// SPMP8000 Game Emulator
#[derive(Parser)]
#[command(name = "spmp8000-emu")]
#[command(about = "A SPMP8000 game emulator written in Rust")]
#[command(version)]
pub struct Cli {
    /// Path to the game BIN file
    pub game_path: PathBuf,

    /// Window scale factor (1-8)
    #[arg(short, long, default_value = "1", value_parser = clap::value_parser!(u32).range(1..=8))]
    pub scale: u32,

    /// Fullscreen mode
    #[arg(short, long)]
    pub fullscreen: bool,

    /// Pixel scaling filter for display output
    #[arg(long, value_enum, default_value_t = ScaleFilter::Nearest)]
    pub filter: ScaleFilter,

    /// Audio volume (0-100)
    #[arg(short, long, default_value = "100", value_parser = clap::value_parser!(u32).range(0..=100))]
    pub volume: u32,

    /// Swap the emulated O and X buttons
    #[arg(long = "swap-ox")]
    pub swap_o_x: bool,

    /// Disable physical gamepad input (keyboard remains available)
    #[arg(long = "no-gamepad")]
    pub no_gamepad: bool,

    /// Remap a logical button in BUTTON:KEY format
    #[arg(long = "remap", value_name = "BUTTON:KEY")]
    pub remappings: Vec<RemapSpec>,

    /// Show the logical button state over the game frame
    #[arg(long)]
    pub show_gamepad: bool,

    /// Freeze a memory address or ARM register using a cheat rule
    #[arg(long = "cheat", value_name = "RULE")]
    pub cheats: Vec<String>,

    /// Enable CPU and HLE debug logging
    #[arg(long)]
    pub debug_logging: bool,

    /// Behavior when an unknown ARM instruction is encountered
    #[arg(long, value_enum, default_value_t = UnknownInstructionMode::Stop)]
    pub unknown_instruction_policy: UnknownInstructionMode,

    /// Run without opening a window
    #[arg(long)]
    pub headless: bool,

    /// Number of frames to run in headless mode
    #[arg(long, default_value = "60")]
    pub frames: u32,

    /// Take a screenshot after N frames and exit (saves as PNG)
    #[arg(short = 'S', long = "screenshot", value_name = "PATH")]
    pub screenshot: Option<PathBuf>,

    /// Number of frames to run before taking screenshot
    #[arg(long = "screenshot-frames", default_value = "30")]
    pub screenshot_frames: u32,
}

impl Cli {
    pub fn parse_args() -> Self {
        Self::parse()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spmp8000emu_core::config::CoreConfig;

    #[test]
    fn defaults_match_libretro_core_configuration() {
        let cli = Cli::try_parse_from(["spmp8000-emu", "game.bin"]).unwrap();
        assert_eq!(cli.volume, CoreConfig::default().volume);
        assert!(!cli.swap_o_x);
        assert!(!cli.no_gamepad);
        assert!(!cli.show_gamepad);
        assert!(cli.remappings.is_empty());
        assert!(cli.cheats.is_empty());
        assert!(!cli.debug_logging);
        assert_eq!(cli.unknown_instruction_policy, UnknownInstructionMode::Stop);
        assert_eq!(cli.filter, ScaleFilter::Nearest);
    }

    #[test]
    fn parses_core_configuration_and_standalone_input_options() {
        let cli = Cli::try_parse_from([
            "spmp8000-emu",
            "--volume",
            "35",
            "--swap-ox",
            "--no-gamepad",
            "--remap",
            "o:space",
            "--remap",
            "select:tab",
            "--show-gamepad",
            "--cheat",
            "mem8:0x1234=99",
            "--cheat",
            "reg:r0=0x1234",
            "--debug-logging",
            "--unknown-instruction-policy",
            "skip",
            "game.bin",
        ])
        .unwrap();

        assert_eq!(cli.volume, 35);
        assert!(cli.swap_o_x);
        assert_eq!(cli.remappings.len(), 2);
        assert!(cli.no_gamepad);
        assert!(cli.show_gamepad);
        assert_eq!(
            cli.cheats,
            ["mem8:0x1234=99".to_string(), "reg:r0=0x1234".to_string()]
        );
        assert!(cli.debug_logging);
        assert_eq!(cli.unknown_instruction_policy, UnknownInstructionMode::Skip);
    }

    #[test]
    fn no_gamepad_flag_is_parsed() {
        let defaults = Cli::try_parse_from(["spmp8000-emu", "game.bin"]).unwrap();
        assert!(!defaults.no_gamepad);
        assert!(
            Cli::try_parse_from(["spmp8000-emu", "--no-gamepad", "game.bin"])
                .unwrap()
                .no_gamepad
        );
    }

    #[test]
    fn parses_every_display_filter() {
        for (name, expected) in [
            ("nearest", ScaleFilter::Nearest),
            ("bilinear", ScaleFilter::Bilinear),
            ("bicubic", ScaleFilter::Bicubic),
            ("xbrz", ScaleFilter::Xbrz),
        ] {
            let cli = Cli::try_parse_from(["spmp8000-emu", "--filter", name, "game.bin"]).unwrap();
            assert_eq!(cli.filter, expected);
        }
    }

    #[test]
    fn rejects_escape_remapping() {
        assert!(
            Cli::try_parse_from(["spmp8000-emu", "--remap", "select:escape", "game.bin"]).is_err()
        );
    }
}
