# API and metrics

API bodies and artifact JSON use machine-readable fields. Error responses have
`{"error":{"code":"...","message":"..."}}`; malformed route identifiers may
also use Axum's HTTP 400 extractor response.

## Submit a run

`POST /api/jobs` accepts JSON:

```json
{
  "filename": "memory.blog",
  "source": "BLOG 1.0\nmodule main {\n0: ZXX [0, 0, 0]\n}\n",
  "distance": 3,
  "target": "studio-ideal-v1",
  "backend": "stim",
  "seed": 1
}
```

`filename` is a basename ending in `.blog`, with at most 128 bytes. `source` is
1–262144 UTF-8 bytes with a BLOG header. Unknown keys are rejected. Distance is
3, 5, or 7; backend is `stim` or `mock`; seed is an unsigned 32-bit integer.
The defaults are distance 3, Stim, seed 1, and the first configured target.
`target` selects a server-configured filename stem, never a client filesystem path.
Unknown selections return HTTP 400. Reports and `request.json` record the resolved
selection; `target.json` preserves its configuration bytes.
`installation.json` records the resolved simulator, its startup discovery response,
and the effective player directory/override. A Stim request against a simulator
without Python support returns HTTP 400 with `backend_unavailable`.

HTTP 202 returns a job report with an opaque UUID. Invalid input returns 400/422,
oversized HTTP bodies return 413, and retained-job capacity returns 503 with
`error.code: capacity_full`. A graph accepted for processing can still fail
parsing, compilation, backend validation, or execution.

## Inspect and manage runs

| Endpoint | Behavior |
| --- | --- |
| `GET /api/health` | Service version, upload format, capacity, target IDs (`targets`), default target (`default_target`), Bloq revision, simulator `installation` metadata, effective `player_root`, and `player_override`. |
| `GET /api/jobs` | Current server-session reports in submission order. |
| `GET /api/jobs/{id}` | Current status, stage timings, metrics, diagnostics, and available artifact names. |
| `POST /api/jobs/{id}/cancel` | Request cancellation; 202 does not mean teardown has finished. Terminal jobs return 409. |
| `DELETE /api/jobs/{id}` | Remove a terminal run and release capacity; active jobs return 409. |
| `GET /api/jobs/{id}/replay` | Validated schema-1 replay bundle; unavailable replay returns 409. |
| `GET /runs/{id}/player` | Standalone host for the shared qsbit-sim player. |
| `GET /api/jobs/{id}/artifacts/{name}` | Download a listed artifact after the run becomes terminal. Unlisted names return 404. |

Stages are `queued`, `exporting`, `compiling`, `simulating`, `analysing`, and one
terminal value: `completed`, `failed`, or `cancelled`. Reports preserve
`failed_stage`, `error`, `replay_ready`, the input `source_sha256`, and
`submitted_at_ms`. The submitted timestamp is Unix host time, not simulated time.

## Metrics

Metrics appear in the report and downloadable `metrics.json`.

| Field | Measurement boundary |
| --- | --- |
| `simulator_success` | Summary's `success`; full simulator drain without a model fault. |
| `program_exit_code` | Summary's program outcome; kept separate from simulator completion. |
| `stop_tick_ns` | Summary's absolute stop tick, matched to a terminal trace event. |
| `simulated_duration_ns` | Stop tick minus the first recorded tick; includes platform startup. |
| `retired_instructions` | Count of `InstructionRetired` events. |
| `last_cpu_cycle` | Maximum recorded CPU edge index; not elapsed nanoseconds. |
| `physical_operations` | Count of `OperationStart` records; includes acquisitions and reset work. |
| `operations` | Operation-start counts grouped by recorded operation name. |
| `measurement_samples` | Count of `MeasurementSampled` records, including reset measurements. |
| `timing_points` | Count of `TimingPointTriggered` records. |
| `timing_queue_peak` | Maximum post-enqueue occupancy in `TimingPointEnqueued.value`. |
| `measurement_delivery` | Sample-to-register-update latency, matched by epoch, core, measurement ID, and target; sample count, mean, and maximum in ns. |
| `physical_qubits` | Exported allocation, including scratch resources. |
| `outputs` | Manifest-decoded bits labeled `record:N` or `raw_observable:N`; no decoder correction. |
| `trace_records` | Number of validated observations. |

Report `host_duration_ms` includes queue waiting and processing. `stage_host_ms`
measures native exporter, compiler, and simulator calls with a monotonic host
clock. These values are execution measurements, not controlled benchmarks.

Faulted/nonzero-status runs have no accepted output bits. A valid partial replay
can still show their recorded execution and metrics. Missing delivery pairs are
excluded from the latency sample count; the browser does not invent missing state.
