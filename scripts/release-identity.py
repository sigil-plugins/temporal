"""Checked public executable identity, not isolation from same-user mutation."""

import hashlib
import json
import re


def validate_spec(spec):
    if set(spec) != {"sigil_version", "sigil_archive", "sigil_archive_sha256", "sigil_binary_sha256"}:
        raise ValueError("unexpected release-tool configuration")
    if spec["sigil_version"] != "0.35.0" or spec["sigil_archive"] != "sigil-x86_64-unknown-linux-gnu.tar.xz":
        raise ValueError("unexpected supporting host release")
    for field in ("sigil_archive_sha256", "sigil_binary_sha256"):
        if not isinstance(spec[field], str) or re.fullmatch(r"[0-9a-f]{64}", spec[field]) is None:
            raise ValueError(f"supporting public Sigil {field} has not been pinned")
    return spec["sigil_archive_sha256"], spec["sigil_binary_sha256"]


def load_spec(root):
    return validate_spec(json.loads((root / "scripts/release-tools.json").read_text()))


def check_digest(binary, expected):
    if not binary.is_file() or binary.is_symlink():
        raise ValueError("release validator must be an ordinary file")
    with binary.open("rb") as source:
        actual = hashlib.file_digest(source, "sha256").hexdigest()
    if actual != expected:
        raise ValueError("release validator differs from the pinned public executable")


def run_checked(binary, expected, runner, *args):
    # The expected digest comes from reviewed source, never whatever bytes
    # happen to be present at this path after dependency-controlled builds.
    check_digest(binary, expected)
    try:
        return runner(str(binary), *args)
    finally:
        # Also check failed executions; an ordinary subprocess error propagates
        # unchanged when the executable identity still matches.
        check_digest(binary, expected)
