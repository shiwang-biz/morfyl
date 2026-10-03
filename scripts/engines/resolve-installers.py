#!/usr/bin/env python3
"""Find the newest official LibreOffice and Calibre installers and add them to manifest.json.

Morfyl installs these large engines privately (inside its own data folder) when the user
clicks Install, so they never need admin rights. Only official download servers are used.

Usage: resolve-installers.py <manifest.json>   (updates the file in place; creates it if missing)

Adds:
  "installers": {
    "libreoffice": {"macos-aarch64": {"url", "kind": "dmg", "version", "size", "sha256"?}, ...},
    "calibre":     {...}
  }
If a lookup fails, the previous entry is kept, so a flaky mirror never removes an installer.
"""
import hashlib
import json
import os
import pathlib
import re
import sys
import urllib.request

UA = {"User-Agent": "Morfyl-manifest/1.0 (+https://craziestcoders.com)"}
LO_BASE = "https://download.documentfoundation.org/libreoffice/stable"


def get(url: str, method: str = "GET", timeout: int = 60):
    req = urllib.request.Request(url, headers=UA, method=method)
    return urllib.request.urlopen(req, timeout=timeout)


def text(url: str) -> str:
    with get(url) as r:
        return r.read().decode("utf-8", "replace")


def head_size(url: str) -> int | None:
    """Follows redirects to a mirror; returns the file size, or None if it doesn't exist."""
    try:
        with get(url, "HEAD") as r:
            if r.status >= 400:
                return None
            n = r.headers.get("Content-Length")
            return int(n) if n else 0
    except Exception as e:  # noqa: BLE001
        print(f"  missing: {url} ({e})", file=sys.stderr)
        return None


def sha256_of(url: str) -> str:
    """Download once and hash, for installers that don't publish a checksum."""
    h = hashlib.sha256()
    with get(url, timeout=600) as r:
        for chunk in iter(lambda: r.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def version_key(v: str):
    return tuple(int(x) for x in v.split("."))


def libreoffice() -> dict:
    listing = text(f"{LO_BASE}/")
    versions = sorted(set(re.findall(r'href="(\d+\.\d+\.\d+)/"', listing)), key=version_key)
    if not versions:
        raise RuntimeError("no LibreOffice versions found")
    v = versions[-1]
    files = {
        "macos-aarch64": ("dmg", f"{LO_BASE}/{v}/mac/aarch64/LibreOffice_{v}_MacOS_aarch64.dmg"),
        "macos-x86_64": ("dmg", f"{LO_BASE}/{v}/mac/x86_64/LibreOffice_{v}_MacOS_x86-64.dmg"),
        "windows-x86_64": ("msi", f"{LO_BASE}/{v}/win/x86_64/LibreOffice_{v}_Win_x86-64.msi"),
    }
    out = {}
    for platform, (kind, url) in files.items():
        size = head_size(url)
        if size is None:
            continue
        entry = {"url": url, "kind": kind, "version": v, "size": size}
        # The download server publishes <file>.sha256 next to every file.
        try:
            digest = text(url + ".sha256").split()[0]
            if re.fullmatch(r"[0-9a-fA-F]{64}", digest):
                entry["sha256"] = digest.lower()
        except Exception:  # noqa: BLE001
            pass
        out[platform] = entry
    print(f"LibreOffice {v}: {sorted(out)}", file=sys.stderr)
    return out


def calibre() -> dict:
    # Calibre publishes every release (with installers) on GitHub.
    req = urllib.request.Request(
        "https://api.github.com/repos/kovidgoyal/calibre/releases/latest",
        headers={**UA, "Accept": "application/vnd.github+json",
                 **({"Authorization": f"Bearer {os.environ['GH_TOKEN']}"} if os.environ.get("GH_TOKEN") else {})},
    )
    with urllib.request.urlopen(req, timeout=60) as r:
        rel = json.load(r)
    v = rel["tag_name"].lstrip("v")
    assets = {a["name"]: a for a in rel.get("assets", [])}
    wanted = {
        "macos-aarch64": ("dmg", f"calibre-{v}.dmg"),  # universal: Apple Silicon and Intel
        "macos-x86_64": ("dmg", f"calibre-{v}.dmg"),
        "windows-x86_64": ("msi", f"calibre-64bit-{v}.msi"),
    }
    out = {}
    for platform, (kind, name) in wanted.items():
        a = assets.get(name)
        if a:
            out[platform] = {"url": a["browser_download_url"], "kind": kind, "version": v, "size": a["size"]}
        else:
            print(f"  calibre asset missing: {name}", file=sys.stderr)
    print(f"Calibre {v}: {sorted(out)}", file=sys.stderr)
    return out


def add_checksums(entries: dict, previous: dict):
    """Every installer gets a SHA-256 so the app can verify the download."""
    cache = {}
    for platform, e in entries.items():
        if e.get("sha256"):
            continue
        old = previous.get(platform, {})
        if old.get("url") == e["url"] and old.get("sha256"):
            e["sha256"] = old["sha256"]  # same file as last time
            continue
        if e["url"] not in cache:
            print(f"  hashing {e['url']}", file=sys.stderr)
            cache[e["url"]] = sha256_of(e["url"])
        e["sha256"] = cache[e["url"]]


def main():
    path = pathlib.Path(sys.argv[1])
    manifest = json.loads(path.read_text()) if path.exists() else {"version": 1, "engines": {}}
    installers = manifest.setdefault("installers", {})
    for key, fn in (("libreoffice", libreoffice), ("calibre", calibre)):
        try:
            found = fn()
            if found:
                add_checksums(found, installers.get(key, {}))
                installers[key] = {**installers.get(key, {}), **found}
        except Exception as e:  # noqa: BLE001
            print(f"WARNING: {key} lookup failed, keeping previous entry: {e}", file=sys.stderr)
    path.write_text(json.dumps(manifest, indent=2) + "\n")
    print(path)


if __name__ == "__main__":
    main()
