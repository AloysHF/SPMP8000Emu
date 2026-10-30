# RetroArch Core

This guide covers installing and running the SPMP8000Emu libretro core for
RetroArch, loading content, supported features, and controls.

## Supported Platforms

| Platform | Architecture | Standalone | Libretro |
|----------|-------------|------------|----------|
| Windows | x86_64 | ✅ | ✅ |
| macOS | x86_64, aarch64 | ✅ | ✅ |
| Linux | x86_64, aarch64 | ✅ | ✅ |
| Android | arm64-v8a, armeabi-v7a, x86, x86_64 | — | ✅ |
| iOS | arm64, x86_64, arm64-simulator | — | ✅ |
| webOS | armv7 | — | ✅ |

> Android, iOS, and webOS are supported through the libretro core only (use with
> RetroArch).

## Installation

### Manual Installation

Build the libretro core:

```bash
cargo build -p spmp8000emu-libretro --release
```

Cargo names the cdylib after its lib target, so this produces
`spmp8000emu.dll` on Windows (`libspmp8000emu.so` on Linux,
`libspmp8000emu.dylib` on macOS) under `target/release/`.

RetroArch expects the core file to be named `spmp8000emu_libretro.<ext>`, so
rename it accordingly before placing it into RetroArch's `cores/` directory.
Copy `spmp8000emu_libretro.info` into RetroArch's `info/` directory so the
frontend can display the core metadata and supported features.

## Loading Games

1. Open RetroArch and select **Load Core > SPMP8000 (SPMP8000Emu)**.
2. Select **Load Content**.
3. Choose a `.bin` file in NGame1.0 format.

## Supported Features

- Video output using the XRGB8888 pixel format
- Stereo audio output with WAVE effects and synthesized MIDI music
- RetroPad input handling
- Full runtime reset from the cached game boot image
- Versioned and checksummed save states for the complete emulator runtime
- Live core options for audio, button layout, and CPU diagnostics
- System RAM, video RAM, and complete SPMP memory descriptors
- Emulator-handled memory and ARM-register cheats
- `.bin` content loading (NGame1.0 format)

## Memory access

The core exposes the standard libretro memory IDs:

| Memory ID | Region | Address range | Size |
|---|---|---|---:|
| `RETRO_MEMORY_SYSTEM_RAM` (`2`) | Main RAM | `0x00000000`–`0x00FFFFFF` | 16 MiB |
| `RETRO_MEMORY_VIDEO_RAM` (`3`) | Video RAM | `0x01000000`–`0x01FFFFFF` | 16 MiB |

It also registers the RAM, VRAM, and peripheral regions as `SPMP` memory
descriptors using their absolute emulated addresses. This lets compatible
RetroArch tools inspect and modify the real SPMP address space. The underlying
buffers keep stable host addresses across Reset and Load State.

## Cheats

SPMP8000Emu uses RetroArch's **Emulator** cheat handler. Enter the same rules
accepted by the standalone `--cheat` option:

```text
mem8:0x00123456=99
mem16:0x00124000=999
mem32:0x01001000=0x12345678
reg:r0=0x1234
```

To configure a rule:

1. Load a game and open **Quick Menu > Cheats**.
2. Add or select a cheat entry.
3. Set **Handler** to **Emulator**.
4. Enter one rule in **Code** and enable the entry.
5. Select **Apply Changes**.

Each RetroArch cheat index is an independent slot. Applying an empty code
removes that slot, disabling a slot retains its parsed rule without applying
it, and **Reset Cheats** clears all slots. Memory values accept decimal or
`0x`-prefixed hexadecimal numbers. `mem16` and `mem32` require natural
alignment, values must fit their width, and only the mapped 16 MiB RAM and
16 MiB VRAM ranges are accepted. Register targets are `r0`–`r15`, `sp`, `lr`,
`pc`, and `cpsr`.

Memory targets are write-protected: the value is applied as soon as the slot is
enabled, and later writes that overlap the range are reverted immediately, so
the game cannot change it between frames. Register targets run once per frame
after CPU execution and before video/audio output.

Cheat slots are frontend configuration, not part of a Save State. Reset and
Load State retain the current slots and re-apply enabled memory freezes as soon
as the state is restored. `SPMP8000_CHEAT_TRACE=1` logs rules whose value
changed anyway, which means a writer bypassed the normal CPU write path. See
the standalone [cheat diagnostics](Standalone-Emulator.md#diagnosing-freezes)
for how to read the log.

## Live Core Options

RetroArch exposes these settings under **Quick Menu > Core Options** while
content is running:

| Option | Default | Effect |
|---|---|---|
| Audio Volume (%) | `100` | Changes the emulator mixer volume from `0` (mute) to `100`. |
| Swap O/X Buttons | disabled | Exchanges the emulated O and X button signals. |
| CPU/HLE Debug Logging | disabled | Enables sampled CPU debug records and HLE debug records. |
| Unknown ARM Instruction Policy | `stop` | `stop` halts on an unsupported instruction; `skip` continues past unknown ARM instructions for diagnostics. |

The core checks for option changes every frame and applies them to the running
emulator without resetting CPU, memory, graphics, or audio state. Reset and
Load State both retain the currently selected frontend options. `skip` remains
conservative in Thumb mode, which is not implemented, and stops instead of
pretending the instruction was handled.

Input repeat timing is intentionally not exposed yet because the corresponding
SPMP8000 hardware behavior has not been wired and validated.

## Save states

The core exposes a fixed 128 MiB serialization capacity to libretro. State payloads use a
versioned binary format with LZ4 compression, content identity checking, payload length
validation, and a CRC32 checksum. A state includes the CPU, all mapped memory, HLE API,
renderer, active audio playback, input, and runtime flags.

States can only be restored while the same game content is loaded. Invalid, corrupted,
incompatible, or cross-game states are rejected before the active emulator state is changed.
RetroArch may compress the fixed-size state file according to its frontend configuration.
Loading a state preserves the current live core options rather than restoring old frontend
settings from the state payload.

## RetroPad Button Mapping

| RetroPad Button | Action |
|---|---|
| D-Pad Up | Up |
| D-Pad Down | Down |
| D-Pad Left | Left |
| D-Pad Right | Right |
| A (SNES East) | O button (A) |
| B (SNES South) | X button (B) |
| Start | START |
| Select | SELECT |
