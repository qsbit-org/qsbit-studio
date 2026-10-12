# Testing

Run checks from the Studio repository root. Provision Rust dependencies once
with `cargo fetch --locked`; tests and lint then run offline.

```sh
cargo fmt --all --check
cargo clippy --locked --offline --all-targets --features exporter -- -D warnings
cargo test --locked --offline --features exporter
node --check web/studio.js
python3 tests/check_dependencies.py
```

Cargo unit tests cover upload bounds, target selection and configuration snapshots, output ABI
bounds, trace schemas, coincident/decreasing timestamps, terminal summary matching,
epoch-scoped measurement latencies, tool failure diagnostics, cancellation,
timeouts, output limits, and symlink rejection. API tests cover malformed and
oversized uploads, retained-job capacity and removal, missing replay behavior,
installation discovery failures, and exact discovered player asset bytes.

## Real toolchain integration

Build both Rust binaries, install the native compiler and simulator, and select a
Python-enabled simulator and activate an environment containing its backend
package and Stim dependencies. The test runner also needs Stim and NumPy for
independent reference calculations:

```sh
uv sync --frozen --group integration
uv run --frozen --group integration python tests/end_to_end.py \
  --studio target/debug/qsbit-studio --compiler qsbitc --simulator qsbit-sim
```

The test launches a fresh Studio process on an OS-assigned loopback port. It
uploads the X-memory BLOG, executes it using real native tools, checks the
17-qubit/33-record layout and raw observable, validates simulator completion and
trace timestamps, compares metrics to observations, and independently verifies
the measurement records using the exported Stim reference's detector converter.
It also checks invalid graphs and a graph whose distance exceeds controller
capacity, selects a second target with different gate/reset durations, and verifies
that the selected configuration reaches the generated simulator profile.

## Workspace checks

```sh
cargo build --locked --offline --bin qsbit-studio
uv sync --frozen --group browser
uv run --frozen --group browser playwright install chromium
uv run --frozen --group browser python tests/workspace.py --player-root /path/to/qsbit-sim/web
```

Set `QSBIT_BROWSER_EXECUTABLE` to use an existing Chromium executable instead of
the Playwright-managed browser.

The browser checks use controlled API responses for empty, running, completed and
failed states. They check file-read and deletion races, reconnection, keyboard
tabs, log selection, theme synchronization, bounded replay height, data font size,
text enlargement and narrow screens. Screenshots are saved in `.local/browser/`.
The baseline CI runs these checks on every push.

## Browser integration

```sh
uv sync --frozen --group integration --group browser
uv run --frozen --group integration --group browser playwright install chromium
uv run --frozen --group integration --group browser python tests/end_to_end.py \
  --studio target/debug/qsbit-studio --compiler qsbitc --simulator qsbit-sim --browser
```

Chromium checks the upload form, completed metrics, selected-run restoration,
the exact simulator-player assets, event stepping, rewind, time stepping, and
download responses. Mobile/dark screenshots and browser diagnostics are written
under ignored `.local/verification/`.

The baseline CI runs Cargo tests, lint, dependency checks, JavaScript syntax
checks and the workspace browser tests. For changes to code, assets, presets, tests, or build configuration, the integration
workflow runs automatically on pull requests and pushes to main. It builds
the latest simulator and compiler default branches and runs
numerical and browser integration. Browser installation and native builds are not
required by the default Rust build.

To verify installed operation, use `make install PREFIX=/path/to/prefix` and run
`tests/end_to_end.py` with `--studio /path/to/prefix/bin/qsbit-studio --installed
--polluted-environment` plus the native tool paths. It launches from an unrelated
directory, poisons outer Python settings, and checks installed player resources.
`tests/installed_startup.py --prefix /path/to/prefix` additionally checks PATH-based
discovery, the installed default target, and the default cache directory.
