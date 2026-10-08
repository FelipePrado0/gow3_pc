#!/usr/bin/env bash
# tools/fsr4_verify.sh [frames]: runs the FSR 4 benchmark (out/gpu/fsr4-bench, pseudo-random
# inputs) for every preset and output size with the original passes and with fsr4_shaders/opt
# (tools/fsr4_optimize.sh) and prints the post pass times.
#   1440p, 2160p: the outputs must be bit-exact (the pass 11 guard never triggers there).
#   1080p: the original races at the left edge (pass 11); the fixed passes must give the same
#          output on every run, and after one frame differ from the original only at that edge.
set -euo pipefail
cd -- "$(dirname -- "$0")/.."
frames=${1:-12}
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
fail=0
bench() { # bench <opt> <render> <out> <preset> <frames> <dump>
    GOW3_FSR4_OPT=$1 BENCH_NOISE=1 BENCH_DUMP=$6 out/gpu/fsr4-bench "$2" "$3" "$4" "$5" >/dev/null 2>&1
}
post_ms() {
    GOW3_FSR4_OPT=$1 out/gpu/fsr4-bench "$2" "$3" "$4" 300 2>&1 |
        grep -E '^ +[0-9.]+ ms/frame  post$' | tail -1 | awk '{print $1}'
}
for out in 1920x1080 2560x1440 3840x2160; do
    ow=${out%x*}; oh=${out#*x}
    preset=0
    for ratio in 1.0 1.5 1.7 2.0 3.0; do
        render=$(awk -v w="$ow" -v h="$oh" -v r="$ratio" 'BEGIN { printf "%dx%d", int(w / r + 0.5), int(h / r + 0.5) }')
        if ((oh <= 1080)); then
            bench 1 "$render" "$out" $preset "$frames" "$tmp/a.raw"
            bench 1 "$render" "$out" $preset "$frames" "$tmp/b.raw"
            bench 0 "$render" "$out" $preset 1 "$tmp/o1.raw"
            bench 1 "$render" "$out" $preset 1 "$tmp/f1.raw"
            # Columns of the bytes that differ after one frame (8 bytes per pixel).
            far=$( (cmp -l "$tmp/o1.raw" "$tmp/f1.raw" || true) |
                awk -v w="$ow" '{ if (int(($1 - 1) / 8) % w >= 200) n++ } END { print n + 0 }')
            if ! cmp -s "$tmp/a.raw" "$tmp/b.raw"; then
                result="NOT DETERMINISTIC"; fail=1
            elif ((far)); then
                result="DIFFERS beyond the left edge ($far bytes)"; fail=1
            else
                result="deterministic, differs from the original only at the left edge"
            fi
        else
            bench 0 "$render" "$out" $preset "$frames" "$tmp/o.raw"
            bench 1 "$render" "$out" $preset "$frames" "$tmp/f.raw"
            if cmp -s "$tmp/o.raw" "$tmp/f.raw"; then result=bit-exact; else result=DIFFERS; fail=1; fi
        fi
        printf '%-9s preset %d %-9s post %s -> %s ms  %s\n' "$out" $preset "$render" \
            "$(post_ms 0 "$render" "$out" $preset)" "$(post_ms 1 "$render" "$out" $preset)" "$result"
        preset=$((preset + 1))
    done
done
exit $fail
