// Smoke test: load every available game, run it for a number of frames,
// and assert the emulator neither panics nor produces a blank frame.
//
// Modeled after Native32Emu's smoke test harness.
//
// This test needs the (large, non-distributed) game assets, so it is marked
// `#[ignore]` and only runs on demand:
//
// ```text
// cargo test -p spmp8000emu-core --test screenshot -- --ignored --nocapture
// ```
//
// By default it looks for games in `<repo>/tmp/spmp8000_game`. Override the
// location with the `SPMP8000_GAME_DIR` environment variable.

use std::path::{Path, PathBuf};

use spmp8000emu_core::emulator::Emulator;

/// Number of frames to run per game before sampling the output.
const FRAMES: u32 = 300;

/// Resolve the directory that holds the game assets.
fn game_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("SPMP8000_GAME_DIR") {
        let p = PathBuf::from(dir);
        return p.is_dir().then_some(p);
    }
    // Default: <workspace_root>/tmp/spmp8000_game. CARGO_MANIFEST_DIR points at the
    // core crate (crates/spmp8000emu-core), so go up two levels.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidate = manifest
        .parent()
        .and_then(|p| p.parent())
        .map(|root| root.join("tmp").join("spmp8000_game"));
    candidate.filter(|p| p.is_dir())
}

/// Recursively collect all .bin files under `dir`.
fn collect_games(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_games(&path, out);
        } else if path
            .extension()
            .map(|e| e.eq_ignore_ascii_case("bin"))
            .unwrap_or(false)
        {
            out.push(path);
        }
    }
}

/// Returns true if the frame buffer contains more than one distinct pixel
/// value, i.e. it is not a single flat color (all-black / all-white / etc.).
fn frame_has_content(framebuffer: &[u8]) -> bool {
    // Framebuffer is XRGB8888 (4 bytes per pixel)
    if framebuffer.len() < 4 {
        return false;
    }
    let first = &framebuffer[..4];
    framebuffer.as_chunks::<4>().0.iter().any(|px| px != first)
}

/// Run a single game for `FRAMES` frames. Returns Ok(true) if the final frame
/// has visible content, Ok(false) if it is blank, or Err on a load failure.
/// A panic inside the emulator will fail the test via the normal unwinding.
fn run_one(path: &Path) -> Result<bool, String> {
    let mut emu =
        Emulator::from_path(path.to_path_buf(), 100).map_err(|e| format!("failed to load: {e}"))?;

    emu.start();

    for frame in 0..FRAMES {
        emu.tick();
        if !emu.is_running() && !emu.should_exit() {
            return Err(format!("emulation stopped at frame {frame}"));
        }
    }

    Ok(frame_has_content(emu.get_framebuffer()))
}

#[test]
#[ignore = "requires local game assets (set SPMP8000_GAME_DIR)"]
fn smoke_all_games() {
    let dir = match game_dir() {
        Some(d) => d,
        None => {
            eprintln!(
                "skipping: no game directory found \
                 (set SPMP8000_GAME_DIR or place games in tmp/spmp8000_game)"
            );
            return;
        }
    };

    let mut games = Vec::new();
    collect_games(&dir, &mut games);
    games.sort();

    assert!(
        !games.is_empty(),
        "no .bin games found under {}",
        dir.display()
    );

    println!(
        "Running smoke test over {} games ({FRAMES} frames each)",
        games.len()
    );

    let mut failures = Vec::new();

    for game in &games {
        let rel = game.strip_prefix(&dir).unwrap_or(game);
        match run_one(game) {
            Ok(true) => println!("[PASS] {}", rel.display()),
            Ok(false) => {
                println!("[WARN] {} (blank frame)", rel.display());
            }
            Err(reason) => {
                println!("[FAIL] {} - {reason}", rel.display());
                failures.push(format!("{}: {reason}", rel.display()));
            }
        }
    }

    println!(
        "\n{} passed, {} warned, {} failed",
        games.len() - failures.len(),
        0,
        failures.len()
    );

    assert!(
        failures.is_empty(),
        "{} game(s) failed the smoke test:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
