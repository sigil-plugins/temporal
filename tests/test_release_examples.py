"""Offline operator-template contracts; parser success is not service acceptance."""

import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import tomllib
import unittest

ROOT = Path(__file__).resolve().parent.parent
EXAMPLE = ROOT / "examples/operator-grant.toml"


class OperatorExampleTests(unittest.TestCase):
    def test_fixed_values_match_the_frozen_contract(self):
        config = tomllib.loads(EXAMPLE.read_text())
        contract = json.loads((ROOT / "conformance/contract.json").read_text())
        plugins = config["plugins"]
        grant = plugins["grants"]["temporal"]
        profile = grant["grpc"]["workflow"]
        endpoint = grant["network"][profile["endpoint"]]
        self.assertEqual(profile["rpcs"], {
            alias: {key: rpc[key] for key in ("path", "kind")}
            for alias, rpc in contract["rpcs"].items()
        })
        self.assertEqual(profile["request_policy"]["identity"], contract["authority"]["identity"])
        self.assertEqual(profile["request_policy"]["kind"], contract["authority"]["request_validator"])
        metadata = {item["name"]: item["value"] for item in profile["request_metadata"]}
        self.assertEqual(len(metadata), len(profile["request_metadata"]))
        self.assertEqual(set(metadata), set(contract["authority"]["frozen_request_metadata"]))
        self.assertEqual(metadata["temporal-namespace"], profile["request_policy"]["namespace"])
        close_seconds = contract["rpcs"]["history"]["close_event_timeout_millis"] // 1000
        self.assertEqual(profile["max_call_seconds"], close_seconds)
        self.assertGreater(plugins["runtime"]["max_call_seconds"], close_seconds)
        self.assertGreater(int(endpoint["io_timeout"].removesuffix("s")), close_seconds)
        self.assertEqual(profile["max_response_bytes"], "4MiB")
        self.assertEqual(4 * 1024 * 1024, contract["limits"]["response_bytes"])
        self.assertEqual(profile["max_request_bytes"], "1MiB")
        self.assertLessEqual(1024 * 1024, contract["limits"]["request_bytes"])
        self.assertEqual(plugins["runtime"]["max_memory"], "256MiB")
        # At least one ceiling-sized request and response plus framing fit.
        self.assertGreater(int(endpoint["max_bytes"].removesuffix("MiB")), 1 + 4)

    def test_template_has_no_raw_guest_or_third_party_authority(self):
        config = tomllib.loads(EXAMPLE.read_text())
        plugins = config["plugins"]
        source = json.loads((ROOT / "conformance/contract.json").read_text())["identity"]["source"]
        self.assertEqual(plugins["require"]["temporal"], "=0.1.0")
        self.assertNotIn("allow_third_party", plugins)
        self.assertEqual(plugins["trust"], {"capability_allowlist": {
            "grpc-unary": [source], "network": [], "secrets": [],
            "random": [], "entropy": [], "log": [], "sigv4": [],
        }})
        grant = plugins["grants"]["temporal"]
        self.assertEqual(set(grant), {"network", "grpc"})
        profile = grant["grpc"]["workflow"]
        self.assertNotIn("bearer_secret", profile)
        self.assertEqual(profile["transport"], "h2-tls")
        self.assertEqual(profile["response_metadata"], [])
        endpoint = grant["network"]["workflow"]
        self.assertEqual(endpoint["tls"], "direct")
        self.assertTrue(endpoint["tls_server_name"].endswith(".invalid"))
        self.assertEqual(profile["authority"], endpoint["tls_server_name"] + ":7233")
        self.assertTrue(endpoint["target"].startswith("replace-me-"))
        self.assertNotIn("plugin_routes", config.get("eval", {}))

    @unittest.skipUnless(os.environ.get("SIGIL_RELEASE_BINARY"),
                         "set SIGIL_RELEASE_BINARY for pinned stable-host config parsing")
    def test_public_host_policy_defaults_and_endpoint_validation(self):
        binary = Path(os.environ["SIGIL_RELEASE_BINARY"]).resolve()
        expected = json.loads((ROOT / "scripts/release-tools.json").read_text())["sigil_binary_sha256"]

        def check_binary():
            with binary.open("rb") as handle:
                self.assertEqual(hashlib.file_digest(handle, "sha256").hexdigest(), expected)

        with tempfile.TemporaryDirectory(prefix="temporal-grant-test-") as scratch:
            root = Path(scratch)
            (root / ".sigil").mkdir()
            config = root / ".sigil/sigil.toml"
            env = {key: value for key, value in os.environ.items()
                   if not key.startswith("SIGIL_") and key not in ("GH_TOKEN", "GITHUB_TOKEN")}
            env.update(SIGIL_DATA_DIR=str(root / "data"), SIGIL_CACHE_DIR=str(root / "cache"))
            # Enumerate every capability and its default from this exact host,
            # rather than assuming omitted TOML fields mean an empty allowlist.
            check_binary()
            try:
                schema_result = subprocess.run([str(binary), "schema", "config"], cwd=root,
                                               env=env, capture_output=True, text=True, timeout=30)
            finally:
                check_binary()
            self.assertEqual(schema_result.returncode, 0, schema_result.stderr)
            capability_schema = json.loads(schema_result.stdout)["$defs"][
                "PluginCapabilityAllowlistConfig"]["properties"]
            defaults = {name: field["default"] for name, field in capability_schema.items()}
            declared = tomllib.loads(EXAMPLE.read_text())["plugins"]["trust"]["capability_allowlist"]
            self.assertEqual(set(declared), set(defaults), "all host capability fields must be explicit")
            expected_policy = {name: [] for name in defaults}
            expected_policy["grpc-unary"] = ["github:sigil-plugins/temporal"]
            self.assertEqual(defaults | declared, expected_policy)
            # Negative control: each omitted unused field would broaden the
            # effective policy under the pinned host's actual schema defaults.
            for name in set(defaults) - {"grpc-unary"}:
                omitted = {key: value for key, value in declared.items() if key != name}
                self.assertNotEqual(defaults | omitted, expected_policy)
            # No --service flag: scenario list must load/validate the config to
            # obtain default_service. It neither installs nor executes plugins.
            for valid in (True, False):
                with self.subTest(valid=valid):
                    text = EXAMPLE.read_text()
                    if not valid:
                        text = text.replace('endpoint = "workflow"', 'endpoint = "missing"')
                    config.write_text(text)
                    check_binary()
                    try:
                        result = subprocess.run([str(binary), "scenario", "list"], cwd=root,
                                                env=env, capture_output=True, text=True, timeout=30)
                    finally:
                        check_binary()
                    if valid:
                        self.assertEqual(result.returncode, 0, result.stderr)
                        self.assertIn("No scenarios found for service: replace-me-service", result.stdout)
                    else:
                        self.assertNotEqual(result.returncode, 0)
                        self.assertIn("must name an existing network grant", result.stderr)
