//! The Studio ideal-controller preset: 20 ns gates, 40 ns acquisition.

use std::collections::BTreeSet;

use anyhow::{Result, bail, ensure};
use regex::Regex;
use serde_json::{Value, json};

pub fn target_for_qir(source: &str, qubits: u32) -> Result<Value> {
    ensure!(
        (1..=32).contains(&qubits),
        "The controller supports 1–32 physical qubits, including scratch resources."
    );
    let calls = Regex::new(r"call void @__quantum__qis__(\w+)__(body|adj)\(([^\n]*)\)")?;
    let resources = Regex::new(r"ptr (?:null|inttoptr \(i64 (\d+) to ptr\))")?;
    let mut operations = BTreeSet::new();
    for call in calls.captures_iter(source) {
        let name = &call[1];
        let adjoint = &call[2] == "adj";
        ensure!(
            matches!(name, "h" | "x" | "z" | "s" | "t" | "cx" | "mz" | "reset"),
            "Unsupported QIR gate: {name}."
        );
        ensure!(
            !adjoint || matches!(name, "s" | "t"),
            "Unsupported adjoint: {name}."
        );
        let ids: Vec<u32> = resources
            .captures_iter(&call[3])
            .map(|r| r.get(1).map_or(Ok(0), |value| value.as_str().parse()))
            .collect::<std::result::Result<_, _>>()?;
        ensure!(
            ids.len() == if matches!(name, "cx" | "mz") { 2 } else { 1 },
            "Expected static resource IDs for {name}."
        );
        let operands = if name == "cx" { ids } else { ids[..1].to_vec() };
        ensure!(
            operands.iter().all(|q| *q < qubits),
            "QIR gate exceeds physical qubit allocation."
        );
        let operation = match name {
            "mz" | "reset" => "measure",
            _ => name,
        };
        operations.insert((
            format!("{operation}{}", if adjoint { "dg" } else { "" }),
            operands.clone(),
        ));
        if name == "reset" {
            operations.insert(("x".to_owned(), operands));
        }
    }
    if operations.is_empty() {
        bail!("The graph exports no quantum operations.");
    }
    let mappings: Vec<_> = operations
        .into_iter()
        .enumerate()
        .map(|(index, (operation, qubits))| {
            json!({"operation": operation, "port": qubits[0], "qubits": qubits,
            "codeword": index + 1, "duration_ns": if operation == "measure" { 40 } else { 20 }})
        })
        .collect();
    Ok(
        json!({"schema": 1, "name": "studio-ideal-v1", "qubits": qubits,
        "ports": qubits, "start_ns": 10000, "block_cycles": 1000, "mappings": mappings}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measurement_results_are_not_qubit_operands() {
        let source = "call void @__quantum__qis__mz__body(ptr null, ptr inttoptr (i64 90 to ptr))\ncall void @__quantum__qis__reset__body(ptr null)\ncall void @__quantum__qis__cx__body(ptr null, ptr inttoptr (i64 1 to ptr))";
        let target = target_for_qir(source, 2).unwrap();
        let mappings = target["mappings"].as_array().unwrap();
        assert_eq!(mappings.len(), 3);
        assert!(mappings.iter().any(|m| m["operation"] == "measure"
            && m["qubits"] == json!([0])
            && m["duration_ns"] == 40));
        assert!(
            mappings
                .iter()
                .any(|m| m["operation"] == "cx" && m["qubits"] == json!([0, 1]))
        );
    }

    #[test]
    fn rejects_invalid_resource_and_operation_boundaries() {
        for (source, count) in [
            (
                "call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))",
                2,
            ),
            ("call void @__quantum__qis__h__body(ptr %dynamic)", 2),
            ("call void @__quantum__qis__rx__body(ptr null)", 2),
            ("call void @__quantum__qis__h__adj(ptr null)", 2),
            ("call void @__quantum__qis__h__body(ptr null)", 33),
            ("", 2),
        ] {
            assert!(target_for_qir(source, count).is_err());
        }
    }
}
