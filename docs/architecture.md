# Architecture

Studio owns the upload interface, run orchestration, artifact validation, and
metric presentation. Bloq owns graph semantics and physical compilation;
qsbit-compiler owns controller code generation; qsbit-sim owns execution and
trace interpretation.

## Compilation boundary

`qsbit-blog-export` reads UTF-8 BLOG using Bloq's public Rust APIs. It compiles the
graph into Bloq IR, lowers it into the typed Bloq VM program, and emits verified
Adaptive QIR. The adapter rejects empty graphs, decoder-dependent tasks,
unsupported exporter semantics, and programs exceeding the selected target’s
qubit limit before emitting QIR.
It never replaces the uploaded graph with a built-in example.

The adapter records terminal measurement slots and top-level raw observables in
`export.json`. `reference.stim` is optional and is produced only when the same
compiled IR supports static Stim emission.

Target files are loaded at startup using repeatable `--target` options. Their file
stems identify selections in the UI and API; the first is the default. Studio
snapshots the selected file as `target.json` in each job. It checks only catalog
identity and JSON structure; qsbit-compiler validates target semantics.

`presets/studio-ideal-v1.json` contains the demonstration controller parameters:
20 ns gates, 40 ns measurement acquisition and native reset, 10000 ns initial
start, `block_cycles: 1000`, 5 ns CPU and 20 ns TCU periods, and a 32-qubit limit.
Its `ideal` object asks qsbit-compiler to derive resources and codeword mappings
from validated QIR. Fixed devices instead supply explicit qubits, ports and
mappings. Studio does not parse QIR operations or implement reset lowering.
These settings describe a demonstration controller, not measured hardware.

The service launches the exporter, `qsbitc`, `qsbit-validate`, and a fresh `qsbit-sim` process for
each job. It passes arguments directly without shell interpolation. The compiler’s shared validator checks the ELF digest, manifest schema, ABI layout,
and run-profile agreement before simulation. The generated
run profile retains the compiler's target settings and overrides only the chosen
backend, seed, output paths, and trace-stall filter.

Job admission reserves capacity before initializing a temporary directory. Only
fully prepared jobs enter the registry. Pipeline execution lives in `pipeline.rs`;
`jobs.rs` owns admission, status, cancellation, and shutdown. Artifact reads open
regular files without following symbolic links.

## Execution player ownership

The only trace reducer and renderer are qsbit-sim's `trace-model.js` and
`trace-player.js`. Shared typography, colors, styles and theme handling also come
from qsbit-sim's `web/` directory.
At startup Studio invokes the selected simulator's `--installation-info` and
validates schema 1, player API 1, and backend API 4 when Python is enabled. It serves the reported `web_root` bytes
through `/player/`. Studio's JavaScript handles uploads, task state, metrics, theme
synchronization and workspace tabs.

The server converts JSONL records and the simulator summary into the existing
schema-1 replay bundle. The HTML host marks the player as standalone and passes
that bundle URL. Each selected run gets an independent iframe document so player
initialization, event listeners, and playback timers belong to one recording.

The simulator installation owns its native executable and player assets. Users
manage the Python backend package and dependencies in an external environment.
Studio inherits its launch environment; the simulator resolves Python and imports
the backend there. Studio does not import Python packages itself. Each job records the discovered metadata and resolved executable in
`installation.json`. Stop Studio before upgrading that installation, then restart
to discover the new resources. Native integration CI builds the latest default
branches of both native tools and records the actual revisions.

A development `--player-root` override is recorded explicitly in health and job
metadata; replay revision is null because custom assets have no verified revision.


## Observations and results

The service rejects unsupported trace schemas, decreasing timestamps, timestamps
outside JavaScript's exact integer range, and summary/terminal-event mismatch.
Metrics use documented trace fields; simulated time and CPU edge indices are
separate from host durations. A completed simulator with a nonzero program exit
status yields a failed Studio job while retaining its replay and metrics.

Model faults can produce a partial replay when both the fault trace and failed
summary validate. Compilation failures, cancelled native stages, and construction
failures may have diagnostics without a replay.

`memory.bin` is interpreted using the compiler manifest's `qsbit-adaptive-v1`
output layout. The accepted output count must match the adapter's labels, stay
within the ABI capacity, and contain only bits. Corrected logical outcomes and
logical error rates are not derived from raw observations.
