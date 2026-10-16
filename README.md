# God of War III Remastered for PC 

<div align="center">
  <a href="https://trendshift.io/repositories/299469?utm_source=trendshift-badge&amp;utm_medium=badge&amp;utm_campaign=badge-trendshift-299469" target="_blank" rel="noopener noreferrer"><img src="https://trendshift.io/api/badge/trendshift/repositories/299469/daily?language=C%2B%2B" alt="FelipePrado0%2Fgow3_pc | Trendshift" width="250" height="55"/></a>
</div>

**English** | [Português (Brasil)](https://github.com/FelipePrado0/gow3_pc/blob/main/README.pt-BR.md) | [Español](https://github.com/FelipePrado0/gow3_pc/blob/main/README.es.md)

A PC port of **God of War III Remastered** (PlayStation 4) for Windows 10/11 and Linux
(experimental AppImage).

> **No game files are included.** You need a dump from your own PS4 (tested: CUSA01623 and
> CUSA01715, versions 01.00 and 01.02).
> This project is not affiliated with Sony Interactive Entertainment or Santa Monica Studio.
> God of War is a trademark of Sony Interactive Entertainment.

## Contents

- [God of War III Remastered for PC](#god-of-war-iii-remastered-for-pc)
  - [Contents](#contents)
  - [Download and play](#download-and-play)
  - [Linux (AppImage, experimental)](#linux-appimage-experimental)
  - [What works](#what-works)
  - [Feature availability](#feature-availability)
  - [Settings](#settings)
  - [Keyboard controls](#keyboard-controls)
  - [Importing shadPS4 saves](#importing-shadps4-saves)
  - [Third-party overlays (Windows)](#third-party-overlays-windows)
  - [Troubleshooting](#troubleshooting)
  - [Performance and compatibility](#performance-and-compatibility)
  - [Building from source](#building-from-source)
    - [Windows](#windows)
    - [Linux (Ubuntu 24.04)](#linux-ubuntu-2404)
  - [Tests](#tests)
    - [Python](#python)
    - [Windows C++ tests (MSYS2 CLANG64)](#windows-c-tests-msys2-clang64)
    - [Linux C/C++ tests](#linux-cc-tests)
    - [Checks using a prepared game](#checks-using-a-prepared-game)
  - [How it works](#how-it-works)
  - [Credits](#credits)
  - [License](#license)

## Download and play

- Windows 10 (1903 or newer) or Windows 11, 64-bit.
- A 64-bit processor with AVX: Intel Sandy Bridge (2011) or newer, AMD FX or newer. On CPUs without
  MOVBE, BMI1 or LZCNT (Sandy/Ivy Bridge, AMD FX) the loader rewrites the game's instructions that
  use them to emulated stubs and logs an `ISA:` line; performance on those CPUs has not been
  measured yet. Older CPUs stop at start with a `STOP: this CPU lacks` line in the log.
- A graphics card with Vulkan 1.3 and a current driver.
- Your copy of the game (the folder with `eboot.bin`, `sce_module` and `sce_sys`). Tested:
  CUSA01623 and CUSA01715, versions 01.00 and 01.02.

**Steps**

1. Download `gow3-windows.zip` from the
   [Releases page](https://github.com/FelipePrado0/gow3_pc/releases/latest).
2. Extract it to a folder of your choice, for example `C:\Games\gow3-windows`. Do not run it from
   inside the zip.
3. Open **`God of War III.exe`**. If Windows shows "Windows protected your PC", click
   **More info**, then **Run anyway**: the program is not digitally signed.
4. Click **Choose the game folder**. It takes you to the **Game** page. Next to **Game folder**,
   click **Browse…** and select the folder with `eboot.bin`.
5. Back on **Home**, the cover art and "CUSA01623 · version 01.02" appear. Adjust **Quick
   settings** if you like: display mode, frame rate limit, render resolution and engine frame
   rate.
6. Click **Save** to keep pending changes, then **PLAY**. The first start prepares the game image;
   shader compilation can also affect startup and performance on later launches.

Patches and fixes are included and applied automatically: texture fix, skip videos with X and
120 FPS are on by default. Change the resolution and frame rate on **Home** or **Performance**.
The startup black-screen fix is always requested, even with an older saved patch selection;
patches apply only where their signatures match. A resolution patch replaces the texture fix.

**While playing**

- **Insert** on the keyboard, or **R3 + L2** on the controller, opens the port's menu: cheats,
  multipliers, image, performance and the performance overlay.
- Saved shaders load in the background. A small "Loading shader cache" note in the corner shows
  the progress.

**Next time**

- **`Play God of War III.exe`** starts the game straight away with your saved settings. You can
  make a desktop shortcut to it, or add it to Steam with **Add a Non-Steam Game**.
- Your saves, shader cache and log are in the `user` folder next to `God of War III.exe`. Keep
  that folder when you update.
- **Advanced → Check for updates** downloads and installs a new version. Your saves and settings
  are kept.

## Linux (AppImage, experimental)

The Linux build runs the same native runtime and the same launcher. It was tested on Arch Linux
with a Radeon RX 7800 XT (RADV) in a Wayland session.

**You need:** x86-64 Linux with glibc 2.39 or newer, an AVX-capable CPU, a Vulkan 1.3 driver
and your own game dump. Launcher startup is checked on Ubuntu 24.04, Debian 13, Fedora 42,
Arch and openSUSE Tumbleweed; this does not establish gameplay compatibility on every GPU.

**Download and run**

1. Download `gow3-x86_64.AppImage` from the
   [Releases page](https://github.com/FelipePrado0/gow3_pc/releases/latest).
2. In its folder, run:
   ```bash
   chmod +x gow3-x86_64.AppImage
   ./gow3-x86_64.AppImage
   ```
3. Choose your game folder, save the settings and press **PLAY**.

If an older download reports `GLIBC_2.44 not found`, download the rebuilt AppImage again.
The Windows updater is not available on Linux; download a newer AppImage to update.
Steam Deck/RADV rendering corruption is a known issue in the published 2.2.0 AppImage.

- **Files:** saves, settings, shader cache and the log are in `~/.local/share/gow3`; the launcher's
  own settings in `~/.config/gow3-launcher`.
- **Update folder:** if the update was dumped apart from the game, choose it under **Game → Update
  folder**. It is laid over the game folder without changing either.
- **Advanced → Linux:** window system (automatic, Wayland or X11), GameMode and MangoHud. Only the
  tools installed on the system can be turned on.
- **Steam:** **Advanced → Add to Steam** adds "God of War III Remastered" as a non-Steam game, with
  the icon, the vertical and horizontal capsules, the library background and the logo made from the
  dump's pictures. Close Steam first (it rewrites its shortcuts when it exits). The button is also in
  the Windows launcher (not tested there yet).
- **Steam Deck / SteamOS (Gaming Mode):** if Steam tries to launch the AppImage with Proton, open
  the shortcut's **Properties → Compatibility** and select **Steam Linux Runtime** instead of Proton.
  The AppImage is a native Linux application.
- **Menu entry:** the AppImage writes `~/.local/share/applications/gow3.desktop` when it starts, so
  the launcher and the game window show their icon in the dock; **Advanced → Linux** turns it off and
  removes the entry. When another tool (Gear Lever, AppImageLauncher…) already made an entry that
  starts the same file, it makes none and removes its own, so there are no duplicates.

## What works

- Opening, menus, video cutscenes (H.264) and gameplay.
- Audio: music, voices and effects (ATRAC9 and MP3).
- Controller (DualSense, Xbox and others, through SDL3) and keyboard at the same time, both
  remappable.
- **In-game button icons** (opt-in, off by default): the PlayStation button icons in the game can
  show your controller's, Xbox or Nintendo ones (Game page, Controls).
- Saving and loading: the save list is shown on screen, as on the PS4. Autosave works too. The
  launcher backs the saves up before every start.
- Patches: render resolution of 480p, 720p, 1440p, 1800p or 4K, corrupted texture fix and skip
  videos with the X button (community); engine frame rate of 120 FPS (kvicken) or 240 FPS
  (adapted by Felipe Prado).
- **Ultrawide (beta):** 21:9, 32:9 and 16:10 render resolutions (3440x1440 tested) with a wider field of view
  instead of a zoomed-in picture, built in: choose one in **Render resolution**, where every size shows its exact
  resolution and aspect ratio. Cutscenes keep their 16:9 borders (see [docs/ultrawide.md](docs/ultrawide.md)).
  Mods can also carry game patches; the [Ultrawide mod](extras/mods/Ultrawide) is the same sizes for older versions.
  **How to install a mod:** put its `.zip` (do not unzip it) in the `mods` folder next to `God of War III.exe`;
  the game unpacks it by itself. The Ultrawide mod and a one-click installer for 2.2.0 are explained in
  [extras/mods/Ultrawide](extras/mods/Ultrawide/README.md).
- **Background shader compilation:** saved pipelines load and compile in the background while
  you play. Stutters, missing effects during compilation and temporary FPS drops can still occur;
  startup time depends on the cache, graphics driver and CPU.
- **Launcher** in the game's colors, with selectable languages and English fallback: five pages
  (**Home, Graphics, Performance, Game, Advanced**) and a **Save** button for pending changes;
  quick settings on the home page, graphics,
  performance, game folder and save backups, patches, mods (order, presets and conflicts),
  controls with PlayStation or Xbox button icons, updates and diagnostics.
- **In-game menu** (Insert on the keyboard, R3+L2 on the controller), with four tabs:
  - **Game:** cheats (infinite health, magic, item meter, Rage of Sparta, max red orbs);
    multipliers from 0.1x to 100x for red, green (health), blue (magic) and gold (Rage of
    Sparta) orbs and for damage dealt and taken; an enemy health bar.
  - **Image:** display mode (windowed, borderless, fullscreen), VSync and RCAS sharpening.
  - **Performance:** frame rate limit (30 to unlimited), frames queued, background shader
    compiling; render resolution, engine frame rate and GPU readback options apply with one
    "Apply and restart" button.
  - **Overlay:** a performance overlay with FPS, frametime, upscaler, GPU (name and use), CPU,
    RAM and VRAM, each optional, with font size, corner, background opacity and layout. The
    numbers are the game's own use, not the whole system's.

## Feature availability

Availability checked against [release 2.2.0](https://github.com/FelipePrado0/gow3_pc/releases/tag/windows-v2.2.0) and the source on October 14, 2026.

| Feature | Published 2.2.0 packages | Current source |
|---|---|---|
| Windows ZIP / experimental Linux AppImage | Available | Build scripts available |
| Startup black-screen patch / controller button icons | Included | Included |
| Discord Rich Presence | Not included | Optional under Advanced, off by default; game name and elapsed time |
| RADV sparse-queue fix | Not included | Merged into `main` |
| FSR 1, FSR 3.1, FSR 4, TAA, DLSS | Disabled | Disabled in the launcher/game settings path |
| RCAS sharpening | Available | Available |

The Linux AppImage was rebuilt on October 14 for glibc 2.39 or newer. The Windows and Linux
packages can use different source revisions. Check the release notes for changes to later downloads.

## Settings

Settings are kept in `gow3.ini` (the game and the in-game menu) and in
`%APPDATA%\gow3-launcher\settings.json` (the launcher). Saves, the shader cache and
`last_run.log` go to the project's `user` folder.

On **Performance → GPU data → Stream buffer memory**, choose **System RAM (cached, default)**,
**System RAM (uncached)** or **GPU memory (previous behavior)**. System RAM can improve frame
rates on NVIDIA laptops. The choice is saved and applies on the next game launch, including
when using `Play God of War III.exe` or `--play`. Choose GPU memory to restore the previous
allocation behavior. For direct launches without the launcher, `GOW3_STREAM_MEMORY` accepts
`cached`, `host` or `device`; the launcher's saved choice takes precedence when using it.

On **Performance → GPU data → Game multisampling**, **Automatic** lets the game decide by the
graphics card it runs on: it creates the game's multisampled images with a single sample on
AMD's and Intel's Windows drivers, where reading the 2-sample depth buffer as a texture
produced black lines in the lighting (Radeon RX 6600 family, Arc), and keeps them multisampled
elsewhere (NVIDIA). The label names the card and the decision, for example "Automatic (Intel(R)
Arc(TM) A750 Graphics: off)", and follows the **Graphics card** choice. The rule covers every
GPU on those two drivers, integrated ones included; the game's log line `GPU: guest
multisampling off` or `on` is what was applied. Choose **Off** if the lines appear on another
card and **On** to keep multisampling anyway. Off and On set `GOW3_NO_MSAA=1` or `0`; Automatic
passes an inherited `GOW3_NO_MSAA` through. Applies on the next game launch.

## Keyboard controls

Defaults; change them in the launcher's **Game** page, **Controls** section.

| Key | Button |
|---|---|
| WASD | left stick |
| Arrows | right stick (camera) |
| Space | Cross |
| Left Shift | Circle |
| E | Square |
| Q | Triangle |
| 1 and 3 | L1 and R1 |
| R and F | L2 and R2 |
| Z and C | L3 and R3 |
| I, K, J, L | d-pad |
| Enter | Options |
| Tab | left touchpad click (photo mode) |
| Backspace | right touchpad click (pause menu) |
| Insert | the port's menu (R3+L2 on the controller) |

On a controller without a touchpad (Xbox and others), View opens the pause menu (the right
touchpad click) and L1 + View opens photo mode (the left click). On a DualSense the touchpad
works as on the PS4, and Share/Create works like View.

**In-game button icons** (off by default) are chosen in the launcher's Game page, Controls section, under
*In-game button icons*: *Match my controller*, *Xbox* or *Nintendo*. They replace the PlayStation icons in the
tutorial, move-list and quick-time prompts; the pause-menu controller drawing stays a DualShock 4.
The launcher writes a patched copy of two game files into the port's data folder and never changes
your game folder. A new controller is picked up after a restart. Without the launcher, set
`GOW3_GLYPH_STYLE` to `auto`, `xbox` or `nintendo` before starting `run.py`.

In the menu: L1/R1 or Q/E switch tabs. In the save list: arrows or the d-pad choose, Enter or
Cross confirm, Esc or Circle cancel.

## Importing shadPS4 saves

Only **shadPS4 saves for God of War III Remastered** are compatible. RPCS3 saves are
from the PS3 game and will not work. Use the same title ID (this example uses `CUSA01623`).

1. Close the game and shadPS4. Back up both save folders.
2. Find the source in shadPS4's data folder: `user\savedata\1\CUSA01623\` in older
   versions, or `home\<user-id>\savedata\CUSA01623\` in newer versions.
3. The destination is **`user\savedata\1\CUSA01623\` next to `God of War III.exe`**.
   Example: `C:\Games\gow3-windows\user\savedata\1\CUSA01623\`. With a custom
   **Saves folder** in the launcher, use `<custom folder>\savedata\1\CUSA01623\`.
4. Copy each slot folder there, keeping its name and game files. Do not nest another
   `CUSA01623` folder. Exclude the slot's internal `sce_sys` folder from the game data.
5. Beside each slot, create `<slot name>.sce_sys` if it does not exist. You can copy
   shadPS4's `sce_sys\icon0.png` there. **Do not rename `param.sfo` to `param.bin`**:
   the formats differ. Preserve any existing port `param.bin`. Without one, the slot
   appears by folder name; saving again in the port writes its metadata and refreshes
   the displayed information.
6. Start the port and select **Load Game**. Completed saves can be imported too.

The port's save dialog displays at most **32 entries**. If the imported slot is missing
from a long list, test it with a separate **Saves folder** containing just that slot
under `savedata\1\CUSA01623\`, with its sibling metadata folder. Keep the original saves
and restore the previous Saves folder setting after testing.

## Third-party overlays (Windows)

The game process is **`bin\gow3-probe.exe`**, not `God of War III.exe` (source builds
use `out\gow3-probe.exe`). Add that executable to RTSS and enable Afterburner's on-screen
display. In the ReShade installer, select that same executable and the **Vulkan** API.

If the overlay still does not appear, report the port and overlay versions, the last
working port version, and attach `user\last_run.log`. These checks do not confirm a fix
for every overlay configuration.

## Troubleshooting

- Black screen at start: use the latest release, which includes the startup deadlock patch.
  If it persists, report your GPU, driver, game build and **Advanced → Log** output. Clearing
  the shader cache is not a general fix and requires rebuilding it on the next launch.
- The game closes with "cannot create ... guest memory (error 1450)": Windows ran out of virtual memory to allocate the guest pool. Increase the page file: press **Win + R**, type `SystemPropertiesAdvanced`, go to **Performance** Settings → **Advanced** → **Virtual memory** Change, uncheck automatic management, select drive **C:**, set a custom size (e.g. 16384 MB initial, 24576 MB maximum), click **Set**, **OK**, and restart your PC.
- The game does not open: check **Advanced → Log**, or send `user\last_run.log` with your report.
  With a custom **Saves folder**, `last_run.log` is in that folder instead.
- Yellow/orange/white textures or wrong lighting: try disabling **Performance → GPU data →
  Deferred GPU readbacks**, then restart. This workaround can cause stutters.
- GPU-specific rendering failures, crashes, audio problems and progress blockers remain under
  investigation. Check the [release notes](https://github.com/FelipePrado0/gow3_pc/releases/latest)
  and [issues](https://github.com/FelipePrado0/gow3_pc/issues) before reporting; include a save
  from before the problem when relevant.

## Performance and compatibility

Previous testing on an AMD Radeon RX 6700 XT, Ryzen 5 5600X and 32 GB recorded 144 FPS on
average (55 to 240) at 1080p with the 240 FPS engine frame rate. These are reference results,
not a benchmark of every release or a guarantee for other systems.

**Tested and playable from start to finish** at native 1080p with the 240 FPS engine frame
rate on the test system. Texture corruption, crashes and progress blockers have also been
reported on other setups, including at 1080p. See the known issues in the release notes.

## Building from source

### Windows

Install Python 3.10 or newer from python.org, with Tk support, then install the launcher and
instruction-rewrite dependencies in PowerShell:

```powershell
python -m pip install pillow iced-x86==1.21.0
```

The following build commands use MSYS2 CLANG64; launcher commands marked PowerShell use the
Windows Python installation.

Development happens on the `gow3` branch; `main` receives tested merges.

1. Install MSYS2 (`winget install MSYS2.MSYS2`) and, in the **MSYS2 CLANG64** shell, the
   packages listed in [packaging/windows/README.md](https://github.com/FelipePrado0/gow3_pc/blob/main/packaging/windows/README.md).
2. Clone the repository with its submodules and build:

   ```bash
   git clone --recursive https://github.com/FelipePrado0/gow3_pc
   cd gow3_pc
   git checkout gow3   # development branch; omit to build main
   bash build.sh
   ```

3. Start the launcher (in PowerShell, in the project folder):

   ```powershell
   python launcher/gow3_launcher_win.py
   ```

4. On **Game**, choose the game folder and patches. Adjust **Graphics** and **Performance**,
   click **Save** for pending changes, then **PLAY**.

To play with the saved settings without opening the launcher's window (for a shortcut or Steam):

```powershell
python launcher/gow3_launcher_win.py --play
```

Or without the launcher at all:

```powershell
$env:GOW3_GAME_DIR = 'D:\path\CUSA01623'
python run.py
```

### Linux (Ubuntu 24.04)

For a fresh checkout, run the following in a Linux shell. The dependency script installs system
packages using sudo, selects GCC 14 as the default compiler and creates `out/venv` with
PyInstaller, Pillow and iced-x86. It is intended for Ubuntu 24.04; adapt dependencies for other
distributions. If already cloned, start in the repository and skip the first two commands.

```bash
git clone --recursive https://github.com/FelipePrado0/gow3_pc
cd gow3_pc
bash packaging/linux/ci-deps.sh
bash build.sh
out/venv/bin/python launcher/gow3_launcher_win.py
```

To package an AppImage after the build:

```bash
PATH="$PWD/out/venv/bin:$PATH" bash packaging/linux/build.sh
./dist/gow3-x86_64.AppImage
```

The build environment is created before the packaging command. Building on a newer distribution
can require a newer glibc; public release builds use Ubuntu 24.04. From the repository, use
`out/venv/bin/python launcher/gow3_launcher_win.py --play` for saved launcher settings, or
`GOW3_GAME_DIR=/path/to/CUSA01623 out/venv/bin/python run.py` for a direct launch.

## Tests

Run these from the repository root after the build. The first C++ checks below do not need a game dump.

### Python

On Windows, use PowerShell. On Linux, replace `python` with `out/venv/bin/python`.

```powershell
python -m unittest discover -s tests -p "test_*.py"
```

### Windows C++ tests (MSYS2 CLANG64)

```bash
source ./msys2-env.sh
ninja -C out/gpu perf-overlay-test warmup-inbox-test graphics-settings-test
out/gpu/perf-overlay-test.exe
out/gpu/warmup-inbox-test.exe
out/gpu/graphics-settings-test.exe
```

### Linux C/C++ tests

```bash
bash build.sh --test
ninja -C out/gpu perf-overlay-test warmup-inbox-test graphics-settings-test
out/gpu/perf-overlay-test
out/gpu/warmup-inbox-test
out/gpu/graphics-settings-test
```

`bash build.sh --test` runs the additional C runtime tests on Linux only. On Windows the build exits before that block; use the Ninja targets above.

### Checks using a prepared game

After preparing your own dump, `out/eboot.elf` and `out/boot-linked.bin` are needed for the
following checks. In MSYS2 CLANG64 on Windows, replace the example game path with yours:

```bash
ninja -C out/gpu stat-multipliers-test
out/gpu/stat-multipliers-test.exe out/eboot.elf
out/gow3-probe.exe out/boot-linked.bin --app0 "D:/Games/CUSA01623" --check-imports
```

On Linux, use `out/gpu/stat-multipliers-test` and `out/gow3-probe` without `.exe`, and a Linux
game path. `--check-imports` checks system imports and exits without starting gameplay.
More test targets are listed in [gpu/CMakeLists.txt](https://github.com/FelipePrado0/gow3_pc/blob/main/gpu/CMakeLists.txt).

## How it works

The CPU side is native: the game's original executable runs directly on the PC's processor (the
PS4 uses the same x86-64 architecture), and a runtime written for this game replaces the PS4
system libraries, without CPU emulation. The graphics are translated: they run on the
[shadPS4](https://github.com/shadps4-emu/shadPS4) video core, which converts the PS4 GPU commands
and shaders to Vulkan, with the God of War III rendering fixes from
[cuesta4's `eltutz` branch](https://github.com/cuesta4/shadPS4/tree/eltutz). Running the game code
directly on the CPU is also how shadPS4 works; what this project changes is the system layer and
the game-specific work (patches, background shader cache, launcher and in-game menu).
It is a fork of [Supermedo's Windows port](https://github.com/Supermedo/bloodborne_pc) of
[deadinside28's bloodborne_pc](https://github.com/deadinside28/bloodborne_pc) (see
[Credits](#credits)).

- The patches and the in-game hooks (cheats, multipliers, health bar) find their sites by
  signature: the original bytes around each one, wherever a release placed that code
  (`tools/make_signatures.py`). A site that is not found leaves only that patch or hook off; the
  game starts without it, and the log and the menu say why.

| Part | Where | What it does |
|---|---|---|
| Preparation | `scripts/` | Reads `eboot.bin`, applies its relocations and links the game's own `libc` and `libSceFios2` into one image |
| Loader | `src/probe.c` | Loads the image, installs the validated hooks (cheats, multipliers, health bar) and starts the game's original code |
| Runtime | `src/runtime_*.c` | Reimplements the PS4 system functions: memory, threads, files, audio, video, controller, saves and services |
| GPU | `gpu/shadps4/` | The shadPS4 video core, translating the PS4 GPU commands and shaders to Vulkan, with cuesta4's God of War III fixes (`eltutz` branch) and this port's speed work (background shader cache, deferred readbacks) |
| In-game menu | `gpu/shim/gow3_overlay.cpp` | The Dear ImGui menu and performance overlay; settings in `gpu/shim/gow3_settings.cpp` |
| Patches | `patches/God_of_War_III_Remastered.xml`, `scripts/patches.py` | Applied in memory at start, without changing the game files |
| Launcher | `launcher/gow3_launcher_win.py` | Settings, patches, mods, controls and the play button |

More documentation: [mods](https://github.com/FelipePrado0/gow3_pc/blob/main/docs/MODS.md), [frame map and historical upscaler research](https://github.com/FelipePrado0/gow3_pc/blob/main/docs/upscaler.md)
and [parallel GPU work](https://github.com/FelipePrado0/gow3_pc/blob/main/docs/parallel_gpu.md).

## Credits

This project is a fork of Supermedo's Windows port, which builds on deadinside28's original:

- **deadinside28**: [bloodborne_pc](https://github.com/deadinside28/bloodborne_pc), the original
  native port.
- **Supermedo** (Mohammed Albarghouthi): [bloodborne_pc for Windows](https://github.com/Supermedo/bloodborne_pc),
  the fork this project comes from.
- **cuesta4**: [shadPS4 fork focused on God of War III Remastered](https://github.com/cuesta4/shadPS4/tree/eltutz)
  (branch `eltutz`). The game's rendering in this port depends on its fixes: the GNM blend
  rewrite, depth/stencil state and image aliasing. cuesta4 is also a co-author of the
  resolution, texture fix, 120 FPS and skip videos patches (below). If you play on shadPS4,
  this is the build to use for God of War III.
- **shadPS4 team**: the [shadPS4](https://github.com/shadps4-emu/shadPS4) video core, which does
  all the GPU and shader translation to Vulkan in this port.

Patches: kvicken and cuesta4 (resolution, texture fix, 120 FPS and skip videos, building on the
PS3 patches by illusion0001), published for shadPS4 in
[shadps4-emu/ps4_cheats#145](https://github.com/shadps4-emu/ps4_cheats/pull/145) and adapted to
this executable. The 240 FPS patch was adapted by Felipe Prado from the 120 FPS patch.
Cheats: adapted from Celogamez's [GoldHEN cheats](https://github.com/GoldHEN/GoldHEN_Cheat_Repository).

Also used: [LibAtrac9](https://github.com/Thealexbarney/LibAtrac9), [FFmpeg](https://ffmpeg.org),
[SDL3](https://github.com/libsdl-org/SDL), [Dear ImGui](https://github.com/ocornut/imgui),
[sirit](https://github.com/shadps4-emu/sirit), [magic_enum](https://github.com/Neargye/magic_enum),
[miniz](https://github.com/richgel999/miniz), [xbyak](https://github.com/herumi/xbyak),
[Vulkan Memory Allocator](https://github.com/GPUOpen-LibrariesAndSDKs/VulkanMemoryAllocator),
[FSR-Vulkan](https://github.com/FireBurn/FSR-Vulkan) and the AMD FidelityFX SDK,
[MSYS2](https://www.msys2.org), [LLVM](https://llvm.org) and [Pillow](https://python-pillow.org).

## License

GNU GPL v2 or later ([LICENSE](https://github.com/FelipePrado0/gow3_pc/blob/main/LICENSE)). Third-party components keep their own licenses.
