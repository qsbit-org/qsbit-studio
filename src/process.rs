//! Each stage owns a child process group; cancellation tears down descendants.

use std::{
    ffi::OsString, fs, os::unix::process::CommandExt, path::Path, process::Stdio, time::Duration,
};

use anyhow::{Context, Result, bail, ensure};
use nix::{
    sys::signal::{Signal, killpg},
    unistd::Pid,
};
use tokio::{
    process::Command,
    sync::watch,
    time::{Instant, interval, sleep_until},
};

use crate::{Config, MAX_JOB_BYTES, MAX_TRACE_BYTES};

struct ProcessGroup(Pid);

impl Drop for ProcessGroup {
    fn drop(&mut self) {
        let _ = killpg(self.0, Signal::SIGKILL);
    }
}

pub fn artifact_bytes(root: &Path) -> Result<u64> {
    let mut pending = vec![root.to_owned()];
    let mut count = 0_u64;
    let mut size = 0_u64;
    while let Some(directory) = pending.pop() {
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        for entry in entries {
            let entry = entry?;
            count += 1;
            ensure!(count <= 4096, "Too many generated files.");
            let metadata = match entry.path().symlink_metadata() {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            ensure!(
                !metadata.is_symlink(),
                "Unexpected symlink in generated artifacts."
            );
            if metadata.is_dir() {
                pending.push(entry.path());
            } else {
                size = size
                    .checked_add(metadata.len())
                    .context("Artifact size overflow.")?;
            }
        }
    }
    Ok(size)
}

pub async fn run_stage(
    config: &Config,
    directory: &Path,
    executable: &Path,
    args: &[OsString],
    stage: &str,
    mut cancelled: watch::Receiver<bool>,
) -> Result<()> {
    let log_path = directory.join(format!("{stage}.log"));
    let log = fs::File::create(&log_path)?;
    let mut command = Command::new(executable);
    command
        .args(args)
        .current_dir(directory)
        .env_remove("PYTHONHOME")
        .env_remove("PYTHONPATH")
        .env_remove("PYTHONUSERBASE")
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log))
        .kill_on_drop(true);
    command.as_std_mut().process_group(0);
    let mut child = command.spawn().with_context(|| {
        format!(
            "Cannot start {}. Configure its executable path.",
            executable.display()
        )
    })?;
    let pid = Pid::from_raw(i32::try_from(
        child.id().context("Child has no process ID.")?,
    )?);
    let group = ProcessGroup(pid);
    let deadline = Instant::now() + Duration::from_secs(config.stage_timeout_seconds);
    let mut monitor = interval(Duration::from_millis(100));
    let outcome = loop {
        tokio::select! {
            status = child.wait() => break status.context("wait for tool"),
            _ = cancelled.wait_for(|value| *value) => {
                break Err(anyhow::anyhow!("Task cancelled."));
            }
            _ = sleep_until(deadline) => {
                break Err(anyhow::anyhow!("{stage} exceeded its {} second host timeout.", config.stage_timeout_seconds));
            }
            _ = monitor.tick() => {
                let trace_size = fs::metadata(directory.join("trace.jsonl")).map(|metadata| metadata.len()).unwrap_or(0);
                if trace_size > MAX_TRACE_BYTES {
                    break Err(anyhow::anyhow!("Trace exceeds the replay size limit."));
                }
                match artifact_bytes(directory) {
                    Ok(size) if size <= MAX_JOB_BYTES => {},
                    Ok(_) => break Err(anyhow::anyhow!("Task artifacts exceed the size limit.")),
                    Err(error) => break Err(error),
                }
            }
        }
    };
    // Also remove descendants if a tool exited without waiting for them.
    drop(group);
    if outcome.is_err() {
        let _ = child.wait().await;
    }
    let status = outcome?;
    ensure!(
        fs::metadata(directory.join("trace.jsonl"))
            .map(|metadata| metadata.len())
            .unwrap_or(0)
            <= MAX_TRACE_BYTES,
        "Trace exceeds the replay size limit."
    );
    ensure!(
        artifact_bytes(directory)? <= MAX_JOB_BYTES,
        "Task artifacts exceed the size limit."
    );
    if !status.success() {
        let bytes = fs::read(log_path)?;
        let tail = String::from_utf8_lossy(&bytes[bytes.len().saturating_sub(4096)..]);
        bail!("{stage} failed ({status}).\n{tail}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[tokio::test]
    async fn failure_preserves_diagnostics_and_cancellation_stops_the_process() {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config::parse_from(["test"]);
        config.stage_timeout_seconds = 1;
        let (_sender, receiver) = watch::channel(false);
        let error = run_stage(
            &config,
            directory.path(),
            Path::new("/bin/sh"),
            &["-c".into(), "echo expected-failure >&2; exit 7".into()],
            "test",
            receiver,
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("expected-failure"));
        let (sender, receiver) = watch::channel(false);
        sender.send(true).unwrap();
        let error = run_stage(
            &config,
            directory.path(),
            Path::new("/bin/sleep"),
            &["30".into()],
            "cancel",
            receiver,
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("cancelled"));
        let (_sender, receiver) = watch::channel(false);
        let error = run_stage(
            &config,
            directory.path(),
            Path::new("/bin/sleep"),
            &["30".into()],
            "timeout",
            receiver,
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("timeout"));
    }

    #[test]
    fn rejects_symlinks_and_measures_artifact_bytes() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("test"), b"hello").unwrap();
        assert_eq!(artifact_bytes(directory.path()).unwrap(), 5);
        std::os::unix::fs::symlink("/etc/passwd", directory.path().join("escape")).unwrap();
        assert!(artifact_bytes(directory.path()).is_err());
    }

    #[tokio::test]
    async fn rejects_oversized_trace_even_when_the_tool_exits_immediately() {
        let directory = tempfile::tempdir().unwrap();
        let config = Config::parse_from(["test"]);
        let (_sender, receiver) = watch::channel(false);
        let script = format!("truncate -s {} trace.jsonl", MAX_TRACE_BYTES + 1);
        let error = run_stage(
            &config,
            directory.path(),
            Path::new("/bin/sh"),
            &["-c".into(), script.into()],
            "trace-limit",
            receiver,
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("Trace exceeds"));
    }
}
