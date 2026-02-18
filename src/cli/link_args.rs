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
