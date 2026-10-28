#!/usr/bin/env bash
# measure-ready.sh <name> <cmd> [args...]
# For windowless probes (tray-only): startup = exec -> first stdout line "ready".
# Same run pattern as measure.sh: 1 cold (drop_caches), 3 warm, memory after IDLE s on the last.
set -u
NAME=$1; shift
IDLE=${IDLE:-10}
FIFO=/tmp/ready-$NAME.fifo
times=(); mem='{}'
for i in 0 1 2 3; do
  if [ $i -eq 0 ]; then sync; echo 3 > /proc/sys/vm/drop_caches; sleep 1; fi
  rm -f "$FIFO"; mkfifo "$FIFO"
  t0=$(date +%s%N)
  env -u WAYLAND_DISPLAY GDK_BACKEND=x11 setsid "$@" >"$FIFO" 2>/tmp/measure-$NAME-$i.err &
  pid=$!
  exec 3<"$FIFO"
  ms=-1
  if read -r -t 60 line <&3 && [ "$line" = ready ]; then
    t1=$(date +%s%N); ms=$(( (t1 - t0) / 100000 )); ms="$((ms / 10)).$((ms % 10))"
  fi
  times+=("$ms")
  if [ $i -eq 3 ] && [ -d /proc/$pid ]; then
    sleep "$IDLE"
    rss=$(awk '/^Rss:/{print $2}' /proc/$pid/smaps_rollup)
    pss=$(awk '/^Pss:/{print $2}' /proc/$pid/smaps_rollup)
    thr=$(awk '/^Threads:/{print $2}' /proc/$pid/status)
    grep -oE "/[^ ]+.so[^ /]*$" /proc/$pid/maps | sort -u > /tmp/maps-$NAME.txt
    mem=$(printf '{"rss_kb":%s,"pss_kb":%s,"threads":%s}' "$rss" "$pss" "$thr")
  fi
  kill -TERM -- -$pid 2>/dev/null; sleep 0.3; kill -KILL -- -$pid 2>/dev/null; sleep 0.5
  exec 3<&-
done
rm -f "$FIFO"
printf '{"probe":"%s","cold_ms":%s,"warm_ms":[%s,%s,%s],"mem":%s}\n' "$NAME" "${times[0]}" "${times[1]}" "${times[2]}" "${times[3]}" "$mem"
