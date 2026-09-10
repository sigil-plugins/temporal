#!/usr/bin/env python3
"""Create an unpublished P6 candidate; only the publication workflow attests it."""

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
SPEC = importlib.util.spec_from_file_location("local_pack", Path(__file__).with_name("pack.py"))
local = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(local)
contracts = local.contracts


def validate_manifest(data):
    parsed = tomllib.loads(data.decode("utf-8"))
    version = parsed.get("version")
    if not isinstance(version, str) or not re.fullmatch(r"0\.1\.0(?:-rc\.[1-9][0-9]*)?", version):
        raise ValueError("release version must be 0.1.0 or 0.1.0-rc.N")
    if parsed.get("requires") != {"sigil": ">=0.35.0, <0.36.0", "host_api": "=1.3.0"}:
        raise ValueError("release requires supporting stable Sigil 0.35 and Host API 1.3")
    # Reuse the closed identity/path/capability validator, without granting
    # local manifests any release authority or relaxing their version check.
    normalized = data.replace(f'version = "{version}"'.encode(), b'version = "0.1.0-dev.1"')
    normalized = normalized.replace(b'sigil = ">=0.35.0, <0.36.0"', b'sigil = "=0.34.0"')
    local.validate_manifest(normalized)
    return parsed


def pack(root, output, sigil, source_commit):
    root = root.resolve()
    if re.fullmatch(r"[0-9a-f]{40}", source_commit) is None:
        raise ValueError("source commit must be exactly 40 lowercase hex characters")
    if contracts.run("git", "-C", str(root), "rev-parse", "HEAD").strip() != source_commit:
        raise ValueError("release source commit does not match checkout")
    if contracts.run("git", "-C", str(root), "status", "--porcelain", "--untracked-files=normal").strip():
        raise ValueError("release candidate requires a clean checkout")
    contracts.check_sources(root)
    manifest_bytes = local.ordinary_bytes(root / "plugin.toml", local.MAX_MANIFEST)
    manifest = validate_manifest(manifest_bytes)
    if any((root / prefix).is_symlink() for prefix in ("target", "target/component")):
        raise ValueError("component directories must not be symlinks")
    component_bytes = local.ordinary_bytes(root / local.COMPONENT, local.MAX_COMPONENT)
    sigil = sigil.resolve(strict=True)
    with sigil.open("rb") as binary:
        binary_sha = hashlib.file_digest(binary, "sha256").hexdigest()
    if contracts.run(str(sigil), "--version").strip() != "sigil 0.35.0":
        raise ValueError("release checks require stable Sigil 0.35.0")
    zstd = os.environ.get("ZSTD", "zstd")
    if not re.search(r"\bv1\.5\.7\b", contracts.run(zstd, "--version")):
        raise ValueError("release bytes require zstd 1.5.7")
    with tempfile.TemporaryDirectory(prefix="temporal-release-pack-") as temporary:
        stage = Path(temporary)
        for name in contracts.INPUTS:
            path = stage / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes((root / name).read_bytes())
        (stage / local.COMPONENT).parent.mkdir(parents=True, exist_ok=True)
        (stage / local.COMPONENT).write_bytes(component_bytes)
        (stage / "plugin.toml").write_bytes(manifest_bytes)
        contracts.check_component(stage / local.COMPONENT, stage)
        contracts.run(str(sigil), "plugin", "validate", str(stage / "plugin.toml"))
        tar = local.member("plugin.toml", manifest_bytes) + local.member(local.COMPONENT, component_bytes) + bytes(1024)
        result = subprocess.run([zstd, "-q", "-10", "--check", "-c"], input=tar,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        if result.returncode:
            raise subprocess.CalledProcessError(result.returncode, result.args, stderr=result.stderr)
        archive = result.stdout
        if len(archive) > 64 * 1024 * 1024:
            raise ValueError("compressed package limit exceeded")
        name = f"temporal-{manifest['version']}.sigil-plugin.tar.zst"
        (stage / name).write_bytes(archive)
        contracts.run(str(sigil), "plugin", "validate", str(stage / name))
        identity = {
            "schema_version": 1,
            "source": manifest["repository"]["source"],
            "source_commit": source_commit,
            "name": "temporal", "version": manifest["version"], "asset_name": name,
            "package_sha256": f"sha256:{local.sha256(archive)}",
            "package_blake3": f"blake3:{local.b3(stage / name)}",
            "manifest_blake3": f"blake3:{local.b3(stage / 'plugin.toml')}",
            "component_blake3": f"blake3:{local.b3(stage / local.COMPONENT)}",
        }
        with sigil.open("rb") as binary:
            if hashlib.file_digest(binary, "sha256").hexdigest() != binary_sha:
                raise ValueError("Sigil binary changed during validation")
        if contracts.run("git", "-C", str(root), "rev-parse", "HEAD").strip() != source_commit:
            raise ValueError("checkout changed during validation")
        if contracts.run("git", "-C", str(root), "status", "--porcelain", "--untracked-files=normal").strip():
            raise ValueError("checkout changed during validation")
        output.mkdir(parents=True, exist_ok=False)
        (output / name).write_bytes(archive)
        (output / "SHA256SUMS").write_text(f"{local.sha256(archive)}  {name}\n", encoding="ascii")
        (output / "release-manifest.json").write_bytes(json.dumps(
            identity, allow_nan=False, ensure_ascii=True, sort_keys=True,
            separators=(",", ":")).encode("ascii"))
    return output / name


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--sigil", required=True, type=Path)
    parser.add_argument("--source-commit", required=True)
    args = parser.parse_args()
    print(pack(contracts.ROOT, args.output, args.sigil, args.source_commit))
    print("Unpublished candidate: no attestation, installation authority or CAPI acceptance")


if __name__ == "__main__":
    contracts.cli(main)
