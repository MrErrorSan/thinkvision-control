#!/usr/bin/env bash
# run-all.sh: final measurement pass over every built probe in ~/probes/out, machine otherwise idle.
# Writes ~/probes/final.jsonl (timings/memory), ~/probes/final-wayland.jsonl, ~/probes/maps/<probe>.txt
# (shared objects actually mapped at runtime, incl. dlopen'ed ones).
set -u
cd ~/probes
M=./measure
mkdir -p maps
: > final.jsonl; : > final-wayland.jsonl
# A working notification daemon (as on a real desktop). Without it, D-Bus activation of the
# broken dunst unit hangs ~25 s inside libnotify (hit by Eto.Gtk at startup).
pgrep -x dunst >/dev/null || (DISPLAY=:0 setsid nohup dunst >/tmp/dunst.log 2>&1 </dev/null &)
sleep 1

# name|title|env|command
PROBES=(
  "gtk4-c|probe||out/gtk4-c/probe"
  "gtk4-c-cairo|probe|GSK_RENDERER=cairo|out/gtk4-c/probe"
  "qt6-widgets|probe||out/qt6-widgets/probe"
  "wx-gtk3|probe||out/wx-gtk3/probe"
  "lazarus-gtk3|probe||out/lazarus-gtk3/probe"
  "lazarus-gtk2|probe||out/lazarus-gtk2/probe"
  "slint|probe||out/slint/probe"
  "slint-sw|probe||out/slint-sw/probe"
  "egui|probe||out/egui/probe"
  "egui-glow|probe||out/egui-glow/probe"
  "iced|probe||out/iced/probe"
  "iced-tinyskia|probe||out/iced-tinyskia/probe"
  "libcosmic|probe||out/libcosmic/probe"
  "gircore-gtk4-aot|probe||out/gircore-ship/probe"
  "eto-trimmed|probe||out/eto-trimmed-ship/probe"
  "eto-aot|probe||out/eto-aot-ship/probe"
  "ava-real-aot|ThinkVision Control||out/ava-real-aot-ship/thinkvision-control"
  "ava-shipped|ThinkVision Control||out/ava-shipped/thinkvision-control"
)
for entry in "${PROBES[@]}"; do
  IFS='|' read -r name title envs cmd <<<"$entry"
  [ -x "$cmd" ] || { echo "skip $name (not built)" >&2; continue; }
  TITLE="$title" env $envs $M/measure.sh "$name" "./$cmd" | tee -a final.jsonl
  cp /tmp/maps-$name.txt maps/$name.txt 2>/dev/null
  env $envs $M/wayland-check.sh "$name" "./$cmd" | tee -a final-wayland.jsonl
done
if [ -x out/tray-only/probe ]; then
  $M/measure-ready.sh tray-only ./out/tray-only/probe | tee -a final.jsonl
  cp /tmp/maps-tray-only.txt maps/tray-only.txt 2>/dev/null
fi
