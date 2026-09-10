#!/usr/bin/env python3
"""Acquire only the checksum-pinned public supporting Sigil binary."""

import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location("release_identity", Path(__file__).with_name("release-identity.py"))
identity = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(identity)
validate_spec = identity.validate_spec


def run(*args):
    return subprocess.check_output(args, text=True)


def acquire(root):
    spec = json.loads((root / "scripts/release-tools.json").read_text())
    digest, binary_digest = validate_spec(spec)
    url = "https://github.com/bobisme/sigil-releases/releases/download/v0.35.0/" + spec["sigil_archive"]
    destination = root / "target/release-tools/sigil"
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
        staged_binary = Path(temporary) / "sigil"
        staged_binary.write_bytes(binary)
        identity.check_digest(staged_binary, binary_digest)
        staged_binary.chmod(0o755)
        if identity.run_checked(staged_binary, binary_digest, run, "--version").strip() != "sigil 0.35.0":
            raise ValueError("wrong supporting host version")
        destination.parent.mkdir(parents=True, exist_ok=True)
        with destination.open("xb") as output:
            output.write(binary)
        destination.chmod(0o755)
        identity.check_digest(destination, binary_digest)
    return {
        "binary_path": str(destination),
        "binary_sha256": hashlib.sha256(binary).hexdigest(),
        "archive_sha256": digest,
        "archive_url": url,
        "version": "sigil 0.35.0",
    }


def main():
    print(json.dumps(acquire(ROOT), sort_keys=True, separators=(",", ":")))


if __name__ == "__main__":
    main()
