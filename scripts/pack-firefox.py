"""Pack the Firefox build into a ZIP that addons.mozilla.org accepts.

The archive is made from `extensions/dist/firefox`, which `extensions/build.mjs`
assembles from the shared source plus the Firefox manifest, so a stale file
cannot be shipped. PowerShell's Compress-Archive writes local file headers with
'\\' but the central directory with '/'; AMO reads the local headers and
rejects the upload with "Invalid file name in archive". Python's zipfile writes
one name to both, so the archive is spec-compliant.

Usage: python scripts/pack-firefox.py [output.zip]
"""

import json
import struct
import subprocess
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "extensions" / "dist" / "firefox"
DESKTOP = Path.home() / "Desktop"


def local_header_names(path: Path) -> list[str]:
    data = path.read_bytes()
    names = []
    offset = 0
    while data[offset : offset + 4] == b"PK\x03\x04":
        *_, compressed, _, name_len, extra_len = struct.unpack_from(
            "<IHHHHHIIIHH", data, offset
        )
        names.append(data[offset + 30 : offset + 30 + name_len].decode("utf-8"))
        offset += 30 + name_len + extra_len + compressed
    return names


def main() -> None:
    subprocess.run(
        ["node", str(ROOT / "extensions" / "build.mjs"), "firefox"], check=True
    )
    version = json.loads((SRC / "manifest.json").read_text(encoding="utf-8"))["version"]
    out = (
        Path(sys.argv[1])
        if len(sys.argv) > 1
        else DESKTOP / f"B-Download-Manager-Firefox-{version}.zip"
    )
    files = sorted(p for p in SRC.rglob("*") if p.is_file())
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as archive:
        for file in files:
            name = file.relative_to(SRC).as_posix()
            assert "\\" not in name and ".." not in name and not name.startswith("/"), name
            archive.write(file, name)

    central = zipfile.ZipFile(out).namelist()
    assert local_header_names(out) == central, "ZIP headers disagree: repack needed"
    print(f"{out}  {out.stat().st_size} bytes  {len(central)} files  paths OK")


if __name__ == "__main__":
    main()
