pub mod api;
pub mod artifact;
pub mod jobs;
pub mod process;
pub mod target;

use std::{net::SocketAddr, path::PathBuf};

use clap::Parser;

pub const BLOQ_REVISION: &str = "0a4962595f32f5e5c63ce9cce224082c87632684";
pub const PLAYER_REVISION: &str = "c98577ef8ed887452bae0b9cc4b6aeee09608553";
pub const MAX_BLOG_BYTES: usize = 256 * 1024;
pub const MAX_TRACE_BYTES: u64 = 12 * 1024 * 1024;
pub const MAX_JOB_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Parser)]
#[command(
    version,
    about = "Upload BLOG graphs, compile and run, and replay execution."
)]
pub struct Config {
    #[arg(long, default_value = "127.0.0.1:3000")]
    pub listen: SocketAddr,
    #[arg(long, env = "QSBIT_COMPILER", default_value = "qsbitc")]
    pub compiler: PathBuf,
    #[arg(long, env = "QSBIT_SIMULATOR", default_value = "qsbit-sim")]
    pub simulator: PathBuf,
    /// Defaults to qsbit-blog-export beside the Studio executable.
    #[arg(long, env = "QSBIT_BLOG_EXPORTER")]
    pub exporter: Option<PathBuf>,
    #[arg(long, default_value = "external/qsbit-sim/docs/_static")]
    pub player_root: PathBuf,
    /// Run artifacts are session-scoped and removed on clean shutdown.
    #[arg(long, default_value = ".local/runs")]
    pub work_dir: PathBuf,
    #[arg(long, default_value = "32", value_parser = clap::value_parser!(u32).range(1..=128))]
    pub max_jobs: u32,
    #[arg(long, default_value = "2", value_parser = clap::value_parser!(u32).range(1..=8))]
    pub max_running: u32,
    #[arg(long, default_value = "120", value_parser = clap::value_parser!(u64).range(1..=600))]
    pub stage_timeout_seconds: u64,
    /// Optional interpreter for the simulator's Python bridge.
    #[arg(long, env = "QSBIT_PYTHON")]
    pub python: Option<PathBuf>,
    /// Optional directory containing qsbit_backend.
    #[arg(long)]
    pub python_path: Option<PathBuf>,
}

impl Config {
    pub fn exporter_path(&self) -> anyhow::Result<PathBuf> {
        Ok(self
            .exporter
            .clone()
            .unwrap_or(std::env::current_exe()?.with_file_name("qsbit-blog-export")))
    }
}
