#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "$0")"
if [[ ${1:-} == --software ]]; then
    shift
    if [[ -z ${VK_DRIVER_FILES:-} ]]; then
        for candidate in /run/opengl-driver/share/vulkan/icd.d/lvp_icd*.json /usr/share/vulkan/icd.d/lvp_icd*.json; do
            if [[ -f $candidate ]]; then export VK_DRIVER_FILES=$candidate; break; fi
        done
    fi
    if [[ -z ${VK_DRIVER_FILES:-} ]]; then echo 'Lavapipe not found; set VK_DRIVER_FILES.' >&2; exit 1; fi
    export VK_LOADER_LAYERS_DISABLE='~implicit~'
fi
# SPARTAN_DATA_DIR: writable directory for the generated files (out/), saves (user/) and spartanport.ini;
# by default this directory. The preparation itself is done by the launcher exe (--run).
data=${SPARTAN_DATA_DIR:-.}
if [[ -z ${SPARTAN_IN_NIX_SHELL:-} ]] && ! { command -v pkg-config >/dev/null && pkg-config --exists vulkan sdl3; } && command -v nix-shell >/dev/null; then
    args=''; if (( $# )); then args=$(printf '%q ' "$@"); fi
    exec env SPARTAN_IN_NIX_SHELL=1 nix-shell shell.nix --run "bash run.sh $args"
fi
# FSR 4: faster post passes next to the downloaded ones (incremental; tools/fsr4_optimize.sh).
if [[ -d fsr4_shaders ]] && command -v spirv-cross >/dev/null; then
    bash tools/fsr4_optimize.sh || echo 'FSR 4: optimized post passes not built' >&2
fi
bash build.sh
# cargo reads .cargo/config.toml from the working directory, so the launcher builds inside its crate.
# Always built: cargo does nothing when nothing changed, so a git pull never runs a stale launcher.
(cd launcher-rust && cargo build --release)
export SPARTAN_DATA_DIR=$data
exec launcher-rust/target/release/spartanport-launcher --run "$@"
