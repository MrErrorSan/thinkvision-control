"""Merge results-*.json from measure.ps1 into results.json and print a Markdown summary table.
Run: python -I combine.py   (from this folder)"""
import glob
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
runs = []
for path in sorted(glob.glob(os.path.join(HERE, "results-*.json"))):
    with open(path, encoding="utf-8-sig") as f:
        runs.append(json.load(f))

out = {
    "description": "Background-mode memory/CPU/hotkey measurements, Windows 11, 100 % scale. See NOTES.md and measure.ps1.",
    "date": "2026-10-28",
    "baselines_from_docs": {
        "wpf_tray_never_shown_taskmgr_mb": 15,
        "wpf_after_window_used_taskmgr_mb": "55-65",
        "wpf_restart_on_close_taskmgr_mb": 15,
        "source": "docs/research/tray-memory/2026-10-28-tray-memory.md",
    },
    "runs": runs,
}
with open(os.path.join(HERE, "results.json"), "w", encoding="utf-8") as f:
    json.dump(out, f, indent=2)

print("| Run | Phase | Task Manager MB | Private (commit) MB | Working set MB | Threads | Handles | GDI | USER | CPU ms |")
print("|---|---|---|---|---|---|---|---|---|---|")
for r in runs:
    for s in r["samples"]:
        print(f"| {r['label']} | {s['phase']} | {s['taskmgr_mb']} | {s['private_bytes_mb']} | {s['ws_mb']} | {s['threads']} | {s['handles']} | {s['gdi']} | {s['user']} | {s['cpu_ms']} |")
for r in runs:
    hk = r.get("hotkeys")
    if not hk:
        continue
    b = hk["burst"]
    print(f"\n{r['label']}: first {hk['first_press_visible_ms']} ms, median {hk['rest_visible_ms_median']} ms, "
          f"p95 {hk['rest_visible_ms_p95']} ms, focus stolen {hk['focus_stolen_count']}; burst {b['hotkey_events']}/40 "
          f"final {b['final_value']} cpu {b['cpu_ms_burst_plus_300ms']} ms (until hidden {b['cpu_ms_until_osd_hidden']}) "
          f"peak private {b['peak_private_mb']} ws {b['peak_ws_mb']} max send->hk {b['send_to_hotkey_ms_max']} ms")
    for l in hk["latency"]:
        print(f"   press {l['press']}: visible {l['visible_ms']} ms, send->hk {l.get('send_to_hotkey_ms')}, hk->shown {l.get('hotkey_to_shown_ms')}, osd {l.get('osd')}, focus {l['focus_stolen']}, ex {l['osd_exstyle']}")
