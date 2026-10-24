# spartanport on Windows

The port builds and runs natively on 64-bit Windows 10 (1903+) / 11 with the same sources as
on Linux. Players: see `README-Windows.txt` (it ships in the package).

## Build

Install [MSYS2](https://www.msys2.org) (`winget install MSYS2.MSYS2`), open the
**MSYS2 CLANG64** shell and install the toolchain and libraries (the counterpart of
`shell.nix`):

```bash
pacman -S --needed git patch \
  mingw-w64-clang-x86_64-{toolchain,cmake,ninja,pkgconf,python} \
  mingw-w64-clang-x86_64-{vulkan-headers,vulkan-loader,vulkan-memory-allocator,glslang} \
  mingw-w64-clang-x86_64-{sdl3,fmt,boost,robin-map,xxhash,ffmpeg,zydis} \
  mingw-w64-clang-x86_64-{spirv-tools,spirv-headers,spirv-cross}
```

magic_enum, miniz and xbyak (not packaged by MSYS2) are submodules under `gpu/third_party/`.
The launcher is Rust and is built outside MSYS2: install [rustup](https://rustup.rs) with the
Visual Studio Build Tools (the `x86_64-pc-windows-msvc` target). The launcher build needs no Python.
Then, in the repository:

```bash
# MSYS2 CLANG64
git submodule update --init --recursive
bash build.sh                      # out/spartanport-probe.exe, out/spartanport-gpu-capabilities.exe
```

```powershell
# PowerShell
cd launcher-rust
cargo build --release --locked   # from inside launcher-rust/: it reads .cargo/config.toml (static CRT)
cd ..
```

```bash
# MSYS2 CLANG64
bash packaging/windows/package.sh  # dist/spartanport-windows/ and .zip: SpartanPort.exe + DLLs
```

To run the launcher from the source tree, in PowerShell at the repository root:
`cd launcher-rust; cargo run --release`.

Choose the game folder in the launcher. For platform-specific tests, see
[Tests in the root README](https://github.com/FelipePrado0/SpartanPort#tests): Windows uses Ninja
test targets; `bash build.sh --test` runs additional runtime tests only on Linux.

`SpartanPort.exe` is the Rust launcher (`launcher-rust/`). It also runs the game (`--run`, the
Windows `run.sh`, same variables), the preparation steps natively and the game without the window
(`--play`): players need no Python. It decodes instructions itself for CPUs without MOVBE, BMI1
or LZCNT. `--voice-swap` swaps voice lines (`docs/VOICE_REPLACEMENT.md`).
`Play SpartanPort.exe` (`packaging/windows/play.c`) is a small native program that runs
`SpartanPort.exe --play` from its folder and waits for it, for players who want to start the
game directly with the saved settings.
The runner identifies SpartanPort by title ID or title and finds built-in patch
sites by signature, rather than requiring one `eboot.bin` hash. Each unmatched patch is skipped
and reported; this is not a guarantee that every game build works. The startup black-screen
fix is always requested, including with patch lists saved by older launchers. The resolution
patch and texture fix are mutually exclusive. See the root README for tested builds.
From a source tree, the launcher adds `C:\msys64\clang64\bin` (or `$MSYS2_ROOT`) to `PATH` for DLLs.

FSR 1 and temporal upscaling (FSR 3.1, FSR 4, TAA and DLSS) are disabled in the current launcher
and game settings path. RCAS sharpening remains available. The DLSS bridge and FSR asset tools
remaining in the tree are experimental development code, not supported player setup steps.
See [the historical research notes](../../docs/upscaler.md) for context.

See the [feature availability table](https://github.com/FelipePrado0/SpartanPort#feature-availability)
for differences between published packages and current source builds, including Discord Rich Presence.

The launcher texts are written in English with the Russian beside them; the other languages
are in `launcher-rust/assets/i18n.json` (a missing or empty text falls back to English). The icon (`launcher/spartanport.ico`/`.png`) is made from the
SP monogram (`packaging/icon/monogram.png`) by `packaging/windows/make_icon.py` and built into `SpartanPort.exe`, `spartanport-probe.exe` (the game
window) and `spartanport-gpu-capabilities.exe`.

## What is different from Linux

| Linux | Windows |
|---|---|
| `mov rax, fs:[0]` rewritten to `gs:[0]`, GS base set with `arch_prctl` | GS is the TEB and cannot move. The loader rewrites the displacement to `gs:[0x1480 + 8*slot]` (the TEB's `TlsSlots[slot]`, same 9-byte instruction) and each guest thread keeps its TCB in that TLS slot. The slot is claimed first thing in `main` (Vulkan drivers take many). |
| One memfd for direct + flexible memory, `mmap(MAP_FIXED)` views | A pagefile-backed section; the PS4 range 64 GiB–1 TiB is reserved as one placeholder and views replace placeholders (`VirtualAlloc2`/`MapViewOfFile3`, as in shadPS4). Partial unmaps split views and restore page protections (GPU tracking). |
| `-no-pie`, heap below 1 TiB | `--disable-high-entropy-va`: heaps and thread stacks stay low and out of the PS4 range. |
| SIGSEGV handler, `sigsetjmp` recovery | Vectored exception handler; `spartanport_setjmp`/`spartanport_longjmp` (`src/compat_win.c`) restore all Win64 callee-saved registers without SEH unwinding (guest frames have no unwind data; clang's `__builtin_longjmp` restores a wrong frame pointer on Win64). |
| Guest thread stacks from `runtime_low_map` | Windows allocates them, fully committed (`pthread_attr_setstacksize`): guest code has no stack probes and would skip the guard page. The main thread stack is 16 MiB committed. |
| glibc `nanosleep`, monotonic condition variables | High-resolution waitable timers (`compat_sleep_ns`; winpthreads' sleep rounds to 15.6 ms); semaphore deadlines use `CLOCK_REALTIME` (winpthreads rejects monotonic condition variables). |
| `libspartanportgpu.so` | `libspartanportgpu.a`, linked into `spartanport-probe.exe` by CMake (`gpu/CMakeLists.txt`), with a manifest for UTF-8 paths and long paths. |
| `execlp bash run.sh` (in-game restart) | The launcher puts its command line in `SPARTAN_RESTART_COMMAND`; the new launch waits for the old process (`--after PID`). The launcher keeps the game in a job object. |
| Mod overlay with symlinks | Symlinks when allowed (Developer Mode), else junctions for folders and hard links for files. On a network share (SMB), which refuses junctions too, folders are mirrored with hard links so the game still reads real files through the view. |

Diagnostics added on the way: `SPARTAN_FRAME_DUMP_TRIGGER=<file>` writes the guest display buffer
and the presented image of the next frame to `SPARTAN_DUMP_DIR` (raw 32-bit pixels), which also
works for menus and movies.

Linux-specific features include watchdog/`--timeout` thread dumps (Linux signals), userfaultfd
tracking (`SPARTAN_UFFD`), MangoHud and PGO (GCC flags). Many Python and C/C++ tests run on Windows;
some tests still require Linux APIs. See the root README and `gpu/CMakeLists.txt` for test commands.
