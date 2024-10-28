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

    /// The command to execute.
    pub cmd: String,

    /// The arguments to the command, if any.
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
    Bash,

    /// Prints a script which can be eval'd by zsh to set up shadowenv.
    Zsh,

    /// Prints a script which can be eval'd by fish to set up shadowenv.
    Fish,
}

/// Print a little glyph you can include in a shell prompt to indicate that shadowenv is active.
#[derive(clap::Args, Debug)]
pub struct PromptWidgetCmd {}

// pub fn app() -> App<'static, 'static> {
//     let version = Box::leak(
//         format!(
//             "{}.{}.{}{}",
//             env!("CARGO_PKG_VERSION_MAJOR"),
//             env!("CARGO_PKG_VERSION_MINOR"),
//             env!("CARGO_PKG_VERSION_PATCH"),
//             option_env!("CARGO_PKG_VERSION_PRE").unwrap_or("")
//         )
//         .into_boxed_str(),
//     );
//     App::new("shadowenv")
//         .version(&version[..])
//         .setting(AppSettings::SubcommandRequiredElseHelp)
//         .subcommand(
//             SubCommand::with_name("hook")
//                 .about("Runs the shell hook. You shouldn't need to run this manually.")
//                 .setting(AppSettings::DisableHelpSubcommand)
//                 .arg(
//                     // Legacy: This is exported now, and in fact this setting is ignored
//                     // completely if $__shadowenv_data is present in the environment.
//                     Arg::with_name("$__shadowenv_data").required(false)
//                 )
//                 .arg(
//                     Arg::with_name("fish")
//                         .long("fish")
//                         .help("Format variable assignments for fish shell"),
//                 )
//                 .arg(
//                     Arg::with_name("posix")
//                         .long("posix")
//                         .help("Format variable assignments for posix shells (default)"),
//                 )
//                 .arg(
//                     Arg::with_name("force")
//                         .long("force")
//                         .help("Force the shadowenv to be applied, even if the working directory has not changed."),
//                 )
//                 .arg(
//                     Arg::with_name("silent")
//                         .long("silent")
//                         .help("Suppress error printing"),
//                 )
//                 .arg(
//                     // this is necessary if shadowenv hook is called from a subshell, as we do in
//                     // the bash hook
//                     Arg::with_name("shellpid")
//                         .long("shellpid")
//                         .takes_value(true)
//                         .help("rather than looking up the PPID, use this as the shell's pid"),
//                 )
//                 .arg(
//                     Arg::with_name("porcelain")
//                         .long("porcelain")
//                         .help("Format variable assignments for machine parsing"),
//                 )
//                 .arg(
//                     Arg::with_name("json")
//                         .long("json")
//                         .help("Format variable assignments as JSON"),
//                 )
//                 .arg(
//                     Arg::with_name("pretty-json")
//                         .long("pretty-json")
//                         .help("Format variable assignments as pretty JSON"),
//                 )
//                 .group(ArgGroup::with_name("format").args(&["porcelain", "posix", "fish", "json", "pretty-json"])),
//         )
//         .subcommand(
//             SubCommand::with_name("diff")
//                 .about("Display a diff of changed environment variables.")
//                 .setting(AppSettings::DisableHelpSubcommand)
//                 .arg(
//                     Arg::with_name("verbose")
//                         .long("verbose")
//                         .short("v")
//                         .help("Show all environment variables, not just those that changed"),
//                 )
//                 .arg(
//                     Arg::with_name("no-color")
//                         .long("no-color")
//                         .short("n")
//                         .help("Do not use color to highlight the diff"),
//                 )
//                 .arg(
//                     // Legacy: This is exported now, and in fact this setting is ignored
//                     // completely if $__shadowenv_data is present in the environment.
//                     Arg::with_name("$__shadowenv_data").required(false)
//                 ),
//         )
//         .subcommand(
//             SubCommand::with_name("trust")
//                 .about("Mark this directory as 'trusted', allowing shadowenv programs to be run.")
//                 .setting(AppSettings::DisableHelpSubcommand)
//         )
//         .subcommand(
//             SubCommand::with_name("exec")
//                 .about(
//                     "Execute a command after loading the environment from the current directory.",
//                 )
//                 .setting(AppSettings::DisableHelpSubcommand)
//                 .arg(
//                     // Legacy: This is exported now, and this flag will likely go away soon.
//                     Arg::with_name("$__shadowenv_data")
//                         .long("shadowenv-data")
//                         .takes_value(true)
//                         .help("Legacy, will be removed soon: Don't use this; provide $__shadowenv_data in the environment instead"),
//                 )
//                 .arg(
//                     Arg::with_name("dir")
//                         .long("dir")
//                         .takes_value(true)
//                         .help("Instead of searching from the current directory for a .shadowenv.d, search from this one."),
//                 )
//                 .arg(
//                     Arg::with_name("child-argv0")
//                         .help("If the command takes no arguments, it can be passed directly as the last arugment."),
//                 )
//                 .arg(
//                     Arg::with_name("child-argv")
//                         .multiple(true)
//                         .last(true)
//                         .help("If the command requires arguments, they must all be passed after a --."),
//                 )
//                 .group(
//                     ArgGroup::with_name("argv")
//                              .args(&["child-argv0", "child-argv"])
//                              .required(true),
//                 )
//         )
//         .subcommand(
//             SubCommand::with_name("init")
//                 .about("Prints a script which can be eval'd to set up shadowenv in various shells.")
//                 .setting(AppSettings::SubcommandRequiredElseHelp)
//                 .setting(AppSettings::DisableHelpSubcommand)
//                 .subcommand(
//                     SubCommand::with_name("bash")
//                         .about("Prints a script which can be eval'd by bash to set up shadowenv."),
//                 )
//                 .subcommand(
//                     SubCommand::with_name("zsh")
//                         .about("Prints a script which can be eval'd by zsh to set up shadowenv."),
//                 )
//                 .subcommand(
//                     SubCommand::with_name("fish")
//                         .about("Prints a script which can be eval'd by fish to set up shadowenv."),
//                 )
//         )
// }
