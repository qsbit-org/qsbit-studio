//! Open job artifacts without following symbolic links.
use anyhow::{Result, ensure};
use nix::{
    fcntl::{OFlag, open, openat},
    sys::stat::Mode,
};
use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};

fn open_file(path: &Path, flags: OFlag) -> Result<File> {
    let name = path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("Missing artifact name"))?;
    let directory = open(
        path.parent()
            .ok_or_else(|| anyhow::anyhow!("Missing artifact directory"))?,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )?;
    let file = File::from(openat(
        &directory,
        Path::new(name),
        flags | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
        Mode::S_IRUSR | Mode::S_IWUSR,
    )?);
    ensure!(file.metadata()?.is_file(), "Artifact is not a regular file");
    Ok(file)
}

pub fn read(path: &Path) -> Result<Vec<u8>> {
    let file = open_file(path, OFlag::O_RDONLY)?;
    ensure!(
        file.metadata()?.len() <= crate::MAX_JOB_BYTES,
        "Artifact exceeds size limit"
    );
    let mut bytes = Vec::new();
    file.take(crate::MAX_JOB_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= crate::MAX_JOB_BYTES,
        "Artifact exceeds size limit"
    );
    Ok(bytes)
}

pub fn is_file(path: &Path) -> bool {
    open_file(path, OFlag::O_RDONLY).is_ok()
}

pub fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = open_file(path, OFlag::O_WRONLY | OFlag::O_CREAT)?;
    file.set_len(0)?;
    file.write_all(bytes)?;
    Ok(())
}
