# Tales of Berseria Remastered — Ultrawide Fix

Fixes the stretched image and HUD at aspect ratios other than 16:9/16:10 (e.g. 3440x1440).

The game runs its original engine (`TLSampleViewerTO12.dll`) inside Unity. That engine only
knows 16:9 and 16:10 and snaps any other aspect ratio to 16:9, which stretches everything. The
mod fixes this in memory as soon as the engine DLL is loaded:

- **3D image**: uses the real aspect ratio of the resolution (no stretching).
- **HUD/menus**: the UI canvas (1280x720, centred) becomes `720·aspect × 720`, keeping the
  HUD proportional and centred in the 16:9 area.
- **Full-screen fades/menu backgrounds**: resized to cover the whole screen.

Patches are located by byte signatures; if a game update breaks one, the mod skips that part
and records it in the log.

## Build

```
cargo build --release
```

Produces `target/release/version.dll` (Rust, MSVC toolchain).

## Installation

1. Copy `version.dll` into the game folder (`...\Tales of Berseria Remastered\`).
2. In the game, use your native resolution (e.g. 3440x1440).
3. Keep the old `TalesOfBerseriaUI.addon` plugin disabled (it applies similar fixes).

After launching the game, `TOBUltrawide.log` in the same folder shows what was applied.

## Uninstall

Delete `version.dll` (and `TOBUltrawide.log`) from the game folder.
