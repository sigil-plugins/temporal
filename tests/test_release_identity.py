"""Harmless byte fixtures and mocked calls test identity continuity, not exploits."""

import hashlib
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import Mock

ROOT = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location("release_identity", ROOT / "scripts/release-identity.py")
identity = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(identity)


class ExecutableIdentityTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="temporal-identity-tests-")
        self.binary = Path(self.temporary.name) / "ordinary-text-fixture"
        self.bytes = b"ordinary initial bytes, never executed\n"
        self.binary.write_bytes(self.bytes)
        self.expected = hashlib.sha256(self.bytes).hexdigest()

    def tearDown(self):
        self.temporary.cleanup()

    def test_changed_bytes_are_refused_before_each_invocation(self):
        self.binary.write_bytes(b"ordinary different bytes, never executed\n")
        for args in (("--version",), ("plugin", "validate", "plugin.toml"),
                     ("plugin", "validate", "candidate.sigil-plugin.tar.zst")):
            runner = Mock(return_value="sigil 0.35.0\n")
            with self.subTest(args=args), self.assertRaises(ValueError):
                identity.run_checked(self.binary, self.expected, runner, *args)
            runner.assert_not_called()

    def test_unchanged_bytes_preserve_success_and_ordinary_failure(self):
        runner = Mock(return_value="ordinary result")
        self.assertEqual(identity.run_checked(self.binary, self.expected, runner, "--version"), "ordinary result")
        runner.assert_called_once_with(str(self.binary), "--version")
        error = subprocess.CalledProcessError(7, ["ordinary validator"], stderr=b"ordinary error")
        with self.assertRaises(subprocess.CalledProcessError) as raised:
            identity.run_checked(self.binary, self.expected, Mock(side_effect=error), "--version")
        self.assertIs(raised.exception, error)

    def test_digest_is_checked_after_success_and_failed_mock_calls(self):
        for fail in (False, True):
            self.binary.write_bytes(self.bytes)

            def changed(*args):
                self.binary.write_bytes(b"ordinary changed bytes after mock call\n")
                if fail:
                    raise subprocess.CalledProcessError(7, args)
                return "ordinary result"

            with self.subTest(fail=fail), self.assertRaises(ValueError):
                identity.run_checked(self.binary, self.expected, changed, "--version")


if __name__ == "__main__":
    unittest.main()
