God of War III Remastered for Mac (gow3_macos, experimental)
============================================================

No game files are included. You need your own copy of God of War III Remastered: CUSA01623 or
CUSA01715 (01.00 or 01.02), as a folder that contains eboot.bin, sce_module and sce_sys.

Requirements
- A Mac with Apple Silicon (M1 or newer) and macOS 26 (Tahoe) or newer. Intel Macs are not
  supported.
- Rosetta 2 (the game's own code is x86-64). If the log says the game needs it, install it in
  Terminal:
      softwareupdate --install-rosetta --agree-to-license
  Nothing else: Python, the libraries and the Vulkan driver (Mesa's KosmicKrisp) are inside the
  application.

Installing
1. Open gow3-macos.dmg and drag "God of War III" onto Applications.
2. The application is not signed with an Apple Developer ID, so macOS blocks it. Allow it once
   in Terminal:
      xattr -dr com.apple.quarantine "/Applications/God of War III.app"
3. Open "God of War III". On the Game page, click Browse... next to Game folder and select the
   folder with eboot.bin. An update dumped apart from the game goes in Update folder.
4. Back on Home, click PLAY. The first start takes longer: Rosetta translates the game's code and
   the shaders are compiled.

Patches and fixes are included and applied automatically: texture fix, skip videos with X and
120 FPS are on by default. Change the resolution and frame rate on Home or Performance. Save (at
the bottom) or PLAY keeps a change.

Faster settings (1080p screen): Home -> Render resolution 720p, and Graphics -> Upscaling ->
Upscale to the window with Upscaler MetalFX (Apple). Every render resolution smaller than the
screen is enlarged; 480p is a little faster still, but bright areas blow out at 480p (a known
bug). In the game, the performance overlay then shows "MetalFX 720p->1080p" ("Native 1080p":
nothing is enlarged).

While playing
- ` (the key left of 1) on the keyboard, or R3 + L2 on the controller, opens the port's menu:
  cheats, multipliers, image, performance and the performance overlay.
- Saved shaders load in the background; a small note in the corner shows the progress.
- Keyboard: WASD move, arrows camera, Space Cross, Left Shift Circle, E Square, Q Triangle,
  1/3 L1/R1, R/F L2/R2, Z/C L3/R3, I/K/J/L d-pad, Enter Options, Tab touchpad. Change them on
  the launcher's Game page (Controls).
- F12 saves a screenshot (Graphics -> Screenshots opens the folder).

Next time
- Start "God of War III" from Applications, Launchpad or the Dock.
- Advanced -> Add to Steam adds it to Steam (close Steam first); Steam then starts the game
  without the launcher.
- Advanced -> Check for updates looks for a newer gow3-macos.dmg and downloads it: drag the new
  application over the old one. Saves and settings are kept.
- Advanced -> macOS -> Metal performance HUD shows Apple's overlay (frame rate, frame times,
  memory) on the game window.

Data (kept when the application is replaced by a new version)
- Saves, shader cache, the log, gow3.ini and the prepared game image:
  ~/Library/Application Support/gow3 (Advanced -> Port folder opens it).
- Launcher options: ~/.config/gow3-launcher/settings.json.

Problems
- "God of War III is damaged" or "cannot be opened": run the xattr line of step 2 again.
- Importing shadPS4 saves: see "Importing shadPS4 saves" in the project's README:
  https://github.com/FelipePrado0/gow3_pc#importing-shadps4-saves
  Destination: ~/Library/Application Support/gow3/user/savedata/1/CUSA01623/, or
  <custom Saves folder>/savedata/1/CUSA01623/. RPCS3 saves are not compatible.
  Do not rename shadPS4's param.sfo to param.bin; the formats differ.
- Overlays: RTSS, Afterburner and ReShade do not exist for macOS; use the Metal performance HUD
  (above) or the port's own performance overlay.
- Black screen at start: Advanced -> Clear shader cache, then start again.
- The game does not open: check the Log page, and send
  ~/Library/Application Support/gow3/user/last_run.log with your report.

This project is not affiliated with Sony Interactive Entertainment or Santa Monica Studio.
God of War is a trademark of Sony Interactive Entertainment.
