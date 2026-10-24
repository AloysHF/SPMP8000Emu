# EmulatorJS Core (Browser)

SPMP8000Emu can run in the browser as a standard
[EmulatorJS](https://emulatorjs.org) core. The build follows EmulatorJS's own
pipeline: the Rust libretro core is compiled for `wasm32-unknown-emscripten`,
linked against the `EmulatorJS/RetroArch` fork, and packaged as the
`<core>-wasm.data` bundles that an EmulatorJS page loads at runtime.

## What you get

Every nightly and tagged release publishes a single asset:

| File | Contents |
|------|----------|
| `spmp8000-emu-emulatorjs.zip` | Both core variants, the build report `cores/reports/spmp8000emu.json`, `core.json`, `build.json`, `license.txt` and a README |

The two `.data` files stay separate inside the ZIP: EmulatorJS requests
`<core>-wasm.data` or `<core>-legacy-wasm.data` by name depending on WebGL2
support, so one bundle cannot serve both cases.

The bundle is laid out like this:

```
spmp8000-emu-emulatorjs.zip
├── cores/
│   ├── spmp8000emu-wasm.data
│   ├── spmp8000emu-legacy-wasm.data
│   └── reports/spmp8000emu.json
├── core.json
├── build.json
├── license.txt
└── README.md
```

Each `.data` file is a 7z archive containing `<core>_libretro.js`,
`<core>_libretro.wasm`, `core.json`, `build.json` and `license.txt`.

## Install into a site

EmulatorJS always fetches cores from `<EJS_pathtodata>cores/`, so host the
runtime yourself (or use the pinned CDN files) and drop the core beside it:

```
data/
├── loader.js
├── emulator.min.js
├── emulator.min.css
├── compression/
│   ├── extract7z.js       # extracts the core .data archive
│   └── extractzip.js      # extracts ZIP content, if you feed the core one
├── localization/          # optional, e.g. zh-CN.json
└── cores/
    ├── spmp8000emu-wasm.data
    ├── spmp8000emu-legacy-wasm.data
    └── reports/spmp8000emu.json
```

`emulator.min.zip` only contains `emulator.min.js` and `emulator.min.css`;
the `compression/` and `localization/` files are loaded on demand from the
same EmulatorJS data directory. Without `extract7z.js` the core download ends
in a "Network Error".

To upgrade the runtime, replace the runtime files with a newer EmulatorJS
release (≥ 4.2.3) and keep this `cores/` directory.

## Use

```html
<div id="game"></div>
<script>
  EJS_player = "#game";
  EJS_core = "spmp8000emu";
  EJS_gameUrl = "games/your-game.bin";
  EJS_pathtodata = "data/";
  EJS_startOnLoaded = false;
</script>
<script src="data/loader.js"></script>
```

`site/play.html` in this repository is a complete example: visitors pick a
game file from their own machine (or drop it on the page) and it runs from a
blob URL, so nothing is uploaded and the site does not need to host any game
data.

When the game comes from a blob URL, EmulatorJS writes it into the wasm
filesystem under `EJS_gameName`. Set that to the original file name, extension
included (for example `game.bin`), otherwise the frontend cannot infer the
content type and the core receives an unknown path.

### Content

NGame1.0 games are self-contained `.bin` files, so the browser player accepts
a single `.bin` — no ZIP packaging is required. The core loads it through
`retro_load_game` with `need_fullpath = true`, which is why the file must be
written into the wasm filesystem rather than passed as a memory buffer. See
[Game-File-Formats.md](Game-File-Formats.md) for the container layout.

Game data is copyrighted and is neither distributed with this repository nor
required by the site: the play page only loads files the visitor selects.

## Saves

The core does not expose battery RAM through `retro_get_memory_data`, so
EmulatorJS's battery-save buttons are turned off by `"save": false` in
`core.json`. Save states are unaffected: they use `retro_serialize` and are
available from the EmulatorJS menu. They live in the browser's IndexedDB-backed
`/data/saves` mount, so they survive page reloads in the same browser profile.

## Core options

The core registers the same options as the desktop build through the legacy
`SET_VARIABLES` interface, which RetroArch/EmulatorJS upgrades to its normal
options menu:

| Option | Default | Effect |
|--------|---------|--------|
| Audio Volume (%) | 100 | Output volume |
| Swap O/X Buttons | disabled | Swap the two face buttons |
| CPU/HLE Debug Logging | disabled | Log CPU and HLE calls to the console |
| Unknown ARM Instruction Policy | stop | Halt or skip unknown instructions |

Values are re-read when the frontend signals a change
(`GET_VARIABLE_UPDATE`), so changes apply without reloading the game.

## Building locally

Requirements:

- an activated [emsdk](https://emscripten.org) 3.1.74 (`emmake` in `PATH`)
- the Rust nightly toolchain with the `rust-src` component
- the Rust target `wasm32-unknown-emscripten`
- `7z` (p7zip), `zip`, `git`

```bash
rustup toolchain install nightly --component rust-src
rustup +nightly target add wasm32-unknown-emscripten
source /path/to/emsdk/emsdk_env.sh          # Linux/macOS/WSL

cargo +nightly build -p spmp8000emu-libretro \
  --target wasm32-unknown-emscripten --profile wasm-release \
  -Zbuild-std=std,panic_abort

scripts/build-emulatorjs-core.sh
```

`-Zbuild-std=std,panic_abort` rebuilds std with `panic=abort`. The prebuilt
emscripten stdlib contains wasm exception-handling instructions, and the
Asyncify pass that EmulatorJS's build forces on (for OpenAL audio) crashes on
them; the abort build emits none.

The build script:

1. fetches the pinned `EmulatorJS/RetroArch` commit
   (`86f72a39736b1fa351ca296d16f568e99e11a34f`) into `target/emulatorjs-build/`,
2. copies `target/wasm32-unknown-emscripten/wasm-release/libspmp8000emu.a` in as
   `spmp8000emu_libretro_emscripten.bc`,
3. runs the fork's `build-emulatorjs.sh` twice (WebGL2 and `--legacy`),
4. writes `core.json`, `build.json`, `license.txt`, the core report and a
   README, and packages everything into `out/spmp8000-emu-emulatorjs.zip`.

Useful flags:

```bash
scripts/build-emulatorjs-core.sh \
  --core path/to/libspmp8000emu.a \
  --out out/emulatorjs \
  --work-dir target/emulatorjs-build \
  --retroarch-ref <commit>
```

The wasm build needs a POSIX shell; on Windows run it inside WSL or an MSYS2
environment with emsdk activated, or rely on CI.

## CI integration

The `Nightly Build` and `Release` workflows run a `build-emulatorjs` job and
attach the artifact to the rolling `nightly-build` / tagged releases. The Pages
workflow then:

1. downloads the pinned EmulatorJS 4.2.3 runtime into `site/data/`,
2. fetches `spmp8000-emu-emulatorjs.zip` from `nightly-build` and copies the
   `.data` files and report into `site/data/cores/`.

No game data is deployed: `site/play.html` asks each visitor for a local `.bin`.
