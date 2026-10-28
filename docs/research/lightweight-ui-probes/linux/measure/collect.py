#!/usr/bin/env python3
"""collect.py <out.json> : merge ~/probes/{deps.jsonl,final.jsonl,final-wayland.jsonl,maps/*.txt,notes.json,meta.json}
into results.json. Runtime libraries are taken from /proc/<pid>/maps (so dlopen'ed libs count), mapped to
Debian packages with dpkg -S, then classified with the table below (desktop-default knowledge; entries marked
guess are not verified on a real install)."""
import json, os, subprocess, sys

HOME = os.path.expanduser("~/probes")

def jsonl(name):
    out = {}
    p = os.path.join(HOME, name)
    if os.path.exists(p):
        for line in open(p):
            line = line.strip()
            if line:
                d = json.loads(line)
                out[d["probe"]] = d  # last one wins
    return out

# package (prefix) -> (ubuntu 24.04 GNOME default, Fedora Workstation default, KDE Plasma default, note)
# "yes"/"no"/"likely"/"unlikely"; guesses say so.
CLASS = {
    "libgtk-4-1": ("yes", "yes", "likely (guess: pulled by GTK4 apps/portals; not guaranteed on Kubuntu)", "GTK 4"),
    "libgtk-3-0": ("yes", "yes", "likely (guess: pulled by Firefox/GTK apps, breeze-gtk)", "GTK 3"),
    "libgtk2.0-0": ("unlikely (guess)", "unlikely (guess)", "unlikely (guess)", "GTK 2 (EOL)"),
    "libqt6": ("no (guess)", "no (guess)", "yes on Plasma 6 (Fedora KDE, KDE neon); Kubuntu 24.04 is Plasma 5/Qt5 (guess: Qt6 not guaranteed)", "Qt 6"),
    "qt6-": ("no (guess)", "no (guess)", "yes on Plasma 6", "Qt 6 plugin"),
    "libwx": ("no", "no", "no", "wxWidgets runtime: declare package dependency or bundle"),
    "libayatana-appindicator3": ("yes (guess)", "no (guess; Fedora ships libappindicator-gtk3 instead)", "unlikely (guess)", "tray (AppIndicator/SNI)"),
    "libayatana-indicator3": ("yes (guess)", "no (guess)", "unlikely (guess)", "tray"),
    "libdbusmenu": ("yes (guess)", "likely (guess)", "likely (guess)", "tray menu export (dbusmenu; pulled in with appindicator)"),
    "libxdo3": ("no (guess)", "no (guess)", "no (guess)", "xdo"),
    "libnotify4": ("yes", "yes", "likely", "notifications"),
    "libvulkan1": ("yes", "yes", "yes", "Vulkan loader"),
    "mesa-": ("yes", "yes", "yes", "Mesa drivers"),
    "libgl1-mesa-dri": ("yes", "yes", "yes", "Mesa drivers"),
    "libegl": ("yes", "yes", "yes", "EGL"),
    "libgl": ("yes", "yes", "yes", "GL"),
}
LOWLEVEL_OK = "default on all three desktops (low-level desktop library)"

def classify(pkg):
    for k, v in CLASS.items():
        if pkg.startswith(k):
            states = [s.split(" ")[0] for s in v[:3]]
            if all(s == "yes" for s in states):
                cls = "a"
            elif any(s in ("no", "unlikely") for s in states):
                cls = "b"  # bundle or declare as a package dependency
            else:
                cls = "a?"  # default on Ubuntu/Fedora GNOME, uncertain on some KDE installs
            return {"ubuntu_24_04_desktop": v[0], "fedora_workstation": v[1], "kde_plasma": v[2], "what": v[3], "class": cls}
    return {"class": "a", "note": LOWLEVEL_OK}

pkg_cache = {}
def pkg_of(path):
    if path in pkg_cache:
        return pkg_cache[path]
    res = None
    for cand in (path, os.path.realpath(path), os.path.realpath(path).replace("/usr/lib", "/lib", 1)):
        out = subprocess.run(["dpkg", "-S", cand], capture_output=True, text=True).stdout
        if out:
            res = out.split(":")[0].split(",")[0].strip()
            break
    pkg_cache[path] = res
    return res

deps, final, wl = jsonl("deps.jsonl"), jsonl("final.jsonl"), jsonl("final-wayland.jsonl")
notes = json.load(open(os.path.join(HOME, "notes.json"))) if os.path.exists(os.path.join(HOME, "notes.json")) else {}
meta = json.load(open(os.path.join(HOME, "meta.json"))) if os.path.exists(os.path.join(HOME, "meta.json")) else {}
alias = meta.pop("deps_alias", {})  # measurement name -> deps name

probes = []
names = list(dict.fromkeys(list(final) + list(notes)))
for name in names:
    f = final.get(name, {})
    d = deps.get(alias.get(name, name), {})
    shipped_dir = d.get("probe")
    runtime = []
    mp = os.path.join(HOME, "maps", name + ".txt")
    if os.path.exists(mp):
        shipped_paths = os.path.join(HOME, "out")
        by_pkg = {}
        for lib in open(mp).read().split():
            if lib.startswith(shipped_paths) or "/.net/" in lib or lib.startswith("/tmp"):
                by_pkg.setdefault("(shipped with app)", []).append(os.path.basename(lib))
                continue
            p = pkg_of(lib) or "(unknown)"
            by_pkg.setdefault(p, []).append(os.path.basename(lib))
        for p, libs in sorted(by_pkg.items()):
            entry = {"package": p, "libs": sorted(set(libs))}
            if not p.startswith("("):
                entry.update(classify(p))
            runtime.append(entry)
    mem = f.get("mem", {})
    w = wl.get(name, {})
    probes.append({
        "probe": name,
        "shipped_bytes": d.get("shipped_bytes"),
        "targz_bytes": d.get("targz_bytes"),
        "files": d.get("files"),
        "ldd_direct_needed": d.get("direct_needed"),
        "ldd_closure_count": d.get("ldd_closure_count"),
        "ldd_closure_packages": d.get("ldd_closure_packages"),
        "runtime_mapped_packages": runtime,
        "must_bundle_or_declare": sorted({r["package"] for r in runtime if r.get("class") == "b"}),
        "default_except_maybe_kde": sorted({r["package"] for r in runtime if r.get("class") == "a?"}),
        "cold_ms": f.get("cold_ms"),
        "warm_ms": f.get("warm_ms"),
        "rss_mb": round(mem["rss_kb"] / 1024, 1) if mem.get("rss_kb") else None,
        "pss_mb": round(mem["pss_kb"] / 1024, 1) if mem.get("pss_kb") else None,
        "threads": mem.get("threads"),
        "wayland_native_ok": w.get("wayland_native_ok"),
        "wayland_check": w or None,
        "notes": notes.get(name, ""),
    })

json.dump({**meta, "probes": probes}, open(sys.argv[1], "w"), indent=1)
for p in probes:
    print(p["probe"], p["shipped_bytes"], p["targz_bytes"], p["cold_ms"], p["warm_ms"], p["rss_mb"], p["pss_mb"], p["threads"], p["wayland_native_ok"], p["must_bundle_or_declare"], p["default_except_maybe_kde"])
