#!/usr/bin/env bash
# measure.sh <name> <cmd> [args...]
# Runs the probe under X11 (Wayland env removed): 1 cold run (after drop_caches),
# 3 warm runs. Startup = exec -> window titled "probe" viewable (launchwait).
# Memory sampled on the last warm run after 10 s idle. Prints one JSON object.
# Must run as root (drop_caches).
set -u
NAME=$1; shift
LW=${LW:-$HOME/probes/measure/launchwait}
IDLE=${IDLE:-10}
TITLE=${TITLE:-probe}

run_x11() {
  env -u WAYLAND_DISPLAY GDK_BACKEND=x11 QT_QPA_PLATFORM=xcb WINIT_UNIX_BACKEND=x11 \
      "$LW" "$TITLE" 60 "$@"
}

killgrp() { kill -TERM -- "-$1" 2>/dev/null; sleep 0.3; kill -KILL -- "-$1" 2>/dev/null; sleep 0.5; }

times=()
mem='{}'
for i in 0 1 2 3; do
  if [ $i -eq 0 ]; then sync; echo 3 > /proc/sys/vm/drop_caches; sleep 1; fi
  out=$(run_x11 "$@" 2>/tmp/measure-$NAME-$i.err)
  ms=${out%% *}; pid=${out##* }
  times+=("${ms:--1}")
  if [ $i -eq 3 ] && [ -n "$pid" ] && [ -d /proc/$pid ]; then
    sleep "$IDLE"
    rss=$(awk '/^Rss:/{print $2}' /proc/$pid/smaps_rollup)
    pss=$(awk '/^Pss:/{print $2}' /proc/$pid/smaps_rollup)
    thr=$(awk '/^Threads:/{print $2}' /proc/$pid/status)
    grep -oE "/[^ ]+.so[^ /]*$" /proc/$pid/maps | sort -u > /tmp/maps-$NAME.txt
    mem=$(printf '{"rss_kb":%s,"pss_kb":%s,"threads":%s}' "${rss:-null}" "${pss:-null}" "${thr:-null}")
  fi
  [ -n "$pid" ] && killgrp "$pid"
done
printf '{"probe":"%s","cold_ms":%s,"warm_ms":[%s,%s,%s],"mem":%s}\n' "$NAME" "${times[0]}" "${times[1]}" "${times[2]}" "${times[3]}" "$mem"
