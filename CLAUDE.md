# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Syncro is a Rust CLI tool that monitors Git repositories for uncommitted changes and unpushed commits. It provides both a terminal UI (TUI) for interactive management and command-line subcommands for scripting.

## Development Commands

### Build and Run
```bash
cargo build                    # Build in debug mode
cargo build --release          # Build optimized binary
cargo run                      # Run the TUI
cargo run -- add <path>        # Add a repo to watch list
cargo run -- remove <path>     # Remove a repo
cargo run -- list              # List all watched repos
```

### Testing
```bash
cargo test                     # Run all tests
cargo test <test_name>         # Run specific test
cargo check                    # Fast check without codegen
cargo clippy                   # Run linter
```

## Architecture

### Module Structure

- **config**: Manages the YAML configuration file stored in `~/.config/syncro/config.yaml`, containing the list of watched paths (can be individual Git repos or parent directories containing multiple Git repos). Provides:
  - `add_repo()`: Adds a path (validates it's either a Git repo or contains Git repos)
  - `expand_repos()`: Expands config paths into actual Git repos (individual repos pass through, parent directories expand to their Git subdirectories)
  - `remove_repo()`, `load()`, and `save()`

- **git**: Git operations module with two main responsibilities:
  - `repo_status()`: Queries a repository's status (branch, modified/untracked/deleted files, unpushed commits, remote tracking). Runs git commands via `run_git()` helper.
  - `sync_repo()`: Stages all changes, commits with "syncro update YYYY-MM-DD" message, and pushes to remote (with automatic `--set-upstream` fallback for new branches).

- **tui**: Terminal UI using ratatui and crossterm. The `App` struct tracks:
  - Current repo statuses
  - Selection state (checkboxes for which repos to sync)
  - Cursor position
  - App state machine: `Browsing` → `Syncing` → `Results` → back to `Browsing`
  - Parallel status queries using threads for performance

  Custom widgets in `tui/widgets.rs`: `RepoListWidget` (shows repos with checkboxes, branch, status) and `HelpBar`.

- **error**: Centralized error handling using `thiserror` with `SyncroError` variants for config, git, IO, and path errors.

### Key Design Patterns

- **Parallel Git Operations**: Repo status queries run in parallel using `thread::spawn()` for better performance when monitoring multiple repos.

- **State Machine TUI**: The app cycles through states (Browsing/Syncing/Results) with different key bindings and UI per state.

- **Git Command Wrapper**: All git operations go through `run_git()` helper that adds `-C <repo_path>` to work in any directory.

### Configuration

Config file is stored at `~/.config/syncro/config.yaml` (or platform equivalent via `dirs::config_dir()`). Supports both individual Git repositories and parent directories containing Git repos:

```yaml
repos:
  - /path/to/individual/repo
  - /path/to/parent/directory  # Will monitor all Git repos in subdirectories
```

When a parent directory is added, the tool dynamically discovers Git repositories in its immediate subdirectories each time it runs, making it future-proof for newly added repos.

### TUI Key Bindings

**Browsing mode:**
- `j`/`k` or arrow keys: navigate
- `g`/`G`: jump to top/bottom
- `space`: toggle repo selection
- `enter`: sync selected repos
- `q` or `esc` or `ctrl+c`: quit

**Results mode:**
- `enter`: refresh and return to browsing
- `q` or `esc` or `ctrl+c`: quit
