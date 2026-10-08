#!/usr/bin/env bash
# A/B of a GOW3_TOGGLE_FILE bit in a running game (started with GOW3_FRAME_STATS=1 and
# GOW3_TOGGLE_FILE=<toggles>). Alternates "on" (bit clear) and "off" (bit set) phases and prints
# the mean FPS and GPU thread us/draw of the "Frame stats" lines each phase produced, skipping
# the first line after a switch.
# usage: ab.sh <log> <toggles> <bit> [phases=8] [seconds=20] [base mask=0] [first=on|off]
set -euo pipefail
export LC_ALL=C
log=$1 toggles=$2 bit=$3 phases=${4:-8} seconds=${5:-20} base=${6:-0} first=${7:-on}
shift_phase=0; [[ $first == off ]] && shift_phase=1
declare -A fps_sum us_sum count
for ((i = 0; i < phases; ++i)); do
    if (((i + shift_phase) % 2 == 0)); then phase=on mask=$base; else phase=off mask=$((base | bit)); fi
    echo $mask > "$toggles"
    start=$(wc -l < "$log")
    sleep "$seconds"
    read -r f u n < <(tail -n +$((start + 1)) "$log" | grep '^Frame stats:' | tail -n +2 |
        sed -E 's/^Frame stats: ([0-9.]+) FPS.*GPU thread ([0-9.]+) us\/draw.*/\1 \2/' |
        awk '{f += $1; u += $2; n++} END {if (n) printf "%.2f %.3f %d\n", f / n, u / n, n; else print "0 0 0"}')
    printf '%-3s phase %d: %6.2f FPS, %.3f us/draw (%d samples)\n' $phase $i "$f" "$u" "$n"
    if ((n)); then
        fps_sum[$phase]=$(awk -v a="${fps_sum[$phase]:-0}" -v b="$f" -v n="$n" 'BEGIN {print a + b * n}')
        us_sum[$phase]=$(awk -v a="${us_sum[$phase]:-0}" -v b="$u" -v n="$n" 'BEGIN {print a + b * n}')
        count[$phase]=$((${count[$phase]:-0} + n))
    fi
done
echo $base > "$toggles"
for phase in on off; do
    if ((${count[$phase]:-0})); then
        awk -v p=$phase -v f="${fps_sum[$phase]}" -v u="${us_sum[$phase]}" -v n="${count[$phase]}" \
            'BEGIN {printf "%s: %.2f FPS, %.3f us/draw\n", p, f / n, u / n}'
    fi
done
