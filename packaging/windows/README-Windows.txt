Bloodborne (gow3) for Windows
===============================

No game files are included. You need your own decrypted dump of Bloodborne CUSA03173
(the folder with eboot.bin, sce_module, sce_sys, dvdroot_ps4). Game version 1.09 is needed for
the community patches (60/90/unlocked FPS, resolution, effects); other versions run at 30 FPS.

Requirements
- Windows 10 (1903 or later) or Windows 11, 64-bit.
- A Vulkan 1.3 graphics card with a current driver.
- About 6 GB of free memory commit (RAM + page file), 10 GB for 1440p/4K output.
  Nothing else: Python and the libraries are inside this folder.

Starting
- Bloodborne.exe opens the launcher. On "Game & effects" pick your game folder, adjust the
  settings, press PLAY. The "Play" page checks the game version, saves, graphics card and
  FSR 4 assets.
- Play Bloodborne.exe starts the game straight away with the settings saved in the launcher,
  without opening it. Set things up once in Bloodborne.exe, then use Play Bloodborne.exe (or a
  shortcut to it, or add it to Steam with "Add a Non-Steam Game"). If no game folder is chosen
  yet it opens the launcher. Bloodborne.exe --play does the same. The log goes to
  user\last_run.log.
- Advanced -> "Desktop shortcut" puts Bloodborne on the desktop.
- Updates: when a new version is out, the launcher shows it at the bottom left; "Update"
  downloads and installs it and opens the launcher again (saves, settings and mods are kept).
  Advanced -> "Check for updates" checks by hand.
- Advanced -> "Launcher language": English, Russian, Arabic, Spanish, Portuguese, French,
  German, Italian, Polish, Turkish, Chinese, Japanese, Korean (default: the Windows language).
- In the game, Insert (or L3+R3 on a gamepad) opens the port's menu (upscaler, resolution,
  effects). Keyboard: WASD move, arrows camera, Space Cross, Left Shift Circle, E Square,
  Q Triangle, 1/3 L1/R1, R/F L2/R2, Z/C L3/R3, I/K/J/L d-pad, Enter Options, Tab touchpad.

Data
- Saves and shader caches: user\ next to Bloodborne.exe (the launcher can pick another folder).
- Settings: gow3.ini next to Bloodborne.exe; launcher options in %APPDATA%\gow3-launcher.
- Generated files (prepared game image, patches): out\.

Cheats
- The "Cheats" page has cheats (never die, enemies do not see or hear you, Rally never fades,
  control the targeted enemy) and gameplay tweaks (no Rally, camera further away, no camera
  auto-rotation, run with less stick tilt, ragdoll physics). They are game patches for 1.09,
  applied when the game starts.

Problems
- Black screen at start: Advanced -> "Clear shader cache", then start again (the first minutes
  stutter while the cache is rebuilt).
- Send user\last_run.log with any bug report.

Upscaling
- DLSS: NVIDIA GeForce RTX 20 series or newer with a current driver (Graphics -> Upscaler ->
  DLSS, or the in-game menu). On other GPUs the option is greyed out.
- FSR 3.1 works on every GPU. FSR 4 needs its assets in fsr4_shaders\ (included in this
  package, or Graphics -> "Download FSR 4 assets"); GPUs without the required shader features
  fall back to FSR 3.1 by themselves.

Mods and patches
- Put each mod in its own folder under mods\ (dvdroot_ps4\..., or chr\, parts\, ... directly);
  enable and order them on "Mods & patches". The game files are never changed. Without Windows
  Developer Mode the port links folders with junctions and files with hard links; when the game
  is on another drive, the temporary mod view is made next to the game folder.
- Third-party patches: shadPS4/GoldHEN XML files for 1.09 in patches\.

Credits
- gow3 (the Linux port this is built on): https://github.com/deadinside28/bloodborne_pc
- Windows port: https://github.com/Supermedo/bloodborne_pc
- The full list of projects and patch authors is in README.md (Credits and licenses).
