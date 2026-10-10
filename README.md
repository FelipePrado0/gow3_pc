# God of War III Remastered for PC

**English** | [Português (Brasil)](README.pt-BR.md)

A native port of **God of War III Remastered** (PlayStation 4) to Windows 10 and 11.

The game's original executable runs directly on the PC's processor (the PS4 uses the same x86-64
architecture). A runtime written for this game replaces the PS4 system libraries, and the
graphics are translated to Vulkan. It is not a general emulator: the project serves one game.

> **No game files are included.** You need a dump from your own PS4 (CUSA01623, version 01.02).
> This project is not affiliated with Sony Interactive Entertainment or Santa Monica Studio.
> God of War is a trademark of Sony Interactive Entertainment.

**Status: alpha in development.** The game boots, plays its cutscenes, has sound and controller
input, and saves. On the test machine (AMD Radeon RX 6700 XT, Ryzen 5 5600X) a heavy gameplay
scene runs at about 66 FPS at 1080p with the 120 FPS patch, and menus reach 240 FPS. There is no
ready-made download yet: for now the project is built from source. Progress is tracked (in
Portuguese) in [docs/gow3/ROADMAP.md](docs/gow3/ROADMAP.md).

## What works

- Opening, menus, video cutscenes (H.264) and gameplay.
- Audio: music, voices and effects (ATRAC9 and MP3).
- Controller (DualSense, Xbox and others, through SDL3) and keyboard at the same time, both
  remappable.
- Saving and loading: the save list is shown on screen, as on the PS4. Autosave works too. The
  launcher backs the saves up before every start.
- Community patches: render resolution of 480p, 720p, 1440p, 1800p or 4K; corrupted texture fix;
  engine frame rate of 120 or 240 FPS; skip videos with the X button.
- **Instant start:** the saved shader cache loads in the background while you play, and new
  shaders compile in the background too, so the game does not stop to build them.
- **Launcher** in the game's colors, in 13 languages: quick settings on the home page, display,
  performance, game folder and save backups, patches, mods (order, presets and conflicts),
  controls with PlayStation or Xbox button icons, updates and diagnostics.
- **In-game menu** (Insert on the keyboard, R3+L2 on the controller), with four tabs:
  - **Game:** cheats (infinite health, magic, item meter, Rage of Sparta, max red orbs);
    multipliers from 0.1x to 100x for red, green (health), blue (magic) and gold (Rage of
    Sparta) orbs and for damage dealt and taken; an enemy health bar.
  - **Image:** display mode (windowed, borderless, fullscreen), VSync, FSR 1 upscaling and
    RCAS sharpening.
  - **Performance:** frame rate limit (30 to unlimited), frames queued, background shader
    compiling; render resolution, engine frame rate and GPU readback options apply with one
    "Apply and restart" button.
  - **Overlay:** a performance overlay with FPS, frametime, upscaler, GPU (name and use), CPU,
    RAM and VRAM, each optional, with font size, corner, background opacity and layout. The
    numbers are the game's own use, not the whole system's.

## Known issues

- **240 FPS** is experimental.
- **Skip Intro** is not available: the community patch does not match this executable.
- With a large shader cache (about 10,000 shaders) the background loading takes about 3 minutes
  at low priority; until it ends, a new area may show an object or effect a moment late.
- Rarely, a start stops before the first frame (the game's intro loader). Starting again works.
- No temporal upscaling (FSR 3/4, DLSS or TAA) yet: the code is in the port but turned off until
  it is calibrated for God of War III.
- Tested only on Windows 11 with an AMD Radeon RX 6700 XT.

## Requirements

- Windows 10 (1903 or newer) or Windows 11, 64-bit.
- A graphics card with Vulkan 1.3 and a current driver.
- Your copy of the game: CUSA01623 with the 01.02 update applied (the folder with `eboot.bin`,
  `sce_module` and `sce_sys`).
- The patches are applied only to the `eboot.bin` they were checked against byte by byte
  (sha256 `d85c8135d330c3b601bf5dc3f1dd86bd6119fb8a9fce372bed2c9785f9a79299`), and the in-game
  hooks (cheats, multipliers, health bar) only where the code they change has the expected
  bytes. Otherwise the game starts without them, and the log and the menu say why.
- To build: [MSYS2](https://www.msys2.org) and [Python 3](https://www.python.org) with Pillow
  (`pip install pillow`, used for the game's icon and cover art).

## Building and playing

Development happens on the `gow3` branch; `main` receives tested merges.

1. Install MSYS2 (`winget install MSYS2.MSYS2`) and, in the **MSYS2 CLANG64** shell, the
   packages listed in [packaging/windows/README.md](packaging/windows/README.md).
2. Clone the repository with its submodules and build:

   ```bash
   git clone --recursive https://github.com/FelipePrado0/gow3_pc
   cd gow3_pc
   git checkout gow3   # the newest work; skip it to build main
   bash build.sh
   ```

3. Start the launcher (in PowerShell, in the project folder):

   ```powershell
   python launcher/gow3_launcher_win.py
   ```

4. On **Game**, choose the game folder. Adjust **Display**, **Performance** and **Patches** if
   you like, then press **PLAY**.

To play with the saved settings without opening the launcher's window (for a shortcut or Steam):

```powershell
python launcher/gow3_launcher_win.py --play
```

Or without the launcher at all:

```powershell
$env:GOW3_GAME_DIR = 'D:\path\CUSA01623'
python run.py
```

Settings are kept in `gow3.ini` (the game and the in-game menu) and in
`%APPDATA%\gow3-launcher\settings.json` (the launcher). Saves, the shader cache and
`last_run.log` go to the project's `user` folder.

### Keyboard controls

Defaults; change them on the launcher's **Controls** page.

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
| Tab | touchpad |
| Insert | the port's menu (R3+L2 on the controller) |

In the menu: L1/R1 or Q/E switch tabs. In the save list: arrows or the d-pad choose, Enter or
Cross confirm, Esc or Circle cancel.

## Tests

```bash
python -m unittest discover -s tests
bash build.sh --test
ninja -C out/gpu stat-multipliers-test perf-overlay-test warmup-inbox-test graphics-settings-test
out/gpu/stat-multipliers-test.exe out/eboot.elf
out/gpu/perf-overlay-test.exe
out/gpu/warmup-inbox-test.exe
out/gpu/graphics-settings-test.exe
```

The C++ tests are targets in [gpu/CMakeLists.txt](gpu/CMakeLists.txt); the ones that check the
game's code take the prepared executable (`out/eboot.elf`) as their argument.

To check that every system function the game uses is implemented, without starting the game:

```bash
out/gow3-probe.exe out/boot-linked.bin --app0 <game folder> --check-imports
```

## How it works

| Part | Where | What it does |
|---|---|---|
| Preparation | `scripts/` | Reads `eboot.bin`, applies its relocations and links the game's own `libc` and `libSceFios2` into one image |
| Loader | `src/probe.c` | Loads the image, installs the validated hooks (cheats, multipliers, health bar) and starts the game's original code |
| Runtime | `src/runtime_*.c` | Reimplements the PS4 system functions: memory, threads, files, audio, video, controller, saves and services |
| GPU | `gpu/shadps4/` | The shadPS4 video core, translating the PS4 GPU commands to Vulkan, with this port's speed work (background shader cache, deferred readbacks) |
| In-game menu | `gpu/shim/gow3_overlay.cpp` | The Dear ImGui menu and performance overlay; settings in `gpu/shim/gow3_settings.cpp` |
| Patches | `patches/God_of_War_III_Remastered.xml`, `scripts/patches.py` | Applied in memory at start, without changing the game files |
| Launcher | `launcher/gow3_launcher_win.py` | Settings, patches, mods, controls and the play button |

More documentation: [mods](docs/MODS.md), [frame map and upscaler](docs/upscaler.md),
[parallel GPU work](docs/parallel_gpu.md) and the [roadmap](docs/gow3/ROADMAP.md).

## Credits

Built on [bbport](https://github.com/deadinside28/bloodborne_pc), the native Bloodborne port by
deadinside28, on the [Windows port](https://github.com/Supermedo/bloodborne_pc) by Supermedo
(Mohammed Albarghouthi), and on the [shadPS4](https://github.com/shadps4-emu/shadPS4) renderer.

God of War III Remastered patches by kvicken and cuesta4 (building on the PS3 patches by
illusion0001), published for shadPS4 in
[shadps4-emu/ps4_cheats#145](https://github.com/shadps4-emu/ps4_cheats/pull/145) and adapted to
this executable. Cheats adapted from Celogamez's
[GoldHEN cheats](https://github.com/GoldHEN/GoldHEN_Cheat_Repository) for this game.

Also used: [LibAtrac9](https://github.com/Thealexbarney/LibAtrac9), [FFmpeg](https://ffmpeg.org),
[SDL3](https://github.com/libsdl-org/SDL), [Dear ImGui](https://github.com/ocornut/imgui),
[sirit](https://github.com/shadps4-emu/sirit), [magic_enum](https://github.com/Neargye/magic_enum),
[miniz](https://github.com/richgel999/miniz), [xbyak](https://github.com/herumi/xbyak),
[Vulkan Memory Allocator](https://github.com/GPUOpen-LibrariesAndSDKs/VulkanMemoryAllocator),
[FSR-Vulkan](https://github.com/FireBurn/FSR-Vulkan) and the AMD FidelityFX SDK,
[MSYS2](https://www.msys2.org), [LLVM](https://llvm.org) and [Pillow](https://python-pillow.org).

## License

GNU GPL v2 or later ([LICENSE](LICENSE)). Third-party components keep their own licenses.
