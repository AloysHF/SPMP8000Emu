# Android Libretro Core

The SPMP8000Emu libretro core runs on Android, so it can be used by most
Android RetroArch-based frontends.

## Install in RetroArch on Android

### Via Online Updater (Recommended)

The easiest way is to download the core directly from RetroArch's built-in Online Updater:

1. Open RetroArch
2. Go to **Main Menu → Online Updater → Core Downloader**
3. Find and select **SPMP8000 (SPMP8000Emu)**, wait for the download to complete
4. Go back to **Main Menu → Load Core** — the SPMP8000 core should appear

To update an installed core:

1. Open RetroArch
2. Go to **Main Menu → Online Updater → Update Installed Cores**

### Manual Installation (Alternative)

If the Online Updater is not available, you can install the core manually:

1. **Download** `spmp8000-emu-android-libretro.tar.gz` from the
   [Releases](https://github.com/AloysHF/SPMP8000Emu/releases) page. It
   contains per-ABI `spmp8000emu_libretro_android.so` files for `arm64-v8a`,
   `armeabi-v7a`, `x86` and `x86_64`.
2. **Install the core**: copy the `spmp8000emu_libretro_android.so` matching
   your device's ABI (most modern devices are `arm64-v8a`) into RetroArch's
   `cores/` directory (typically
   `/storage/emulated/0/RetroArch/cores/` or the app's internal `cores/` path),
   and copy `spmp8000emu_libretro.info` into RetroArch's `info/` directory.
3. **Load** the core and content the same way as on desktop.

## Building the Android core locally

Building for Android requires the [Android NDK](https://developer.android.com/ndk)
and [`cargo-ndk`](https://github.com/bbqsrc/cargo-ndk):

```bash
cargo install cargo-ndk
rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android
export ANDROID_NDK_HOME=/path/to/android-ndk

# Build all four ABIs (artifacts land in target/<triple>/release/)
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86 -t x86_64 --platform 21 \
  build -p spmp8000emu-libretro --release
```

Each ABI produces `libspmp8000emu.so`; rename it to
`spmp8000emu_libretro_android.so` when installing into RetroArch on Android.
The CI release workflow performs this packaging automatically.

## RetroDroid

The same core ships inside [RetroDroid](https://github.com/AloysHF/RetroDroid),
the all-in-one Android app for this emulator family. RetroDroid loads
`libspmp8000emu.so` directly (no RetroArch required) and reads the neighbouring
`android.json` for the system profile: display names, content extensions,
virtual-controller preset, firmware requirements and orientation.

Rules the core follows for any Android host:

- Content is loaded from the path supplied by the frontend (`need_fullpath`);
  in-memory buffers are rejected. RetroDroid copies the SAF selection into an
  app-private cache before loading, so the core never writes beside the content.
- SPMP8000 content needs no firmware and produces no battery save files. Any
  future save data would go to `RETRO_ENVIRONMENT_GET_SAVE_DIRECTORY`, and any
  firmware or user-visible assets to `RETRO_ENVIRONMENT_GET_SYSTEM_DIRECTORY`;
  neither directory is read or written today.
- Load failures are reported with `RETRO_ENVIRONMENT_SET_MESSAGE` and the log
  interface, naming the content file and the path the core expected, never as
  a silent `false`.
- Panics are caught at the C ABI boundary, so a bad frame cannot kill the host.
  The release profile keeps `panic = "unwind"`; only the wasm profile aborts.
- Save states carry an `SP8S` magic header plus a little-endian format version
  (`1`); states from another system or version are rejected instead of being
  misread. The serialized payload keeps the core's existing layout.
- Cores are linked with 16 KB page alignment
  (`-C link-arg=-Wl,-z,max-page-size=16384`) for Android 15 devices.
