# Game Cheat Codes

This document lists verified game-specific cheat rules. The same codes work in
the [standalone emulator](Standalone-Emulator.md#cheats) and the
[RetroArch core](RetroArch-Core.md#cheats).

Addresses are tied to the tested game build. If a different build moves the
data, use the pointer and memory rules described in the frontend guides to find
the new location.

## Elemental Spirit (精灵勇士)

| Effect | Code | Notes |
|---|---|---|
| Health lock | `ptr16:0x00BBE054+0xCC=300` | Keeps HP at 30. The pointer is re-read every frame, so the lock follows the player object in Story Mode, Cultivation Mode, and level transitions. |
