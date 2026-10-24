SpartanPort for PC (SpartanPort)
==========================================

No game files are included. You need your own copy of the game (CUSA01623) as a folder
containing eboot.bin, sce_module and sce_sys. Tested title IDs: CUSA01623 and CUSA01715;
versions 01.00 and 01.02. Other builds may work, but are not guaranteed.

Requirements
- Windows 10 (1903 or newer) or Windows 11, 64-bit.
- A 64-bit processor with AVX: Intel Sandy Bridge (2011) or newer, AMD FX or newer. Sandy/Ivy
  Bridge and AMD FX run with emulated instructions (an "ISA:" log line; speed not measured yet).
  Older processors stop at start with a "STOP: this CPU lacks" line in the log.
- A graphics card with Vulkan 1.3 and an up-to-date driver.
  Nothing else: no Python is needed, and the libraries are inside this folder.

Starting
1. Extract this folder anywhere, for example C:\Games\spartanport-windows (not inside the zip).
2. Open "SpartanPort.exe". If Windows shows "Windows protected your PC", click More info,
   then Run anyway: the program is not digitally signed.
3. Click "Choose the game folder", then Browse... next to Game folder, and select the folder
   with eboot.bin.
4. Back on Home, adjust the quick settings if you like, then click PLAY. The first start takes
   a little longer while the game image is prepared.

Patches and fixes are included and applied automatically: texture fix, skip videos with X and
120 FPS are on by default. Change the resolution and frame rate on Home or Performance.
The startup black-screen fix is always requested, even with older saved patch lists. Each
patch applies only where its signature matches; a resolution patch replaces the texture fix.
The launcher has Home, Graphics, Performance, Game and Advanced pages. Click Save to keep
pending changes before playing. FSR 1 and temporal upscalers are disabled; RCAS remains available.

While playing
- Insert on the keyboard, or R3 + L2 on the controller, opens the port's menu: cheats,
  multipliers, image, performance and the performance overlay.
- Saved pipelines load and compile in the background; a small note shows the progress.
  Stutters, missing effects during compilation and temporary FPS drops can still occur.
- Keyboard: WASD move, arrows camera, Space Cross, Left Shift Circle, E Square, Q Triangle,
  1/3 L1/R1, R/F L2/R2, Z/C L3/R3, I/K/J/L d-pad, Enter Options, Tab photo mode and Backspace
  pause menu. Change them under Game -> Controls. On controllers without a touchpad, View opens
  pause and L1 + View opens photo mode.
- Game -> Controls -> In-game button icons: Original (default), Match my controller, Xbox,
  Nintendo or Keyboard (your keyboard mapping's keys; also Match my controller without a gamepad).
  Generated copies do not change the game folder. Changing controllers needs a restart.

Next time
- "Play SpartanPort.exe" starts the game straight away with the saved settings. Make a
  desktop shortcut to it (Advanced -> Desktop shortcut), or add it to Steam with "Add a
  Non-Steam Game".
- Advanced -> Check for updates downloads and installs a new version. Saves and settings are
  kept.

Data
- Saves, shader cache and the log: user\ next to SpartanPort.exe (keep it when updating).
- Settings: spartanport.ini next to SpartanPort.exe; launcher options in %APPDATA%\spartanport-launcher.
- Generated files (prepared game image, patches): out\.

Problems
- Importing shadPS4 saves: see "Importing shadPS4 saves" in the project's README:
  https://github.com/FelipePrado0/SpartanPort#importing-shadps4-saves
  Destination: user\savedata\1\CUSA01623\ next to SpartanPort.exe, or
  <custom Saves folder>\savedata\1\CUSA01623\. RPCS3 saves are not compatible.
  Do not rename shadPS4's param.sfo to param.bin; the formats differ.
  The save dialog displays at most 32 entries. If the imported slot is missing, test
  with a separate Saves folder containing only that slot; keep the original saves.
- RTSS/Afterburner and ReShade must target bin\spartanport-probe.exe, not the launcher.
  Select Vulkan in the ReShade installer. Report the overlay versions if it still fails.
- Black screen at start: use the latest release, which includes the startup deadlock patch.
  If it persists, report the GPU, driver, game build and Advanced -> Log output. Clearing the
  shader cache is not a general fix and requires rebuilding it on the next launch.
- Yellow/orange/white textures or wrong lighting: try disabling Performance -> GPU data ->
  Deferred GPU readbacks and restart. This workaround can cause stutters.
- Memory error (error 1450): increase the Windows page file. Win+R ->
  SystemPropertiesAdvanced -> Performance Settings -> Advanced -> Virtual memory Change ->
  set C: to 16384 MB initial, 24576 MB maximum -> Set -> OK, then restart your PC.
- The game does not open: check Advanced -> Log, and send user\last_run.log with your report.
  With a custom Saves folder, last_run.log is in that folder instead.
- Other known GPU, audio, crash and progress issues are tracked in the release notes and issues:
  https://github.com/FelipePrado0/SpartanPort/releases/latest
  https://github.com/FelipePrado0/SpartanPort/issues

Documentation and other platforms
- This text is maintained with the source; changes reach packaged copies in the next build.
- The main README's Contents links work inside this package. Links to other documentation and
  translated READMEs open the online repository and need an internet connection.
- For differences between published packages and current source builds, including Discord
  Rich Presence, see the feature availability table:
  https://github.com/FelipePrado0/SpartanPort#feature-availability
- An experimental Linux AppImage is also available on the Releases page. It requires Linux
  x86-64, glibc 2.39 or newer, AVX and Vulkan 1.3; see the project's README for instructions.

This project is not affiliated with Sony Interactive Entertainment or Santa Monica Studio.
God of War is a trademark of Sony Interactive Entertainment.
