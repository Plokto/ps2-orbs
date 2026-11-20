# ps2-orbs

Ultra-lightweight native Windows live wallpaper recreating the iconic **PlayStation 2 `OSDSYS` clock orbs** on pure `#000000` black in **Rust + Win32 + Direct3D 11** — under **50 MB RAM/VRAM** and **< 1% GPU**.

> Нативний живий фон робочого столу Windows 10/11 у стилі меню PlayStation 2 (`OSDSYS`) без браузерних рушіїв, відеодекодерів і Wallpaper Engine.

## Features
- **Authentic PS2 `OSDSYS` Clock Physics:** 7 orbs driven by the real local PC clock (`GetLocalTime` + sub-second QPC interpolation) using the exact `(i + 21) * rateX` and `1100 * rateY` 3D orbital transform (`:00` merges all 7, `:15/:45` forms 4 groups, `:20/:40` forms 3 streams, `:30` forms 2).
- **2.2s Chromatic Trails:** Analytical trail geometry with PS2's asymmetric `(R·u⁴, G·u², B·u)` color decay in a single dynamic vertex buffer (1 draw call per frame, zero textures, zero MSAA, zero Z-buffer).
- **Two Built-in Modes:**
  - `PS2 Original (BIOS Clock)` — faithful steel-aqua halo + white core + fading cobalt trails.
  - `PS2 Vivid Violet Trails` — vivid cyan → electric blue → violet nebula trails with plasma beads on the same 3D clock orbit.
- **Desktop Integration & Tray Controls:** Attaches behind desktop icons (`Progman` / `WorkerW`) with clean wallpaper restoration on exit, high-DPI awareness (`PerMonitorV2`), and system tray menu (left/right click or re-launching the `.exe` opens the menu at the cursor):
  - Mode switch (`PS2 Original` / `PS2 Vivid Violet Trails`)
  - Resolution (`FHD 1920×1080` stretched by DWM / `QHD 2560×1440` native)
  - FPS cap (`30` / `60 FPS` via high-resolution waitable timer)
  - Auto-pause when covered by maximized/fullscreen windows (`≥98%` work area, skipping cloaked & transparent/layered overlays), screen lock, or monitor power-off
  - `Run at Windows startup` toggle (`HKCU\...\Run`)

## Measured Resource Usage (`Windows 10 Pro`, `RTX 3070 Laptop`, `2560×1440 @ 165 Hz`)

| Profile | Binary Size | Working Set RAM | Private RAM | Dedicated VRAM | GPU 3D Load |
|---|---:|---:|---:|---:|---:|
| **FHD (`1920×1080` @ 60 FPS)** | ~336 KB | ~38.5 MB | ~47.3 MB | ~26.0 MB | < 0.5% |
| **QHD (`2560×1440` @ 60 FPS)** | ~336 KB | ~38.2 MB | ~46.4 MB | ~39.1 MB | < 0.5% |

*`d3dcompiler_47.dll` is loaded dynamically only at startup to compile the two tiny HLSL shaders and immediately unloaded via `FreeLibrary`.*

## CLI Flags
- `--preview` — launch in a standard resizable window (`1280×720`) instead of attaching to the desktop wallpaper.
- `--mode menu|trail` — start in `PS2 Original` (`menu`) or `PS2 Vivid Violet Trails` (`trail`).
- `--fhd` / `--qhd` — override render resolution.
- `--no-autopause` — keep rendering even when desktop is occluded.
- `--time <seconds>` — override clock time (useful with `--capture`).
- `--capture <path.bmp>` — render a single frame offscreen to a 24-bit BMP and exit.

## Building from Source
Requires Rust (`stable-x86_64-pc-windows-gnu` or `stable-x86_64-pc-windows-msvc`):
```powershell
cargo clippy --release -- -D warnings
cargo build --release
```
Output binary: `target\release\ps2-orbs.exe`.

## Acknowledgments
Orb orbital equations, camera setup, and `(u⁴, u², u)` chromatic trail decay are adapted from the clean-room PlayStation 2 `OSDSYS` reverse-engineering projects [`aap/osdbits`](https://github.com/aap/osdbits) and [`JeanxPereira/CrystalOSD`](https://github.com/JeanxPereira/CrystalOSD).
