"""Verify the final image with blocked egress and real native tools."""

import argparse
import json
import subprocess
import sys
import time
import urllib.error
import urllib.request
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--image", required=True)
args = parser.parse_args()
name = "qsbit-test-" + uuid.uuid4().hex[:12]
containers = []
output = ROOT / ".local/verification"
output.mkdir(parents=True, exist_ok=True)


def docker(*arguments):
    return subprocess.check_output(["docker", *arguments], text=True).strip()


def api(base, path, body=None):
    request = urllib.request.Request(
        base + path,
        data=json.dumps(body).encode() if body is not None else None,
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(request, timeout=5) as response:
        data = response.read()
        return json.loads(data) if data else None


def wait_for(check, seconds=60):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        try:
            value = check()
            if value:
                return value
        except (OSError, urllib.error.URLError):
            pass
        time.sleep(0.1)
    raise AssertionError("Timed out waiting for container condition")


def start(suffix, timeout=120):
    identifier = name + suffix
    docker(
        "run",
        "-d",
        "--init",
        "--name",
        identifier,
        "--network",
        name,
        "-p",
        "127.0.0.1::3000",
        args.image,
        "--listen",
        "0.0.0.0:3000",
        "--allow-remote",
        "--work-dir",
        "/home/qsbit/runs",
        "--max-running",
        "1",
        "--stage-timeout-seconds",
        str(timeout),
    )
    containers.append(identifier)
    info = json.loads(docker("inspect", identifier))[0]
    assert not info["Mounts"]
    port = info["NetworkSettings"]["Ports"]["3000/tcp"][0]["HostPort"]
    base = "http://127.0.0.1:" + port
    wait_for(lambda: api(base, "/api/health"))
    return identifier, base


def submit(base):
    return api(
        base,
        "/api/jobs",
        {
            "filename": "x-memory.blog",
            "source": (ROOT / "examples/x-memory.blog").read_text(),
            "distance": 3,
            "seed": 7,
            "backend": "stim",
            "target": "studio-ideal-v1",
        },
    )


def hold_exporter(identifier):
    # Suspend a real exporter to exercise bounded host time deterministically.
    script = """
import os, signal, time
from pathlib import Path
Path('/tmp/watcher-ready').touch()
deadline = time.monotonic() + 30
while time.monotonic() < deadline:
    for path in Path('/proc').glob('[0-9]*/exe'):
        try:
            if path.resolve().name == 'qsbit-blog-export':
                os.kill(int(path.parent.name), signal.SIGSTOP)
                Path('/tmp/exporter-held').touch()
                raise SystemExit(0)
        except (FileNotFoundError, PermissionError, ProcessLookupError):
            pass
    time.sleep(0.001)
raise SystemExit(1)
"""
    docker("exec", "-d", identifier, "python", "-c", script)
    wait_for(
        lambda: (
            docker(
                "exec",
                identifier,
                "python",
                "-c",
                "from pathlib import Path; print(int(Path('/tmp/watcher-ready').exists()))",
            )
            == "1"
        )
    )


try:
    docker("network", "create", "--internal", name)
    identifier, base = start("-normal")
    docker(
        "exec", identifier, "python", "/opt/qsbit/share/qsbit-studio/check-runtime.py"
    )
    assert docker("exec", identifier, "id", "-u") != "0"
    docker(
        "exec",
        identifier,
        "python",
        "-c",
        "import socket\ntry:\n socket.create_connection(('1.1.1.1',443),timeout=1)\nexcept OSError:\n pass\nelse:\n raise AssertionError('egress available')",
    )
    subprocess.run(
        [sys.executable, str(ROOT / "tests/end_to_end.py"), "--url", base, "--browser"],
        check=True,
    )

    limited, limited_base = start("-timeout", timeout=5)
    hold_exporter(limited)
    running = submit(limited_base)
    wait_for(
        lambda: (
            docker(
                "exec",
                limited,
                "python",
                "-c",
                "from pathlib import Path; print(int(Path('/tmp/exporter-held').exists()))",
            )
            == "1"
        )
    )
    queued = submit(limited_base)
    api(limited_base, f"/api/jobs/{queued['id']}/cancel", {})
    wait_for(
        lambda: api(limited_base, f"/api/jobs/{queued['id']}")["stage"] == "cancelled"
    )
    report = wait_for(
        lambda: (
            report
            if (report := api(limited_base, f"/api/jobs/{running['id']}"))["stage"]
            == "failed"
            else None
        )
    )
    assert report["failed_stage"] == "exporting" and "timeout" in report["error"], (
        report
    )

    closing, closing_base = start("-shutdown")
    hold_exporter(closing)
    submit(closing_base)
    wait_for(
        lambda: (
            docker(
                "exec",
                closing,
                "python",
                "-c",
                "from pathlib import Path; print(int(Path('/tmp/exporter-held').exists()))",
            )
            == "1"
        )
    )
    docker("stop", "--time", "15", closing)
    state = json.loads(docker("inspect", closing))[0]["State"]
    assert state["ExitCode"] == 0, state
    docker("start", closing)
    wait_for(lambda: api(closing_base, "/api/health"))
    assert api(closing_base, "/api/jobs")["jobs"] == []
    docker(
        "exec",
        closing,
        "python",
        "-c",
        "from pathlib import Path; assert len(list(Path('/home/qsbit/runs').glob('session-*'))) == 1",
    )
    print(
        "Final image passed offline simulation, browser, cancellation, timeout, and shutdown checks."
    )
finally:
    for identifier in containers:
        result = subprocess.run(
            ["docker", "logs", identifier], capture_output=True, text=True
        )
        (output / (identifier + ".log")).write_text(result.stdout + result.stderr)
        subprocess.run(["docker", "rm", "-f", identifier], check=False)
    subprocess.run(["docker", "network", "rm", name], check=False)
