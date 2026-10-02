#!/usr/bin/env python3
"""Write manifest.json for the downloadable engine packs.

Usage: make-manifest.py <folder with *.zip> <base download URL>
Pack names must be <engine>-<os>-<arch>.zip, e.g. pandoc-macos-aarch64.zip.
The app reads: {"engines": {"pandoc": {"macos-aarch64": {"url", "sha256", "size"}}}}
"""
import hashlib
import json
import pathlib
import re
import sys

folder, base = pathlib.Path(sys.argv[1]), sys.argv[2].rstrip("/")
pattern = re.compile(r"^(?P<engine>[a-z0-9]+)-(?P<platform>[a-z]+-[a-z0-9_]+)\.zip$")
manifest = {"version": 1, "engines": {}}

for z in sorted(folder.glob("*.zip")):
    m = pattern.match(z.name)
    if not m:
        print(f"skipping {z.name}", file=sys.stderr)
        continue
    data = z.read_bytes()
    manifest["engines"].setdefault(m["engine"], {})[m["platform"]] = {
        "url": f"{base}/{z.name}",
        "sha256": hashlib.sha256(data).hexdigest(),
        "size": len(data),
    }
    print(f"{m['engine']:<12} {m['platform']:<16} {len(data) / 1e6:6.1f} MB", file=sys.stderr)

out = folder / "manifest.json"
out.write_text(json.dumps(manifest, indent=2) + "\n")
print(out)
