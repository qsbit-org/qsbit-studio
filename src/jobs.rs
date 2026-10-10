use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Instant};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::{
    fs,
    sync::{Mutex, RwLock, Semaphore, watch},
};
use uuid::Uuid;

use crate::{
    Config, MAX_BLOG_BYTES,
    artifact::{Metrics, inspect_run},
    process::run_stage,
};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    #[default]
    Stim,
    Mock,
}

impl Backend {
    pub fn name(self) -> &'static str {
        match self {
            Self::Stim => "stim",
            Self::Mock => "mock",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunRequest {
    pub filename: String,
    pub source: String,
    #[serde(default = "default_distance")]
    pub distance: u32,
    #[serde(default = "default_seed")]
    pub seed: u32,
    #[serde(default)]
    pub backend: Backend,
}

fn default_distance() -> u32 {
    3
}
fn default_seed() -> u32 {
    1
}

impl RunRequest {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.filename.is_empty()
                && self.filename.len() <= 128
                && self.filename.ends_with(".blog"),
            "Upload a .blog file exported by Bloq."
        );
        ensure!(
            !self.filename.contains(['/', '\\']) && !self.filename.chars().any(char::is_control),
            "Filename must be a basename."
        );
        ensure!(
            !self.source.trim().is_empty() && self.source.len() <= MAX_BLOG_BYTES,
            "BLOG must contain 1–262144 UTF-8 bytes."
        );
        ensure!(
            self.source.trim_start().starts_with("BLOG "),
            "Expected a BLOG document header."
        );
        ensure!(
            matches!(self.distance, 3 | 5 | 7),
            "Code distance must be 3, 5, or 7."
        );
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Queued,
    Exporting,
    Compiling,
    Simulating,
    Analysing,
    Completed,
    Failed,
    Cancelled,
}

impl Stage {
    pub fn terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub id: Uuid,
    pub submitted_at_ms: u64,
    pub filename: String,
    pub stage: Stage,
    pub backend: Backend,
    pub distance: u32,
    pub seed: u32,
    pub source_sha256: String,
    pub error: Option<String>,
    pub failed_stage: Option<Stage>,
    pub replay_ready: bool,
    /// Host time, measured independently from simulator ticks.
    pub host_duration_ms: Option<u64>,
    pub stage_host_ms: BTreeMap<String, u64>,
    pub metrics: Option<Metrics>,
    pub artifacts: Vec<String>,
}

pub struct Job {
    pub report: Mutex<Report>,
    pub directory: PathBuf,
    pub cancel: watch::Sender<bool>,
}

pub struct Studio {
    pub config: Config,
    pub directory: PathBuf,
    pub jobs: RwLock<BTreeMap<Uuid, Arc<Job>>>,
    pub running: Arc<Semaphore>,
}

pub const ARTIFACTS: &[&str] = &[
    "input.blog",
    "request.json",
    "program.ll",
    "program.bc",
    "program.bloqir",
    "program.vm.json",
    "export.json",
    "reference.stim",
    "target.json",
    "program.elf",
    "program.manifest.json",
    "program.schedule.json",
    "program.lowered.ll",
    "program.run.json",
    "run.json",
    "trace.jsonl",
    "summary.json",
    "memory.bin",
    "metrics.json",
    "replay.json",
    "report.json",
    "export.log",
    "compile.log",
    "simulate.log",
];

impl Studio {
    pub async fn new(mut config: Config) -> Result<Arc<Self>> {
        for (filename, _) in crate::PLAYER_ASSETS {
            ensure!(
                config.player_root.join(filename).is_file(),
                "Missing player asset {filename}. Run git submodule update --init, or set --player-root."
            );
        }
        config.player_root = config.player_root.canonicalize()?;
        // A venv interpreter is normally a symlink. Preserve its venv path:
        // resolving it to the base executable changes Python's environment.
        if let Some(path) = &config.python {
            ensure!(path.is_file(), "Python interpreter does not exist.");
            config.python = Some(std::path::absolute(path)?);
        }
        for executable in [&mut config.compiler, &mut config.simulator] {
            if executable.components().count() > 1 {
                *executable = std::path::absolute(&*executable)?;
            }
        }
        if let Some(path) = &config.exporter
            && path.components().count() > 1
        {
            config.exporter = Some(std::path::absolute(path)?);
        }
        if let Some(path) = &config.python_path {
            config.python_path = Some(
                path.canonicalize()
                    .context("Python adapter directory does not exist.")?,
            );
        }
        fs::create_dir_all(&config.work_dir).await?;
        let directory = config
            .work_dir
            .canonicalize()?
            .join(format!("session-{}", Uuid::new_v4()));
        fs::create_dir(&directory).await?;
        Ok(Arc::new(Self {
            running: Arc::new(Semaphore::new(config.max_running as usize)),
            config,
            directory,
            jobs: RwLock::new(BTreeMap::new()),
        }))
    }

    pub async fn submit(self: &Arc<Self>, request: RunRequest) -> Result<Arc<Job>> {
        use sha2::{Digest, Sha256};
        request.validate()?;
        let mut jobs = self.jobs.write().await;
        ensure!(
            jobs.len() < self.config.max_jobs as usize,
            "Job capacity is full. Remove a finished run before submitting another."
        );
        let id = Uuid::new_v4();
        let directory = self.directory.join(id.to_string());
        fs::create_dir(&directory).await?;
        fs::write(directory.join("input.blog"), &request.source).await?;
        fs::write(directory.join("request.json"), serde_json::to_vec_pretty(&serde_json::json!({"filename":request.filename,"distance":request.distance,"seed":request.seed,"backend":request.backend,"preset":"studio-ideal-v1","bloq_revision":crate::BLOQ_REVISION,"player_revision":crate::PLAYER_REVISION}))?).await?;
        let (cancel, _) = watch::channel(false);
        let job = Arc::new(Job {
            directory,
            cancel,
            report: Mutex::new(Report {
                id,
                submitted_at_ms: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_millis()
                    .try_into()?,
                filename: request.filename.clone(),
                stage: Stage::Queued,
                backend: request.backend,
                distance: request.distance,
                seed: request.seed,
                source_sha256: format!("{:x}", Sha256::digest(request.source.as_bytes())),
                error: None,
                failed_stage: None,
                replay_ready: false,
                host_duration_ms: None,
                stage_host_ms: BTreeMap::new(),
                metrics: None,
                artifacts: Vec::new(),
            }),
        });
        jobs.insert(id, job.clone());
        let state = self.clone();
        let task = job.clone();
        tokio::spawn(async move {
            state.execute(task, request).await;
        });
        Ok(job)
    }

    pub async fn get(&self, id: Uuid) -> Option<Arc<Job>> {
        self.jobs.read().await.get(&id).cloned()
    }

    async fn execute(&self, job: Arc<Job>, request: RunRequest) {
        let started = Instant::now();
        let result = self.pipeline(&job, &request).await;
        let mut report = job.report.lock().await;
        report.host_duration_ms =
            Some(started.elapsed().as_millis().try_into().unwrap_or(u64::MAX));
        if let Err(error) = result {
            report.failed_stage = Some(report.stage);
            report.stage = if *job.cancel.borrow() {
                Stage::Cancelled
            } else {
                Stage::Failed
            };
            report.error = Some(format!("{error:#}"));
        } else {
            report.stage = Stage::Completed;
        }
        report.artifacts = ARTIFACTS
            .iter()
            .filter(|name| job.directory.join(name).is_file())
            .map(|name| (*name).to_owned())
            .collect();
        if !report.artifacts.iter().any(|name| name == "report.json") {
            report.artifacts.push("report.json".to_owned());
        }
        if let Ok(bytes) = serde_json::to_vec_pretty(&*report)
            && let Err(error) = fs::write(job.directory.join("report.json"), bytes).await
        {
            eprintln!("Could not save run report: {error}");
        }
    }

    async fn pipeline(&self, job: &Arc<Job>, request: &RunRequest) -> Result<()> {
        let mut cancelled = job.cancel.subscribe();
        let _permit = tokio::select! {
            permit = self.running.clone().acquire_owned() => permit?,
            _ = cancelled.wait_for(|value| *value) => anyhow::bail!("Task cancelled while queued."),
        };
        ensure!(!*job.cancel.borrow(), "Task cancelled.");
        let exporter = self.config.exporter_path()?;
        self.stage(
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
            ],
        )
        .await?;
        self.stage(
            job,
            Stage::Compiling,
            "compile",
            &self.config.compiler,
            &[
                "program.bc".into(),
                "--target".into(),
                "target.json".into(),
                "-o".into(),
                "program.elf".into(),
            ],
        )
        .await?;
        let mut run: Value =
            serde_json::from_slice(&fs::read(job.directory.join("program.run.json")).await?)?;
        run["backend"] = request.backend.name().into();
        run["profile"]["seed"] = request.seed.into();
        run["trace"] = "trace.jsonl".into();
        run["summary"] = "summary.json".into();
        run["memory_dump"] = "memory.bin".into();
        run["trace_stalls"] = false.into();
        if let Some(path) = &self.config.python_path {
            run["python_path"] = path.to_string_lossy().as_ref().into();
        }
        fs::write(
            job.directory.join("run.json"),
            serde_json::to_vec_pretty(&run)?,
        )
        .await?;
        let simulation = self
            .stage(
                job,
                Stage::Simulating,
                "simulate",
                &self.config.simulator,
                &["--config".into(), "run.json".into()],
            )
            .await;
        ensure!(!*job.cancel.borrow(), "Task cancelled.");
        // A model fault can still yield a valid partial replay and failed summary.
        if job.directory.join("trace.jsonl").is_file()
            && job.directory.join("summary.json").is_file()
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
                anyhow::bail!(
                    "The simulator drained, but the program returned a nonzero exit status."
                );
            }
        } else {
            simulation?;
            anyhow::bail!("Simulator did not produce trace and summary artifacts.");
        }
        ensure!(!*job.cancel.borrow(), "Task cancelled.");
        Ok(())
    }

    async fn stage(
        &self,
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
            &self.config,
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

    pub async fn shutdown(&self) {
        for job in self.jobs.read().await.values() {
            job.cancel.send_replace(true);
        }
        let finished = async {
            loop {
                let jobs: Vec<_> = self.jobs.read().await.values().cloned().collect();
                let mut all_done = true;
                for job in jobs {
                    if !job.report.lock().await.stage.terminal() {
                        all_done = false;
                        break;
                    }
                }
                if all_done {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        };
        if tokio::time::timeout(std::time::Duration::from_secs(10), finished)
            .await
            .is_ok()
            && let Err(error) = fs::remove_dir_all(&self.directory).await
        {
            eprintln!("Could not remove session artifacts: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upload_validation_preserves_the_file_boundary() {
        let request = |filename: &str, source: &str, distance| RunRequest {
            filename: filename.into(),
            source: source.into(),
            distance,
            seed: 1,
            backend: Backend::Mock,
        };
        assert!(
            request("memory.blog", "BLOG 1.0\n0: ZXX [0,0,0]", 3)
                .validate()
                .is_ok()
        );
        for r in [
            request("../memory.blog", "BLOG 1.0", 3),
            request("memory.stim", "BLOG 1.0", 3),
            request("memory.blog", "", 3),
            request("memory.blog", "garbage", 3),
            request("memory.blog", "BLOG 1.0", 4),
            request("memory.blog", &"x".repeat(MAX_BLOG_BYTES + 1), 3),
        ] {
            assert!(r.validate().is_err());
        }
    }
}
