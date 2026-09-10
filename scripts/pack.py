#!/usr/bin/env python3
"""Build a canonical LOCAL NON-GATING archive, never release provenance."""

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import tomllib

sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location("contracts", Path(__file__).with_name("check-contracts.py"))
contracts = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(contracts)
COMPONENT = "target/component/temporal.wasm"
MAX_MANIFEST = 1024 * 1024
MAX_COMPONENT = 128 * 1024 * 1024


def ordinary_bytes(path, limit):
    if path.is_symlink() or not path.is_file():
        raise ValueError(f"input must be an ordinary file: {path.name}")
    if path.stat().st_size > limit:
        raise ValueError("input exceeds package limit")
    with path.open("rb") as handle:
        data = handle.read(limit + 1)
    if len(data) > limit:
        raise ValueError("input exceeds package limit")
    return data


def validate_manifest(data):
    manifest = tomllib.loads(data.decode("utf-8"))
    if set(manifest) != {"schema_version", "name", "version", "description", "license",
                         "component", "requires", "capabilities", "repository"}:
        raise ValueError("unexpected local manifest fields")
    if type(manifest["schema_version"]) is not int or manifest["schema_version"] != 4:
        raise ValueError("local manifest requires schema 4")
    if manifest["name"] != "temporal" or manifest["license"] != "MIT":
        raise ValueError("wrong plugin identity")
    if not isinstance(manifest["description"], str) or not manifest["description"]:
        raise ValueError("description is required")
    version = manifest["version"]
    if not isinstance(version, str) or not re.fullmatch(r"0\.1\.0-dev\.[1-9][0-9]*", version):
        raise ValueError("only explicit 0.1.0-dev.N local versions are admitted")
    if manifest["component"] != {"file": COMPONENT, "entrypoint": contracts.CLIENT}:
        raise ValueError("component path or entrypoint differs from the local contract")
    if manifest["requires"] != {"sigil": "=0.34.0", "host_api": "=1.3.0"}:
        raise ValueError("local source-build requirements must not claim a stable supporting release")
    capabilities = manifest["capabilities"]
    if capabilities != {"grpc-unary": True} or capabilities["grpc-unary"] is not True:
        raise ValueError("only semantic grpc-unary capability is admitted")
    if manifest["repository"] != {"source": "github:sigil-plugins/temporal"}:
        raise ValueError("wrong intended source identity")
    return manifest


def member(path, data):
    # Exact two-member POSIX ustar layout used by Sigil and the plugin template.
    name = path.encode("ascii")
    if len(name) > 100 or path not in ("plugin.toml", COMPONENT):
        raise ValueError("unapproved archive member")
    block = bytearray(512)
    block[:len(name)] = name
    for start, size, value in ((100, 8, 0o644), (108, 8, 0), (116, 8, 0),
                               (124, 12, len(data)), (136, 12, 0)):
        digits = f"{value:0{size - 1}o}".encode("ascii")
        if len(digits) != size - 1:
            raise ValueError("tar numeric overflow")
        block[start:start + size] = digits + b"\0"
    block[148:156] = b"        "
    block[156] = ord("0")
    block[257:263] = b"ustar\0"
    block[263:265] = b"00"
    block[148:156] = f"{sum(block):06o}".encode("ascii") + b"\0 "
    return bytes(block) + data + bytes((-len(data)) % 512)


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def b3(path):
    value = contracts.run(os.environ.get("B3SUM", "b3sum"), "--no-names", str(path)).strip()
    if re.fullmatch(r"[0-9a-f]{64}", value) is None:
        raise ValueError("invalid BLAKE3 output")
    return value


def pack(manifest_path, output_dir, sigil):
    if manifest_path.name not in ("plugin.toml", "plugin.local.toml"):
        raise ValueError("input must be named plugin.toml or plugin.local.toml")
    root = manifest_path.parent.resolve()
    contracts.check_sources(root)
    manifest_bytes = ordinary_bytes(manifest_path, MAX_MANIFEST)
    manifest = validate_manifest(manifest_bytes)
    component_path = root / COMPONENT
    # Reject symlinked directories too; inputs must be inside this candidate.
    if any((root / prefix).is_symlink() for prefix in ("target", "target/component")):
        raise ValueError("component directories must not be symlinks")
    component_bytes = ordinary_bytes(component_path, MAX_COMPONENT)
    sigil = sigil.resolve(strict=True)
    with sigil.open("rb") as binary:
        sigil_sha = hashlib.file_digest(binary, "sha256").hexdigest()
    sigil_version = contracts.run(str(sigil), "--version").strip()
    if sigil_version != "sigil 0.34.0":
        raise ValueError("local candidate expects the explicitly selected 0.34.0 source build")
    zstd = os.environ.get("ZSTD", "zstd")
    if not re.search(r"\bv1\.5\.7\b", contracts.run(zstd, "--version")):
        raise ValueError("zstd 1.5.7 is required for reproducible local bytes")
    # One immutable snapshot is checked, hashed and packed: later input edits
    # cannot replace the bytes between contract validation and compression.
    with tempfile.TemporaryDirectory(prefix="temporal-local-pack-") as temporary:
        stage = Path(temporary)
        for name in contracts.INPUTS:
            destination = stage / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes((root / name).read_bytes())
        (stage / COMPONENT).parent.mkdir(parents=True, exist_ok=True)
        (stage / COMPONENT).write_bytes(component_bytes)
        (stage / "plugin.toml").write_bytes(manifest_bytes)
        contracts.check_component(stage / COMPONENT, stage)
        contracts.run(str(sigil), "plugin", "validate", str(stage / "plugin.toml"))
        tar = member("plugin.toml", manifest_bytes) + member(COMPONENT, component_bytes) + bytes(1024)
        compressed = subprocess.run(
            [zstd, "-q", "-10", "--check", "-c"], input=tar,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        if compressed.returncode != 0:
            # Unlike validator stdout, compression stdout is partial binary
            # archive data, not a diagnostic suitable for the terminal.
            raise subprocess.CalledProcessError(
                compressed.returncode, compressed.args, stderr=compressed.stderr)
        archive = compressed.stdout
        if len(archive) > 64 * 1024 * 1024:
            raise ValueError("compressed package limit exceeded")
        name = f"temporal-{manifest['version']}.sigil-plugin.tar.zst"
        (stage / name).write_bytes(archive)
        contracts.run(str(sigil), "plugin", "validate", str(stage / name))
        evidence = {
            "schema": "sigil.temporal-local-package.v1",
            "status": "NON-GATING-LOCAL-DEVELOPMENT-ONLY",
            "provenance_authority": False,
            "project_lock_authority": False,
            "stable_sigil_support_claim": False,
            "sigil_source_reference": contracts.SIGIL_SOURCE,
            "sigil_product_reference": contracts.SIGIL_PRODUCT,
            "sdk_source_reference": contracts.SDK_SOURCE,
            "source_references_prove_binary_origin": False,
            "sigil_binary_sha256": sigil_sha,
            "sigil_version": sigil_version,
            "packer_checkout_commit": contracts.run("git", "-C", str(contracts.ROOT), "rev-parse", "HEAD").strip(),
            "packer_checkout_dirty": bool(contracts.run("git", "-C", str(contracts.ROOT), "status", "--porcelain").strip()),
            "component_source_origin_verified": False,
            "contract_inputs_sha256": contracts.INPUTS,
            "manifest_sha256": sha256(manifest_bytes),
            "component_sha256": sha256(component_bytes),
            "manifest_blake3": b3(stage / "plugin.toml"),
            "component_blake3": b3(stage / COMPONENT),
            "package_sha256": sha256(archive),
            "package_blake3": b3(stage / name),
            "wasm_tools_version": "1.252.0",
            "zstd_version": "1.5.7",
        }
        with sigil.open("rb") as binary:
            if hashlib.file_digest(binary, "sha256").hexdigest() != sigil_sha:
                raise ValueError("selected Sigil binary changed during package checks")
        # Never overwrite an earlier local candidate or mix it with release files.
        output_dir.mkdir(parents=True, exist_ok=False)
        (output_dir / name).write_bytes(archive)
        (output_dir / "SHA256SUMS").write_text(f"{sha256(archive)}  {name}\n", encoding="ascii")
        (output_dir / "LOCAL-EVIDENCE.json").write_text(
            json.dumps(evidence, sort_keys=True, separators=(",", ":")) + "\n", encoding="ascii")
    return output_dir / name


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path)
    parser.add_argument("output_dir", type=Path, help="new directory; existing candidates are never overwritten")
    parser.add_argument("--sigil", required=True, type=Path, help="explicit reviewed source-build binary, NOT stable 0.34.0")
    args = parser.parse_args()
    print(pack(args.manifest, args.output_dir, args.sigil))
    print("NON-GATING local archive: no release manifest, provenance or project-lock authority")


if __name__ == "__main__":
    contracts.cli(main)
