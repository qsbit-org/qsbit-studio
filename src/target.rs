//! Load immutable target snapshots. Compiler owns target semantics and validation.
use anyhow::{Context, Result, ensure};
use std::{collections::BTreeMap, path::PathBuf};

pub fn load_targets(paths: &[PathBuf]) -> Result<(String, BTreeMap<String, Vec<u8>>)> {
    let mut targets = BTreeMap::new();
    let mut default = None;
    for path in paths {
        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .context("Target needs a UTF-8 filename")?;
        ensure!(!id.is_empty(), "Target needs a nonempty filename");
        let bytes =
            std::fs::read(path).with_context(|| format!("Read target {}", path.display()))?;
        let value: serde_json::Value = serde_json::from_slice(&bytes)
            .with_context(|| format!("Parse target {}", path.display()))?;
        ensure!(value.is_object(), "Target must be a JSON object");
        ensure!(
            targets.insert(id.to_owned(), bytes).is_none(),
            "Duplicate target ID: {id}"
        );
        default.get_or_insert_with(|| id.to_owned());
    }
    Ok((default.context("At least one target is required")?, targets))
}
