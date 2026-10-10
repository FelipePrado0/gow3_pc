God of War III Remastered for PC (gow3_pc)
==========================================

No game files are included. You need your own copy of God of War III Remastered: CUSA01623 with
the 01.02 update, as a folder that contains eboot.bin, sce_module and sce_sys.

Requirements
- Windows 10 (1903 or newer) or Windows 11, 64-bit.
- A graphics card with Vulkan 1.3 and an up-to-date driver.
  Nothing else: Python and the libraries are inside this folder.

Starting
1. Extract this folder anywhere, for example C:\Games\gow3-windows (not inside the zip).
2. Open "God of War III.exe". If Windows shows "Windows protected your PC", click More info,
   then Run anyway: the program is not digitally signed.
3. Click "Choose the game folder", then Browse... next to Game folder, and select the folder
   with eboot.bin.
4. Back on Home, adjust the quick settings if you like, then click PLAY. The first start takes
   a little longer while the game image is prepared.

Patches and fixes are included and applied automatically: texture fix, skip videos with X and
120 FPS are on by default. Change the resolution and frame rate on Home or Performance.

While playing
- Insert on the keyboard, or R3 + L2 on the controller, opens the port's menu: cheats,
  multipliers, image, performance and the performance overlay.
- Saved shaders load in the background; a small note in the corner shows the progress.
- Keyboard: WASD move, arrows camera, Space Cross, Left Shift Circle, E Square, Q Triangle,
  1/3 L1/R1, R/F L2/R2, Z/C L3/R3, I/K/J/L d-pad, Enter Options, Tab touchpad. Change them on
  the launcher's Controls page.

Next time
- "Play God of War III.exe" starts the game straight away with the saved settings. Make a
  desktop shortcut to it (Advanced -> Desktop shortcut), or add it to Steam with "Add a
  Non-Steam Game".
- Advanced -> Check for updates downloads and installs a new version. Saves and settings are
  kept.

Data
- Saves, shader cache and the log: user\ next to God of War III.exe (keep it when updating).
- Settings: gow3.ini next to God of War III.exe; launcher options in %APPDATA%\gow3-launcher.
- Generated files (prepared game image, patches): out\.

Problems
- Black screen at start: Advanced -> Clear shader cache, then start again.
- The game does not open: check the Log page, and send user\last_run.log with your report.

This project is not affiliated with Sony Interactive Entertainment or Santa Monica Studio.
God of War is a trademark of Sony Interactive Entertainment.
