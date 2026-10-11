# Setup

Run commands from the Studio repository root. The Rust toolchain is pinned in
[rust-toolchain.toml](../rust-toolchain.toml), Rust dependencies and Bloq's Git
revision in [Cargo.toml](../Cargo.toml) and [Cargo.lock](../Cargo.lock).

The server and BLOG adapter are Rust programs. Python is optional for numerical
backends and browser tests; a default Cargo build does not install Python packages.

## Native tools

Install `qsbitc` following the [compiler build instructions](https://github.com/qsbit-org/qsbit-compiler#build)
and `qsbit-sim` following the [simulator build instructions](https://github.com/qsbit-org/qsbit-sim#build).
The simulator must support the selected backend. The BLOG adapter also uses the
LLVM optimizer and assembler required by Bloq's pinned QIR exporter.

The pinned simulator checkout used for the player can also be built as the
simulator; Studio never builds it implicitly. Dependency provisioning and native
builds use each project's standard CMake workflow.

Initialize the player sources and build both Studio executables:

```sh
git submodule update --init
cargo build --locked --features exporter
```

On Linux, run with installed native executables on PATH:

```sh
cargo run --locked
```

Or select local native builds:

```sh
cargo run --locked -- \
  --compiler ../qsbit-compiler/build-clang/qsbitc \
  --simulator ../qsbit-sim/build-clang/qsbit-sim
```

Relative executable paths are resolved from the launch directory. Commands run
in isolated job directories; uploaded filenames are never used as paths or shell
commands. The default exporter is `qsbit-blog-export` beside the server binary.

## Target selection

The default target is `presets/studio-ideal-v1.json`, read at startup rather than
embedded in the executable. Edit its timing and resource settings and restart
Studio; no Rust rebuild is needed. Load additional ideal or fixed-device targets:

```sh
cargo run --locked -- \
  --compiler ../qsbit-compiler/build-clang/qsbitc \
  --target presets/studio-ideal-v1.json \
  --target /path/to/device.json
```

The first target is the default; each filename stem must be unique. Select a
target for each run in the browser. The server snapshots files at startup and
copies the chosen bytes into each job's `target.json`. Backend and seed remain
per-run settings. Explicit device targets use the compiler's normal target format.
Ideal targets use its `ideal` configuration, including a native reset duration.

Native integration CI builds qsbit-compiler from the latest commit on its remote
default branch and logs the selected commit. The compiler is not version-pinned.
For local development, select a compiler build with `--compiler`. Ideal targets
require a compiler supporting the `ideal` configuration and a simulator with
native reset support. The player submodule is independent of executable selection.

## Optional Stim backend

Python dependencies are declared in [pyproject.toml](../pyproject.toml) and resolved
in [uv.lock](../uv.lock). The backend package comes from the pinned simulator
submodule rather than a Studio implementation.

```sh
uv sync --frozen --extra stim
uv run --frozen --extra stim cargo run --locked
```

The simulator selects the active virtual environment. For an existing environment,
pass `--python PATH_TO_ENV/bin/python`. The path is preserved even if it is a
symlink, so the simulator uses that environment's packages. `--python-path` can
point to a directory containing an externally installed `qsbit_backend` package.
Neither option installs packages.

## Service configuration

`qsbit-studio --help` lists the complete options. The default listener is loopback.
The server currently targets Linux and uses Unix process groups for teardown.

| Option | Default and behavior |
| --- | --- |
| `--listen` | `127.0.0.1:3000`; use port 0 for an OS-assigned port. |
| `--compiler`, `--simulator` | Installed `qsbitc` and `qsbit-sim`; environment overrides are `QSBIT_COMPILER` and `QSBIT_SIMULATOR`. |
| `--target` | Repeatable target JSON path; defaults to `presets/studio-ideal-v1.json`. First target is the default selection. |
| `--exporter` | Exporter beside the Studio binary; `QSBIT_BLOG_EXPORTER` overrides it. |
| `--player-root` | `external/qsbit-sim/web`; supports another simulator checkout for development. |
| `--python` | Optional simulator interpreter; also accepts `QSBIT_PYTHON`. |
| `--python-path` | Optional simulator Python adapter directory. |
| `--work-dir` | `.local/runs`; Studio creates a fresh owned session directory. |
| `--max-jobs` | 32 retained jobs; full capacity returns HTTP 503. |
| `--max-running` | Two concurrently executing pipelines; others queue. |
| `--stage-timeout-seconds` | 120 host seconds per native stage. |

Uploads are capped at 256 KiB. Replay traces are capped at 12 MiB and total native
artifacts at 64 MiB per job. A 100 ms monitor terminates stages exceeding limits;
an output-producing process can briefly overshoot between checks. These are
application limits, not an OS memory sandbox.

Run artifacts survive page reload within the same server session. Removing a
terminal run deletes its directory and releases capacity. SIGINT/SIGTERM cancels
queued/running jobs, kills their process groups, and removes session artifacts
after teardown. An unclean process exit can leave directories in `.local/runs`;
the next launch creates a new session and does not resume old jobs.

The service has no authentication or tenant isolation. Hosted multiuser operation
requires a separate authenticated service boundary and OS resource isolation.
