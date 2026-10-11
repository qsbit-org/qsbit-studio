# Testing

Run checks from the Studio repository root after initializing the simulator
submodule. Provision Rust dependencies once with `cargo fetch --locked`; tests
and lint then run offline.

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
virtual-environment interpreter selection, and exact shared-player asset bytes.

## Real toolchain integration

Build both Rust binaries, install the native compiler and simulator, and select a
Python-enabled simulator for Stim. Optional dependencies stay in the local venv:

```sh
uv sync --frozen --extra stim
uv run --frozen --extra stim python tests/end_to_end.py \
  --studio target/debug/qsbit-studio --compiler qsbitc --simulator qsbit-sim
```

The test launches a fresh Studio process on an OS-assigned loopback port. It
uploads the X-memory BLOG, executes it using real native tools, checks the
17-qubit/33-record layout and raw observable, validates simulator completion and
trace timestamps, compares metrics to observations, and independently verifies
the measurement records using the exported Stim reference's detector converter.
It also checks invalid graphs and a graph whose distance exceeds controller
capacity, selects a second target with different gate/reset durations, and verifies
that the selected configuration reaches the generated simulator profile. It does not use a prerecorded trace as the uploaded graph's result.

## Workspace checks

```sh
cargo build --locked --offline --bin qsbit-studio
uv sync --frozen --group browser
uv run --frozen --group browser playwright install chromium
uv run --frozen --group browser python tests/workspace.py
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
uv sync --frozen --extra stim --group browser
uv run --frozen --extra stim --group browser playwright install chromium
uv run --frozen --extra stim --group browser python tests/end_to_end.py \
  --studio target/debug/qsbit-studio --compiler qsbitc --simulator qsbit-sim --browser
```

Chromium checks the upload form, completed metrics, selected-run restoration,
the exact simulator-player assets, event stepping, rewind, time stepping, and
download responses. Mobile/dark screenshots and browser diagnostics are written
under ignored `.local/verification/`.

The baseline CI runs Cargo tests, lint, dependency checks, JavaScript syntax
checks and the workspace browser tests. The optional integration workflow builds
the pinned simulator and the latest qsbit-compiler default branch and runs
numerical and browser integration. Browser installation and native builds are not
required by the default Rust build.
