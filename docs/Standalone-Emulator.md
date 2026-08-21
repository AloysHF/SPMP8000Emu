# Standalone Emulator

This guide covers installing and running the standalone `spmp8000-emu` binary,
loading games, keyboard controls, display scaling, headless mode, and all
command-line options.

## Supported Platforms

| Platform | Architecture | Status |
|----------|-------------|--------|
| Windows | x86_64 | ✅ |
| macOS | x86_64, aarch64 | ✅ |
| Linux | x86_64, aarch64 | ✅ |

## Installation

Download the latest standalone binary for your platform from the
[Releases](https://github.com/AloysHF/SPMP8000Emu/releases) page.

You can also build it from source:

```bash
cargo build -p spmp8000emu --release
```

The binary is produced at `target/release/spmp8000-emu` (`.exe` on Windows).

## Synopsis

```text
spmp8000-emu [OPTIONS] <GAME_PATH>
```

## Options

| Option | Value | Default | Description |
|---|---|---|---|
| `<GAME_PATH>` | path | *required* | Path to the game file (`.bin`). |
| `-s, --scale <N>` | `1`–`8` | `1` | Integer scaling factor for the window. |
| `-f, --fullscreen` | flag | off | Run in fullscreen mode. |
| `--filter <FILTER>` | `nearest`, `bilinear`, `bicubic`, `xbrz` | `nearest` | Select the display scaling filter. |
| `-v, --volume <N>` | `0`–`100` | `100` | Volume level (`0` = mute, `100` = original). |
| `--swap-ox` | flag | off | Exchange the emulated O and X button signals. |
| `--remap <BUTTON:KEY>` | repeatable mapping | — | Replace a standalone keyboard mapping. |
| `--show-gamepad` | flag | off | Draw the effective logical button state over the displayed frame. |
| `--cheat <RULE>` | repeatable rule | — | Freeze a RAM/VRAM value or ARM register once per frame. |
| `--debug-logging` | flag | off | Enable sampled CPU debug records and HLE debug records. |
| `--unknown-instruction-policy <MODE>` | `stop`, `skip` | `stop` | Stop on unknown ARM instructions or skip them for diagnostics. |
| `--headless` | flag | off | Run without opening a window (for testing/batch processing). |
| `--frames <N>` | integer | `60` | Number of frames to run in headless mode. |
| `-S, --screenshot <PATH>` | path | — | Run N frames headlessly, save a PNG screenshot, then exit. |
| `--screenshot-frames <N>` | integer | `30` | Number of frames to run before the screenshot is taken. |

`--screenshot-frames` only has an effect together with `--screenshot`.

The core-configuration defaults match the RetroArch core defaults. The `skip`
unknown-instruction policy is diagnostic-only; unsupported Thumb mode still
stops rather than advancing with undefined behavior.

## Display Scaling

The default `nearest` filter preserves hard pixel edges. `bilinear` smooths the
image, `bicubic` provides sharper interpolation, and `xbrz` smooths pixel-art
diagonals while retaining hard edges. For example:

```bash
spmp8000-emu --scale 4 --filter xbrz path/to/game.bin
```

The window opens at the game's native resolution (`--scale 1`) by default; use
`--scale <N>` to open it enlarged.

The window can be resized at runtime. All filters preserve the native aspect
ratio and center the image with black bars when the window or fullscreen
display has a different ratio. The `--scale` value selects only the initial
window size and is ignored for the fullscreen dimensions.

## Audio Output

The standalone emulator sends WAVE sound effects and synthesized MIDI music to
the system's default audio output device. Audio is converted automatically from
the emulator's 22050 Hz stereo stream to the device's native sample rate and
channel count. `--volume` scales the mixed output; use `--volume 0` to mute it.

If no supported output device is available, the emulator logs a warning and
continues without audio. Headless and screenshot modes do not open an audio
device.

## Default Key Mappings

| Physical Key | Action |
|---|---|
| Arrow Up | D-pad Up |
| Arrow Down | D-pad Down |
| Arrow Left | D-pad Left |
| Arrow Right | D-pad Right |
| Z | O button (A / Cross) |
| X | X button (B / Circle) |
| Enter | START |
| Backspace | SELECT |
| Escape | Exit |

All eight logical buttons can be remapped by repeating `--remap`. Valid button
names are `up`, `down`, `left`, `right`, `o`, `x`, `start`, and `select`.
Letter and number keys, F1–F12, arrows, Space, Enter, Backspace, Tab, navigation
keys, Shift, Ctrl, and Alt are supported. For example:

```bash
spmp8000-emu --remap o:space --remap select:tab path/to/game.bin
```

Each remapping replaces that button's default key, and the last mapping wins
when the same button appears more than once. Escape is permanently reserved
for exiting the standalone emulator and cannot be assigned to a game button,
so it never conflicts with SELECT.

Remapping converts physical keys into logical SPMP buttons first.
`--swap-ox` then exchanges the logical O/X signals. In the standalone
frontend, Z defaults to O and X defaults to X; in RetroArch, RetroPad A maps to
O and RetroPad B maps to X. The shared swap option therefore has the same
logical effect in both frontends.

## Virtual Gamepad Overlay

Use `--show-gamepad` to display the held direction, O, X, START, and SELECT
states. The overlay shows the effective logical state after `--swap-ox`, is
drawn at native framebuffer resolution, and then passes through the selected
display filter. It is intended for diagnostics, demonstrations, and visual
input confirmation; it is not an interactive or touch input frontend.

The overlay affects only the standalone presentation buffer. Native-resolution
PNG screenshots created with `--screenshot` do not contain the overlay,
display filter, or letterboxing.

## Cheats

Use repeatable `--cheat <RULE>` options to freeze values in the emulated
address space:

```bash
spmp8000-emu \
  --cheat "mem8:0x00123456=99" \
  --cheat "mem16:0x00124000=999" \
  --cheat "mem32:0x01001000=0x12345678" \
  --cheat "reg:r0=0x1234" \
  path/to/game.bin
```

Supported targets are:

| Target | Value range | Requirements |
|---|---:|---|
| `mem8:<ADDRESS>` | `0`–`255` | Address must be in RAM or VRAM. |
| `mem16:<ADDRESS>` | `0`–`65535` | Address must be in RAM or VRAM and 2-byte aligned. |
| `mem32:<ADDRESS>` | `0`–`4294967295` | Address must be in RAM or VRAM and 4-byte aligned. |
| `reg:<REGISTER>` | 32-bit unsigned | `r0`–`r15`, `sp`, `lr`, `pc`, or `cpsr`. |

Addresses and values accept decimal or `0x`-prefixed hexadecimal notation.
RAM occupies `0x00000000`–`0x00FFFFFF`; VRAM occupies
`0x01000000`–`0x01FFFFFF`. Peripheral addresses are intentionally rejected
for core-managed cheats. Invalid rules are logged and ignored without stopping
the game.

Enabled rules are applied after game logic and before video/audio output on
each running frame. Cheat rules are frontend configuration rather than
emulator state: Reset and Load State retain the currently configured rules,
and a loaded state receives the enabled freezes on the next running frame.

## Loading Games

The standalone emulator accepts `.bin` files in NGame1.0 format:

```bash
# Load a game directly
spmp8000-emu path/to/game.bin

# Load with 3x scaling and 80% volume
spmp8000-emu --scale 3 --volume 80 path/to/game.bin

# Fullscreen mode
spmp8000-emu --fullscreen path/to/game.bin

# Swap O/X and continue past unknown ARM instructions for diagnostics
spmp8000-emu --swap-ox --unknown-instruction-policy skip path/to/game.bin

# Remap O and SELECT, then show the effective logical state
spmp8000-emu --remap o:space --remap select:tab --show-gamepad path/to/game.bin
```

## Headless Mode

Run the emulator without a window — useful for automated testing and batch
processing:

```bash
# Run 120 frames silently
spmp8000-emu --headless --frames 120 path/to/game.bin
```

## Screenshot Mode

Capture a PNG screenshot after a number of frames, then exit:

```bash
# Take a screenshot after 300 frames (10 seconds at 30fps)
spmp8000-emu --screenshot screenshot.png --screenshot-frames 300 path/to/game.bin
```

This is used by the batch screenshot script (`scripts/batch-screenshots.ps1`)
to generate screenshots for all games at once. Screenshots always use the
native framebuffer resolution and do not include the display filter or black
bars. They also do not include the virtual gamepad overlay.

## Examples

```bash
# Basic usage
spmp8000-emu path/to/game.bin

# 4x scaling
spmp8000-emu --scale 4 path/to/game.bin

# xBRZ display filtering
spmp8000-emu --filter xbrz path/to/game.bin

# Remapped controls with the virtual gamepad state overlay
spmp8000-emu --remap o:space --show-gamepad path/to/game.bin

# Fullscreen with 50% volume
spmp8000-emu --fullscreen --volume 50 path/to/game.bin

# Take a screenshot and exit
spmp8000-emu --screenshot shot.png --screenshot-frames 300 path/to/game.bin

# Batch screenshot (PowerShell)
scripts/batch-screenshots.ps1
```
