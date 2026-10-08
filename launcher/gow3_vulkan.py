#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Select Vulkan ICDs for the packaged port before starting GTK or the game.

Mesa comes with the package. NVIDIA's userspace driver must come from the host,
matching its kernel module. Expose only NVIDIA libraries, rather than the host's
whole library directory (which would also replace the package's libc/GTK/SDL).
"""

import hashlib
import json
import os
from pathlib import Path
import sys

MANIFEST_DIRS = (
    Path("/etc/vulkan/icd.d"), Path("/usr/share/vulkan/icd.d"),
    Path("/usr/local/share/vulkan/icd.d"), Path("/run/opengl-driver/share/vulkan/icd.d"),
)
LIBRARY_DIRS = (
    Path("/usr/lib/x86_64-linux-gnu"), Path("/lib/x86_64-linux-gnu"),
    Path("/usr/lib64"), Path("/lib64"), Path("/usr/lib"), Path("/lib"),
    Path("/run/opengl-driver/lib"),
)


def elf64(path):
    """Reject 32-bit ICDs in distributions that install both architectures."""
    try:
        with path.open("rb") as stream:
            header = stream.read(20)
        return header[:6] == b"\x7fELF\x02\x01" and header[18:20] == b"\x3e\x00"
    except OSError:
        return False


def host_nvidia(manifest_dirs, library_dirs):
    for directory in manifest_dirs:
        for manifest in sorted(directory.glob("*nvidia*.json")):
            try:
                data = json.loads(manifest.read_text())
                library = Path(data["ICD"]["library_path"])
            except (OSError, ValueError, KeyError, TypeError):
                continue
            if "nvidia" not in library.name.lower():
                continue
            if library.is_absolute():
                candidates = [library, *(d / library.name for d in library_dirs)]
            elif len(library.parts) > 1:
                candidates = [manifest.parent / library]
            else:
                candidates = [d / library for d in library_dirs]
            for candidate in candidates:
                if elf64(candidate):
                    return data, candidate.resolve()
    return None


def configure(env, manifest_dirs=MANIFEST_DIRS, library_dirs=LIBRARY_DIRS):
    # Both overrides belong to the user; VK_DRIVER_FILES takes precedence over
    # VK_ICD_FILENAMES. In particular don't replace an explicit Lavapipe setup.
    if env.get("VK_DRIVER_FILES") or env.get("VK_ICD_FILENAMES"):
        return "Vulkan: using explicit driver override"
    bundled = env.get("GOW3_BUNDLED_VK_DRIVER_FILES", "")
    if env.get("VK_ADD_DRIVER_FILES"):
        bundled = env["VK_ADD_DRIVER_FILES"] + (":" + bundled if bundled else "")
    extra = env.get("GOW3_NVIDIA_LIB_DIR")
    if extra:
        library_dirs = (Path(extra), *library_dirs)
    found = host_nvidia(manifest_dirs, library_dirs)
    if not found:
        if bundled:
            env["VK_DRIVER_FILES"] = bundled
        return "Vulkan: bundled AMD/Intel drivers; no accessible NVIDIA ICD found"

    data, driver = found
    # The driver version/path and mtime give each installed driver its own small
    # cache. exec preserves process/signal behavior; the cache survives launch.
    key = hashlib.sha256(f"{driver}:{driver.stat().st_mtime_ns}".encode()).hexdigest()[:16]
    data_dir = Path(env.get("GOW3_DATA_DIR") or
                    str(Path(env.get("XDG_DATA_HOME", str(Path.home() / ".local/share"))) / "gow3"))
    cache = data_dir / "vulkan" / "nvidia" / key
    libraries = cache / "lib"
    libraries.mkdir(parents=True, exist_ok=True)
    # NVIDIA dlopens versioned shader/compiler libraries at runtime, so expose
    # the driver's siblings as well as its ICD. Never link generic host libs.
    sources = [driver]
    for pattern in ("libnvidia-*.so*", "libGLX_nvidia.so*", "libEGL_nvidia.so*"):
        sources.extend(sorted(driver.parent.glob(pattern)))
    for source in sources:
        if not elf64(source):
            continue
        link = libraries / source.name
        try:
            link.symlink_to(source.resolve())
        except FileExistsError:
            pass
    # Keep the manifest's API/architecture metadata, replace its library path.
    data["ICD"]["library_path"] = str(libraries / driver.name)
    manifest = cache / "nvidia_icd.json"
    temporary = cache / f"nvidia_icd.{os.getpid()}.tmp"
    temporary.write_text(json.dumps(data, indent=2) + "\n")
    temporary.replace(manifest)
    env["VK_DRIVER_FILES"] = str(manifest) + (":" + bundled if bundled else "")
    env["LD_LIBRARY_PATH"] = str(libraries) + (
        ":" + env["LD_LIBRARY_PATH"] if env.get("LD_LIBRARY_PATH") else "")
    return f"Vulkan: host NVIDIA driver {driver}; bundled AMD/Intel also available"


def main():
    if len(sys.argv) < 2:
        print("Usage: gow3_vulkan.py <program> [args...]", file=sys.stderr)
        return 1
    try:
        print(configure(os.environ), file=sys.stderr, flush=True)
    except OSError as error:
        # Startup diagnostics should identify driver/cache failures rather than
        # falling through to an unrelated Vulkan assertion.
        print(f"gow3: cannot prepare NVIDIA driver: {error}", file=sys.stderr)
        return 1
    if "--vulkan-info" in sys.argv[1:]:
        tool = os.environ.get("GOW3_VULKANINFO", "vulkaninfo")
        os.environ.setdefault("VK_LOADER_DEBUG", "error,warn,driver")
        for key in ("VK_DRIVER_FILES", "VK_ICD_FILENAMES", "LD_LIBRARY_PATH"):
            print(f"{key}={os.environ.get(key, '')}", file=sys.stderr, flush=True)
        os.execvpe(tool, [tool, "--summary"], os.environ)
    os.execvpe(sys.argv[1], sys.argv[1:], os.environ)


if __name__ == "__main__":
    sys.exit(main())
