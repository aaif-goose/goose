#!/usr/bin/env python3
"""Assemble the goose-sdk Python wheel (debug profile).

Builds the cargo dylib + uniffi bindings, drops them into src/goose_sdk/, then
runs `python -m build --wheel` inside a throwaway venv (system Python is often
PEP 668 externally-managed). Output: packaging/python/dist/*.whl
"""

from __future__ import annotations

import platform
import shutil
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[3]
PROFILE_DIR = REPO_ROOT / "target" / "debug"
PKG_SRC = HERE / "src" / "goose_sdk"

LIB_NAME = {"Darwin": "libgoose_sdk.dylib", "Windows": "goose_sdk.dll"}.get(
    platform.system(), "libgoose_sdk.so"
)

# Platform tag for the wheel; setuptools defaults to a pure-python tag, but we
# embed a native library, so force a platform-specific one.
def _plat_tag() -> str:
    system, machine = platform.system(), platform.machine().lower()
    arm = machine in ("arm64", "aarch64")
    if system == "Darwin":
        return f"macosx_{'11_0_arm64' if arm else '10_12_x86_64'}"
    if system == "Linux":
        return f"linux_{'aarch64' if arm else 'x86_64'}"
    return "win_amd64"


def run(cmd: list[str], **kw) -> None:
    print("$", *cmd, flush=True)
    subprocess.run(cmd, check=True, **kw)


def main() -> int:
    run(["cargo", "build", "-p", "goose-sdk", "--features", "uniffi"], cwd=REPO_ROOT)

    lib = PROFILE_DIR / LIB_NAME
    bindgen = PROFILE_DIR / "goose-uniffi-bindgen"
    for p in (lib, bindgen):
        if not p.exists():
            sys.exit(f"missing: {p}")

    shutil.rmtree(PKG_SRC, ignore_errors=True)
    PKG_SRC.mkdir(parents=True)

    run([str(bindgen), "generate", "--library", str(lib), "--language", "python",
         "--no-format", "--out-dir", str(PKG_SRC)])
    shutil.copy(lib, PKG_SRC / lib.name)
    (PKG_SRC / "__init__.py").write_text(
        "from .goose_sdk import Agent, EventSink  # noqa: F401\n"
        "from . import goose_sdk_types  # noqa: F401\n"
    )

    shutil.rmtree(HERE / "dist", ignore_errors=True)

    venv = HERE / ".build" / "venv"
    venv_py = venv / ("Scripts" if platform.system() == "Windows" else "bin") / "python"
    if not venv.exists():
        run([sys.executable, "-m", "venv", str(venv)])
    run([str(venv_py), "-m", "pip", "install", "-q", "build", "setuptools", "wheel"])

    run([str(venv_py), "-m", "build", "--wheel", "--no-isolation",
         "-C--build-option=--plat-name", f"-C--build-option={_plat_tag()}"], cwd=HERE)

    print("\nBuilt:", *sorted((HERE / "dist").glob("*.whl")), sep="\n  ")
    return 0


if __name__ == "__main__":
    sys.exit(main())
