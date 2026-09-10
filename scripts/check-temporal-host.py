"""Real decoder boundary on the pinned host and an existing official project.

Does not install/acquire plugins, add grants, edit the project, or contact Temporal.
Normal Sigil loading still requires the module's declared wasm.temporal capability.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess
import sys

sys.dont_write_bytecode = True

ROOT = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location("host_identity", ROOT / "scripts/release-identity.py")
identity = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(identity)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sigil", type=Path, required=True)
    parser.add_argument("--project", type=Path, required=True)
    args = parser.parse_args()
    project = args.project.resolve(strict=True)
    if not (project / ".sigil/sigil.plugins.lock").is_file():
        parser.error("--project must have a normal, already-synced official Temporal lock")
    _, expected = identity.load_spec(ROOT)

    def run(*command):
        result = subprocess.run(command, cwd=project, capture_output=True, text=True, check=True)
        return result.stdout

    output = identity.run_checked(
        args.sigil.resolve(strict=True), expected, run, "run",
        str(ROOT / "tests/temporal_host.lua"), "--lib-dir", str(ROOT / "examples/lib"), "--json",
    )
    report = json.loads(output)
    if report["status"] != "passed" or report["total"] != 1 or report["failed"] != 0:
        raise RuntimeError("companion host decoder boundary did not pass")
    print(output, end="")


if __name__ == "__main__":
    main()
