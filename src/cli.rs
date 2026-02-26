//! This module contains the CLI setup, which is based on
//! the `clap` crate.

use clap::{Parser, Subcommand};

use crate::{config, error, tui};

use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "syncro",
    about = "Sync uncommitted work across Git repositories"
)]
pub struct CLI {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Clone, Subcommand)]
pub enum Commands {
    #[command(
        about = "Adds the provided directory to the configuration. Defaults to the current working directory."
    )]
    Add { path_arg: Option<PathBuf> },

    #[command(
        about = "Removes a directory from the configuration. Defaults to the current working diredtory."
    )]
    Remove { path_arg: Option<PathBuf> },

    #[command(about = "List the watched Git repositories.")]
    List,
}

/// Runs the CLI logic and delegates execution to the corresponding functions.
pub fn run() -> Result<(), error::SyncroError> {
    let cli = CLI::parse();
    match cli.command {
        None => tui::run(),
        Some(Commands::Add { path_arg: path }) => {
            let target = path.unwrap_or(PathBuf::from("."));
            config::add_repo(&target)
        }
        Some(Commands::Remove { path_arg }) => {
            let target = path_arg.unwrap_or(PathBuf::from("."));
            config::remove_repo(&target)
        }
        Some(Commands::List) => config::list(),
    }
}
