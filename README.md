# QSBIT studio

[![CI](https://github.com/qsbit-org/qsbit-studio/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/qsbit-org/qsbit-studio/actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/qsbit-org/qsbit-studio)](LICENSE)

QSBIT Studio runs [Bloq](https://github.com/inmzhang/bloq) experiments in a local
web interface: upload a [BLOG file](https://github.com/inmzhang/bloq/blob/main/docs/graphs/blog.md),
run it, and inspect the results.

## Local setup

Install **[qsbit-compiler](https://github.com/qsbit-org/qsbit-compiler#build)**
and **[qsbit-sim](https://github.com/qsbit-org/qsbit-sim#build)** first.
Both `qsbitc` and `qsbit-sim` must be on PATH. Enable the simulator's
[Python bridge](https://qsbit-org.github.io/qsbit-sim/building.html#install-with-a-python-backend)
for Stim. Before starting Studio, activate a compatible Python environment with
`qsbit-sim-backends` and its Stim dependencies installed. Studio passes that
environment to the simulator and serves the installed simulator’s player assets.
Python’s major/minor version must match the simulator bridge; see the
[simulator backend setup](https://qsbit-org.github.io/qsbit-sim/backends.html).

On Linux, with Rust and LLVM 21 tools installed,
run from the repository root:

```sh
make install PREFIX="$HOME/.local"
export PATH="$HOME/.local/bin:$PATH"
source /path/to/your/venv/bin/activate
qsbit-studio
```

The venv can be the one used to develop qsbit-sim, prepared with
`uv sync --frozen --extra stim` in its repository. Studio uses an existing
environment; it does not create one or install Python packages. A missing
interpreter or backend package causes a startup error.

## Use

Open <http://127.0.0.1:3000>:

1. **Upload** a `.blog` file exported from the [Bloq editor](https://bloqec.com/editor/).
2. **Run** by clicking **Compile & run**.
3. **Inspect** the execution trace in **Replay** and measurement outputs in **Results**.

## Limitations

Decoder feedback and dynamic quantum operations are not yet supported.

## Learn more

[Setup details](docs/setup.md) · [Architecture](docs/architecture.md) ·
[API](docs/api.md) · [Testing](docs/testing.md)

## License

[Apache License 2.0](LICENSE).
