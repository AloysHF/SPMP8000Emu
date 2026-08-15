# SPMP8000 Emulator — A SPMP8000 game emulator written in Rust

<p align="center">
  <img src="res/logo-banner.png" alt="SPMP8000 Emulator" width="600">
</p>

<p align="center">
  <a href="https://jiangxincode.github.io/SPMP8000Emu/"><img src="https://img.shields.io/badge/Website-SPMP8000Emu-E8553A?logo=githubpages&logoColor=white" alt="Website"></a>
  <a href="https://github.com/jiangxincode/SPMP8000Emu/actions/workflows/ci.yml"><img src="https://github.com/jiangxincode/SPMP8000Emu/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://git.libretro.com/libretro/spmp8000emu/-/pipelines"><img src="https://img.shields.io/gitlab/pipeline-status/spmp8000emu?gitlab_url=https%3A%2F%2Fgit.libretro.com%2Flibretro&branch=master&logo=gitlab&label=Pipeline%20Status" alt="Gitlab Pipeline Status" ></a>
  <a href="https://github.com/jiangxincode/SPMP8000Emu/releases/latest"><img src="https://img.shields.io/github/v/release/jiangxincode/SPMP8000Emu" alt="Release"></a>
  <a href="https://github.com/jiangxincode/SPMP8000Emu/releases"><img src="https://img.shields.io/github/downloads/jiangxincode/SPMP8000Emu/total" alt="Downloads"></a>
  <a href="https://sonarcloud.io/dashboard?id=jiangxincode_SPMP8000Emu"><img src="https://sonarcloud.io/api/project_badges/measure?project=jiangxincode_SPMP8000Emu&metric=alert_status" alt="Quality Gate Status"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-BSD%203--Clause-blue.svg" alt="License: BSD 3-Clause"></a>
  <a href="https://discord.gg/7XDdSrYD"><img src="https://img.shields.io/badge/Discord-Join%20Us-5865F2?logo=discord&logoColor=white" alt="Discord"></a>
  <a href="https://qm.qq.com/q/LAO7DKAWUC"><img src="https://img.shields.io/badge/QQ%E7%BE%A4-Join%20Us-12B7F5?logo=tencent-qq&logoColor=white" alt="QQ Group"></a>
</p>

A SPMP8000 game emulator written in Rust, supporting both standalone mode and
RetroArch integration.

SPMP8000 is a Sunplus multimedia SoC commonly found in portable gaming devices
(circa 2005–2011). Games use `.bin` files in the NGame1.0 format with an
ARM-based CPU and HLE system API.

## Features

- **NGame1.0 format support** — file loading, header parsing, DES decryption, LZ77/RLE decompression
- **ARM CPU emulation** — ARM mode instruction set (data processing, load/store, block transfer, branch, multiply, SVC)
- **HLE system API** — emuIf, NativeGE, and eCos interfaces with instruction-driven timing
- **Graphics rendering** — presentation-synchronized RGB565 and indexed-palette surfaces, sprite color-key transparency, 8 transformation modes, 320×240 display
- **Audio emulation** — WAV decoding and sample-based MIDI synthesis using the original SPMP8000 tone bank, mixed to 22050 Hz stereo output
- **Input handling** — keyboard input with configurable mappings
- **RetroArch integration** — libretro core for RetroArch frontend
- **True reset** — rebuilds CPU, memory, HLE, graphics, audio, and input runtime state
- **Save states** — versioned, checksummed snapshots of the complete emulator runtime
- **Live core options** — adjust volume, O/X layout, and diagnostics without resetting the game
- **Memory and cheats** — inspect SPMP RAM/VRAM and freeze validated memory addresses or ARM registers
- **Standalone mode** — minifb window with CLI
- **Cross-platform** — Windows, macOS, Linux, Android, iOS, webOS
- **Headless mode** — run without a window for testing and batch processing
- **Screenshot capture** — automated PNG screenshot generation

## Usage

### Standalone Mode

Download the latest binary from the
[Releases](https://github.com/jiangxincode/SPMP8000Emu/releases) page and run:

```bash
spmp8000-emu path/to/game.bin
```

See the [Standalone Emulator](docs/Standalone-Emulator.md) guide for
installation, keyboard controls, headless mode, screenshots, and all
command-line options.

### RetroArch Mode

Install the core and load a game through RetroArch's **Load Content** menu.

See the [RetroArch Core](docs/RetroArch-Core.md) guide for installation,
supported platforms, RetroPad mapping, and features.

## Building

Requires [Rust](https://www.rust-lang.org/tools/install) (stable).

### Standalone Mode

```bash
cargo build -p spmp8000emu --release
cargo run -p spmp8000emu --release -- path/to/game.bin
```

### Libretro Core (for RetroArch)

```bash
cargo build -p spmp8000emu-libretro --release
```

The binary is produced at `target/release/spmp8000emu.dll`
(`libspmp8000emu.so` on Linux, `libspmp8000emu.dylib` on macOS). Rename it to
`spmp8000emu_libretro.<ext>` before placing it in RetroArch's `cores/`
directory.

For Android cross-compilation, see [Android Libretro Core](docs/Android-Libretro-Core.md).
For iOS, see [iOS Libretro Core](docs/iOS-Libretro-Core.md).

## Architecture

```
crates/
├── spmp8000emu-core/         # Platform-independent emulator engine (library)
│   ├── assets/
│   │   └── tonelib.bin      # Embedded SPMP8000 PCM tone bank
│   └── src/
│       ├── lib.rs            # Crate root
│       ├── emulator.rs       # Core emulator tying all components together
│       ├── arm_cpu.rs        # ARM CPU emulation
│       ├── memory.rs         # Memory map (RAM, VRAM, peripherals)
│       ├── bin_loader.rs     # NGame1.0 BIN file parser
│       ├── decompressor.rs   # DES decryption + LZ77/RLE decompression
│       ├── renderer.rs       # RGB565 → XRGB8888 framebuffer conversion
│       ├── audio_engine.rs   # Audio source mixing and frame output
│       ├── audio_resource.rs # WAV decoding and MIDI synthesis
│       ├── tone_library.rs   # Tone-bank patch and PCM sample parser
│       ├── input_handler.rs  # Button state management
│       ├── function_table.rs # HLE function trampolines
│       ├── save_state.rs     # Save-state data structures
│       ├── cheats.rs         # Memory and register cheat support
│       ├── config.rs         # Runtime emulator configuration
│       └── api/              # HLE system API (emuIf, NativeGE, eCos)
│           ├── mod.rs        # SVC dispatch and API state
│           ├── emu_graph.rs  # Graphics API (MCatch, emuIf graph)
│           ├── emu_sound.rs  # Sound API (emuIf sound)
│           ├── emu_key.rs    # Input API (emuIf + NativeGE keys)
│           ├── emu_fs.rs     # Filesystem API (emuIf + NativeGE fs)
│           └── native_ge.rs  # NativeGE resource/system API
├── spmp8000emu/              # Standalone binary (→ spmp8000-emu)
│   └── src/
│       ├── main.rs           # Window loop, CLI, keyboard input
│       └── audio_output.rs   # cpal-based audio output with resampling
└── spmp8000emu-libretro/     # libretro cdylib (→ spmp8000emu_libretro.{dll,so,dylib})
    └── src/
        ├── lib.rs            # cdylib crate root
        └── libretro/         # libretro C API implementation
```

## Key Mappings (Standalone)

| Key | Button |
|-----|--------|
| Arrow Up/Down/Left/Right | D-pad |
| Z | O (A / Cross) |
| X | X (B / Circle) |
| Enter | START |
| Backspace | SELECT |
| Escape | Exit |

## Known SPMP8XXX Devices

Handheld gaming devices based on SunPlus SPMP8XXX chips that can run NGame1.0 games:

| Chip | Manufacturer | Device | Image | Region | Links |
|------|--------------|--------|-------|--------|-------|
| SPMP8010A | 金星 (JXD) | JXD1000 | <img src="docs/devices/JXD1000.jpg" width="120"> | China | [Official](https://jxd.hk/products.asp?id=554&selectclassid=009002001) · [Wiki](https://jxd.fandom.com/wiki/Jxd1000) · [Video](https://www.bilibili.com/video/BV1kR57zzEGQ) |
| SPMP8010A | 金星 (JXD) | JXD2000 | <img src="docs/devices/JXD2000.jpg" width="120"> | China | [Official](https://jxd.hk/products.asp?id=555&selectclassid=009002001) |
| SPMP8000? | 金星 (JXD) | JXD980 | <img src="docs/devices/JXD980.jpg" width="120"> | China | [Baidu](https://baike.baidu.com/item/%E9%87%91%E6%98%9FJXD980/8365407) |
| SPMP8000? | 金星 (JXD) | JXD300 | <img src="docs/devices/JXD300.jpg" width="120"> | China | [Baidu](https://baike.baidu.com/item/%E9%87%91%E6%98%9FJXD300/312367) · [Video](https://www.youtube.com/watch?v=-J2uHjPQ2VQ) |
| SPMP8000? | 金星 (JXD) | JXD206 | <img src="docs/devices/JXD206.jpg" width="120"> | China | [Blog](https://surajbkmshah.wordpress.com/2010/05/05/frm-pro-v3-3-for-pmp8000-flashfile-procedure-here/) |
| SPMP8000 | Letcool | N350JP | <img src="docs/devices/N350JP.jpg" width="120"> | China | [Handhelds Arena](https://handheldsarena.com/devices/letcool/n350jp/) |
| SPMP8010 | Ritmix | RZX-40 | <img src="docs/devices/RZX-40.jpg" width="120"> | Russia | [Official](http://old.ritmixrussia.ru/products/rzx-40) |

> **Note**: Devices marked with "?" have unconfirmed chip models.

## Game Compatibility

The emulator supports games in NGame1.0 format (`.bin` files) for SPMP8000 and
SPCA556 chips. 44 out of 45 tested games load and render successfully.

| Status | Count |
|--------|-------|
| ✅ Title screen rendered | 40 |
| ⚠️ Blank frame (initialization incomplete) | 4 |
| ❌ Crash | 1 |

For the full game list with screenshots, see [Game Compatibility](docs/Game-Compatibility.md).

## Testing

Run the unit tests:

```bash
cargo test --workspace
```

There is also a smoke test that loads every available game, runs it for a number
of frames, and checks that the emulator neither panics nor produces a blank
frame. It needs the (non-distributed) game assets, so it is `#[ignore]`d by
default and only runs on demand:

```bash
# Uses <repo>/tmp/spmp8000_game by default, or set SPMP8000_GAME_DIR
cargo test -p spmp8000emu-core --test screenshot -- --ignored --nocapture
```

To refresh the compatibility screenshots, run:

```powershell
pwsh scripts/batch-screenshots.ps1
```

The script rebuilds the release executable before capturing, uses 300 frames by
default, and captures SmartBlocks at 270 frames for its shorter title-screen
window. Pass `-Frames <count>` to use one explicit frame count for every game,
or `-Binary <path>` to capture with an existing executable without rebuilding
it.

## Contributing

Contributions are welcome! Whether you're interested in fixing bugs, adding
features, improving documentation, or testing game compatibility, we'd love your
help. See [CONTRIBUTING.md](docs/CONTRIBUTING.md) for details.

## License

This project is licensed under the [BSD 3-Clause License](LICENSE).
