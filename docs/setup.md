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

Install both Studio executables and the default target preset:

```sh
make install PREFIX="$HOME/.local"
export PATH="$HOME/.local/bin:$PATH"
qsbit-studio
```

After adding the installation's `bin` directory to PATH, launch Studio from any
working directory. No active Python venv or Studio source checkout is needed.
`qsbit-blog-export` is installed beside Studio; `qsbitc` is installed by the
compiler's CMake install into its prefix's `bin` directory. Both are native
executables. The BLOG exporter also requires LLVM tools on PATH; the compiler
itself does not require Python. Optional compiler Python helpers are separate.

For development, `cargo build --locked --features exporter` copies the default
preset beside the build output. Select local native builds explicitly:

```sh
cargo run --locked -- \
  --compiler ../qsbit-compiler/build-clang/qsbitc \
  --simulator ../qsbit-sim/build-clang/qsbit-sim
```

Relative executable paths are resolved from the launch directory. Commands run
in isolated job directories; uploaded filenames are never used as paths or shell
commands. The default exporter is `qsbit-blog-export` beside the server binary.

## Target selection

The installed default target is `PREFIX/share/qsbit-studio/presets/studio-ideal-v1.json`,
resolved from the Studio executable and read at startup. A development build uses
`target/debug/presets/studio-ideal-v1.json` (or the selected build profile). Edit
its timing and resource settings and restart Studio; no Rust rebuild is needed. Load additional ideal or fixed-device targets:

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
native reset support and the `--installation-info` discovery interface.

## Simulator installation and upgrades

Follow the simulator's [installed Python runtime workflow](https://qsbit-org.github.io/qsbit-sim/building.html#install-with-a-python-backend).
The default layout is:

```text
PREFIX/bin/qsbit-sim                       launcher
PREFIX/lib/qsbit-sim/qsbit-sim-native       native executable
PREFIX/lib/qsbit-sim/runtime.json           installation metadata
PREFIX/lib/qsbit-sim/venv/                  dedicated Python environment
PREFIX/lib/qsbit-sim/web/                   matching player assets
```

The simulator's installation step requires its backend package from the same
checkout, installed non-editably in the dedicated environment. Its Python
major/minor version must match the linked Python library. The installed simulator
ignores outer `VIRTUAL_ENV`, `QSBIT_PYTHON`, and Python import-path variables.
Studio obtains the player path and runtime metadata from `--installation-info`
at startup; missing resources or unsupported interface versions fail startup.
Studio has no Python runtime flags or dependency on `qsbit_backend`.

Treat the prefix as a fixed installation location: venvs are not relocatable.
Keep the base Python installation and native shared libraries available. Normal
launches never install packages or access the network. To upgrade, stop Studio
and active simulations, update the simulator checkout, provision its locked
runtime dependencies and backend in the same prefix, rebuild and reinstall,
then restart Studio. Reinstallation updates native code, metadata, and web assets;
it validates the matching backend before replacing the native executable.
Do not replace installation files while Studio or simulations are running.
A Python version change requires recreating the dedicated venv and rebuilding the
bridge with that interpreter. Installing elsewhere requires selecting that new
simulator on PATH or through `--simulator`.

## Service configuration

`qsbit-studio --help` lists the complete options. The default listener is loopback.
The server currently targets Linux and uses Unix process groups for teardown.

| Option | Default and behavior |
| --- | --- |
| `--listen` | `127.0.0.1:3000`; use port 0 for an OS-assigned port. |
| `--compiler`, `--simulator` | Installed `qsbitc` and `qsbit-sim`; environment overrides are `QSBIT_COMPILER` and `QSBIT_SIMULATOR`. |
| `--target` | Repeatable target JSON path; defaults to the preset beside the build or under the installation’s `share` directory. First target is the default selection. |
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
