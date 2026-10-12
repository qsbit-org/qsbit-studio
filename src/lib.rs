pub mod api;
pub mod artifact;
mod files;
pub mod jobs;
mod pipeline;
pub mod process;
pub mod runtime;
pub mod target;

use std::{net::SocketAddr, path::PathBuf};

use clap::Parser;

pub const BLOQ_REVISION: &str = "0a4962595f32f5e5c63ce9cce224082c87632684";
pub const PLAYER_ASSETS: &[(&str, &str)] = &[
    ("trace-model.js", "text/javascript; charset=utf-8"),
    ("trace-player.js", "text/javascript; charset=utf-8"),
    ("qsbit-theme.js", "text/javascript; charset=utf-8"),
    ("qsbit-fonts.css", "text/css; charset=utf-8"),
    ("qsbit-tokens.css", "text/css; charset=utf-8"),
    ("qsbit-base.css", "text/css; charset=utf-8"),
    ("trace-player.css", "text/css; charset=utf-8"),
    ("fonts/source-sans-3.woff2", "font/woff2"),
    ("fonts/source-sans-3-italic.woff2", "font/woff2"),
    ("fonts/source-code-pro.woff2", "font/woff2"),
];
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
    /// Allow unauthenticated access from non-loopback addresses.
    #[arg(long)]
    pub allow_remote: bool,
    #[arg(long, env = "QSBIT_VALIDATOR", default_value = "qsbit-validate")]
    pub validator: PathBuf,
    #[arg(long, env = "QSBIT_COMPILER", default_value = "qsbitc")]
    pub compiler: PathBuf,
    /// Target JSON files offered to clients; the first is the default.
    #[arg(long = "target")]
    pub targets: Vec<PathBuf>,
    #[arg(long, env = "QSBIT_SIMULATOR", default_value = "qsbit-sim")]
    pub simulator: PathBuf,
    /// Defaults to qsbit-blog-export beside the Studio executable.
    #[arg(long, env = "QSBIT_BLOG_EXPORTER")]
    pub exporter: Option<PathBuf>,
    /// Explicit development override for simulator player resources.
    #[arg(long)]
    pub player_root: Option<PathBuf>,
    /// Run artifacts are session-scoped and removed on clean shutdown.
    #[arg(long)]
    pub work_dir: Option<PathBuf>,
    #[arg(long, default_value = "32", value_parser = clap::value_parser!(u32).range(1..=128))]
    pub max_jobs: u32,
    #[arg(long, default_value = "2", value_parser = clap::value_parser!(u32).range(1..=8))]
    pub max_running: u32,
    #[arg(long, default_value = "120", value_parser = clap::value_parser!(u64).range(1..=600))]
    pub stage_timeout_seconds: u64,
}

impl Config {
    pub fn exporter_path(&self) -> anyhow::Result<PathBuf> {
        Ok(self
            .exporter
            .clone()
            .unwrap_or(std::env::current_exe()?.with_file_name("qsbit-blog-export")))
    }
}
