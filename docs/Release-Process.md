# Release Process

This document describes how to publish a new release of SPMP8000Emu.

## Prerequisites

- Push access to the `master` branch
- Permission to create tags and releases on GitHub

## Steps

### 1. Update version numbers

Version numbers must be updated in **three files**:

| File | Field | Current |
|------|-------|---------|
| `Cargo.toml` (workspace root) | `[workspace.package] version` | `"0.1.0"` |
| `crates/spmp8000emu-libretro/spmp8000emu_libretro.info` | `display_version` | `"0.1.0"` |
| `crates/spmp8000emu-libretro/src/libretro/api.rs` | `library_version` | `"0.1.0"` |

All three values must match. The `.info` file is copied directly into the
release artifacts — RetroArch reads `display_version` to display the core
version to users.

```bash
# Example: bumping to 0.2.0
# 1. Edit Cargo.toml
sed -i 's/^version = "0.1.0"/version = "0.2.0"/' Cargo.toml

# 2. Edit .info file
sed -i 's/^display_version = "0.1.0"/display_version = "0.2.0"/' \
  crates/spmp8000emu-libretro/spmp8000emu_libretro.info

# 3. Edit api.rs
sed -i 's/library_version: c"0.1.0"/library_version: c"0.2.0"/' \
  crates/spmp8000emu-libretro/src/libretro/api.rs
```

### 2. Commit the version bump

```bash
git add Cargo.toml Cargo.lock crates/spmp8000emu-libretro/spmp8000emu_libretro.info crates/spmp8000emu-libretro/src/libretro/api.rs
git commit -m "chore: bump version to 0.2.0"
git push origin master
```

### 3. Create and push a tag

The release workflow triggers on tags matching `v*` (e.g. `v0.2.0`).

```bash
git tag v0.2.0
git push origin v0.2.0
```

### 4. CI builds and creates a draft release

Pushing the tag triggers `.github/workflows/release.yml`, which:

1. **Builds standalone binaries** for Linux, macOS (x86_64 + aarch64), and Windows
2. **Builds libretro cores** for the same platforms, renaming the cdylib to
   `spmp8000emu_libretro.<ext>` and bundling `spmp8000emu_libretro.info`
3. **Builds Android libretro cores** for `arm64-v8a`, `armeabi-v7a`, `x86`,
   `x86_64`, packaged as `spmp8000-android-libretro.tar.gz`
4. **Creates a draft GitHub Release** with:
   - Auto-generated release notes (PRs and commits since the previous tag)
   - A download table for standalone, libretro, and Android artifacts
   - All build artifacts attached

### 5. Review and publish the release

1. Go to [Releases](https://github.com/AloysHF/SPMP8000Emu/releases)
2. Find the draft release created by CI
3. Review the auto-generated changelog — edit if needed (refer to previous release changelogs for style)
4. Verify all expected artifacts are attached:
   - `spmp8000-emu-linux-x86_64.tar.gz`
   - `spmp8000-emu-macos-x86_64.tar.gz`
   - `spmp8000-emu-macos-aarch64.tar.gz`
   - `spmp8000-emu-windows-x86_64.zip`
   - `*-libretro.*` (one per platform)
   - `spmp8000-android-libretro.tar.gz`
5. Click **Publish release**

### 6. Sync `.info` file to upstream libretro-super

RetroArch's **Online Updater > Core Downloader** reads the `.info` file from the
upstream [libretro-super](https://github.com/libretro/libretro-super) repository
(`dist/info/spmp8000emu_libretro.info`), not from this repo. If the `.info` file
was changed in this release (version, metadata, supported extensions, etc.), a
PR must be submitted to sync the changes upstream.

1. Fork [libretro/libretro-super](https://github.com/libretro/libretro-super)
2. Copy `crates/spmp8000emu-libretro/spmp8000emu_libretro.info` from this repo
   to `dist/info/spmp8000emu_libretro.info` in the fork
3. Submit a PR to `libretro/libretro-super` — reference the SPMP8000Emu release
   tag and list the changed fields in the PR description

## Troubleshooting

### CI build fails

- Check the [Actions](https://github.com/AloysHF/SPMP8000Emu/actions) tab
  for the failed run
- The most common failure is a missing Linux build dependency — the CI installs
  `libasound2-dev`, `libx11-dev`, and `libxkbcommon-dev` automatically

### Re-triggering a release

The release workflow only runs on tag pushes. To re-trigger:

```bash
# 1. Delete the tag locally and remotely
git tag -d v0.2.0
git push origin --delete v0.2.0

# 2. Re-push the tag (CI will re-run)
git push origin v0.2.0
```

If a draft release was already created by the failed run, delete it from the
[Releases](https://github.com/AloysHF/SPMP8000Emu/releases) page before
re-pushing the tag, otherwise the new run may conflict with the existing draft.

### Release artifacts missing

- Verify the tag name starts with `v` (e.g. `v0.2.0`, not `0.2.0`)
- The release workflow only triggers on tag pushes (`v*`); manually dispatching
  the workflow from the Actions tab will not create a release

### Version mismatch in RetroArch

- Ensure all three files have the same version string:
  `Cargo.toml`, `spmp8000emu_libretro.info`, and `api.rs`
- The `.info` file is bundled as-is into the release artifacts
