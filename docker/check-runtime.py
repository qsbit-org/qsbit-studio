"""Validate the installed runtime without a source checkout or network access."""

import json
import os
import re
import shutil
import subprocess
import sys
import tomllib
from pathlib import Path

import stim

assert os.getuid() != 0
assert not Path("/src").exists()
for command in ("gcc", "g++", "clang", "cargo", "cmake", "conan"):
    assert shutil.which(command) is None, command
assert Path(sys.prefix) == Path("/opt/qsbit/venv")
root = Path("/opt/qsbit/share/qsbit-studio")
lock = json.loads((root / "toolchain.json").read_text())
assert list(sys.version_info[:3]) == lock["python"]
packages = tomllib.loads((root / "simulator-uv.lock").read_text())["package"]
assert stim.__version__ == next(p["version"] for p in packages if p["name"] == "stim")
for command in (
    "qsbit-studio",
    "qsbit-blog-export",
    "qsbitc",
    "opt-21",
    "llvm-as-21",
    "ld.lld-21",
):
    subprocess.run([command, "--version"], check=True, stdout=subprocess.DEVNULL)
for command in ("qsbit-run", "qsbit-validate"):
    subprocess.run([command, "--help"], check=True, stdout=subprocess.DEVNULL)
info = json.loads(subprocess.check_output(["qsbit-sim", "--installation-info"]))
assert info["installed"] and info["backend_api"] == 4
assert re.fullmatch(r"[a-f0-9]{7,40}", info["revision"]), info
assert lock["simulator"]["revision"].startswith(info["revision"]), info
assert info["python"] == os.environ["QSBIT_PYTHON"]
assert Path(info["web_root"]).is_dir()
assert stim.Circuit("X 0\nM 0").compile_sampler().sample(1)[0, 0]
for command in ("qsbit-studio", "qsbit-blog-export", "qsbitc", "qsbit-sim-native"):
    path = Path("/opt/qsbit/bin") / command
    if command == "qsbit-sim-native":
        path = Path("/opt/qsbit/lib/qsbit-sim") / command
    result = subprocess.run(
        ["ldd", str(path)], capture_output=True, text=True, check=True
    )
    assert "not found" not in result.stdout, result.stdout
print("Installed toolchain, Python bridge, and Stim are ready.")
