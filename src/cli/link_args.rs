use std::path::PathBuf;

use clap::{Args, Subcommand};

#[derive(Debug, Clone, Args)]
pub struct LinkArgs {
    #[command(subcommand)]
    pub command: LinkCommands,
}

#[derive(Debug, Clone, Subcommand)]
pub enum LinkCommands {
    #[command(about = "List all registered workspace links.")]
    List,
    #[command(about = "Show one registered workspace link.")]
    Show(LinkNameArgs),
    #[command(about = "Add or update a workspace link.")]
    Add(LinkAddArgs),
    #[command(about = "Remove a workspace link.")]
    Remove(LinkNameArgs),
    #[command(about = "Prune stale links (missing paths and temporary empty workspaces).")]
    Prune(LinkPruneArgs),
}

#[derive(Debug, Clone, Args)]
pub struct LinkNameArgs {
    #[arg(long, help = "Link name.")]
    pub name: String,
}

#[derive(Debug, Clone, Args)]
pub struct LinkAddArgs {
    #[arg(long, help = "Link name.")]
    pub name: String,
    #[arg(
        long,
        help = "Workspace path. Defaults to current directory when omitted."
    )]
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, Args)]
pub struct LinkPruneArgs {
    #[arg(
        long,
        default_value_t = false,
        help = "Preview matching links without removing them."
    )]
    pub dry_run: bool,
    #[arg(
        long,
        default_value_t = false,
        conflicts_with = "temp_only",
        help = "Match only links whose path no longer exists."
    )]
    pub missing_only: bool,
    #[arg(
        long,
        default_value_t = false,
        conflicts_with = "missing_only",
        help = "Match only temporary-path links with empty state."
    )]
    pub temp_only: bool,
}
