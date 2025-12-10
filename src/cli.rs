use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[clap(about, version)]
#[command(propagate_version = true)]
pub enum ShadowenvApp {
    Diff(DiffCmd),
    Exec(ExecCmd),
    Hook(HookCmd),
    #[command(subcommand)]
    Init(InitCmd),
    Trust(TrustCmd),
    PromptWidget(PromptWidgetCmd),
}

/// Execute a command after loading the environment from the current directory.
#[derive(clap::Args, Debug)]
pub struct ExecCmd {
    /// Instead of searching from the current directory for a .shadowenv.d, search from this one.
    #[arg(long)]
    pub dir: Option<String>,

    /// The command to execute if there are no arguments.
    pub cmd_argv0: Option<String>,

    /// The command and arguments if it has any.
    #[arg(last = true)]
    pub cmd_argv: Vec<String>,
}

/// Display a diff of changed environment variables.
#[derive(Args, Debug)]
pub struct DiffCmd {
    /// Do not use color to highlight the diff.
    #[arg(long, short)]
    pub no_color: bool,

    /// Show all environment variables, not just those that change.
    #[arg(long, short)]
    pub verbose: bool,
}

/// Runs the shell hook. You shouldn't need to run this manually.
#[derive(clap::Args, Debug)]
pub struct HookCmd {
    /// Force the shadowenv to be applied, even if the working directory has not changed.
    #[arg(long, short)]
    pub force: bool,

    /// Suppress error printing.
    #[arg(long, short)]
    pub silent: bool,

    /// Rather than looking up the PPID, use this as the shell's pid.
    #[arg(long)]
    pub shellpid: Option<u32>,

    /// Clobber overridden environment variables when unshadowing.
    #[arg(long)]
    pub clobber: bool,

    #[command(flatten)]
    pub format: FormatOptions,
}

#[derive(Args, Debug)]
#[group(required = false, multiple = false)]
pub struct FormatOptions {
    /// Format variable assignments for posix shells (default).
    #[arg(long)]
    pub posix: bool,

    /// Format variable assignments for machine parsing.
    #[arg(long)]
    pub porcelain: bool,

    /// Format variable assignments for fish shell.
    #[arg(long)]
    pub fish: bool,

    /// Format variable assignments for nushell.
    #[arg(long)]
    pub nushell: bool,

    /// Format variable assignments as JSON.
    #[arg(long)]
    pub json: bool,

    /// Format variable assignments as pretty JSON.
    #[arg(long)]
    pub pretty_json: bool,
}

/// Mark this directory as 'trusted', allowing shadowenv programs to be run.
#[derive(clap::Args, Debug)]
pub struct TrustCmd {}

/// Prints a script which can be eval'd to set up shadowenv in various shells.
#[derive(Subcommand, Debug)]
#[clap(disable_help_subcommand = true)]
pub enum InitCmd {
    /// Prints a script which can be eval'd by bash to set up shadowenv.
    Bash(InitOptions),

    /// Prints a script which can be eval'd by zsh to set up shadowenv.
    Zsh(InitOptions),

    /// Prints a script which can be eval'd by fish to set up shadowenv.
    Fish,

    /// Prints a script which can be eval'd by nushell to set up shadowenv.
    Nushell,
}

/// Options shared by all init subcommands
#[derive(Args, Debug)]
pub struct InitOptions {
    /// Don't print hookbook inline (it's still required -- only use if you've already loaded it)
    #[arg(long)]
    pub no_hookbook: bool,
}

/// Print a little glyph you can include in a shell prompt to indicate that shadowenv is active.
#[derive(clap::Args, Debug)]
pub struct PromptWidgetCmd {}
