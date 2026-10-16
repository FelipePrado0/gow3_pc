# gow3 on macOS (experimental)

The port builds and runs on Macs with Apple Silicon from the same sources as on Linux. The game's
own code is x86-64 and runs inside the loader's process, so the whole process is x86-64 and runs
under Rosetta 2; the graphics go through the same shadPS4 video core to Vulkan, which Mesa's
KosmicKrisp driver implements on Metal.

**Status:** built and tested on an M4 Pro with macOS 27: `bash build.sh --test` (runtime tests and
a GPU start: window, Vulkan device, presenter) and `python3 -m unittest discover -s tests`
(including the loader tests with synthetic x86-64 images). The game itself has not been run on
macOS yet, so expect issues; send `user/last_run.log` with a report.

## Requirements

- A Mac with Apple Silicon (M1 or newer) and **macOS 26 (Tahoe) or newer**: KosmicKrisp needs
  both. Intel Macs are not supported.
- Rosetta 2: `softwareupdate --install-rosetta --agree-to-license`.
- The Xcode command line tools (`xcode-select --install`) and, from Homebrew,
  `brew install cmake ninja pkg-config`.
- Python 3 with Tk and Pillow for the launcher (`brew install python python-tk`, then
  `python3 -m pip install pillow`).
- Your own dump of the game (CUSA01623 or CUSA01715, versions 01.00 or 01.02), a folder with
  `eboot.bin`, `sce_module` and `sce_sys`.

## Build

```bash
git clone --recursive <this repository> && cd gow3_pc
bash build.sh            # first run: builds the x86-64 libraries (about 5 minutes)
bash build.sh --test     # optional: runtime tests and the GPU start test (opens a window briefly)
```

`build.sh` runs `packaging/macos/deps.sh` first. It downloads pinned sources (checked by SHA-256)
and builds, for x86-64, everything the port links into `out/macos-deps`: SDL3, FFmpeg, fmt, Zydis,
xxHash, mimalloc, the Khronos Vulkan loader, and header-only Boost, robin-map, VMA and
Vulkan-Headers. It also builds the host tools glslang and nasm. Homebrew's Intel bottles are no
longer offered on current macOS, so nothing comes from Homebrew except cmake, ninja and
pkg-config. The Vulkan driver, `libvulkan_kosmickrisp.dylib`, is taken from shadPS4's macOS
release (v0.19.0, the same video core); `build.sh` copies it to `out/vulkan/`. A package is built
again when its stamp in `out/macos-deps/stamps` is deleted; `GOW3_MACOS_DEPS` moves the folder.

## Play

```bash
./play-mac.sh                                         # finds the dump and starts the game
./play-mac.sh /path/to/CUSA01623 [/path/to/update]    # that folder (and an update dumped apart)
./play-mac.sh --launcher                              # the launcher: choose the game folder, then PLAY
GOW3_GAME_DIR=/path/to/CUSA01623 python3 run.py       # without the script (after build.sh)
```

`play-mac.sh` builds what changed (quietly once built; `out/play-build.log`), then looks for the
dump, a folder with `eboot.bin`, `sce_module` and `sce_sys`, in `~/Downloads`, `~/Desktop`,
`~/Documents`, `~/Games` and next to the repository, and for an update dumped apart from it next
to the game. It cannot read `.pkg` files and says so when it finds only those.

On a Mac keyboard the port's menu opens with **`** (the key left of 1); Insert and R3+L2 work
too. The first start takes longer: Rosetta translates the game's code and the shaders are compiled.
Saves, `gow3.ini`, the shader cache and `user/last_run.log` are in the repository folder, as on
Linux; the launcher keeps its own settings in `~/.config/gow3-launcher/settings.json`.
**Advanced → Desktop shortcut** puts `God of War III.app` on the desktop, which opens the launcher.

## Package

```bash
bash build.sh
bash packaging/macos/package.sh   # dist/gow3-macos.dmg and dist/God of War III.app
```

`package.sh` freezes the launcher with PyInstaller (a venv in `out/pyenv`; Python 3 with Tk, as
for the launcher, `PYTHON` overrides `python3`) into `God of War III.app`, so players need neither
Python nor a build. The application holds, in `Contents/Resources`, what a packaged Windows build
holds next to its executable: `run.py`, `scripts/`, `patches/` and `bin/` with `gow3-probe`,
`gow3-gpu-capabilities`, `gpu/libgow3gpu.dylib`, the libraries they load through their run paths
in `lib/` (SDL3, FFmpeg, the Vulkan loader) and KosmicKrisp in `vulkan/`. The executables lose
their debug information and their run paths into the checkout. The launcher runs on the Python's
architecture (arm64 on Apple Silicon); the game is x86-64 under Rosetta 2, as from the repository,
and `run.py` says how to install Rosetta when it is missing. The disk image holds the application,
a link to Applications, `README-macOS.txt` (for players) and the license. The script stops when
game files, saves, caches or settings would ship. `dist/God of War III.app` is a copy to play
from (not refreshed while it runs).

The packaged launcher keeps nothing inside the application: generated files, saves, the shader
cache and `gow3.ini` go to `~/Library/Application Support/gow3` (`GOW3_DATA_DIR` overrides), and
it offers no desktop shortcut (the application itself goes in the Dock).

A release attaches `dist/gow3-macos.dmg` to the GitHub release of this repository, beside the
Windows zip, with `VERSION` from `launcher/gow3_launcher_win.py`: the launcher's update check
compares that number and offers the `.dmg` asset (a release without one opens its page). Raise
`VERSION` before a new release.

The application is signed ad hoc (Apple Silicon runs only signed code), not with an Apple Developer
ID nor notarized, so macOS blocks a downloaded copy: players clear its quarantine once with
`xattr -dr com.apple.quarantine "/Applications/God of War III.app"` (`README-macOS.txt`), which
covers the executables inside it too. Tested on an M4 Pro (macOS 27): the game started from the mounted disk image
with a fresh data folder (MetalFX at 720p, no faults), and the preparation and the launcher ran
from the application without the checkout's Python.

## What is different from Linux

| Linux | macOS |
|---|---|
| `mov rax, fs:[0]` rewritten to `gs:[0]`, GS base set with `arch_prctl` | GS is the thread's pthread TSD array (Rosetta keeps it so) and cannot move. As on Windows, the loader rewrites the displacement: `gs:[8*key]`, the slot of a pthread key whose value is the thread's guest TCB. |
| One memfd for direct and flexible memory | A POSIX shared memory object; the host's own view of it lies above 1 TiB. |
| PS4 user range 64 GiB–1 TiB | On Apple Silicon, [63 GiB, 448 GiB) cannot be mapped (commpage, GPU carveout), so the user range is 448 GiB–1008 GiB, as in shadPS4's macOS build. It and the low range for the image and stacks (48–63 GiB) are reserved first thing in `main`; unmapping returns a range to the reservation. |
| `-no-pie`, glibc's brk heap keeps host objects below 1 TiB | macOS's allocator places heaps above 1 TiB. The loader links mimalloc as the process's malloc zone (`MI_NO_ALIGNED_HINT`): it takes memory with plain `mmap`, which macOS places bottom-up from 4 GiB, so every heap object (thread handles, mutexes, TLS, C++ objects) stays below 1 TiB. |
| The window on a thread of its own, the loader on the main thread | Cocoa windows and their events belong to the main thread: the loader runs on a second thread and the main thread hosts the window (`gow3gpu_main_loop`). |
| SIGSEGV for GPU page tracking | Protection faults arrive as SIGBUS; both go to the GPU's handler. Write faults are told apart by `__es.__err` bit 1, which Rosetta fills in. |
| CPUID lists AVX, F16C, BMI1, MOVBE, LZCNT | Rosetta runs them but lists them only for processes started with `ROSETTA_ADVERTISE_AVX=1`: the loader starts itself again with it set. |
| `nanosleep`, `clock_nanosleep`, monotonic condition variables, timed mutex/rwlock waits | Ordinary macOS timers fire up to a third of their length late (coalescing): sleeps use kqueue timers marked `NOTE_CRITICAL` (`compat_sleep_ns`). Semaphore deadlines use `CLOCK_REALTIME` and timed lock waits poll (`src/compat_mac.c`). |
| Vulkan loader and the system's drivers | The loader shipped in `out/macos-deps` and KosmicKrisp: `VK_DRIVER_FILES` points at `out/vulkan/kosmickrisp_mesa_icd.json` unless set. MoltenVK cannot run the video core: it lacks `robustBufferAccess2` and `nullDescriptor`. |
| `libgow3gpu.so` with `--allow-shlib-undefined` | `libgow3gpu.dylib` with `-undefined dynamic_lookup`; the libraries are found through run paths (`@executable_path/gpu`, `@executable_path/lib`, then the build folders). |

Overrides: `VK_DRIVER_FILES` (another Vulkan driver), `GOW3_VULKAN_LIBRARY` (another Vulkan
loader or library), `GOW3_LTO=OFF bash build.sh` (faster builds of the GPU library).

Not available on macOS: the Linux options (Wayland/X11, GameMode), DLSS, the graphics card and
stream buffer memory choices (one GPU sharing the system's memory), userfaultfd tracking
(`GOW3_UFFD`) and PGO.

The rest of what the Windows and Linux launchers offer is there, the Mac's way:

- **Advanced → macOS → Metal performance HUD**, in MangoHud's place: the launcher sets
  `MTL_HUD_ENABLED` (1 or 0) for the game, and Apple's HUD shows frame rate, frame times and memory
  on the game window (`libMTLHud` loads into `gow3-probe`; Command-F9 toggles it).
- **Advanced → Add to Steam** writes the shortcut and its pictures into
  `~/Library/Application Support/Steam/userdata/<account>/config` (the Mac client's process is
  `steam_osx`; close Steam first). The shortcut starts
  `God of War III.app/Contents/MacOS/God of War III --play`, or the source tree's launcher with
  `--play`. The pictures use Georgia Bold from `/System/Library/Fonts/Supplemental`.
- **Advanced → Check for updates** asks the latest release of this repository (asset
  `gow3-macos.dmg`) and, when it is newer than the launcher's `VERSION`, offers to download the
  disk image in the browser, or opens the release page when it has none; the new application replaces the old one by
  hand. The Windows build installs its zip in place instead.
- The God of War title font (`launcher/gowfont.ttf`) is registered for the launcher's process with
  Core Text (`CTFontManagerRegisterFontsForURL`), as GDI does on Windows.
- The performance overlay measures, as on Windows: GPU use (the whole GPU's, from the accelerator's
  `PerformanceStatistics` in IOKit, the figure Activity Monitor shows: macOS has no counter per
  process), the game's CPU time (`getrusage`) and memory (its physical footprint, Activity
  Monitor's "Memory"), and VRAM (`VK_EXT_memory_budget`, as KosmicKrisp reports it). The log has
  the first sample once: `Performance overlay: GPU …% (whole GPU), CPU …%, RAM … GB, VRAM … GB`.

## Fewer GPU waits (experimental)

Launcher: **Graphics → Upscaling → Fewer GPU waits** (on Windows and Linux: **Performance → GPU
data**); in the game's menu: **Performance → Applied on restart**;
`gow3.ini`: `fewer_gpu_waits=1` (`GOW3_FEWER_GPU_WAITS=1` overrides it at start). Off by default,
applied at the next start, and only with `stale_readback=1` (both controls are greyed out without
it). At every end-of-pipe fence the texture cache queues the readbacks of small
linear images; it first waited for every guest memory copy in flight (`Scheduler::WaitHostCopies`),
so the draw recording thread stopped until the Vulkan recording thread had drained, 12 to 14 times
a frame in God of War III's gameplay, mostly at fences that queue no readback. With stale readbacks
the watched pages keep read access and those copies only read guest memory, so the wait is skipped
(`texture_cache.h`, `readbacks_skip_copy_wait`). It applies on every system.

Measured on the M4 Pro (macOS 27) in the first fight after New Game, 720p with MetalFX, VSync off:
the draw recording thread waited 40% of the time before and 2% after, and the command processor
and draw recording threads went from 62% and 57% of a core to 28% and 23%. The frame rate stayed at
52 FPS: on this Mac the GPU side limits it. Instruments' Metal System Trace shows the GPU busy 83%
of the time and idle 2 to 4 ms at every frame start with the next command buffer already committed,
and KosmicKrisp creating and releasing about 30 Metal heaps of 128 KiB a frame. `frames_queued`
0, 1 or 2 and 480p change nothing measurable either.

## MetalFX upscaling

### Screen upscaling (MetalFX in FSR 1's place)

On a Mac, Apple's MetalFX (its spatial scaler) can enlarge the game's image to the window where
FSR 1 does it otherwise. With a render resolution below the window's it raises the frame rate:

- Launcher: **Graphics → Upscaling → Upscale to the window** on, **Upscaler → MetalFX (Apple)**,
  and **Render resolution → 720p** or **480p** on **Home** or **Performance** (applied at the next
  start; **Save** or **PLAY** keeps the change).
- In the game's menu (**`**): **Image → Upscale to the window** and **Upscaler**.
- `gow3.ini`: `fsr1=1`, `screen_upscaler=metalfx`, `render_resolution=720p`
  (`GOW3_SCREEN_UPSCALER=metalfx` overrides the file at start).

Nothing changes by default: upscaling stays off (`fsr1=0`) and its upscaler is FSR 1
(`screen_upscaler=fsr1`). MetalFX sharpens the image by itself, more than FSR 1 with RCAS at 75%,
so the sharpening setting (RCAS) applies to FSR 1 and to images the size of the window, not on top
of MetalFX. The performance overlay's upscaler item says what enlarges the image: `MetalFX
720p->1080p`, `FSR 1 720p->1080p` (also where MetalFX cannot run), `Native 1080p` when the image is
the window's size, or the temporal upscaler's name when one runs. Every render resolution smaller
than the window is enlarged, 480p as well as 720p (480p blows out bright areas in this game:
upstream issue #390, marked ⚠ in the launcher). With upscaling on and nothing to enlarge (Native on
a 1080p screen), the menu's Image tab says so under the upscaler and offers **Render at: 720p /
480p** (then **Apply and restart**), and the launcher's Graphics page shows the same note with
**720p** and **480p** buttons. The launcher's status line names the upscaler (`720p → MetalFX
(Apple)`). The log says `Screen upscaler: MetalFX spatial 1280x720 -> 1920x1080
(...)` when it starts. When MetalFX or the Vulkan/Metal interop below cannot run, the log says once
`Screen upscaler: MetalFX unavailable (<why>); using FSR 1` and FSR 1 enlarges the image; the
menu's Information rows show which one ran and why. On Windows and Linux a `screen_upscaler=metalfx`
in `gow3.ini` falls back the same way, and the launcher and the menu do not offer it.

How it works: the presenter copies the game's finished image (the display buffer, in the format the
game gave it: 8-bit sRGB or 10-bit) into an image whose memory Metal shares (as for the temporal
scaler below), submits the frame's commands so far, and MetalFX enlarges the copy on a Metal queue
of its own, in the same format and in its perceptual color mode, into a private texture that a
blit moves into the shared output. The frame's last submission (post processing, then the window)
waits for it through the same host relay (`host_passes/metalfx_pass.h`, `vk_metalfx.mm`).

Tested in the game on an M4 Pro (macOS 27), borderless 1920x1080, VSync off, the 120 FPS engine
patch, in the first fight on Gaia after New Game; GPU time per frame from the Metal performance
HUD's log (`MTL_HUD_ENABLED=1 MTL_HUD_LOG_ENABLED=1`), averaged over 15-second windows (a test
build switched between FSR 1 and MetalFX every 15 seconds in one run; native 1080p ran apart):

| Render resolution, upscaling | GPU time per frame | FPS |
|---|---|---|
| Native 1080p (RCAS at the window's size) | 30.9 ms | 32 |
| 720p, FSR 1 (with RCAS at 75%) | 18.9 ms | 52.7 |
| 720p, MetalFX | 18.9 ms | 52.0 |

MetalFX itself takes 0.30 ms of GPU time per frame for 1280x720 to 1920x1080 in that fight (about
1.4 ms in the menus, where the GPU's clock stays low); `GOW3_FRAME_STATS=1` prints it with the
relay's gaps, 0.09 ms before the upscale and 0.06 ms after it. With the relay a frame takes about
0.25 ms longer than with FSR 1. The image it presents matched, bit for bit, the same 720p frames
upscaled by MetalFX outside the game. HDR output with MetalFX was not tested.

### Temporal upscaling (experimental; waits for this game's frame mapping)

On a Mac the temporal upscaler can also be Apple's MetalFX (its temporal scaler). Like FSR 3.1,
FSR 4 and TAA it stays off unless the experimental switch is on. A Mac's launcher leaves out the
page with that switch (Graphics (experimental), see the status below) and starts the game with it
off, so it is tried without the launcher: `GOW3_EXPERIMENTAL_UPSCALER=1 GOW3_UPSCALER=metalfx
./play-mac.sh` (or `upscaler=metalfx` in `gow3.ini`). The preset and output resolution work as with FSR: Native AA is anti-aliasing at
1:1, Ultra Performance (3x) is MetalFX's largest scale on Apple GPUs. MetalFX has no sharpening
of its own; the upscaler sharpening setting runs the port's RCAS pass on its output, as for TAA.
The log says `Upscaler: MetalFX temporal <input> -> <output>` when it runs; when it cannot, it
says why and the game uses FSR 3.1 (`Upscaler: MetalFX unavailable (...)` or `... failed (...)`).

**Status:** in God of War III the temporal upscalers do not run yet on any system: the scene,
depth and camera detection they rely on is still the one of the Bloodborne port this one comes
from (`docs/gow3/ROADMAP.md`: kept in the code, off, until this game's frame is mapped), so
MetalFX, like FSR 3.1, changes nothing in the game for now: the temporal path waits for that
mapping, and until then MetalFX helps through the screen upscaling above. Its whole path was
checked inside the game on the game's own frames (a 1280x720 crop upscaled to 1920x1080, inputs
and output read back), and with tests outside it. FSR 3.1 does not start on KosmicKrisp (its
backend cannot create the context), so on a Mac falling back to FSR 3.1 means no temporal
upscaling.

How it works: MetalFX gets FSR 3.1's inputs (scene color, depth, the port's motion vectors and
jitter) and its output replaces the scene color the same way (`vk_metalfx.mm`). KosmicKrisp backs
every `VkDeviceMemory` with an `MTLHeap` and exports it (`VK_EXT_external_memory_metal`), so the
upscaler's images get memory of their own and a Metal texture is placed at the start of each
heap with the descriptor KosmicKrisp gives the image: Vulkan and Metal see the same pixels, with
no copy through the CPU. MetalFX writes only private textures, so a blit in the same Metal command
buffer moves its result into the shared output image. KosmicKrisp exports no `MTLSharedEvent`
(no `VK_EXT_metal_objects`), so the host relays the synchronization: the upscale is committed at
once behind a wait for an event of its own, a thread releases it when the Vulkan submission that
copied the inputs has completed, and its completion handler signals a Vulkan timeline semaphore
that the next submission waits for. `GOW3_FRAME_STATS=1` adds a line every 5 s with MetalFX's GPU
time and the relay's gaps.

Cost on an M4 Pro: outside the game, MetalFX takes 2.1 ms of GPU time at Native AA for a 1080p
output, 1.5 ms at Quality, 1.1 ms at Performance and 0.7 ms at Ultra Performance (4K output: 6.9,
5.7, 4.6 and 2.8 ms). In the game, upscaling 1280x720 to 1920x1080 took 1.7 ms on the GPU, the
relay about 0.4 ms before it and 0.06 ms after it; with the copies and the sharpening pass the
frame took about 2.5 to 3 ms longer. Under Rosetta, MetalFX logs an `appleneuralengine ... Bad
argument error` when it creates a scaler (an arm64 build does not); its output matched the arm64
output in a test outside the game (64 to 74 dB PSNR).
