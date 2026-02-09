use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::SyncroError;

#[derive(Serialize, Deserialize, Default)]
pub struct Config {
    pub repos: Vec<PathBuf>,
}

fn config_path() -> Result<PathBuf, SyncroError> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| SyncroError::Config("could not determine config directory".into()))?;
    Ok(config_dir.join("syncro").join("config.yaml"))
}

pub fn load() -> Result<Config, SyncroError> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(Config::default());
    }
    let contents = std::fs::read_to_string(&path)?;
    serde_yaml::from_str(&contents)
        .map_err(|e| SyncroError::Config(format!("failed to parse config: {e}")))
}

pub fn save(config: &Config) -> Result<(), SyncroError> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let contents = serde_yaml::to_string(config)
        .map_err(|e| SyncroError::Config(format!("failed to serialize config: {e}")))?;
    std::fs::write(&path, contents)?;
    Ok(())
}

pub fn add_repo(path: &Path) -> Result<(), SyncroError> {
    let canonical = path
        .canonicalize()
        .map_err(|_| SyncroError::PathNotFound(path.to_path_buf()))?;

    if !canonical.join(".git").exists() {
        return Err(SyncroError::NotAGitRepo(canonical));
    }

    let mut config = load()?;
    if !config.repos.contains(&canonical) {
        config.repos.push(canonical);
        save(&config)?;
    }
    Ok(())
}

pub fn remove_repo(path: &Path) -> Result<(), SyncroError> {
    let canonical = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf());

    let mut config = load()?;
    config.repos.retain(|r| r != &canonical);
    save(&config)?;
    Ok(())
}
