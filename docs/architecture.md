# Architecture

Studio owns the upload interface, run orchestration, artifact validation, and
metric presentation. Bloq owns graph semantics and physical compilation;
qsbit-compiler owns controller code generation; qsbit-sim owns execution and
trace interpretation.

## Compilation boundary

`qsbit-blog-export` reads UTF-8 BLOG using Bloq's public Rust APIs. It compiles the
graph into Bloq IR, lowers it into the typed Bloq VM program, and emits verified
Adaptive QIR. The adapter rejects empty graphs, decoder-dependent tasks,
unsupported exporter semantics, and resource counts exceeding the controller.
It never replaces the uploaded graph with a built-in example.

The adapter records terminal measurement slots and top-level raw observables in
`export.json`. `reference.stim` is optional and is produced only when the same
compiled IR supports static Stim emission.

`studio-ideal-v1` derives port/codeword mappings from verified QIR calls. It uses
20 ns gate intervals, 40 ns measurement acquisition, a 10000 ns initial target
start, and `block_cycles: 1000`. These values describe a demonstration controller,
not measured physical hardware. Scheduling and admission behavior remain owned
by qsbit-compiler and qsbit-sim.

The service launches the exporter, `qsbitc`, and a fresh `qsbit-sim` process for
each job. It passes arguments directly without shell interpolation. The generated
run profile retains the compiler's target settings and overrides only the chosen
backend, seed, output paths, trace-stall filter, and optional Python adapter path.

## Execution player ownership

The only trace reducer and renderer are qsbit-sim's `trace-model.js` and
`trace-player.js`. Their accompanying stylesheet also comes from qsbit-sim.
Studio serves those exact bytes through `/player/` from its pinned simulator
submodule. Studio's JavaScript handles uploads, task state, metrics, and iframe
sizing; it does not decode instructions or reconstruct simulator state.

The server converts JSONL records and the simulator summary into the existing
schema-1 replay bundle. The HTML host marks the player as standalone and passes
that bundle URL. Each selected run gets an independent iframe document so player
initialization, event listeners, and playback timers belong to one recording.

Changing the player requires a change in qsbit-sim. Advance the gitlink and
`PLAYER_REVISION` together and run the dependency and browser checks. A development
`--player-root` override uses that checkout's actual bytes; the declared revision
still identifies the repository's baseline pin.

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
