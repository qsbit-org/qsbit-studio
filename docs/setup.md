# Setup

Run commands from the Studio repository root. The Rust toolchain is pinned in
[rust-toolchain.toml](../rust-toolchain.toml), Rust dependencies and Bloq's Git
revision in [Cargo.toml](../Cargo.toml) and [Cargo.lock](../Cargo.lock).

The server and BLOG adapter are Rust programs. The compiler’s Python tools
provide artifact validation. Numerical backends and browser tests have separate
Python dependencies; Cargo does not install them.

## Native tools

Install `qsbitc` following the [compiler build instructions](https://github.com/qsbit-org/qsbit-compiler#build)
and `qsbit-sim` following the [simulator build instructions](https://github.com/qsbit-org/qsbit-sim#build).
The simulator must support the selected backend. The BLOG adapter also uses the
LLVM optimizer and assembler required by Bloq's pinned QIR exporter.

Install the compiler’s artifact validator, then both Studio executables:

```sh
uv tool install "git+https://github.com/qsbit-org/qsbit-compiler"
make install PREFIX="$HOME/.local"
export PATH="$HOME/.local/bin:$PATH"
source /path/to/your/venv/bin/activate
qsbit-studio
```

After adding the installation's `bin` directory to PATH, launch Studio from any
working directory. Activate the simulator’s Python environment before launching
Studio. The Studio source checkout is not needed at runtime.
`qsbit-blog-export` is installed beside Studio; `qsbitc` is installed by the
compiler's CMake install into its prefix's `bin` directory. Both are native
executables. The BLOG exporter also requires LLVM tools on PATH; the compiler
itself does not require Python. Studio requires `qsbit-validate`, which shares
its validator with `qsbit-run`.

For development, build both binaries with `cargo build --locked --features exporter`.
Select tools through PATH or environment variables:

```sh
export QSBIT_COMPILER=/path/to/qsbitc
export QSBIT_SIMULATOR=/path/to/qsbit-sim
cargo run --locked
```

Relative executable paths are resolved from the launch directory. Commands run
in isolated job directories; uploaded filenames are never used as paths or shell
commands. Child processes discard `PYTHONHOME`, `PYTHONPATH`, and `PYTHONUSERBASE`;
`QSBIT_PYTHON` and `VIRTUAL_ENV` retain their interpreter-selection semantics.
The default exporter is `qsbit-blog-export` beside the server binary.

## Target selection

Studio embeds `presets/studio-ideal-v1.json` as its default target. Installation also
copies it to `PREFIX/share/qsbit-studio/presets/` for customization. Pass edited
presets with `--target` and restart Studio; no rebuild is needed:

```sh
cargo run --locked -- \
  --target presets/studio-ideal-v1.json \
  --target /path/to/device.json
```

The first target is the default; each filename stem must be unique. Select a
target for each run in the browser. The server snapshots files at startup and
copies the chosen bytes into each job's `target.json`. Backend and seed remain
per-run settings. Explicit device targets use the compiler's normal target format.
Ideal targets use its `ideal` configuration, including a native reset duration.

Container integration uses the compiler and simulator revisions pinned in
`toolchain.lock.json`.
For local development, select a compiler build with `--compiler`. Ideal targets
require a compiler supporting the `ideal` configuration and a simulator with
native reset support and the `--installation-info` discovery interface.

## Simulator installation and upgrades

Follow the simulator's [Python backend setup](https://qsbit-org.github.io/qsbit-sim/backends.html).
Its native installation contains the executable, web assets, and metadata:

```text
PREFIX/bin/qsbit-sim                       launcher
PREFIX/lib/qsbit-sim/qsbit-sim-native       native executable
PREFIX/lib/qsbit-sim/runtime.json           installation metadata
PREFIX/lib/qsbit-sim/web/                   player assets
```

Users install `qsbit-sim-backends` and the required optional dependencies in an
external Python environment. For local simulator development, `uv sync --frozen
--extra stim` from the simulator repository prepares its `.venv`. Activate that
environment before starting Studio:

```sh
source /path/to/qsbit-sim/.venv/bin/activate
qsbit-studio
```

The simulator selects explicit `QSBIT_PYTHON` when supplied, then the active
`VIRTUAL_ENV/bin/python`; installed commands otherwise find a matching versioned
Python on PATH. Python's major/minor version must match the simulator bridge.
Studio inherits these settings and obtains the actual interpreter, backend package
location, verified backend API version, and player path from `--installation-info`
at startup. A missing interpreter,
backend package, resource or unsupported interface version fails startup. Users
are responsible for installing the selected numerical backend's dependencies
and updating the package with the native executable.

Keep the linked Python library and native dependencies available. To upgrade,
stop Studio and simulations, update native code with the simulator's normal CMake
build/install and update the backend package in the external environment. Restart
Studio with that environment active to refresh discovery metadata. A Python
major/minor change requires rebuilding the native bridge against that version.

## Service configuration

`qsbit-studio --help` lists the complete options. The default listener is loopback.
The server currently targets Linux and uses Unix process groups for teardown.

| Option | Default and behavior |
| --- | --- |
| `--listen` | `127.0.0.1:3000`; non-loopback addresses require `--allow-remote`. |
| `--allow-remote` | Acknowledge unauthenticated network access without process isolation. |
| `--validator` | `qsbit-validate`; `QSBIT_VALIDATOR` overrides it. |
| `--compiler`, `--simulator` | Installed `qsbitc` and `qsbit-sim`; environment overrides are `QSBIT_COMPILER` and `QSBIT_SIMULATOR`. |
| `--target` | Repeatable target JSON path; defaults to the embedded preset. First target is the default selection. |
| `--exporter` | Exporter beside the Studio binary; `QSBIT_BLOG_EXPORTER` overrides it. |
| `--player-root` | Discovered from the simulator; explicit override for development, labeled as custom resources. |
| `--work-dir` | `$XDG_CACHE_HOME/qsbit-studio/runs` or `$HOME/.cache/qsbit-studio/runs`; creates an owned session directory. |
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
after teardown. An unclean process exit can leave directories in the configured cache;
the next launch creates a new session and does not resume old jobs.

The service has no authentication or tenant isolation. Hosted multiuser operation
requires a separate authenticated service boundary and OS resource isolation.
