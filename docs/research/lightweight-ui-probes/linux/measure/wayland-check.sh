#!/usr/bin/env bash
# wayland-check.sh <name> <cmd> [args...]
# Runs the probe with DISPLAY unset (so X11 is impossible) and WAYLAND_DEBUG=client,
# for 5 s. "ok" = process still alive after 5 s and it created an xdg_toplevel
# (i.e. a native Wayland window) - this does not prove the window rendered correctly.
set -u
NAME=$1; shift
LOG=/tmp/wl-$NAME.log
env -u DISPLAY WAYLAND_DEBUG=client GDK_BACKEND=wayland QT_QPA_PLATFORM=wayland \
    setsid "$@" >"$LOG" 2>&1 &
pid=$!
sleep 5
alive=false; [ -d /proc/$pid ] && alive=true
top=$(grep -c 'xdg_toplevel' "$LOG" 2>/dev/null || true)
kill -TERM -- -$pid 2>/dev/null; sleep 0.3; kill -KILL -- -$pid 2>/dev/null
ok=false; if $alive && [ "${top:-0}" -gt 0 ]; then ok=true; fi
err=$(grep -v '^\[' "$LOG" | grep -iE 'error|fail|cannot|could not' | head -3 | tr '"\n' "' " )
printf '{"probe":"%s","alive_after_5s":%s,"xdg_toplevel_msgs":%s,"wayland_native_ok":%s,"err":"%s"}\n' "$NAME" "$alive" "${top:-0}" "$ok" "$err"
