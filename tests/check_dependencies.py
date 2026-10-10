"""Check the declared toolchain pins against their consumed source revisions."""

import json
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
pins = json.loads((ROOT / "toolchain.lock.json").read_text())
manifest = (ROOT / "Cargo.toml").read_text()
constants = (ROOT / "src/lib.rs").read_text()
simulator = subprocess.check_output(
    ["git", "-C", str(ROOT / "external/qsbit-sim"), "rev-parse", "HEAD"], text=True
).strip()
assert simulator == pins["simulator"]["revision"]
assert re.search(r'PLAYER_REVISION: &str = "([a-f0-9]+)"', constants)[1] == simulator
bloq = re.search(r'bloq = \{ git = "([^"]+)", rev = "([a-f0-9]+)"', manifest)
assert bloq[1] == pins["bloq"]["repository"]
assert bloq[2] == pins["bloq"]["revision"]
assert re.search(r'BLOQ_REVISION: &str = "([a-f0-9]+)"', constants)[1] == bloq[2]
print("Toolchain source revisions match their pins.")
