# Packaged port: the prebuilt game binaries (build.sh first), the start-up scripts and the GTK4
# launcher, with their whole Nix closure and Mesa's Vulkan drivers. `bash packaging/appimage.sh`
# turns it into an AppImage (Steam Deck); `nix-build packaging` alone gives result/bin/gow3.
{ pkgs ? import <nixpkgs> { }
  # Store paths the prebuilt binaries load libraries from (their RUNPATHs), written by
  # appimage.sh. Nix only finds references to its inputs, and the binaries were built outside.
, runtimePaths ? (if builtins.pathExists ./runtime-paths.nix then import ./runtime-paths.nix else [ ])
}:
let
  lib = pkgs.lib;
  root = ./..;
  # FSR 4.1.1 models are extracted from AMD's DLLs: never in a public package. GOW3_PACKAGE_FSR411=1
  # (appimage.sh runs nix with --impure) bundles the local fsr4_411 for one's own devices.
  fsr411 = builtins.getEnv "GOW3_PACKAGE_FSR411" == "1";
  assetDirs = [ "scripts" "patches" "fsr4_shaders" "launcher" ] ++ lib.optional fsr411 "fsr4_411";
  # Only what the package needs (the tree also holds builds, profiles and captures).
  wanted = [
    "run.sh" "out" "out/gow3-probe" "out/gow3-gpu-capabilities" "out/gpu" "out/gpu/libgow3gpu.so"
  ] ++ assetDirs;
  src = builtins.path {
    name = "gow3-src";
    path = root;
    filter = path: type:
      let rel = lib.removePrefix (toString root + "/") (toString path);
      in builtins.elem rel wanted
        || lib.any (dir: lib.hasPrefix (dir + "/") rel) assetDirs;
  };
  python = pkgs.python3.withPackages (ps: [ ps.pygobject3 ]);
  # Mesa comes with the package. gow3_vulkan.py adds the host NVIDIA ICD and
  # only its vendor libraries (matching the host's kernel module).
  icds = lib.concatMapStringsSep ":" (name: "${pkgs.mesa}/share/vulkan/icd.d/${name}")
    [ "radeon_icd.x86_64.json" "intel_icd.x86_64.json" ];
  # Fonts: bundled DejaVu and Adwaita plus the host's usual font directories, but not the host's
  # /etc/fonts: on NixOS it names fonts in the host's /nix/store, which the AppImage hides behind
  # its own store in some environments (Steam's FHS sandbox), and the launcher showed boxes.
  fontsConf = pkgs.writeText "gow3-fonts.conf" ''
    <?xml version="1.0"?>
    <!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">
    <fontconfig>
      <dir>${pkgs.dejavu_fonts}/share/fonts</dir>
      <dir>${pkgs.adwaita-fonts}/share/fonts</dir>
      <dir>/usr/share/fonts</dir>
      <dir>/usr/local/share/fonts</dir>
      <dir prefix="xdg">fonts</dir>
      <cachedir prefix="xdg">gow3/fontconfig</cachedir>
      <include ignore_missing="yes">${pkgs.fontconfig.out}/etc/fonts/conf.d</include>
    </fontconfig>
  '';
  # Environment the closure needs on any host: icon themes, SVG icon loader, fonts (above) and
  # a UTF-8 locale built into glibc.
  common = ''
      --prefix XDG_DATA_DIRS : ${pkgs.mangohud}/share:${pkgs.adwaita-icon-theme}/share:${pkgs.hicolor-icon-theme}/share:${pkgs.gtk4}/share/gsettings-schemas/${pkgs.gtk4.name} \
      --set-default GDK_PIXBUF_MODULE_FILE ${pkgs.librsvg}/${pkgs.gdk-pixbuf.moduleDir}.cache \
      --set-default FONTCONFIG_FILE ${fontsConf} \
      --set-default LC_ALL C.UTF-8 \
      --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath [ pkgs.libglvnd pkgs.libx11 pkgs.libxext ]} \
      --set GOW3_VULKANINFO ${pkgs.vulkan-tools}/bin/vulkaninfo \
  '';
in
pkgs.stdenv.mkDerivation {
  pname = "gow3";
  version = "0.1";
  inherit src;
  nativeBuildInputs = [ pkgs.makeShellWrapper pkgs.wrapGAppsHook4 pkgs.gobject-introspection ];
  buildInputs = [ pkgs.gtk4 pkgs.libadwaita pkgs.adwaita-icon-theme pkgs.librsvg ]
    ++ map builtins.storePath runtimePaths;
  dontBuild = true;
  dontConfigure = true;
  # The binaries live under share/ (next to the scripts run.sh expects): strip them too, which
  # also drops the compiler and header paths their debug info would keep in the closure.
  stripDebugList = [ "share/gow3/bin/gpu" ];
  # patchelf (RPATH shrinking) corrupts the non-PIE game binary's symbol versions; it finds
  # its library through $ORIGIN/gpu and its other libraries through the build's RUNPATH.
  dontPatchELF = true;
  dontWrapGApps = true; # wrapped once below, together with the launcher's own variables
  installPhase = ''
    runHook preInstall
    d=$out/share/gow3
    mkdir -p $d/bin $out/bin
    cp run.sh $d/
    cp -r scripts patches fsr4_shaders launcher $d/
    # FSR 4.1.1 models only with GOW3_PACKAGE_FSR411=1 (see above); otherwise run.sh finds them in
    # the data directory (~/.local/share/gow3/fsr4_411).
    if [ -d fsr4_411 ]; then
      ${pkgs.python3}/bin/python3 - <<'PY'
    import sys
    from pathlib import Path
    sys.path.insert(0, 'launcher')
    from gow3_assets import fsr411_problem
    for output in ('1920x1080', '3840x2160'):
        for preset in (0, 4):
            problem = fsr411_problem(Path('fsr4_411'), output, preset)
            if problem:
                raise SystemExit(problem)
    PY
      cp -r fsr4_411 $d/
    fi
    install -m755 out/gow3-probe $d/bin/gow3-probe
    install -m755 out/gow3-gpu-capabilities $d/bin/gow3-gpu-capabilities
    install -Dm755 out/gpu/libgow3gpu.so $d/bin/gpu/libgow3gpu.so
    runHook postInstall
  '';
  postFixup = ''
    # bin/gow3 is a static program (gow3-entry.c) that clears the host's LD_PRELOAD (Steam's
    # overlay) and similar before the wrapper's own dynamic programs start.
    mkdir -p $out/libexec
    gcc -O2 -Wall -Werror -static -L${pkgs.glibc.static}/lib -DTARGET="\"$out/libexec/gow3\"" \
      ${./gow3-entry.c} -o $out/bin/gow3
    # The game (settings from the data directory's gow3.ini).
    makeShellWrapper ${pkgs.python3}/bin/python3 $out/libexec/gow3 \
      ${common}      --add-flags "$out/share/gow3/launcher/gow3_vulkan.py ${pkgs.bash}/bin/bash $out/share/gow3/run.sh" \
      --set GOW3_PREBUILT 1 \
      --set PYTHON ${pkgs.python3}/bin/python3 \
      --set GOW3_BUNDLED_VK_DRIVER_FILES ${icds} \
      --prefix PATH : ${lib.makeBinPath [ pkgs.bash pkgs.coreutils ]} \
      --run 'export GOW3_DATA_DIR=''${GOW3_DATA_DIR:-''${XDG_DATA_HOME:-$HOME/.local/share}/gow3}; mkdir -p "$GOW3_DATA_DIR"'
  '';
  meta.mainProgram = "gow3";
}
