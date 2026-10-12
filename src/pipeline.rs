use crate::{
    Config,
    artifact::inspect_run,
    jobs::{Job, RunRequest, Stage},
    process::run_stage,
};
use anyhow::{Result, ensure};
use serde_json::Value;
use std::{sync::Arc, time::Instant};
pub(crate) async fn run(
    config: &Config,
    running: &Arc<tokio::sync::Semaphore>,
    job: &Arc<Job>,
    request: &RunRequest,
) -> Result<()> {
    let mut cancelled = job.cancel.subscribe();
    let _permit = tokio::select! {
        permit = running.clone().acquire_owned() => permit?,
        _ = cancelled.wait_for(|value| *value) => anyhow::bail!("Task cancelled while queued."),
    };
    ensure!(!*job.cancel.borrow(), "Task cancelled.");
    let exporter = config.exporter_path()?;
    stage(
        config,
        job,
        Stage::Exporting,
        "export",
        &exporter,
        &[
            "input.blog".into(),
            "--output".into(),
            job.directory.clone().into_os_string(),
            "--distance".into(),
            request.distance.to_string().into(),
            "--target".into(),
            "target.json".into(),
        ],
    )
    .await?;
    stage(
        config,
        job,
        Stage::Compiling,
        "compile",
        &config.compiler,
        &[
            "program.bc".into(),
            "--target".into(),
            "target.json".into(),
            "-o".into(),
            "program.elf".into(),
        ],
    )
    .await?;
    stage(
        config,
        job,
        Stage::Compiling,
        "validate",
        &config.validator,
        &["program.elf".into()],
    )
    .await?;
    let mut run: Value = serde_json::from_slice(&crate::files::read(
        &job.directory.join("program.run.json"),
    )?)?;
    run["backend"] = request.backend.name().into();
    run["profile"]["seed"] = request.seed.into();
    run["trace"] = "trace.jsonl".into();
    run["summary"] = "summary.json".into();
    run["memory_dump"] = "memory.bin".into();
    run["trace_stalls"] = false.into();
    crate::files::write(
        &job.directory.join("run.json"),
        &serde_json::to_vec_pretty(&run)?,
    )?;
    let simulation = stage(
        config,
        job,
        Stage::Simulating,
        "simulate",
        &config.simulator,
        &["--config".into(), "run.json".into()],
    )
    .await;
    ensure!(!*job.cancel.borrow(), "Task cancelled.");
    // A model fault can still yield a valid partial replay and failed summary.
    if crate::files::is_file(&job.directory.join("trace.jsonl"))
        && crate::files::is_file(&job.directory.join("summary.json"))
    {
        job.report.lock().await.stage = Stage::Analysing;
        let directory = job.directory.clone();
        let filename = request.filename.clone();
        let metrics =
            tokio::task::spawn_blocking(move || inspect_run(&directory, &filename)).await??;
        let accepted = metrics.simulator_success && metrics.program_exit_code == Some(0);
        {
            let mut report = job.report.lock().await;
            report.replay_ready = true;
            report.metrics = Some(metrics);
        }
        if let Err(error) = simulation {
            job.report.lock().await.stage = Stage::Simulating;
            return Err(error);
        }
        if !accepted {
            job.report.lock().await.stage = Stage::Simulating;
            anyhow::bail!("The simulator drained, but the program returned a nonzero exit status.");
        }
    } else {
        simulation?;
        anyhow::bail!("Simulator did not produce trace and summary artifacts.");
    }
    ensure!(!*job.cancel.borrow(), "Task cancelled.");
    Ok(())
}

async fn stage(
    config: &Config,
    job: &Job,
    stage: Stage,
    name: &str,
    executable: &std::path::Path,
    args: &[std::ffi::OsString],
) -> Result<()> {
    ensure!(!*job.cancel.borrow(), "Task cancelled.");
    job.report.lock().await.stage = stage;
    let start = Instant::now();
    let result = run_stage(
        config,
        &job.directory,
        executable,
        args,
        name,
        job.cancel.subscribe(),
    )
    .await;
    job.report.lock().await.stage_host_ms.insert(
        name.to_owned(),
        start.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
    );
    result
}
