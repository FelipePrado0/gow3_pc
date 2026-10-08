#!/usr/bin/env bash
# tools/soak.sh <secs>: enters the level, rotates the camera now and then, reports survival.
cd -- "$(dirname -- "$0")/.."
unset GOW3_GPU_PROFILE GOW3_PRESET_FILE GOW3_SCENE_DEBUG
bash "$(dirname "$0")/restart.sh" > /dev/null
end=$((SECONDS + $1))
while ((SECONDS < end)); do
    "$(dirname "$0")/press.sh" "rx=60" 2; sleep 8
    "$(dirname "$0")/press.sh" "rx=195" 2; sleep 8
    if ! pgrep -x gow3-probe > /dev/null; then
        echo "DIED after $((SECONDS)) s: $(grep -m1 -E 'fault|Assert|Fault' out/session.log | cut -c1-160)"; exit 1
    fi
done
echo "alive after $1 s: $(grep '^Frame stats' out/session.log | tail -1 | cut -c1-40)"
