"""Offline mocked acquisition contracts, not public release qualification."""

import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location("acquire_release", ROOT / "scripts/acquire-release-sigil.py")
acquisition = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(acquisition)


class AcquisitionTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="temporal-acquisition-tests-")
        self.root = Path(self.temporary.name)
        self.binary = b"ordinary mock release binary, never executed\n"
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode="w:xz") as bundle:
            member = tarfile.TarInfo("sigil-x86_64-unknown-linux-gnu/sigil")
            member.size = len(self.binary)
            bundle.addfile(member, io.BytesIO(self.binary))
        self.archive = buffer.getvalue()
        self.spec = {
            "sigil_version": "0.35.0",
            "sigil_archive": "sigil-x86_64-unknown-linux-gnu.tar.xz",
            "sigil_archive_sha256": hashlib.sha256(self.archive).hexdigest(),
        }
        (self.root / "scripts").mkdir()
        self.write_spec()

    def tearDown(self):
        self.temporary.cleanup()

    def write_spec(self):
        (self.root / "scripts/release-tools.json").write_text(json.dumps(self.spec))

    def test_pinned_public_archive_and_binary_identity_are_recorded_without_overwrite(self):
        with patch.object(acquisition.urllib.request, "urlopen", return_value=io.BytesIO(self.archive)) as request, \
                patch.object(acquisition.subprocess, "check_output", return_value="sigil 0.35.0\n"):
            result = acquisition.acquire(self.root)
        self.assertEqual(result["archive_sha256"], self.spec["sigil_archive_sha256"])
        self.assertEqual(result["binary_sha256"], hashlib.sha256(self.binary).hexdigest())
        self.assertEqual(result["archive_url"],
                         "https://github.com/bobisme/sigil-releases/releases/download/v0.35.0/sigil-x86_64-unknown-linux-gnu.tar.xz")
        request.assert_called_once_with(result["archive_url"], timeout=120)
        destination = Path(result["binary_path"])
        self.assertEqual(destination.read_bytes(), self.binary)
        with patch.object(acquisition.urllib.request, "urlopen", return_value=io.BytesIO(self.archive)), \
                patch.object(acquisition.subprocess, "check_output", return_value="sigil 0.35.0\n"), \
                self.assertRaises(FileExistsError):
            acquisition.acquire(self.root)
        self.assertEqual(destination.read_bytes(), self.binary)

    def test_missing_pin_stops_before_network_and_wrong_checksum_before_execution(self):
        self.spec["sigil_archive_sha256"] = "AWAITING_PUBLIC_SIGIL_0_35_0_ASSETS"
        self.write_spec()
        with patch.object(acquisition.urllib.request, "urlopen") as request, self.assertRaises(ValueError):
            acquisition.acquire(self.root)
        request.assert_not_called()
        self.spec["sigil_archive_sha256"] = "0" * 64
        self.write_spec()
        with patch.object(acquisition.urllib.request, "urlopen", return_value=io.BytesIO(self.archive)), \
                patch.object(acquisition.subprocess, "check_output") as execute, self.assertRaises(ValueError):
            acquisition.acquire(self.root)
        execute.assert_not_called()
        self.assertFalse((self.root / "target").exists())

    def test_older_host_version_publishes_no_selected_binary(self):
        with patch.object(acquisition.urllib.request, "urlopen", return_value=io.BytesIO(self.archive)), \
                patch.object(acquisition.subprocess, "check_output", return_value="sigil 0.34.0\n"), \
                self.assertRaises(ValueError):
            acquisition.acquire(self.root)
        self.assertFalse((self.root / "target").exists())

    def test_release_tool_spec_is_closed(self):
        for key, value in (("sigil_version", "latest"), ("sigil_archive", "other.tar.xz"),
                           ("sigil_archive_sha256", "ABCDEF"), ("extra", True)):
            with self.subTest(key=key), self.assertRaises(ValueError):
                acquisition.validate_spec({**self.spec, key: value})


if __name__ == "__main__":
    unittest.main()
