"""Record component revisions, dependency inventory, and source licenses."""

import json
import shutil
import sys
from pathlib import Path

prefix = Path(sys.argv[1])
destination = prefix / "share/qsbit-studio"
lock = json.loads(Path("toolchain.lock.json").read_text())
lock["studio"] = {"revision": sys.argv[2]}
lock["python"] = list(sys.version_info[:3])
(destination / "toolchain.json").write_text(json.dumps(lock, indent=2) + "\n")
packages = json.loads(Path("/tmp/cargo-metadata.json").read_text())["packages"]
inventory = [
    {key: item.get(key) for key in ("name", "version", "license", "source")}
    for item in packages
]
(destination / "rust-packages.json").write_text(json.dumps(inventory, indent=2) + "\n")
licenses = destination / "licenses"
roots = [
    (p["name"] + "-" + p["version"], Path(p["manifest_path"]).parent) for p in packages
]
roots += [
    ("qsbit-sim", Path("/src/simulator")),
    ("qsbit-compiler", Path("/src/compiler")),
]
roots += [("systemc", path) for path in Path("/root/.conan2").glob("p/b/*/p/licenses")]
for name, root in roots:
    for parent in (root, root.parent):
        found = [
            p
            for pattern in ("LICENSE*", "COPYING*", "NOTICE*")
            for p in parent.glob(pattern)
            if p.is_file()
        ]
        if found:
            out = licenses / name
            out.mkdir(parents=True, exist_ok=True)
            for path in found:
                shutil.copy2(path, out / path.name)
            break

for name, path in (
    ("Cargo.lock", Path("Cargo.lock")),
    ("conan.lock", Path("/src/simulator/conan.lock")),
    ("simulator-uv.lock", Path("/src/simulator/uv.lock")),
):
    shutil.copy2(path, destination / name)
