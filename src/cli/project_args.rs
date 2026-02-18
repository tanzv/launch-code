use clap::{ArgGroup, Args, Subcommand, ValueEnum};

#[derive(Debug, Clone, Args)]
pub struct ProjectArgs {
    #[command(subcommand)]
    pub command: ProjectCommands,
}

#[derive(Debug, Clone, Subcommand)]
pub enum ProjectCommands {
    #[command(about = "Show workspace project metadata.")]
    Show,
    #[command(about = "List project metadata fields in a compact row format.")]
    List(ProjectListArgs),
    #[command(about = "Set or update workspace project metadata.")]
    Set(ProjectSetArgs),
    #[command(about = "Unset selected project metadata fields.")]
    Unset(ProjectUnsetArgs),
    #[command(about = "Clear all workspace project metadata.")]
    Clear(ProjectClearArgs),
}

#[derive(Debug, Clone, Args)]
pub struct ProjectListArgs {
    #[arg(
        long = "field",
        value_enum,
        help = "Project metadata field to include. Repeat for multiple fields."
    )]
    pub fields: Vec<ProjectListFieldArg>,
    #[arg(
        long,
        default_value_t = false,
        help = "Include fields without values (shown as null)."
    )]
    pub all: bool,
    #[arg(
        long,
        default_value_t = false,
        help = "Aggregate metadata rows across all linked workspaces."
    )]
    pub all_links: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ValueEnum)]
pub enum ProjectListFieldArg {
    Name,
    Description,
    Repository,
    Languages,
    Runtimes,
    Tools,
    Tags,
}

#[derive(Debug, Clone, Args)]
#[command(group(
    ArgGroup::new("project_set_fields")
        .required(true)
        .multiple(true)
        .args(["name", "description", "repository", "language", "runtime", "tool", "tag"])
))]
pub struct ProjectSetArgs {
    #[arg(long, help = "Project display name.")]
    pub name: Option<String>,
    #[arg(long, help = "Project summary description.")]
    pub description: Option<String>,
    #[arg(long, help = "Repository URL or canonical identifier.")]
    pub repository: Option<String>,
    #[arg(
        long = "language",
        help = "Primary language tag. Repeat for multiple values."
    )]
    pub language: Vec<String>,
    #[arg(
        long = "runtime",
        help = "Runtime identifier. Repeat for multiple values."
    )]
    pub runtime: Vec<String>,
    #[arg(
        long = "tool",
        help = "Development tool tag. Repeat for multiple values."
    )]
    pub tool: Vec<String>,
    #[arg(
        long = "tag",
        help = "Arbitrary project tag. Repeat for multiple values."
    )]
    pub tag: Vec<String>,
    #[arg(
        long,
        default_value_t = false,
        help = "Apply updates to all linked workspaces."
    )]
    pub all_links: bool,
}

#[derive(Debug, Clone, Args)]
pub struct ProjectUnsetArgs {
    #[arg(
        long = "field",
        value_enum,
        required = true,
        help = "Project metadata field to clear. Repeat for multiple fields."
    )]
    pub fields: Vec<ProjectUnsetFieldArg>,
    #[arg(
        long,
        default_value_t = false,
        help = "Apply field clear operation to all linked workspaces."
    )]
    pub all_links: bool,
}

#[derive(Debug, Clone, Args)]
pub struct ProjectClearArgs {
    #[arg(
        long,
        default_value_t = false,
        help = "Clear project metadata across all linked workspaces."
    )]
    pub all_links: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ProjectUnsetFieldArg {
    Name,
    Description,
    Repository,
    Languages,
    Runtimes,
    Tools,
    Tags,
    All,
}
