mod config;
mod error;
mod git;
mod tui;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "syncro", about = "Sync uncommitted work across Git repos")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Add a folder to the watch list
    Add { path: PathBuf },
    /// Remove a folder from the watch list
    Remove { path: PathBuf },
    /// Print watched folders and their status
    List,
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        None => tui::run().map_err(|e| e.to_string()),
        Some(Commands::Add { path }) => config::add_repo(&path).map_err(|e| e.to_string()),
        Some(Commands::Remove { path }) => config::remove_repo(&path).map_err(|e| e.to_string()),
        Some(Commands::List) => cmd_list(),
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn cmd_list() -> Result<(), String> {
    let cfg = config::load().map_err(|e| e.to_string())?;
    if cfg.repos.is_empty() {
        println!("No repos watched. Use `syncro add <path>` to add repositories.");
        return Ok(());
    }

    for repo_path in &cfg.repos {
        let status = git::repo_status(repo_path);
        let name = status.display_name();
        let branch = &status.branch;
        let summary = status.summary();

        let indicator = if status.error.is_some() {
            "!"
        } else if status.is_clean() {
            "✓"
        } else {
            "●"
        };

        let remote_note = if status.has_remote && !status.branch_on_remote {
            " (branch not on remote)"
        } else {
            ""
        };

        println!(" {indicator} {name:<20} {branch:<12} {summary}{remote_note}");
    }
    Ok(())
}
