#!/usr/bin/env bash
# Downloads the FSR 4 v07 INT8/DOT4 asset set (SPIR-V passes, model initializers, pre-pass
# weights) used by the FSR-Vulkan provider into fsr4_shaders/. The assets were built by
# Q2RTX from AMD's MIT-licensed FidelityFX SDK FSR 4 source (see LICENSE-FSR4-v07.txt);
# they are not part of this repository. Output 1920x1080 needs the 1080 tier, 1440p and 2160p
# outputs (menu: output resolution) the 2160 tier; GOW3_FSR4_TIERS selects them.
set -euo pipefail
cd -- "$(dirname -- "$0")/.."
commit=ae8d628fae208813172446d1e49ed94150b04658
base="https://raw.githubusercontent.com/FireBurn/Q2RTX/$commit/baseq2/fsr4_shaders"
dest=fsr4_shaders
mkdir -p "$dest"
files=(LICENSE-FSR4-v07.txt rcas.spv spd_auto_exposure.spv)
for model in native quality balanced performance ultraperf drs; do
    files+=("fsr4_model_v07_i8_${model}_initializers.bin"
            "fsr4_model_v07_i8_${model}_pre_weights.bin"
            "fsr4_model_v07_i8_${model}_shader_manifest.json")
    for tier in ${GOW3_FSR4_TIERS:-1080 2160}; do
        files+=("fsr4_model_v07_i8_${model}_${tier}_pre.spv"
                "fsr4_model_v07_i8_${model}_${tier}_post.spv")
        for pass in $(seq 1 12); do
            files+=("fsr4_model_v07_i8_${model}_${tier}_pass${pass}.spv")
        done
    done
done
fetched=0
for file in "${files[@]}"; do
    if [[ -s $dest/$file ]]; then continue; fi
    curl -fsSL --retry 3 -o "$dest/$file.part" "$base/$file"
    mv "$dest/$file.part" "$dest/$file"
    fetched=$((fetched + 1))
done
echo "FSR 4 assets: ${#files[@]} files in $PWD/$dest ($fetched downloaded)"
# Faster, bit-exact post passes (fsr4_shaders/opt), when spirv-cross and glslang are available.
if command -v spirv-cross >/dev/null && command -v glslangValidator >/dev/null; then
    bash tools/fsr4_optimize.sh
fi
