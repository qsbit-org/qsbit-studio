"""Check workspace layout and asynchronous selection with controlled API responses."""

import argparse
import subprocess
from pathlib import Path

from playwright.sync_api import expect, sync_playwright

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / ".local/browser"
OUTPUT.mkdir(parents=True, exist_ok=True)
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--player-root", default=str(ROOT / "external/qsbit-sim/web"))
args = parser.parse_args()


def report(identifier, stage="completed"):
    return {
        "id": identifier,
        "filename": f"{identifier}.blog",
        "distance": 3,
        "seed": 1,
        "backend": "stim",
        "stage": stage,
        "failed_stage": None,
        "error": "Invalid graph" if stage == "failed" else None,
        "host_duration_ms": 100,
        "replay_ready": stage == "completed",
        "artifacts": ["simulate.log", "metrics.json"] if stage == "completed" else [],
        "metrics": {
            "simulated_duration_ns": 80,
            "stop_tick_ns": 80,
            "retired_instructions": 8,
            "last_cpu_cycle": 16,
            "physical_operations": 2,
            "timing_points": 2,
            "measurement_samples": 1,
            "physical_qubits": 1,
            "timing_queue_peak": 1,
            "measurement_delivery": {"samples": 1, "mean_ns": 20, "max_ns": 20},
            "outputs": [{"label": "record:0", "value": 1}],
        }
        if stage == "completed"
        else None,
    }


def check(base):
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch()
        page = browser.new_page(viewport={"width": 1440, "height": 900})
        errors = []
        page.on("pageerror", lambda error: errors.append(str(error)))
        records = []
        pending_delete = []
        disconnected = False

        def handle(route):
            path = route.request.url.removeprefix(base)
            if path == "/api/jobs":
                route.fulfill(json={"jobs": records})
            elif route.request.method == "DELETE":
                pending_delete.append(route)
            elif path.endswith("/replay"):
                route.fulfill(
                    json={
                        "schema": 1,
                        "examples": [
                            {
                                "name": "test",
                                "backend": "stim",
                                "configuration": {},
                                "events": [
                                    {
                                        "schema": 1,
                                        "kind": "SessionStarted",
                                        "tick": 0,
                                        "epoch": 1,
                                    },
                                    {
                                        "schema": 1,
                                        "kind": "OperationStart",
                                        "tick": 40,
                                        "epoch": 1,
                                        "port": 0,
                                        "id": 1,
                                        "operation": "x",
                                        "value": 20,
                                    },
                                    {
                                        "schema": 1,
                                        "kind": "OperationEnd",
                                        "tick": 60,
                                        "epoch": 1,
                                        "port": 0,
                                        "id": 1,
                                        "operation": "x",
                                    },
                                    {
                                        "schema": 1,
                                        "kind": "SimulationCompleted",
                                        "tick": 80,
                                        "epoch": 1,
                                    },
                                ],
                            }
                        ],
                    }
                )
            elif path.endswith("/simulate.log"):
                route.fulfill(body="simulation completed", content_type="text/plain")
            else:
                identifier = path.split("/")[3]
                if disconnected:
                    route.abort()
                else:
                    route.fulfill(
                        json=next(r for r in records if r["id"] == identifier)
                    )

        page.route("**/api/jobs**", handle)
        # Serve the real shared player without requiring a native job for layout tests.
        page.route(
            "**/runs/*/player",
            lambda route: route.fulfill(
                body=(ROOT / "web/player.html")
                .read_text()
                .replace("{{BUNDLE}}", "/api/jobs/a/replay"),
                content_type="text/html",
            ),
        )
        page.goto(base)
        page.screenshot(path=str(OUTPUT / "empty.png"), full_page=True)

        page.evaluate("""() => {
            const read = File.prototype.arrayBuffer;
            File.prototype.arrayBuffer = function() {
                if (this.name === 'slow.blog') return new Promise(resolve => {
                    window.finishRead = () => read.call(this).then(resolve);
                });
                return read.call(this);
            };
        }""")

        def upload(name):
            page.locator("#file-input").set_input_files(
                {
                    "name": name,
                    "mimeType": "text/plain",
                    "buffer": b"BLOG 1.0\n",
                }
            )

        upload("slow.blog")
        upload("new.blog")
        expect(page.locator("#filename")).to_have_text("new.blog")
        page.evaluate("window.finishRead()")
        expect(page.locator("#filename")).to_have_text("new.blog")
        expect(page.locator("#dropzone")).to_be_hidden()
        expect(page.locator("#source-details")).not_to_have_attribute("open", "")
        page.click("#clear-file")
        expect(page.locator("#dropzone")).to_be_visible()

        records[:] = [report("a", "simulating")]
        page.reload()
        expect(page.locator("#status-badge")).to_have_text("Running")
        page.screenshot(path=str(OUTPUT / "running.png"), full_page=True)
        disconnected = True
        expect(page.locator("#connection-status")).to_contain_text(
            "Status unavailable", timeout=10000
        )
        expect(page.locator("#retry-connection")).to_be_visible()
        disconnected = False
        records[0] = report("a", "failed")
        page.click("#retry-connection")
        expect(page.locator("#status-badge")).to_have_text("Failed")
        page.click("#tab-logs")
        expect(page.locator("#run-error")).to_have_text("Invalid graph")
        page.screenshot(path=str(OUTPUT / "failed.png"), full_page=True)

        records[:] = [report("a"), report("b")]
        page.reload()
        expect(page.locator("#status-badge")).to_have_text("Completed")
        frame = page.frame_locator("#replay")
        expect(frame.locator("#trace-tick")).to_be_enabled()
        page.evaluate("document.fonts.ready")
        assert page.evaluate("document.documentElement.scrollHeight <= innerHeight + 1")
        assert page.locator("#replay").bounding_box()["height"] < 700
        assert frame.locator("#trace-timeline").bounding_box()["y"] < 750
        for theme in ("light", "dark"):
            page.select_option("[data-theme-select]", theme)
            expect(frame.locator("html")).to_have_attribute("data-theme", theme)
            page.screenshot(path=str(OUTPUT / f"completed-{theme}.png"), full_page=True)
        page.locator("#tab-replay").focus()
        page.keyboard.press("ArrowRight")
        expect(page.locator("#tab-results")).to_be_focused()
        expect(page.locator("#outputs")).to_be_visible()
        assert (
            page.locator("#outputs td").first.evaluate(
                "el => parseFloat(getComputedStyle(el).fontSize)"
            )
            >= 14
        )

        # A late deletion of b must not delete or deselect a.
        page.click("#remove-run")
        page.get_by_role("button", name="a.blog · Completed", exact=True).click()
        expect(page.locator("#run-name")).to_have_text("a.blog")
        assert pending_delete
        pending_delete.pop().fulfill(status=204)
        expect(page.locator("#run-name")).to_have_text("a.blog")
        expect(page.locator("#history-list button")).to_have_count(1)
        page.click("#tab-logs")
        page.get_by_role("button", name="simulate.log", exact=True).click()
        expect(page.locator("#log-content")).to_have_text("simulation completed")
        page.click("#tab-replay")
        frame.locator("#trace-tab-machine").focus()
        frame.locator("#trace-tab-machine").press("End")
        expect(frame.locator("#trace-panel-records")).to_be_visible()
        frame.locator("#trace-next").click()
        expect(frame.locator("#trace-time")).to_have_text("40 ns")
        frame.locator("#trace-expand").click()
        frame.locator("#trace-expand").press("Escape")
        expect(frame.locator("#trace-expand")).to_have_attribute(
            "aria-pressed", "false"
        )

        for width, scale in [(1440, "32px"), (390, "16px")]:
            page.set_viewport_size({"width": width, "height": 900})
            page.evaluate(
                "(scale) => { document.documentElement.style.fontSize = scale; }", scale
            )
            assert page.evaluate("document.documentElement.scrollWidth <= innerWidth")
            page.screenshot(
                path=str(OUTPUT / f"responsive-{width}.png"), full_page=True
            )
        assert not errors, errors
        browser.close()


process = subprocess.Popen(
    [
        str(ROOT / "target/debug/qsbit-studio"),
        "--listen",
        "127.0.0.1:0",
        "--compiler",
        "/bin/true",
        "--simulator",
        "/bin/true",
        "--exporter",
        "/bin/true",
        "--player-root",
        args.player_root,
    ],
    cwd=ROOT,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
    text=True,
)
try:
    line = process.stdout.readline()
    assert line.startswith("QSBIT Studio: http://"), process.stderr.read()
    check(line.strip().split()[-1])
finally:
    process.terminate()
    process.wait(timeout=15)
print("PASS workspace layout, theme, keyboard navigation and asynchronous selection")
