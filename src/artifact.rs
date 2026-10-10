//! Validate simulator observations and derive metrics from documented fields.

use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::Path,
};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::MAX_TRACE_BYTES;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputBit {
    pub label: String,
    pub value: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeliveryLatency {
    pub samples: u64,
    pub mean_ns: Option<f64>,
    pub max_ns: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metrics {
    pub simulator_success: bool,
    pub program_exit_code: Option<u32>,
    pub stop_tick_ns: u64,
    pub simulated_duration_ns: u64,
    pub retired_instructions: u64,
    pub last_cpu_cycle: Option<u64>,
    pub physical_operations: u64,
    pub measurement_samples: u64,
    pub timing_points: u64,
    pub timing_queue_peak: u64,
    pub trace_records: usize,
    pub physical_qubits: u32,
    pub backend: String,
    pub operations: BTreeMap<String, u64>,
    pub measurement_delivery: DeliveryLatency,
    pub outputs: Vec<OutputBit>,
}

pub fn validate_trace(text: &str) -> Result<Vec<Value>> {
    ensure!(
        text.len() as u64 <= MAX_TRACE_BYTES,
        "Trace exceeds the replay size limit."
    );
    let mut events = Vec::new();
    let mut previous = 0;
    for (line, text) in text
        .lines()
        .enumerate()
        .filter(|(_, text)| !text.trim().is_empty())
    {
        let event: Value = serde_json::from_str(text)
            .with_context(|| format!("invalid trace JSON at line {}", line + 1))?;
        ensure!(
            event["schema"] == 1,
            "Unsupported trace schema at line {}.",
            line + 1
        );
        let tick = event["tick"]
            .as_u64()
            .context("Trace tick must be a nonnegative integer.")?;
        ensure!(
            tick <= 9_007_199_254_740_991,
            "Trace tick exceeds the player's exact integer range."
        );
        ensure!(
            tick >= previous,
            "Trace time decreases at line {}.",
            line + 1
        );
        ensure!(
            event["kind"].as_str().is_some_and(|kind| !kind.is_empty()),
            "Trace kind is missing."
        );
        previous = tick;
        events.push(event);
    }
    ensure!(!events.is_empty(), "Trace contains no events.");
    Ok(events)
}

pub fn metrics_for(
    events: &[Value],
    summary: &Value,
    export: &Value,
    outputs: Vec<OutputBit>,
) -> Result<Metrics> {
    ensure!(
        summary["schema"] == 1,
        "Unsupported simulator summary schema."
    );
    let success = summary["success"]
        .as_bool()
        .context("Simulator summary has no completion status.")?;
    let stop_tick = summary["stop_tick"]
        .as_u64()
        .context("Simulator summary has no stop tick.")?;
    let expected_terminal = if success {
        "SimulationCompleted"
    } else {
        "Fault"
    };
    ensure!(
        events
            .iter()
            .any(|e| e["kind"] == expected_terminal && e["tick"] == stop_tick),
        "Summary does not match the terminal trace event."
    );
    let start = events
        .first()
        .and_then(|event| event["tick"].as_u64())
        .context("Empty trace.")?;
    let mut metrics = Metrics {
        simulator_success: success,
        program_exit_code: summary["exit_code"]
            .as_u64()
            .map(u32::try_from)
            .transpose()?,
        stop_tick_ns: stop_tick,
        simulated_duration_ns: stop_tick
            .checked_sub(start)
            .context("Stop tick precedes the trace.")?,
        retired_instructions: 0,
        last_cpu_cycle: None,
        physical_operations: 0,
        measurement_samples: 0,
        timing_points: 0,
        timing_queue_peak: 0,
        trace_records: events.len(),
        physical_qubits: u32::try_from(
            export["qubits"]
                .as_u64()
                .context("Missing exported resource count.")?,
        )?,
        backend: summary["backend"]
            .as_str()
            .context("Missing backend name.")?
            .to_owned(),
        operations: BTreeMap::new(),
        measurement_delivery: DeliveryLatency::default(),
        outputs,
    };
    let mut samples = HashMap::new();
    let mut latencies = Vec::new();
    for event in events {
        match event["kind"].as_str().unwrap_or_default() {
            "InstructionRetired" => {
                metrics.retired_instructions += 1;
                if let Some(cycle) = event["cycle"].as_u64() {
                    metrics.last_cpu_cycle = Some(metrics.last_cpu_cycle.unwrap_or(0).max(cycle));
                }
            }
            "CpuPipelineUpdated" => {
                if let Some(cycle) = event["cycle"].as_u64() {
                    metrics.last_cpu_cycle = Some(metrics.last_cpu_cycle.unwrap_or(0).max(cycle));
                }
            }
            "OperationStart" => {
                metrics.physical_operations += 1;
                *metrics
                    .operations
                    .entry(event["operation"].as_str().unwrap_or("unknown").to_owned())
                    .or_default() += 1;
            }
            "TimingPointEnqueued" => {
                metrics.timing_queue_peak = metrics
                    .timing_queue_peak
                    .max(event["value"].as_u64().unwrap_or(0));
            }
            "TimingPointTriggered" => metrics.timing_points += 1,
            "MeasurementSampled" => {
                metrics.measurement_samples += 1;
                if let Some(targets) = event["targets"].as_array() {
                    for target in targets {
                        let key = (
                            event["epoch"].as_u64(),
                            event["core"].as_u64(),
                            event["id"].as_u64(),
                            target.as_u64(),
                        );
                        ensure!(
                            samples
                                .insert(key, event["tick"].as_u64().unwrap())
                                .is_none(),
                            "Duplicate measurement sample identity."
                        );
                    }
                }
            }
            "MeasurementRegisterUpdated" => {
                if let Some(targets) = event["targets"].as_array() {
                    for target in targets {
                        let key = (
                            event["epoch"].as_u64(),
                            event["core"].as_u64(),
                            event["id"].as_u64(),
                            target.as_u64(),
                        );
                        if let Some(sample) = samples.remove(&key) {
                            latencies.push(
                                event["tick"]
                                    .as_u64()
                                    .unwrap()
                                    .checked_sub(sample)
                                    .context("Measurement was delivered before sampling.")?,
                            );
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if !latencies.is_empty() {
        let sum: u64 = latencies
            .iter()
            .try_fold(0_u64, |sum, value| sum.checked_add(*value))
            .context("Latency sum overflow.")?;
        metrics.measurement_delivery = DeliveryLatency {
            samples: latencies.len() as u64,
            mean_ns: Some(sum as f64 / latencies.len() as f64),
            max_ns: latencies.iter().max().copied(),
        };
    }
    Ok(metrics)
}

pub fn read_outputs(memory: &[u8], manifest: &Value, export: &Value) -> Result<Vec<OutputBit>> {
    ensure!(
        manifest["abi"] == "qsbit-adaptive-v1",
        "Unsupported executable output ABI."
    );
    let layout = &manifest["output_buffer"];
    let labels = export["output_labels"]
        .as_array()
        .context("Missing output labels.")?;
    // This compiler ABI uses RAM base zero; addresses are offsets in memory.bin.
    let count_address = layout["count_address"]
        .as_u64()
        .context("Missing output count address.")?;
    let data_address = layout["data_address"]
        .as_u64()
        .context("Missing output data address.")?;
    let word_at = |offset: u64| -> Result<u32> {
        let end = offset.checked_add(4).context("Output address overflow.")?;
        let bytes = memory
            .get(usize::try_from(offset)?..usize::try_from(end)?)
            .context("Output address exceeds RAM dump.")?;
        Ok(u32::from_le_bytes(bytes.try_into()?))
    };
    let count = word_at(count_address)? as usize;
    ensure!(
        count == labels.len() && count <= 16383,
        "Output count differs from exported record layout."
    );
    (0..count)
        .map(|index| {
            let offset = data_address
                .checked_add(
                    (index as u64)
                        .checked_mul(4)
                        .context("Output offset overflow.")?,
                )
                .context("Output address overflow.")?;
            let value = word_at(offset)?;
            ensure!(value <= 1, "Output {index} is not a bit.");
            Ok(OutputBit {
                label: labels[index]
                    .as_str()
                    .context("Invalid output label.")?
                    .to_owned(),
                value,
            })
        })
        .collect()
}

pub fn inspect_run(directory: &Path, filename: &str) -> Result<Metrics> {
    let trace_path = directory.join("trace.jsonl");
    ensure!(
        fs::metadata(&trace_path)?.len() <= MAX_TRACE_BYTES,
        "Trace exceeds the replay size limit."
    );
    let events = validate_trace(&fs::read_to_string(trace_path)?)?;
    let summary: Value = serde_json::from_slice(&fs::read(directory.join("summary.json"))?)?;
    let export: Value = serde_json::from_slice(&fs::read(directory.join("export.json"))?)?;
    let outputs = if summary["success"] == true && summary["exit_code"] == 0 {
        let manifest: Value =
            serde_json::from_slice(&fs::read(directory.join("program.manifest.json"))?)?;
        read_outputs(&fs::read(directory.join("memory.bin"))?, &manifest, &export)?
    } else {
        Vec::new()
    };
    let metrics = metrics_for(&events, &summary, &export, outputs)?;
    let bundle = json!({"schema": 1, "revision": crate::PLAYER_REVISION, "examples": [{
        "name": filename, "program": "", "configuration": summary.get("configuration"),
        "cores": summary.get("cores"), "backend": summary["backend"], "events": events
    }]});
    fs::write(directory.join("replay.json"), serde_json::to_vec(&bundle)?)?;
    fs::write(
        directory.join("metrics.json"),
        serde_json::to_vec_pretty(&metrics)?,
    )?;
    Ok(metrics)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_time_and_schema_without_reordering_coincident_events() {
        let text =
            "{\"schema\":1,\"tick\":5,\"kind\":\"A\"}\n{\"schema\":1,\"tick\":5,\"kind\":\"B\"}";
        assert_eq!(validate_trace(text).unwrap()[1]["kind"], "B");
        for text in [
            "",
            "{}",
            "{\"schema\":2,\"tick\":0,\"kind\":\"A\"}",
            "{\"schema\":1,\"tick\":-1,\"kind\":\"A\"}",
            "{\"schema\":1,\"tick\":9007199254740992,\"kind\":\"A\"}",
            "{\"schema\":1,\"tick\":5,\"kind\":\"A\"}\n{\"schema\":1,\"tick\":4,\"kind\":\"B\"}",
        ] {
            assert!(validate_trace(text).is_err());
        }
    }

    #[test]
    fn metrics_distinguish_cpu_cycles_physical_time_and_program_status() {
        let events = vec![
            json!({"kind":"SessionStarted","tick":0}),
            json!({"kind":"InstructionRetired","tick":10,"cycle":2}),
            json!({"kind":"OperationStart","tick":20,"operation":"h"}),
            json!({"kind":"TimingPointEnqueued","tick":20,"value":3}),
            json!({"kind":"MeasurementSampled","tick":40,"epoch":0,"id":1,"targets":[0]}),
            json!({"kind":"MeasurementRegisterUpdated","tick":55,"epoch":0,"id":1,"targets":[0]}),
            json!({"kind":"SimulationCompleted","tick":60}),
        ];
        let summary =
            json!({"schema":1,"success":true,"exit_code":2,"stop_tick":60,"backend":"mock"});
        let metrics = metrics_for(&events, &summary, &json!({"qubits":2}), vec![]).unwrap();
        assert!(metrics.simulator_success);
        assert_eq!(metrics.program_exit_code, Some(2));
        assert_eq!(metrics.simulated_duration_ns, 60);
        assert_eq!(metrics.last_cpu_cycle, Some(2));
        assert_eq!(metrics.retired_instructions, 1);
        assert_eq!(metrics.timing_queue_peak, 3);
        assert_eq!(metrics.measurement_delivery.mean_ns, Some(15.0));
        assert!(
            metrics_for(
                &events,
                &json!({"schema":1,"success":false,"stop_tick":60}),
                &json!({"qubits":2}),
                vec![]
            )
            .is_err()
        );
    }

    #[test]
    fn measurement_ids_are_scoped_by_epoch_and_target() {
        let events = vec![
            json!({"kind":"SessionStarted","tick":0}),
            json!({"kind":"MeasurementSampled","tick":1,"epoch":0,"id":1,"targets":[0]}),
            json!({"kind":"MeasurementSampled","tick":3,"epoch":1,"id":1,"targets":[0]}),
            json!({"kind":"MeasurementRegisterUpdated","tick":5,"epoch":1,"id":1,"targets":[0]}),
            json!({"kind":"SimulationCompleted","tick":6}),
        ];
        let metrics = metrics_for(
            &events,
            &json!({"schema":1,"success":true,"exit_code":0,"stop_tick":6,"backend":"mock"}),
            &json!({"qubits":1}),
            vec![],
        )
        .unwrap();
        assert_eq!(metrics.measurement_delivery.samples, 1);
        assert_eq!(metrics.measurement_delivery.max_ns, Some(2));
    }

    #[test]
    fn output_reader_rejects_truncation_nonbits_and_layout_mismatch() {
        let manifest =
            json!({"abi":"qsbit-adaptive-v1","output_buffer":{"count_address":0,"data_address":4}});
        let export = json!({"output_labels":["record:0"]});
        let bytes = [1_u32.to_le_bytes(), 1_u32.to_le_bytes()].concat();
        assert_eq!(
            read_outputs(&bytes, &manifest, &export).unwrap()[0].value,
            1
        );
        assert!(read_outputs(&bytes[..6], &manifest, &export).is_err());
        assert!(
            read_outputs(
                &[1_u32.to_le_bytes(), 2_u32.to_le_bytes()].concat(),
                &manifest,
                &export
            )
            .is_err()
        );
        assert!(read_outputs(&bytes, &manifest, &json!({"output_labels":[]})).is_err());
        assert!(read_outputs(&bytes, &json!({"abi":"other"}), &export).is_err());
    }
}
