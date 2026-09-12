"""Release packaging contracts; dummy fixtures are not Temporal acceptance."""

import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location("release_pack", ROOT / "scripts/release-pack.py")
release = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(release)


class ReleaseContractTests(unittest.TestCase):
    def test_release_manifest_keeps_local_and_official_versions_separate(self):
        data = (ROOT / "plugin.toml").read_bytes()
        self.assertEqual(release.validate_manifest(data)["version"], "0.1.1")
        version_line = b'version = "0.1.1"'
        release.validate_manifest(data.replace(version_line, b'version = "0.1.1-rc.1"'))
        for version in (b"0.1.0", b"0.1.0-rc.1", b"0.1.0-dev.1", b"0.1.1-dev.1",
                        b"0.1.1-rc.0", b"0.1.1-rc.01", b"0.1.1+build", b"0.1.2", b"0.2.0"):
            with self.subTest(version=version), self.assertRaises(ValueError):
                release.validate_manifest(data.replace(version_line, b'version = "' + version + b'"'))
        with self.assertRaises(ValueError):
            release.validate_manifest((ROOT / "plugin.local.toml").read_bytes())
        with self.assertRaises(ValueError):
            release.local.validate_manifest(data)

    def test_release_manifest_requires_exact_support_and_capability(self):
        data = (ROOT / "plugin.toml").read_bytes()
        self.assertEqual(release.validate_manifest(data)["requires"],
                         {"sigil": ">=0.35.0", "host_api": "=1.3.0"})
        for old, new in ((b">=0.35.0", b">=0.34.0"),
                         (b">=0.35.0", b">=0.35.0, <0.36.0"),
                         (b">=0.35.0", b"*"),
                         (b">=0.35.0", b"invalid"),
                         (b"=1.3.0", b"=1.2.0"),
                         (b"=1.3.0", b"^1.3.0"),
                         (b"grpc-unary = true", b"grpc-unary = true\nnetwork = true")):
            with self.subTest(new=new), self.assertRaises(ValueError):
                release.validate_manifest(data.replace(old, new))

    def test_publication_workflow_is_exact_candidate_main_only_no_build(self):
        publish = (ROOT / ".github/workflows/publish-release.yml").read_text()
        for required in (".run_attempt == 1", 'test "$GITHUB_RUN_ATTEMPT" = 1',
                         'test "$GITHUB_REF" = refs/heads/main', 'test "$GITHUB_SHA" = "$SOURCE_COMMIT"',
                         "environment: release", "cancel-in-progress: false", "id-token: write",
                         'test "$IMMUTABLE_RELEASES_VERIFIED" = true',
                         "--prerelease=\"$prerelease\"", ".isPrerelease == $prerelease",
                         'test "$immutable" = true'):
            self.assertIn(required, publish)
        for forbidden in ("cargo build", "cargo install", "just release-dist", "pull_request_target"):
            self.assertNotIn(forbidden, publish)
        self.assertIn('^0\\.1\\.1(-rc\\.[1-9][0-9]*)?$', publish)
        for required in ('existing tag burns', 'existing release burns',
                         'preexisting-attestations.json', 'test "$status" = 404', '--draft',
                         'sha256sum --check --strict'):
            self.assertIn(required, publish)

    def test_publisher_and_packer_admit_only_the_new_version_family(self):
        publish = (ROOT / ".github/workflows/publish-release.yml").read_text()
        rules = re.findall(r'^\s*(\[\[ "\$VERSION" =~ .* \]\])$', publish, re.MULTILINE)
        self.assertEqual(len(rules), 1)
        data = (ROOT / "plugin.toml").read_bytes()
        for version in ("0.1.1", "0.1.1-rc.1", "0.1.1-rc.23", "0.1.0", "0.1.0-rc.1",
                        "0.1.1-dev.1", "0.1.1-rc.0", "0.1.1-rc.01", "0.1.1+build",
                        "0.1.1-rc.1+build", "v0.1.1", "0.1.2", "0.2.0", "0.1.1\n"):
            with self.subTest(version=version):
                # Execute only the workflow's version predicate, never its publisher.
                admitted = subprocess.run(["bash", "-c", rules[0]],
                                          env={**os.environ, "VERSION": version},
                                          capture_output=True, timeout=5).returncode == 0
                candidate = data.replace(b'version = "0.1.1"',
                                         f'version = "{version}"'.encode())
                if version in ("0.1.1", "0.1.1-rc.1", "0.1.1-rc.23"):
                    self.assertTrue(admitted)
                    self.assertEqual(release.validate_manifest(candidate)["version"], version)
                else:
                    self.assertFalse(admitted)
                    with self.assertRaises(ValueError):
                        release.validate_manifest(candidate)

    def test_package_patch_keeps_the_frozen_client_and_minimum_validator(self):
        manifest = release.validate_manifest((ROOT / "plugin.toml").read_bytes())
        crate = release.tomllib.loads((ROOT / "Cargo.toml").read_text())
        # Stable plugin packaging is not a Rust crate release. Preserve the
        # accepted, unpublished runtime crate and measure its component bytes.
        self.assertEqual(crate["package"]["version"], "0.1.1-rc.1")
        self.assertFalse(crate["package"]["publish"])
        contract = json.loads((ROOT / "conformance/contract.json").read_text())
        self.assertEqual(manifest["component"]["entrypoint"], contract["identity"]["entrypoint"])
        self.assertEqual(manifest["schema_version"], contract["identity"]["manifest_schema"])
        self.assertIn(f'const IDENTITY: &str = "{contract["authority"]["identity"]}";',
                      (ROOT / "src/client.rs").read_text())
        self.assertIn('package sigil:temporal@0.1.0;', (ROOT / "wit/temporal.wit").read_text())
        self.assertIn('import sigil:host/grpc-unary@1.3.0;', (ROOT / "wit/temporal.wit").read_text())
        host = json.loads((ROOT / "scripts/release-tools.json").read_text())
        self.assertEqual(host["sigil_version"], "0.35.0")
        release.host_identity.validate_spec(host)


@unittest.skipUnless(os.environ.get("SIGIL_RELEASE_BINARY"),
                     "set SIGIL_RELEASE_BINARY for real stable-host release pack integration")
class ReleasePackIntegrationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.scratch = tempfile.TemporaryDirectory(prefix="temporal-release-tests-")
        cls.root = Path(cls.scratch.name)
        for name in release.contracts.INPUTS:
            target = cls.root / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes((ROOT / name).read_bytes())
        (cls.root / "plugin.toml").write_bytes((ROOT / "plugin.toml").read_bytes())
        (cls.root / "scripts").mkdir()
        (cls.root / "scripts/release-tools.json").write_bytes((ROOT / "scripts/release-tools.json").read_bytes())
        (cls.root / ".gitignore").write_text("/target/\n/dist*/\n")
        release.contracts.run("git", "init", "--initial-branch=main", str(cls.root))
        release.contracts.run("git", "-C", str(cls.root), "add", ".")
        release.contracts.run("git", "-C", str(cls.root), "-c", "user.name=Package Fixture",
                              "-c", "user.email=fixture@example.invalid", "-c", "commit.gpgsign=false",
                              "commit", "-m", "benign release fixture")
        cls.commit = release.contracts.run("git", "-C", str(cls.root), "rev-parse", "HEAD").strip()
        component = cls.root / release.local.COMPONENT
        component.parent.mkdir(parents=True)
        tool = release.contracts.wasm_tools()
        release.contracts.run(tool, "component", "embed", str(cls.root / "wit"),
                              "--world", release.contracts.WORLD, "--dummy", "-o", str(cls.root / "target/dummy.wasm"))
        release.contracts.run(tool, "component", "new", str(cls.root / "target/dummy.wasm"), "-o", str(component))
        cls.sigil = Path(os.environ["SIGIL_RELEASE_BINARY"])

    @classmethod
    def tearDownClass(cls):
        cls.scratch.cleanup()

    def test_exact_canonical_identity_and_no_overwrite(self):
        first = release.pack(self.root, self.root / "dist-first", self.sigil, self.commit)
        second = release.pack(self.root, self.root / "dist-second", self.sigil, self.commit)
        self.assertEqual(first.read_bytes(), second.read_bytes())
        raw = (first.parent / "release-manifest.json").read_bytes()
        self.assertEqual(raw, (second.parent / "release-manifest.json").read_bytes())
        value = json.loads(raw)
        self.assertEqual(value["source_commit"], self.commit)
        self.assertEqual(value["package_sha256"], "sha256:" + hashlib.sha256(first.read_bytes()).hexdigest())
        self.assertEqual(raw, json.dumps(value, sort_keys=True, separators=(",", ":")).encode("ascii"))
        self.assertEqual({p.name for p in first.parent.iterdir()}, {first.name, "SHA256SUMS", "release-manifest.json"})
        _, expected = release.host_identity.load_spec(self.root)
        release.host_identity.run_checked(self.sigil.resolve(), expected, release.contracts.run,
                                          "plugin", "pack", str(self.root / "plugin.toml"),
                                          "--output-dir", str(self.root / "dist-canonical"))
        self.assertEqual(first.read_bytes(), (self.root / "dist-canonical" / first.name).read_bytes())
        with self.assertRaises(FileExistsError):
            release.pack(self.root, first.parent, self.sigil, self.commit)

    def test_multiblock_component_matches_the_public_canonical_writer(self):
        def leb(value):
            encoded = bytearray()
            while value >= 128:
                encoded.append((value & 127) | 128)
                value >>= 7
            encoded.append(value)
            return bytes(encoded)

        component = self.root / release.local.COMPONENT
        original = component.read_bytes()
        # An inert custom section makes this a valid, multi-block component.
        # Its deterministic mixed content exercises compression choices that
        # the small dummy fixture does not. It adds no guest code or imports.
        name = b"compression-parity-fixture"
        padding = b"".join(
            f"function-{index % 4001}: shared-prefix {index % 23} "
            f"shared-suffix-{index % 1999}\n".encode()
            for index in range(12000))
        section = leb(len(name)) + name + padding
        enlarged = original + b"\x00" + leb(len(section)) + section
        self.assertGreater(len(enlarged), 4 * 128 * 1024)
        component.write_bytes(enlarged)
        try:
            first = release.pack(self.root, self.root / "dist-large-first", self.sigil, self.commit)
            second = release.pack(self.root, self.root / "dist-large-second", self.sigil, self.commit)
            self.assertEqual(first.read_bytes(), second.read_bytes())
            _, expected = release.host_identity.load_spec(self.root)
            release.host_identity.run_checked(self.sigil.resolve(), expected, release.contracts.run,
                                              "plugin", "pack", str(self.root / "plugin.toml"),
                                              "--output-dir", str(self.root / "dist-large-canonical"))
            canonical = self.root / "dist-large-canonical" / first.name
            self.assertEqual(hashlib.sha256(first.read_bytes()).hexdigest(),
                             hashlib.sha256(canonical.read_bytes()).hexdigest())
            with self.assertRaises(FileExistsError):
                release.pack(self.root, first.parent, self.sigil, self.commit)
        finally:
            component.write_bytes(original)

    def test_wrong_source_dirty_tree_and_old_host_fail_before_output(self):
        output = self.root / "dist-rejected"
        with self.assertRaises(ValueError):
            release.pack(self.root, output, self.sigil, "0" * 40)
        marker = self.root / "ordinary-untracked-note"
        marker.write_text("ordinary fixture change\n")
        try:
            with self.assertRaises(ValueError):
                release.pack(self.root, output, self.sigil, self.commit)
        finally:
            marker.unlink()
        real_run = release.contracts.run

        def older_host(*args):
            return "sigil 0.34.0\n" if args == (str(self.sigil.resolve()), "--version") else real_run(*args)

        with patch.object(release.contracts, "run", side_effect=older_host), self.assertRaises(ValueError):
            release.pack(self.root, output, self.sigil, self.commit)
        self.assertFalse(output.exists())

    def test_each_validator_failure_produces_no_candidate(self):
        real_run = release.contracts.run
        for phase in ("manifest", "archive"):
            output = self.root / f"dist-rejected-{phase}"

            def reject_validator(*args):
                if args[1:3] == ("plugin", "validate") and args[-1].endswith("plugin.toml") == (phase == "manifest"):
                    raise subprocess.CalledProcessError(7, args, stderr=b"ordinary fixture rejection")
                return real_run(*args)

            with self.subTest(phase=phase), patch.object(release.contracts, "run", side_effect=reject_validator), \
                    self.assertRaises(subprocess.CalledProcessError):
                release.pack(self.root, output, self.sigil, self.commit)
            self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
