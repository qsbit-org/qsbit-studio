# QSBIT Studio

[![CI](https://github.com/qsbit-org/qsbit-studio/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/qsbit-org/qsbit-studio/actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/qsbit-org/qsbit-studio)](LICENSE)

QSBIT Studio is a local web interface for compiling and running quantum-control
experiments. Upload a Bloq `.blog` graph, compile it with qsbit-compiler, run it
on qsbit-sim, and inspect the execution trace and measurement results.

[Setup](docs/setup.md) ·
[Architecture](docs/architecture.md) ·
[API reference](docs/api.md)

## Build

Install Rust and the [native tools](docs/setup.md#native-tools), then run from
the repository root on Linux:

```sh
git submodule update --init
cargo build --locked --features exporter
```

## Run

```sh
cargo run --locked
```

Open <http://127.0.0.1:3000>, choose **Use X-memory example**, keep distance 3,
select **Mock**, and click **Compile & run**. Inspect the completed run in Replay,
Results, Artifacts, and Logs. The mock backend supplies measurement bits without
maintaining quantum state.

For quantum-state evolution with Stim, stop the service and restart it with the
optional backend installed:

```sh
uv sync --frozen --extra stim
uv run --frozen --extra stim cargo run --locked
```

Select **Stim** and run the example. It uses 17 physical qubits and produces
33 measurement records with raw observable 0.

To use your own graph, select **Download BLOG File** in Bloq's editor and upload
the `.blog` file. Download run artifacts before removing a run or stopping Studio.

## Scope

Each run executes one shot with at most 32 physical qubits, including exporter
scratch resources. Studio accepts closed graphs supported by the pinned Bloq
exporter. Decoder feedback, injected noise, external logical patch I/O, qubit
placement, and routing are not supported. Results contain raw observables,
without decoder correction. Replay shows completed execution, not live stepping.

## Learn more

- [Setup](docs/setup.md): tool discovery, backends, and service configuration.
- [Architecture](docs/architecture.md): compilation and the shared qsbit-sim player.
- [API reference](docs/api.md): runs, artifacts, and metrics.
- [Testing](docs/testing.md): development checks and end-to-end verification.

## License

[Apache License 2.0](LICENSE).
