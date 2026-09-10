#!/usr/bin/env python3
"""Acquire only the checksum-pinned public supporting Sigil binary."""

import hashlib
import json
from pathlib import Path
import re
import subprocess
import tarfile
import tempfile
import urllib.request

ROOT = Path(__file__).resolve().parent.parent


def main():
    spec = json.loads((ROOT / "scripts/release-tools.json").read_text())
    if set(spec) != {"sigil_version", "sigil_archive", "sigil_archive_sha256"}:
        raise ValueError("unexpected release-tool configuration")
    if spec["sigil_version"] != "0.35.0" or spec["sigil_archive"] != "sigil-x86_64-unknown-linux-gnu.tar.xz":
        raise ValueError("unexpected supporting host release")
    digest = spec["sigil_archive_sha256"]
    if re.fullmatch(r"[0-9a-f]{64}", digest) is None:
        raise ValueError("supporting public Sigil archive digest has not been pinned")
    url = "https://github.com/bobisme/sigil-releases/releases/download/v0.35.0/" + spec["sigil_archive"]
    destination = ROOT / "target/release-tools/sigil"
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="temporal-sigil-") as temporary:
        archive = Path(temporary) / "host.tar.xz"
        # No token, private checkout, installer execution or latest-version lookup.
        with urllib.request.urlopen(url, timeout=120) as response, archive.open("wb") as output:
            while block := response.read(1024 * 1024):
                output.write(block)
                if output.tell() > 512 * 1024 * 1024:
                    raise ValueError("supporting host archive exceeds acquisition ceiling")
        with archive.open("rb") as source:
            if hashlib.file_digest(source, "sha256").hexdigest() != digest:
                raise ValueError("supporting host archive checksum mismatch")
        with tarfile.open(archive, "r:xz") as bundle:
            entries = [entry for entry in bundle.getmembers() if Path(entry.name).name == "sigil"]
            if len(entries) != 1 or not entries[0].isfile() or entries[0].size > 512 * 1024 * 1024:
                raise ValueError("expected one ordinary Sigil binary")
            # Do not extract archive paths. Copy the one checked member only.
            binary = bundle.extractfile(entries[0]).read()
        with destination.open("xb") as output:
            output.write(binary)
        destination.chmod(0o755)
    if subprocess.check_output([str(destination), "--version"], text=True).strip() != "sigil 0.35.0":
        raise ValueError("wrong supporting host version")
    print(destination)


if __name__ == "__main__":
    main()
