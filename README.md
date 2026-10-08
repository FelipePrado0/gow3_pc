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
input, and gameplay runs at 60 FPS at 1080p on an AMD Radeon RX 6700 XT. There is no ready-made
download yet: for now the project is built from source. Progress is tracked (in Portuguese) in
[docs/gow3/ROADMAP.md](docs/gow3/ROADMAP.md).

## What works

- Opening, menus, video cutscenes (H.264) and gameplay.
- Audio: music, voices and effects (ATRAC9 and MP3).
- Controller (DualSense and others, through SDL3) and keyboard at the same time.
- Saving and loading: the save list is shown on screen, as on the PS4. Autosave works too.
- Community patches, chosen in the launcher before playing:
  - resolution of 480p, 720p, 1440p, 1800p or 4K;
  - corrupted texture fix;
  - 120 FPS;
  - skip videos with the X button.
- A launcher with the God of War III identity (the game's own banner and icon), in 13 languages
  (the game patches page is in English, Portuguese and Russian).
- An in-game settings menu (Insert or L3+R3).

## Known issues

- **120 FPS:** the game caps each frame's time step at 1/target FPS. Below 120 FPS it runs in
  slow motion (the PS4 does the same below 60). It is only worth it where the PC holds 120.
- **Skip Intro** is not available: the community patch does not match this executable.
- The first visit to each area can stutter while shaders compile. Starting the game can take
  more than a minute when the shader cache is large.
- No temporal upscaling (FSR, DLSS or TAA) yet: the code is in the port but turned off until it
  is calibrated for God of War III.
- Tested only on Windows 11 with an AMD Radeon RX 6700 XT.

## Requirements

- Windows 10 (1903 or newer) or Windows 11, 64-bit.
- A graphics card with Vulkan 1.3 and a current driver.
- Your copy of the game: CUSA01623 with the 01.02 update applied (the folder with `eboot.bin`,
  `sce_module` and `sce_sys`).
- The patches are applied only to the `eboot.bin` they were checked against byte by byte
  (sha256 `d85c8135d330c3b601bf5dc3f1dd86bd6119fb8a9fce372bed2c9785f9a79299`). With another
  executable the game starts without patches, and the log says why.
- To build: [MSYS2](https://www.msys2.org) and [Python 3](https://www.python.org) with Pillow
  (`pip install pillow`, used for the game's icon).

## Building and playing

1. Install MSYS2 (`winget install MSYS2.MSYS2`) and, in the **MSYS2 CLANG64** shell, the
   packages listed in [packaging/windows/README.md](packaging/windows/README.md).
2. Clone the repository with its submodules and build. God of War III is on the main branch
   (`main`): there is no need to switch branches.

   ```bash
   git clone --recursive https://github.com/FelipePrado0/gow3_pc
   cd gow3_pc
   bash build.sh
   ```

3. Start the launcher (in PowerShell, in the project folder):

   ```powershell
   python launcher/gow3_launcher_win.py
   ```

4. On **Game & effects**, choose the game folder. On **Game patches**, choose the resolution and
   the patches. Press **PLAY**.

You can also play without the launcher:

```powershell
$env:GOW3_GAME_DIR = 'D:\path\CUSA01623'
python run.py
```

Saves, the shader cache and the log go to the project's `user` folder.

### Keyboard controls

| Key | Button |
|---|---|
| WASD | left stick |
| Arrows | right stick (camera) |
| Space | Cross (X) |
| Left Shift | Circle |
| E | Square |
| Q | Triangle |
| 1 and 3 | L1 and R1 |
| R and F | L2 and R2 |
| Z and C | L3 and R3 |
| I, K, J, L | d-pad |
| Enter | Options |
| Tab | touchpad |
| Insert | the port's settings menu |

In the save list: arrows or the d-pad choose, Enter or Cross confirm, Esc or Circle cancel.

## Tests

```bash
python -m unittest discover -s tests
ninja -C out/gpu mp3-test videodec-test services-test
out/gpu/mp3-test.exe tests/data/sine.mp3
out/gpu/videodec-test.exe tests/data/tiny.h264
out/gpu/services-test.exe
```

To check that every system function the game uses is implemented, without starting the game:

```bash
out/gow3-probe.exe out/boot-linked.bin --app0 <game folder> --check-imports
```

## How it works

| Part | Where | What it does |
|---|---|---|
| Preparation | `scripts/` | Reads `eboot.bin`, applies its relocations and links the game's own `libc` and `libSceFios2` into one image |
| Loader | `src/probe.c` | Loads the image into memory and starts the game's original code |
| Runtime | `src/runtime_*.c` | Reimplements the PS4 system functions: memory, threads, files, audio, video, controller, saves and services |
| GPU | `gpu/` | The shadPS4 video core, translating the PS4 GPU commands to Vulkan |
| Patches | `patches/God_of_War_III_Remastered.xml`, `scripts/patches.py` | Applied in memory at start, without changing the game files |
| Launcher | `launcher/gow3_launcher_win.py` | Settings, patches and the play button |

The temporal upscalers (FSR 3/4, DLSS, TAA) and the motion vectors inherited from the base port
stay in the code, turned off, until they are calibrated for God of War III (see the roadmap).

## Credits

Built on [bbport](https://github.com/deadinside28/bloodborne_pc), the native Bloodborne port by
deadinside28, on the [Windows port](https://github.com/Supermedo/bloodborne_pc) by Supermedo
(Mohammed Albarghouthi), and on the [shadPS4](https://github.com/shadps4-emu/shadPS4) renderer.

God of War III Remastered patches by kvicken and cuesta4 (building on the PS3 patches by
illusion0001), published for shadPS4 in
[shadps4-emu/ps4_cheats#145](https://github.com/shadps4-emu/ps4_cheats/pull/145) and adapted to
this executable.

Also used: [LibAtrac9](https://github.com/Thealexbarney/LibAtrac9), [FFmpeg](https://ffmpeg.org),
[SDL3](https://github.com/libsdl-org/SDL), [Dear ImGui](https://github.com/ocornut/imgui),
[sirit](https://github.com/shadps4-emu/sirit), [magic_enum](https://github.com/Neargye/magic_enum),
[miniz](https://github.com/richgel999/miniz), [xbyak](https://github.com/herumi/xbyak),
[Vulkan Memory Allocator](https://github.com/GPUOpen-LibrariesAndSDKs/VulkanMemoryAllocator),
[FSR-Vulkan](https://github.com/FireBurn/FSR-Vulkan) and the AMD FidelityFX SDK,
[MSYS2](https://www.msys2.org), [LLVM](https://llvm.org) and [Pillow](https://python-pillow.org).

## License

GNU GPL v2 or later ([LICENSE](LICENSE)). Third-party components keep their own licenses.
