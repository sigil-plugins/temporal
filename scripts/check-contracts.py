#!/usr/bin/env python3
"""Offline contract checks; neither guest execution nor release authority."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
WORLD = "sigil:temporal/plugin@0.1.0"
HOST = "sigil:host/grpc-unary@1.3.0"
CLIENT = "sigil:temporal/client@0.1.0"
SIGIL_SOURCE = "99fae9f553ee3f2e139897bedf64a1a33f8f2d51"
SIGIL_PRODUCT = "d2130839530d7e11a26e5eba4720c52fa42d1776"
SDK_SOURCE = "3467153cc7c87979bd55db84c5a03f2fc77c7fe7"
INPUTS = {
    "wit/temporal.wit": "c40154c8c9ca366055ad032176472947a3a59506b2c7275a4b9ba7c0aa67b1ae",
    "wit/deps/sigil-host/host.wit": "25888c5834c237a8d37cf44ca3d0ae6c1335eee441e17e2afaf629512288df19",
    "conformance/contract.json": "b96fb486cf67a0733ee14628d3f88f03e74a92f969d4700aa8998a8b74bf3f0a",
}


def run(*args):
    return subprocess.check_output(args, stderr=subprocess.PIPE).decode("utf-8")


def cli(main):
    """Expose failed tool diagnostics without turning failure into CLI success."""
    try:
        main()
    except subprocess.CalledProcessError as error:
        command = error.cmd if isinstance(error.cmd, str) else shlex.join(map(str, error.cmd))
        print(f"error: command failed with status {error.returncode}: {command}", file=sys.stderr)
        for label, output in (("stdout", error.stdout), ("stderr", error.stderr)):
            if output:
                text = output.decode("utf-8", errors="backslashreplace") if isinstance(output, bytes) else output
                print(f"--- {label} ---", file=sys.stderr)
                print(text, end="" if text.endswith("\n") else "\n", file=sys.stderr)
        status = error.returncode if 1 <= error.returncode <= 255 else 1
        raise SystemExit(status) from None


def wasm_tools():
    tool = os.environ.get("WASM_TOOLS", "wasm-tools")
    if run(tool, "--version").strip() != "wasm-tools 1.252.0":
        raise ValueError("wasm-tools 1.252.0 is required")
    return tool


def check_sources(root=ROOT):
    for name, expected in INPUTS.items():
        path = root / name
        if path.is_symlink() or not path.is_file():
            raise ValueError(f"contract input must be an ordinary file: {name}")
        if hashlib.sha256(path.read_bytes()).hexdigest() != expected:
            raise ValueError(f"frozen contract drift: {name}")


def type_shape(doc, ty, depth=0):
    if depth > 100:
        raise ValueError("recursive or excessively nested WIT type")
    if ty is None or isinstance(ty, str):
        return ty
    kind = doc["types"][ty]["kind"]
    key, value = next(iter(kind.items()))
    expand = lambda item: type_shape(doc, item, depth + 1)
    if key == "type":
        return expand(value)
    if key in ("list", "option"):
        return [key, expand(value)]
    if key == "result":
        return [key, expand(value.get("ok")), expand(value.get("err"))]
    if key == "record":
        return [key, [[field["name"], expand(field["type"])] for field in value["fields"]]]
    if key in ("enum", "variant"):
        return [key, [[case["name"], expand(case.get("type"))] for case in value["cases"]]]
    raise ValueError(f"unreviewed WIT type kind: {key}")


def interface_shape(doc, identifier):
    interface = doc["interfaces"][identifier]
    package, version = doc["packages"][interface["package"]]["name"].split("@")
    identity = f"{package}/{interface['name']}@{version}"
    functions = {}
    for name, function in interface["functions"].items():
        if function["kind"] != "freestanding":
            raise ValueError("only synchronous freestanding functions are admitted")
        functions[name] = [
            [[param["name"], type_shape(doc, param["type"])] for param in function["params"]],
            type_shape(doc, function.get("result")),
        ]
    return identity, {
        "types": {name: type_shape(doc, ty) for name, ty in interface["types"].items()},
        "functions": functions,
    }


def world_shape(doc):
    candidates = []
    for world in doc["worlds"]:
        if not world["exports"]:
            continue
        shape = {}
        for direction in ("imports", "exports"):
            entries = {}
            for item in world[direction].values():
                if set(item) != {"interface"}:
                    raise ValueError("unexpected top-level function or type")
                identity, interface = interface_shape(doc, item["interface"]["id"])
                if identity in entries:
                    raise ValueError("duplicate interface")
                entries[identity] = interface
            shape[direction] = entries
        candidates.append(shape)
    if len(candidates) != 1:
        raise ValueError("expected exactly one exported world")
    shape = candidates[0]
    if set(shape["imports"]) != {HOST} or set(shape["exports"]) != {CLIENT}:
        raise ValueError("component must have exactly the approved host import and client export")
    return shape


def check_component(component, root=ROOT):
    check_sources(root)
    tool = wasm_tools()
    run(tool, "validate", "--features", "all", str(component))
    run(tool, "component", "targets", str(root / "wit"), "--world", WORLD, str(component))
    expected = json.loads(run(tool, "component", "wit", "--json", str(root / "wit")))
    actual = json.loads(run(tool, "component", "wit", "--json", str(component)))
    if world_shape(actual) != world_shape(expected):
        raise ValueError("component semantic WIT differs from the frozen contract")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--component", type=Path)
    args = parser.parse_args()
    check_sources()
    tool = wasm_tools()
    world_shape(json.loads(run(tool, "component", "wit", "--json", str(ROOT / "wit"))))
    if args.component:
        check_component(args.component)
    print("Local contract check passed; NON-GATING, no execution or provenance authority")


if __name__ == "__main__":
    cli(main)
