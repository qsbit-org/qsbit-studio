# QSBIT studio

[![CI](https://github.com/qsbit-org/qsbit-studio/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/qsbit-org/qsbit-studio/actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/qsbit-org/qsbit-studio)](LICENSE)

QSBIT Studio runs [Bloq](https://github.com/inmzhang/bloq) experiments in a local
web interface: upload a [BLOG file](https://github.com/inmzhang/bloq/blob/main/docs/graphs/blog.md),
run it, and inspect the results.

## Build

On Linux, install Rust, LLVM 21 tools, [qsbit-compiler](https://github.com/qsbit-org/qsbit-compiler#build)
and [qsbit-sim with Stim support](https://qsbit-org.github.io/qsbit-sim/building.html#install-with-a-python-backend).
Put `qsbitc` and `qsbit-sim` on PATH, then run from the repository root:

```sh
make install PREFIX="$HOME/.local"
export PATH="$HOME/.local/bin:$PATH"
```

## Run

Activate a compatible Python environment with `qsbit-sim-backends` and Stim installed:

```sh
source /path/to/venv/bin/activate
qsbit-studio
```

Open <http://127.0.0.1:3000>, upload a `.blog` file from the
[Bloq editor](https://bloqec.com/editor/), and click **Compile & run**.
Inspect execution in **Replay** and measurements in **Results**.
See [setup details](docs/setup.md) for Python compatibility and configuration.

## Limitations

Decoder feedback and dynamic quantum operations are not yet supported.

## Learn more

[Setup details](docs/setup.md) · [Architecture](docs/architecture.md) ·
[API](docs/api.md) · [Testing](docs/testing.md)

## License

[Apache License 2.0](LICENSE).
