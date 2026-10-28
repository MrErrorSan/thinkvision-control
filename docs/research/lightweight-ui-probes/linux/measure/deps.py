#!/usr/bin/env python3
"""deps.py <name> <dir> <main-binary>
Prints JSON: shipped files + sizes, tar.gz size, direct NEEDED libs of every ELF
in <dir> (readelf -d) resolved via ldd, owning Debian package, plus the size of the
full ldd closure. Libraries that are part of the shipped dir itself are skipped."""
import json, os, subprocess, sys, tarfile, tempfile

name, root, main = sys.argv[1], os.path.abspath(sys.argv[2]), sys.argv[3]

files = []
for dp, _, fns in os.walk(root):
    for fn in fns:
        p = os.path.join(dp, fn)
        if os.path.islink(p):
            continue
        files.append((os.path.relpath(p, root), os.path.getsize(p)))
files.sort(key=lambda x: -x[1])
shipped = sum(s for _, s in files)

with tempfile.TemporaryDirectory() as td:
    tgz = os.path.join(td, "x.tar.gz")
    with tarfile.open(tgz, "w:gz", compresslevel=9) as t:
        t.add(root, arcname=name)
    targz = os.path.getsize(tgz)

def is_elf(p):
    try:
        with open(p, "rb") as f:
            return f.read(4) == b"\x7fELF"
    except OSError:
        return False

def run(cmd):
    return subprocess.run(cmd, capture_output=True, text=True).stdout

shipped_names = {os.path.basename(f) for f, _ in files}
needed, closure = {}, {}
for rel, _ in files:
    p = os.path.join(root, rel)
    if not is_elf(p):
        continue
    for line in run(["readelf", "-d", p]).splitlines():
        if "(NEEDED)" in line:
            lib = line.split("[")[1].split("]")[0]
            if lib not in shipped_names:
                needed.setdefault(lib, None)
    env = dict(os.environ, LD_LIBRARY_PATH=os.path.dirname(p))
    out = subprocess.run(["ldd", p], capture_output=True, text=True, env=env).stdout
    for line in out.splitlines():
        parts = line.strip().split()
        if "=>" in parts and len(parts) >= 3:
            lib, path = parts[0], parts[2]
            if path == "not":
                closure[lib] = "NOT FOUND"
            elif not path.startswith(root):
                closure[lib] = path
                if lib in needed:
                    needed[lib] = path

pkg_cache = {}
def pkg(path):
    if not path or path == "NOT FOUND":
        return None
    if path not in pkg_cache:
        real = os.path.realpath(path)
        out = run(["dpkg", "-S", path]) or run(["dpkg", "-S", real])
        if not out:
            out = run(["dpkg", "-S", real.replace("/usr/lib", "/lib", 1)])
        pkg_cache[path] = out.split(":")[0].strip() if out else None
    return pkg_cache[path]

closure_bytes = sum(os.path.getsize(os.path.realpath(p)) for p in closure.values() if p != "NOT FOUND")
closure_pkgs = sorted({pkg(p) for p in closure.values() if pkg(p)})

print(json.dumps({
    "probe": name,
    "shipped_bytes": shipped,
    "targz_bytes": targz,
    "files": [{"path": f, "bytes": s} for f, s in files[:60]],
    "file_count": len(files),
    "direct_needed": [{"lib": k, "path": v, "package": pkg(v)} for k, v in sorted(needed.items())],
    "ldd_closure_count": len(closure),
    "ldd_closure_bytes": closure_bytes,
    "ldd_closure_packages": closure_pkgs,
    "ldd_not_found": sorted(k for k, v in closure.items() if v == "NOT FOUND"),
}))
