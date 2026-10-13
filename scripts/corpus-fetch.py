#!/usr/bin/env python3
"""Download the real-file test set listed in tests/corpus/manifest.json into a folder.

    python3 scripts/corpus-fetch.py ~/tenun-corpus

The files are public data under the licence named in the manifest. They are never committed: the
manifest keeps only where each one comes from and its SHA-256. A file whose publisher changed it is
kept and reported, so the run can still use it; `--update` writes the new hashes into the manifest.
Uses the Python standard library only.
"""

import hashlib
import io
import json
import sys
import urllib.request
import zipfile
from pathlib import Path

MANIFEST = Path(__file__).resolve().parent.parent / "tests" / "corpus" / "manifest.json"
AGENT = "tenun-corpus/1 (+https://github.com/galohot/tenun)"


def fetch(url: str) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": AGENT})
    with urllib.request.urlopen(req, timeout=120) as r:
        return r.read()


def main() -> int:
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    update = "--update" in sys.argv
    if not args:
        print(__doc__)
        return 2
    out = Path(args[0]).expanduser()
    out.mkdir(parents=True, exist_ok=True)
    manifest = json.loads(MANIFEST.read_text())
    counts = {"ok": 0, "kept": 0, "changed": 0, "failed": 0}
    for f in manifest["files"]:
        target = out / f["file"]
        if target.exists() and hashlib.sha256(target.read_bytes()).hexdigest() == f["sha256"]:
            counts["kept"] += 1
            continue
        try:
            data = fetch(f["url"])
            if f.get("zip"):
                with zipfile.ZipFile(io.BytesIO(data)) as z:
                    member = next(n for n in z.namelist() if n.startswith(f["zip"]))
                    data = z.read(member)
        except Exception as e:  # noqa: BLE001 - a dead link is reported, not fatal
            counts["failed"] += 1
            print(f"failed   {f['file']}: {e}")
            continue
        target.write_bytes(data)
        digest = hashlib.sha256(data).hexdigest()
        if digest == f["sha256"]:
            counts["ok"] += 1
        else:
            counts["changed"] += 1
            print(f"changed  {f['file']} (the publisher changed the file)")
            if update:
                f["sha256"] = digest
    if update:
        MANIFEST.write_text(json.dumps(manifest, indent=1, ensure_ascii=False) + "\n")
    print(", ".join(f"{v} {k}" for k, v in counts.items()), f"→ {out}")
    return 1 if counts["failed"] else 0


if __name__ == "__main__":
    sys.exit(main())
