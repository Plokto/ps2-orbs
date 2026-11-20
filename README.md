# ps2-orbs (PlayStation Live Wallpapers)

Ultra-lightweight native Windows live wallpaper suite recreating classic **PlayStation 2, PlayStation 3, PSP, and PlayStation 4** backgrounds in a single native **Rust + Win32 + Direct3D 11** executable (~`439 KB`) — using **~38–44 MB RAM**, **~26–32 MB VRAM**, and **< 0.8% GPU** (0% when paused or occluded).

> Нативний живий фон робочого столу Windows 10/11 із 7 сценами у стилі PlayStation без браузерних рушіїв, відеодекодерів і Wallpaper Engine. Неактивні сцени повністю вивантажують свої GPU-ресурси (0 байтів VRAM).

## Included Scenes (7 Modes in 1 Executable)
1. **PS2 Original (`ps2_original` / `menu`)** — authentic PlayStation 2 `OSDSYS` clock orbs: 7 spheres driven by the real local PC clock (`(i + 21) * sec` & `1100 * min` 3D orbital transform) with 2.2s `(R·u⁴, G·u², B·u)` chromatic trails on pure `#000000` black.
2. **PS2 Vivid Violet Trails (`ps2_vivid` / `trail`)** — vivid cyan → electric blue → violet trails with plasma-echo beads on the same 3D clock orbit.
3. **PS2 Startup Towers (`ps2_towers`)** — faithful recreation of the PS2 `OSDSYS` boot sequence (`14 × 9` boot-history monolith field with `HeightGrid` radial falloff, weathered stone texture, 5-layer 3D depth-tested cobalt fog, 4 orbiting colored light trails, and 5 tumbling 2-depth refractive glass cubes).
4. **PS3 XMB Wave — OLED Black (`ps3_oled`)** — faithful port of the PS3 `spline.elf` simulation (`19 × 19` spring-mass sheet, 3D FFD deformation lattice, `128 × 128` bicubic B-spline silk surface, Fresnel LUT) + drifting sparkles on pure `#000000` black.
5. **PS3 XMB Wave — Monthly Color (`ps3_classic`)** — the same PS3 B-spline silk wave and sparkles over the authentic 12-month day/night cubic gradient backdrop (adapts automatically to local month, day, and hour).
6. **PSP XMB Wave (`psp_wave`)** — 7 layered asymmetric silk sine crests over a midnight-cobalt / OLED-black vignette.
7. **PS4 Flow Wave (`ps4_flow`)** — 3D Perlin `cnoise`-displaced dual silk ribbons in PlayStation royal blue with drifting soft bokeh orbs.

## Features
- **Desktop Integration & Tray Controls:** Attaches behind desktop icons (`Progman` / `WorkerW`) with clean wallpaper restoration on exit, high-DPI awareness (`PerMonitorV2`), and system tray menu (left/right click or re-launching the `.exe` opens the menu at the cursor):
  - Instant scene switcher (lazy allocation — drops old scene GPU resources before creating the next)
  - Resolution toggle: `FHD (1920×1080)` (DWM-stretched, minimum VRAM) / `QHD (2560×1440)` (native)
  - FPS cap: `30` / `60 FPS` via high-resolution `CreateWaitableTimerExW`
  - Auto-pause when covered by maximized/fullscreen windows (`≥98%` work area, skipping cloaked & transparent/layered overlays), screen lock, or monitor power-off
  - `Run at Windows startup` toggle (`HKCU\...\Run`)

## Measured Resource Usage (`Windows 10 Pro`, `RTX 3070 Laptop`, `FHD @ 60 FPS`)

| Scene | Working Set RAM | Private Commit | Dedicated VRAM | GPU 3D Load |
|---|---:|---:|---:|---:|
| **PS2 Original (`ps2_original`)** | `38.64 MB` | `46.38 MB` | `25.96 MB` | `0.24%` |
| **PS2 Startup Towers (`ps2_towers`)** | `41.53 MB` | `53.84 MB` | `31.86 MB` | `0.72%` |
| **PS3 XMB Wave OLED (`ps3_oled`)** | `43.64 MB` | `48.85 MB` | `26.19 MB` | `0.46%` |
| **PS3 XMB Wave Monthly (`ps3_classic`)** | `44.22 MB` | `48.85 MB` | `26.18 MB` | `0.49%` |
| **PSP XMB Wave (`psp_wave`)** | `38.21 MB` | `44.76 MB` | `25.94 MB` | `0.25%` |
| **PS4 Flow Wave (`ps4_flow`)** | `43.13 MB` | `47.00 MB` | `26.01 MB` | `0.23%` |

*Executable size: `439.0 KB`. `d3dcompiler_47.dll` is loaded dynamically only on scene creation to compile tiny HLSL shaders and immediately unloaded via `FreeLibrary`.*

## CLI Flags
- `--scene <name>` (or `--mode <name>`) — launch with a specific scene: `ps2_original` (`menu`), `ps2_vivid` (`trail`), `ps2_towers`, `ps3_oled`, `ps3_classic`, `psp_wave`, `ps4_flow`.
- `--preview` — launch in a standard resizable window (`1280×720`) instead of attaching to the desktop wallpaper.
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
- **PS2 `OSDSYS` Orbs & Startup Towers:** [`aap/osdbits`](https://github.com/aap/osdbits) and [`JeanxPereira/CrystalOSD`](https://github.com/JeanxPereira/CrystalOSD)
- **PS3 XMB Wave & Monthly Day/Night Backdrop:** [`bungholesurfers/Nagare`](https://github.com/bungholesurfers/Nagare) & [`linkev/PlayStation-3-XMB`](https://github.com/linkev/PlayStation-3-XMB)
- **PSP XMB Wave Shader:** [`fchavonet/creative_coding-xmb_wave_background`](https://github.com/fchavonet/creative_coding-xmb_wave_background)
- **PS4 Flow Shader:** [`alexkorzin/PlayStationBG`](https://github.com/alexkorzin/PlayStationBG)
