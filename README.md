# QSBIT Studio

Upload a Bloq `.blog` block graph, compile it into a controller executable, run
it on qsbit-sim, and inspect the execution trace and measured outputs.

From the repository root, with Rust and the [native tools](docs/setup.md) installed:

```sh
git submodule update --init
cargo build --locked --features exporter
cargo run --locked
```

Open <http://127.0.0.1:3000>, choose **Try a distance-3 X-memory example**, select
**Mock** for a controller-only run, and click **Compile & run**. The execution
view reports the actual simulator timestamps, instructions, operations, and
measurement deliveries. Mock bits do not model a quantum state.

For ideal quantum simulation with Stim, install the optional backend in a local
virtual environment, then launch Studio in that environment:

```sh
uv sync --frozen --extra stim
uv run --frozen --extra stim cargo run --locked
```

Keep the default **Stim** backend selected. The included X-memory example at
distance 3 uses 17 physical qubits and produces 33 measurement records and raw
observable 0. Individual measurement records may vary with the seed.

In Bloq's editor, use **Download BLOG File** to export a graph for uploading.
Studio accepts `.blog` files; Stim circuit files and compiled Bloq IR files are
not upload formats. Bloq's editor sources remain unmodified.

## Execution player

The player has one source: **qsbit-sim**. This repository pins qsbit-sim as the
`external/qsbit-sim` submodule and serves its `docs/_static/trace-model.js`,
`trace-player.js`, and `site.css` directly. There is no Studio copy of the trace
model or renderer. The Studio page supplies only the replay bundle and HTML host.

Update the player in qsbit-sim, then advance the Studio submodule and its declared
revision together. [Architecture](docs/architecture.md) describes the boundary.

## Scope

The initial preset is an ideal controller with 20 ns gates and 40 ns acquisition,
one shot per run, and at most 32 physical qubits including exporter scratch
resources. It supports closed graphs accepted by the pinned Bloq QIR exporter.
Distance choices are 3, 5, and 7; a graph can still exceed the physical limit.

Decoder-dependent control, injected noise, external logical patch I/O, hardware
placement/routing, and live simulation stepping are unsupported. Playback
navigates an already completed run. Outputs are measurement records and raw
observables, without decoder correction or logical error-rate estimates.

Artifacts are session-scoped. Download the input, QIR, executable, run settings,
trace, summary, and metrics before removing a run or stopping the service.
The GitHub repository is public; this command starts a local service.

## Development

```sh
cargo fmt --all --check
cargo clippy --locked --offline --all-targets --features exporter -- -D warnings
cargo test --locked --offline --features exporter
node --check web/studio.js
python3 tests/check_dependencies.py
```

Run `cargo fetch --locked` once to provision Rust dependencies before offline
checks. Optional [integration and browser tests](docs/testing.md) exercise a real
upload through Bloq, qsbit-compiler, qsbit-sim, and the shared player.

- [Setup and runtime configuration](docs/setup.md)
- [Architecture and player ownership](docs/architecture.md)
- [API and metric contracts](docs/api.md)
- [Testing](docs/testing.md)

Licensed under [Apache-2.0](LICENSE).
