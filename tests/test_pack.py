"""Benign generated dummy proves packaging shape, NOT Temporal execution."""

import hashlib
from contextlib import redirect_stderr, redirect_stdout
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location("pack", ROOT / "scripts/pack.py")
pack = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(pack)


class ContractTests(unittest.TestCase):
    def test_cli_reports_benign_subprocess_diagnostics_without_traceback(self):
        stdout, stderr = io.StringIO(), io.StringIO()

        def failed_validator():
            # An ordinary mocked tool error, not a malformed component input.
            raise subprocess.CalledProcessError(
                7, ["validator", "ordinary-candidate"],
                output=b"validation summary\n", stderr=b"ordinary validation error\n")

        with redirect_stdout(stdout), redirect_stderr(stderr), self.assertRaises(SystemExit) as raised:
            pack.contracts.cli(failed_validator)
        self.assertEqual(raised.exception.code, 7)
        self.assertEqual(stdout.getvalue(), "")
        self.assertIn("command failed with status 7: validator ordinary-candidate", stderr.getvalue())
        self.assertIn("--- stdout ---\nvalidation summary\n", stderr.getvalue())
        self.assertIn("--- stderr ---\nordinary validation error\n", stderr.getvalue())
        self.assertNotIn("Traceback", stderr.getvalue())

    def test_frozen_inputs_and_exact_world(self):
        pack.contracts.check_sources()
        doc = json.loads(pack.contracts.run(pack.contracts.wasm_tools(), "component", "wit", "--json", str(ROOT / "wit")))
        shape = pack.contracts.world_shape(doc)
        self.assertEqual(set(shape["imports"]), {pack.contracts.HOST})
        self.assertEqual(set(shape["exports"]), {pack.contracts.CLIENT})

    def test_extra_top_level_item_is_rejected(self):
        doc = json.loads(pack.contracts.run(pack.contracts.wasm_tools(), "component", "wit", "--json", str(ROOT / "wit")))
        world = next(world for world in doc["worlds"] if world["exports"])
        world["imports"]["extra"] = {"function": {}}
        with self.assertRaises(ValueError):
            pack.contracts.world_shape(doc)

    def test_semantic_shape_preserves_record_field_order(self):
        doc = json.loads(pack.contracts.run(pack.contracts.wasm_tools(), "component", "wit", "--json", str(ROOT / "wit")))
        expected = pack.contracts.world_shape(doc)
        call = next(ty for ty in doc["types"] if ty["name"] == "call")
        call["kind"]["record"]["fields"].reverse()
        self.assertNotEqual(pack.contracts.world_shape(doc), expected)

    def test_local_manifest_version_and_authority_are_closed(self):
        source = (ROOT / "plugin.local.toml").read_bytes()
        pack.validate_manifest(source)
        substitutions = [
            (b'0.1.0-dev.1', b'0.1.0'),
            (b'0.1.0-dev.1', b'0.1.0-dev.01'),
            (b'schema_version = 4', b'schema_version = 3'),
            (b'host_api = "=1.3.0"', b'host_api = "^1.2"'),
            (b'sigil = "=0.34.0"', b'sigil = ">=0.34.0"'),
            (b'grpc-unary = true', b'grpc-unary = true\nnetwork = true'),
            (b'target/component/temporal.wasm', b'other.wasm'),
        ]
        for old, new in substitutions:
            with self.subTest(new=new), self.assertRaises(ValueError):
                pack.validate_manifest(source.replace(old, new))

    def test_member_metadata_and_terminator(self):
        archive = pack.member("plugin.toml", b"benign") + bytes(1024)
        with tarfile.open(fileobj=io.BytesIO(archive), mode="r:") as reader:
            info = reader.getmembers()[0]
            self.assertEqual((info.name, info.uid, info.gid, info.mtime, info.mode),
                             ("plugin.toml", 0, 0, 0, 0o644))
            self.assertEqual(reader.extractfile(info).read(), b"benign")
        self.assertEqual(archive[257:265], b"ustar\0" + b"00")


@unittest.skipUnless(os.environ.get("SIGIL_SOURCE_BINARY"),
                     "set SIGIL_SOURCE_BINARY to run local pack integration with an explicit source build")
class PackIntegrationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.scratch = tempfile.TemporaryDirectory(prefix="temporal-pack-tests-")
        cls.stage = Path(cls.scratch.name)
        for name in pack.contracts.INPUTS:
            target = cls.stage / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes((ROOT / name).read_bytes())
        (cls.stage / "plugin.toml").write_bytes((ROOT / "plugin.local.toml").read_bytes())
        component = cls.stage / pack.COMPONENT
        component.parent.mkdir(parents=True)
        tool = pack.contracts.wasm_tools()
        pack.contracts.run(tool, "component", "embed", str(cls.stage / "wit"),
                           "--world", pack.contracts.WORLD, "--dummy", "-o", str(cls.stage / "dummy.wasm"))
        pack.contracts.run(tool, "component", "new", str(cls.stage / "dummy.wasm"), "-o", str(component))
        cls.sigil = Path(os.environ["SIGIL_SOURCE_BINARY"])

    @classmethod
    def tearDownClass(cls):
        cls.scratch.cleanup()

    def test_reproducible_and_matches_sigil_canonical_package(self):
        first = pack.pack(self.stage / "plugin.toml", self.stage / "first", self.sigil)
        second = pack.pack(self.stage / "plugin.toml", self.stage / "second", self.sigil)
        self.assertEqual(first.read_bytes(), second.read_bytes())
        for name in ("SHA256SUMS", "LOCAL-EVIDENCE.json"):
            self.assertEqual((first.parent / name).read_bytes(), (second.parent / name).read_bytes())
        pack.contracts.run(str(self.sigil), "plugin", "pack", str(self.stage / "plugin.toml"),
                           "--output-dir", str(self.stage / "sigil-canonical"))
        self.assertEqual(first.read_bytes(), (self.stage / "sigil-canonical" / first.name).read_bytes())
        evidence = json.loads((first.parent / "LOCAL-EVIDENCE.json").read_text())
        self.assertFalse(evidence["provenance_authority"])
        self.assertFalse(evidence["project_lock_authority"])
        self.assertFalse(evidence["stable_sigil_support_claim"])
        self.assertEqual(evidence["package_sha256"], hashlib.sha256(first.read_bytes()).hexdigest())
        self.assertEqual({path.name for path in first.parent.iterdir()},
                         {first.name, "SHA256SUMS", "LOCAL-EVIDENCE.json"})
        with self.assertRaises(FileExistsError):
            pack.pack(self.stage / "plugin.toml", first.parent, self.sigil)

    def test_dummy_exact_component_contract_and_empty_component_rejection(self):
        pack.contracts.check_component(self.stage / pack.COMPONENT, self.stage)
        empty = self.stage / "empty.wat"
        empty.write_text("(component)\n")
        with self.assertRaises((ValueError, subprocess.CalledProcessError)):
            pack.contracts.check_component(empty, self.stage)

    def test_validator_failure_cli_reports_diagnostics_without_publishing_outputs(self):
        real_run = pack.contracts.run
        for phase in ("manifest", "archive"):
            with self.subTest(phase=phase):
                output = self.stage / f"rejected-{phase}"
                stdout, stderr = io.StringIO(), io.StringIO()

                def reject_validator(*args):
                    if args[1:3] == ("plugin", "validate"):
                        manifest = args[-1].endswith("plugin.toml")
                        if manifest == (phase == "manifest"):
                            raise subprocess.CalledProcessError(
                                7, args, output=b"validation summary\n",
                                stderr=b"ordinary validation error\n")
                    return real_run(*args)

                argv = ["pack.py", str(self.stage / "plugin.toml"), str(output), "--sigil", str(self.sigil)]
                with patch.object(pack.contracts, "run", side_effect=reject_validator), \
                        patch.object(sys, "argv", argv), redirect_stdout(stdout), redirect_stderr(stderr), \
                        self.assertRaises(SystemExit) as raised:
                    pack.contracts.cli(pack.main)
                self.assertEqual(raised.exception.code, 7)
                self.assertEqual(stdout.getvalue(), "")
                self.assertIn("--- stdout ---\nvalidation summary\n", stderr.getvalue())
                self.assertIn("--- stderr ---\nordinary validation error\n", stderr.getvalue())
                self.assertNotIn("Traceback", stderr.getvalue())
                self.assertFalse(output.exists(), "failed validation must publish no archive or evidence")


if __name__ == "__main__":
    unittest.main()
