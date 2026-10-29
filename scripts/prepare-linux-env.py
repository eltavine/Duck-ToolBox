#!/usr/bin/env python3
"""Stage the pinned static ARM64 init helper and verify the exact distributed bytes."""
import hashlib
import json
from pathlib import Path
import shutil
import urllib.request

ROOT = Path(__file__).resolve().parents[1]


def validate(path, expected):
    data = path.read_bytes()
    if hashlib.sha256(data).hexdigest() != expected:
        raise ValueError("Init helper SHA-256 mismatch")
    if data[:6] != b"\x7fELF\x02\x01" or data[18:20] != b"\xb7\x00":
        raise ValueError("Init helper must be little-endian ARM64 ELF64")


def main():
    manifest = json.loads((ROOT / "components/linux-env/tini.json").read_text())
    cache = ROOT / "target/linux-env/tini-arm64"
    cache.parent.mkdir(parents=True, exist_ok=True)
    if not cache.is_file():
        temporary = cache.with_suffix(".download")
        try:
            with urllib.request.urlopen(manifest["asset"], timeout=45) as response:
                with temporary.open("wb") as output:
                    shutil.copyfileobj(response, output, 131072)
            validate(temporary, manifest["sha256"])
            temporary.replace(cache)
        finally:
            temporary.unlink(missing_ok=True)
    validate(cache, manifest["sha256"])
    notice = ROOT / manifest["notice"]
    if not notice.is_file():
        raise ValueError("Tini copyright and MIT license are missing")
    target = ROOT / "module/bin/tini-arm64"
    shutil.copy2(cache, target)
    target.chmod(0o755)


if __name__ == "__main__":
    main()
