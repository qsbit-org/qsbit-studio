//! Native BLOG adapter. Bloq's editor and compiler sources remain unmodified.

use std::{collections::BTreeMap, fs, path::PathBuf};

use anyhow::{Context, Result, ensure};
use bloq::{graph::BlockGraph, qir::QirOptions, vm::instruction::Instruction};
use clap::Parser;
use serde_json::json;

#[derive(Parser)]
#[command(
    version,
    about = "Compile a BLOG graph into verified QIR and a Studio target."
)]
struct Args {
    input: PathBuf,
    #[arg(long)]
    output: PathBuf,
    #[arg(long, default_value = "3")]
    distance: u32,
}

fn main() -> Result<()> {
    let args = Args::parse();
    ensure!(
        (3..=7).contains(&args.distance) && args.distance % 2 == 1,
        "Code distance must be 3, 5, or 7."
    );
    let source = fs::read_to_string(&args.input).context("read BLOG")?;
    ensure!(
        source.len() <= qsbit_studio::MAX_BLOG_BYTES,
        "BLOG exceeds upload limit."
    );
    let graph = BlockGraph::from_text(&source).context("parse BLOG")?;
    ensure!(graph.blocks().count() > 0, "The graph contains no blocks.");
    let ir = bloq::compile::compile(&graph, args.distance).context("compile block graph")?;
    let program = bloq::vm::lower(&ir, &bloq::vm::LoweringConfig::default())
        .context("lower physical program")?;
    ensure!(
        program.qubit_count <= 32,
        "The graph needs {} physical qubits; the controller limit is 32.",
        program.qubit_count
    );
    ensure!(
        !program
            .tasks
            .iter()
            .any(|task| matches!(task.instruction, Instruction::Decode(_))),
        "This Studio preset does not configure decoder bindings. Upload a closed graph without decoder-dependent control."
    );
    let mut observables = BTreeMap::new();
    for &id in &program.entry.tasks {
        let task = &program.tasks[id as usize];
        if let Instruction::Observable { index, .. } = task.instruction
            && let Some(bit) = task.output
        {
            observables.insert(index, bit);
        }
    }
    let options = QirOptions {
        output_records: (0..program.record_count).collect(),
        output_bits: observables.values().copied().collect(),
        ..Default::default()
    };
    let artifact =
        bloq::qir::emit_program_qir(&program, &options).context("export Adaptive QIR")?;
    let target = qsbit_studio::target::target_for_qir(&artifact.llvm_ir, artifact.qubit_count)?;
    fs::create_dir_all(&args.output)?;
    fs::write(args.output.join("program.ll"), &artifact.llvm_ir)?;
    fs::write(args.output.join("program.bc"), &artifact.bitcode)?;
    fs::write(args.output.join("program.bloqir"), ir.to_text())?;
    fs::write(
        args.output.join("program.vm.json"),
        program.to_json_pretty()?,
    )?;
    fs::write(
        args.output.join("target.json"),
        serde_json::to_vec_pretty(&target)?,
    )?;
    let output_labels: Vec<_> = (0..program.record_count)
        .map(|id| format!("record:{id}"))
        .chain(
            observables
                .keys()
                .map(|index| format!("raw_observable:{index}")),
        )
        .collect();
    let metadata = json!({"schema": 1, "bloq_revision": qsbit_studio::BLOQ_REVISION,
        "distance": args.distance, "qubits": artifact.qubit_count, "results": artifact.result_count,
        "measurement_records": program.record_count, "output_labels": output_labels,
        "decoder": null, "preset": "studio-ideal-v1"});
    fs::write(
        args.output.join("export.json"),
        serde_json::to_vec_pretty(&metadata)?,
    )?;
    // A static reference is optional; adaptive programs may have no Stim emission.
    if let Ok(reference) = bloq::stim::emit_bloq_stim(&ir) {
        fs::write(args.output.join("reference.stim"), reference)?;
    }
    println!(
        "Exported {} physical qubits and {} measurement records",
        artifact.qubit_count, program.record_count
    );
    Ok(())
}
