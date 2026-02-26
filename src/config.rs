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

/// Check if a directory is a Git repository
fn is_git_repo(path: &Path) -> bool {
    path.join(".git").exists()
}

/// Find all Git repositories in immediate subdirectories
fn find_git_repos(parent: &Path) -> Result<Vec<PathBuf>, SyncroError> {
    let mut repos = Vec::new();

    if !parent.is_dir() {
        return Ok(repos);
    }

    let entries = std::fs::read_dir(parent)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() && is_git_repo(&path) {
            repos.push(path);
        }
    }

    repos.sort();
    Ok(repos)
}

pub fn add_repo(path: &Path) -> Result<(), SyncroError> {
    let canonical = path
        .canonicalize()
        .map_err(|_| SyncroError::PathNotFound(path.to_path_buf()))?;

    // Validate that the path is either a git repo or contains git repos
    if !is_git_repo(&canonical) {
        let found_repos = find_git_repos(&canonical)?;
        if found_repos.is_empty() {
            return Err(SyncroError::NotAGitRepo(canonical));
        }
    }

    let mut config = load()?;
    if !config.repos.contains(&canonical) {
        config.repos.push(canonical);
        save(&config)?;
    }
    Ok(())
}

/// Expand config paths into actual Git repositories.
/// Individual repos are returned as-is, parent directories are expanded to their git repo subdirectories.
pub fn expand_repos(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut result = Vec::new();

    for path in paths {
        if is_git_repo(path) {
            // It's a git repo itself, add it directly
            result.push(path.clone());
        } else if path.is_dir() {
            // It's a directory, expand to subdirectories that are git repos
            if let Ok(repos) = find_git_repos(path) {
                result.extend(repos);
            }
        }
    }

    result
}

/// Holds the inforation about a given watched folder and the contained Git repositories
/// in its subfolders.
pub struct WatchedFolder {
    path: PathBuf,
    sub_dirs: Option<Vec<String>>,
}

/// Returns a structured representation of the Git repositories
/// in the provided list of folders.
///
/// NOTE: This implementation currently works either with Git repositories or
/// folders that contain Git repositories on the first level (no deeper levels supported).
fn expand_repos_structured(paths: &[PathBuf]) -> Vec<WatchedFolder> {
    assert!(paths.len() > 0, "empty paths");

    let mut expanded = Vec::with_capacity(paths.len());

    for path in paths.iter() {
        let watched_folder = match is_git_repo(path) {
            true => WatchedFolder {
                path: path.to_owned(),
                sub_dirs: None,
            },
            false => {
                let git_repos_in_folder: Vec<String> = find_git_repos(path)
                    .expect("no git repositories found in folder")
                    .iter()
                    .map(|repo| {
                        repo.components()
                            .last()
                            .unwrap()
                            .as_os_str()
                            .to_string_lossy()
                            .to_string()
                    })
                    .collect();
                WatchedFolder {
                    path: path.to_owned(),
                    sub_dirs: Some(git_repos_in_folder),
                }
            }
        };

        expanded.push(watched_folder);
    }

    expanded
}

/// List the configured watched directories (and expanded subdirectories).
pub fn list() -> Result<(), SyncroError> {
    let config = load()?;

    let watched_folders = expand_repos_structured(&config.repos);

    if watched_folders.is_empty() {
        println!("No watched repositories.");
        return Ok(());
    }

    for watched_folder in watched_folders {
        match &watched_folder.sub_dirs {
            None => println!("{}", watched_folder.path.to_string_lossy()),
            Some(sd) => println!(
                "{} (contains {} repositories)",
                watched_folder.path.to_string_lossy(),
                sd.len()
            ),
        }
    }

    Ok(())
}

pub fn remove_repo(path: &Path) -> Result<(), SyncroError> {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    let mut config = load()?;
    config.repos.retain(|r| r != &canonical);
    save(&config)?;
    Ok(())
}
