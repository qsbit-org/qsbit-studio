"""Run the installed default configuration from an unrelated working directory."""

import argparse
import json
import os
import queue
import subprocess
import tempfile
import threading
import time
import urllib.request
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--prefix", type=Path, required=True)
args = parser.parse_args()
prefix = args.prefix.resolve()
source = (Path(__file__).resolve().parents[1] / "examples/x-memory.blog").read_text()
with tempfile.TemporaryDirectory(prefix="studio-installed-") as directory:
    root = Path(directory)
    launch = root / "read-only cwd"
    launch.mkdir()
    environment = dict(os.environ)
    for key in ("QSBIT_COMPILER", "QSBIT_SIMULATOR", "QSBIT_BLOG_EXPORTER"):
        environment.pop(key, None)
    environment.update(
        PATH=str(prefix / "bin") + os.pathsep + environment.get("PATH", ""),
        XDG_CACHE_HOME=str(root / "cache"),
        VIRTUAL_ENV="/missing/venv",
        QSBIT_PYTHON="/missing/python",
        PYTHONHOME="/missing/python",
        PYTHONPATH=str(launch),
    )
    (launch / "qsbit_backend.py").write_text("raise RuntimeError('wrong backend')\n")
    launch.chmod(0o555)
    with (root / "server.log").open("w+") as log:
        process = subprocess.Popen(
            ["qsbit-studio", "--listen", "127.0.0.1:0"],
            cwd=launch,
            env=environment,
            stdout=subprocess.PIPE,
            stderr=log,
            text=True,
        )
        startup = queue.Queue()
        reader = threading.Thread(
            target=lambda: startup.put(process.stdout.readline()), daemon=True
        )
        try:
            reader.start()
            line = startup.get(timeout=15)
            log.flush()
            assert line.startswith("QSBIT Studio: http://"), (
                root / "server.log"
            ).read_text()
            base = line.split()[-1]

            def api(path, body=None):
                request = urllib.request.Request(
                    base + path,
                    data=json.dumps(body).encode() if body else None,
                    headers={"Content-Type": "application/json"},
                )
                with urllib.request.urlopen(request, timeout=10) as response:
                    return json.load(response)

            health = api("/api/health")
            assert health["default_target"] == "studio-ideal-v1"
            assert health["targets"] == ["studio-ideal-v1"]
            assert health["installation"]["installed"] and not health["player_override"]
            assert Path(health["player_root"]).is_relative_to(prefix)
            job = api(
                "/api/jobs",
                {"filename": "x-memory.blog", "source": source, "backend": "stim"},
            )
            deadline = time.monotonic() + 180
            while time.monotonic() < deadline:
                report = api(f"/api/jobs/{job['id']}")
                if report["stage"] in ("completed", "failed", "cancelled"):
                    break
                time.sleep(0.2)
            assert report["stage"] == "completed", report
            assert report["metrics"]["operations"]["reset"] > 0
            snapshot = api(f"/api/jobs/{job['id']}/artifacts/installation.json")
            assert snapshot["installation"] == health["installation"]
            assert Path(snapshot["simulator"]) == prefix / "bin/qsbit-sim"
            runs = root / "cache/qsbit-studio/runs"
            assert list(runs.glob(f"session-*/{job['id']}/target.json"))
        finally:
            process.terminate()
            try:
                process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
            reader.join(timeout=1)
            process.stdout.close()
            launch.chmod(0o755)
        assert not list(runs.glob("session-*")), (
            "Graceful shutdown must remove session artifacts"
        )
print(
    "Installed PATH discovery, default preset, isolated Stim reset and cache cleanup passed."
)
