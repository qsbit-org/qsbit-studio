# QSBIT Studio

[![CI](https://github.com/qsbit-org/qsbit-studio/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/qsbit-org/qsbit-studio/actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/qsbit-org/qsbit-studio)](LICENSE)

QSBIT Studio runs [Bloq](https://github.com/inmzhang/bloq) experiments in a local
web interface: upload a [BLOG file](https://github.com/inmzhang/bloq/blob/main/docs/graphs/blog.md),
run it, and inspect the results.

## Local setup

Install **[qsbit-compiler](https://github.com/qsbit-org/qsbit-compiler#build)**
and **[qsbit-sim](https://github.com/qsbit-org/qsbit-sim#build)** first.
Both `qsbitc` and `qsbit-sim` must be on PATH. Enable the simulator's
[Python bridge](https://qsbit-org.github.io/qsbit-sim/backends.html#python-runtime)
for Stim.

On Linux, with Rust, LLVM 21 tools, and [uv](https://docs.astral.sh/uv/) installed,
run from the repository root:

```sh
git submodule update --init
cargo build --locked --features exporter
uv sync --frozen --extra stim
uv run --frozen --extra stim cargo run --locked
```

## Use

Open <http://127.0.0.1:3000>:

1. **Upload** a `.blog` file exported from the [Bloq editor](https://bloqec.com/editor/).
2. **Run** by clicking **Compile & run**.
3. **Inspect** the execution trace in **Replay** and measurement outputs in **Results**.

## Limitations

One shot per run, with up to 32 physical qubits including scratch qubits.
Only closed graphs supported by the pinned Bloq exporter are accepted.
Decoder feedback, noise, qubit placement, and routing are not supported.
Results are uncorrected; replay shows completed runs rather than live execution.

[Setup details](docs/setup.md) · [Architecture](docs/architecture.md) ·
[API](docs/api.md) · [Testing](docs/testing.md)

Licensed under [Apache License 2.0](LICENSE).
