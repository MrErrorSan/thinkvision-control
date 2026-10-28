#!/usr/bin/env bash
# shot.sh <scheme 0|1|2> <accent r,g,b or -> <out.png> <click "x y" or -> <cmd...>
# Runs <cmd> on a private session bus with a stub XDG portal (so light/dark is forced without touching the real
# desktop settings), under X11/XWayland, then captures the client window with ImageMagick `import`.
set -u
SCHEME=$1; ACCENT=$2; OUT=$3; CLICK=$4; shift 4
HERE=$(dirname "$0")
export -n WAYLAND_DISPLAY; unset WAYLAND_DISPLAY
export GDK_BACKEND=x11
dbus-run-session -- bash -c '
  if [ "$1" = - ]; then python3 "$4/stub_portal.py" "$0" > /tmp/portal.out 2>&1 &
  else python3 "$4/stub_portal.py" "$0" "$1" > /tmp/portal.out 2>&1 & fi
  for i in $(seq 50); do grep -q portal-ready /tmp/portal.out && break; sleep 0.1; done
  OUT=$2; CLICK=$3; shift 4
  "$@" > /tmp/shot-app.log 2>&1 &
  APP=$!
  WID=$(timeout 30 xdotool search --sync --onlyvisible --name "^ThinkVision Control$" | head -1)
  if [ -z "$WID" ]; then echo "no window"; kill $APP; exit 1; fi
  sleep 4
  # Re-resolve: some apps (Avalonia) replace their first window; take the largest visible match.
  WID=$(for w in $(xdotool search --onlyvisible --name "^ThinkVision Control$"); do echo "$(xwininfo -id $w | awk "/Width:/{print \$2}") $w"; done | sort -n | tail -1 | cut -d" " -f2)
  echo "window $WID"
  if [ "$CLICK" != - ]; then
    xdotool mousemove --window "$WID" $CLICK click 1; sleep 0.3
    xdotool mousemove --window "$WID" 900 5; sleep 2
  fi
  import -window "$WID" "$OUT"
  xwininfo -id "$WID" | grep -E "Width|Height"
  kill $APP; sleep 0.5; kill -9 $APP 2>/dev/null
' "$SCHEME" "$ACCENT" "$OUT" "$CLICK" "$HERE" "$@"
