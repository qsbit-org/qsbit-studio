"""Exercise real BLOG uploads through native compilation, simulation, and replay."""

import argparse
import collections
import hashlib
import json
import os
import queue
import shutil
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--studio", default="target/debug/qsbit-studio")
parser.add_argument("--compiler", default="qsbitc")
parser.add_argument("--simulator", default="qsbit-sim")
parser.add_argument("--browser", action="store_true")
args = parser.parse_args()
OUTPUT = ROOT / ".local/verification"
OUTPUT.mkdir(parents=True, exist_ok=True)


def executable(name):
    path = Path(name)
    if path.is_file():
        return str(path.absolute())
    resolved = shutil.which(name)
    assert resolved, f"Missing native executable: {name}"
    return resolved


def http(path, body=None, method=None):
    request = urllib.request.Request(
        base + path,
        data=json.dumps(body).encode() if body is not None else None,
        method=method,
        headers={"Content-Type": "application/json"} if body is not None else {},
    )
    with urllib.request.urlopen(request, timeout=10) as response:
        return response.read(), response.headers


def api(path, body=None, method=None):
    payload, _ = http(path, body, method)
    return json.loads(payload) if payload else None


def submit(source, distance=3, seed=7):
    return api("/api/jobs", {
        "filename": "x-memory.blog", "source": source,
        "distance": distance, "backend": "stim", "seed": seed,
    })


def finished(identifier):
    deadline = time.monotonic() + 180
    while time.monotonic() < deadline:
        report = api(f"/api/jobs/{identifier}")
        if report["stage"] in {"completed", "failed", "cancelled"}:
            return report
        time.sleep(0.2)
    raise AssertionError("Run exceeded integration timeout")


def artifact(identifier, name):
    return http(f"/api/jobs/{identifier}/artifacts/{name}")[0]


def verify_run(report):
    assert report["stage"] == "completed", report
    assert report["replay_ready"]
    identifier = report["id"]
    metrics = report["metrics"]
    assert metrics["simulator_success"] and metrics["program_exit_code"] == 0
    assert metrics["physical_qubits"] == 17
    records = [output["value"] for output in metrics["outputs"] if output["label"].startswith("record:")]
    observables = [output["value"] for output in metrics["outputs"] if output["label"].startswith("raw_observable:")]
    assert len(records) == 33 and observables == [0]
    bundle = api(f"/api/jobs/{identifier}/replay")
    assert bundle["schema"] == 1
    events = bundle["examples"][0]["events"]
    assert all(event["schema"] == 1 for event in events)
    assert [event["tick"] for event in events] == sorted(event["tick"] for event in events)
    assert any(event["kind"] == "SimulationCompleted" and event["tick"] == metrics["stop_tick_ns"] for event in events)
    counts = collections.Counter(event["kind"] for event in events)
    assert metrics["retired_instructions"] == counts["InstructionRetired"] > 0
    assert metrics["physical_operations"] == counts["OperationStart"] > 0
    assert metrics["measurement_samples"] == counts["MeasurementSampled"] > 0
    assert metrics["measurement_delivery"]["samples"] > 0
    assert metrics["timing_queue_peak"] == max(event["value"] for event in events if event["kind"] == "TimingPointEnqueued")
    assert report["source_sha256"] == hashlib.sha256(source.encode()).hexdigest()
    assert json.loads(artifact(identifier, "metrics.json")) == metrics
    assert json.loads(artifact(identifier, "report.json"))["stage"] == "completed"
    manifest = json.loads(artifact(identifier, "program.manifest.json"))
    assert hashlib.sha256(artifact(identifier, "program.elf")).hexdigest() == manifest["elf_sha256"]
    # Independent physical reference: signed detectors and raw logical parity.
    import numpy as np
    import stim
    reference = stim.Circuit(artifact(identifier, "reference.stim").decode())
    assert reference.num_measurements == 33
    detectors, logical = reference.compile_m2d_converter().convert(
        measurements=np.array([records], dtype=np.bool_), separate_observables=True
    )
    assert not detectors.any()
    assert logical[0].tolist() == observables
    (OUTPUT / "report.json").write_text(json.dumps(report, indent=2))
    (OUTPUT / "replay.json").write_text(json.dumps(bundle))
    return identifier


def browser_check():
    from playwright.sync_api import expect, sync_playwright
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch()
        page = browser.new_page(viewport={"width": 1440, "height": 1100})
        errors = []
        page.on("pageerror", lambda error: errors.append(str(error)))
        page.goto(base)
        # Session history is restored after a page reload.
        expect(page.locator("#status-badge")).to_have_text("Completed", timeout=10000)
        page.locator("#file-input").set_input_files(ROOT / "examples/x-memory.blog")
        expect(page.locator("#filename")).to_have_text("x-memory.blog")
        expect(page.locator("#run")).to_be_enabled()
        with page.expect_response(lambda response: response.url.endswith("/api/jobs") and response.request.method == "POST" and response.status == 202) as submission:
            page.locator("#run").click()
        submitted = submission.value.json()
        verify_run(finished(submitted["id"]))
        expect(page.locator("#replay")).to_have_attribute("src", f"/runs/{submitted['id']}/player", timeout=180000)
        expect(page.locator("#status-badge")).to_have_text("Completed", timeout=180000)
        expect(page.locator("#metrics")).to_be_visible()
        expect(page.locator("#outputs tr")).to_have_count(34)
        frame = page.frame_locator("#replay")
        expect(frame.locator("#trace-next")).to_be_enabled(timeout=10000)
        frame.locator(".replay-records > summary").click()
        initial = frame.locator("#trace-event").inner_text()
        frame.locator("#trace-next").click()
        assert frame.locator("#trace-event").inner_text() != initial
        frame.locator("#trace-back").click()
        assert frame.locator("#trace-event").inner_text() == initial
        time_before = frame.locator("#trace-time").inner_text()
        frame.locator("#trace-tick").click()
        assert frame.locator("#trace-time").inner_text() != time_before
        with page.expect_download() as download_info:
            page.get_by_role("link", name="metrics.json", exact=True).click()
        download = download_info.value
        download.save_as(OUTPUT / "downloaded-metrics.json")
        assert json.loads((OUTPUT / "downloaded-metrics.json").read_text())["program_exit_code"] == 0
        page.screenshot(path=str(OUTPUT / "studio-desktop.png"), full_page=True)
        page.emulate_media(color_scheme="dark")
        page.screenshot(path=str(OUTPUT / "studio-dark.png"), full_page=True)
        page.set_viewport_size({"width":390,"height":844})
        page.screenshot(path=str(OUTPUT / "studio-mobile.png"), full_page=True)
        assert page.evaluate("document.documentElement.scrollWidth <= window.innerWidth")
        page.reload()
        expect(page.locator("#status-badge")).to_have_text("Completed", timeout=10000)
        assert not errors, errors
        browser.close()


log = (OUTPUT / "server.log").open("w")
process = subprocess.Popen([
    executable(args.studio), "--listen", "127.0.0.1:0",
    "--compiler", executable(args.compiler), "--simulator", executable(args.simulator),
    "--python", sys.executable,
], cwd=ROOT, stdout=subprocess.PIPE, stderr=log, text=True)
startup = queue.Queue()
reader = threading.Thread(target=lambda: startup.put(process.stdout.readline()), daemon=True)
try:
    reader.start()
    line = startup.get(timeout=15).strip()
    assert line.startswith("QSBIT Studio: http://"), line
    base = line.split()[-1]
    source = (ROOT / "examples/x-memory.blog").read_text()
    for asset in ("trace-model.js", "trace-player.js", "site.css"):
        payload, _ = http(f"/player/{asset}")
        assert payload == (ROOT / "external/qsbit-sim/docs/_static" / asset).read_bytes()
    # Failed inputs cannot silently become a gallery example's run.
    for invalid_source, distance in [("BLOG 1.0\nthis is not a graph", 3), (source, 5)]:
        report = finished(submit(invalid_source, distance)["id"])
        assert report["stage"] == "failed" and report["failed_stage"] == "exporting", report
        assert not report["replay_ready"]
        api(f"/api/jobs/{report['id']}", method="DELETE")
    identifier = verify_run(finished(submit(source)["id"]))
    if args.browser:
        browser_check()
    print(f"Verified BLOG → QIR → ELF → Stim → shared replay: {identifier}")
finally:
    process.terminate()
    try:
        process.wait(timeout=15)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=5)
    reader.join(timeout=1)
    process.stdout.close()
    log.close()
