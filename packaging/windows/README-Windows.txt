God of War III (gow3_pc) for Windows
====================================

No game files are included. You need your own decrypted dump of God of War III Remastered
CUSA01623 (the folder with eboot.bin, sce_module, sce_sys). Game version 01.02 is needed for the
community patches (resolution, 120 FPS, texture fix); other versions run unpatched.

Requirements
- Windows 10 (1903 or later) or Windows 11, 64-bit.
- A Vulkan 1.3 graphics card with a current driver.
- About 6 GB of free memory commit (RAM + page file), 10 GB for 1440p/4K.
  Nothing else: Python and the libraries are inside this folder.

Starting
- God of War III.exe opens the launcher. On "Game & effects" pick your game folder, choose the
  patches on "Game patches", press PLAY. The "Play" page checks the game version, saves and
  graphics card.
- Play God of War III.exe starts the game straight away with the settings saved in the
  launcher, without opening it. Set things up once in God of War III.exe, then use
  Play God of War III.exe (or a shortcut to it, or add it to Steam with "Add a Non-Steam Game").
  If no game folder is chosen yet it opens the launcher. God of War III.exe --play does the
  same. The log goes to user\last_run.log.
- Advanced -> "Desktop shortcut" puts the game on the desktop.
- Advanced -> "Launcher language": English, Russian, Arabic, Spanish, Portuguese, French,
  German, Italian, Polish, Turkish, Chinese, Japanese, Korean (default: the Windows language).
- In the game, Insert (or R3+L2 on a gamepad) opens the port's menu. Keyboard: WASD move,
  arrows camera, Space Cross, Left Shift Circle, E Square, Q Triangle, 1/3 L1/R1, R/F L2/R2,
  Z/C L3/R3, I/K/J/L d-pad, Enter Options, Tab touchpad.

Data
- Saves and shader caches: user\ next to God of War III.exe (the launcher can pick another
  folder).
- Settings: gow3.ini next to God of War III.exe; launcher options in %APPDATA%\gow3-launcher.
- Generated files (prepared game image, patches): out\.

Problems
- Black screen at start: Advanced -> "Clear shader cache", then start again (the first minutes
  stutter while the cache is rebuilt).
- Send user\last_run.log with any bug report.

Mods and patches
- Put each mod in its own folder under mods\ (with dvdroot_ps4\...); enable and order them on
  "Mods & patches". The game files are never changed. Without Windows Developer Mode the port
  links folders with junctions and files with hard links; when the game is on another drive,
  the temporary mod view is made next to the game folder.
- Third-party patches: shadPS4/GoldHEN XML files for the game version in patches\.

Credits
- bbport (the Linux port this is built on): https://github.com/deadinside28/bloodborne_pc
- Windows port: https://github.com/Supermedo/bloodborne_pc
- The full list of projects and patch authors is in README.md (Credits and licenses).
