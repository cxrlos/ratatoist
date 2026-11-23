use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "ratatoist",
    version,
    about = "A terminal client for Todoist",
    args_conflicts_with_subcommands = true
)]
pub struct Cli {
    /// Open a fictional Todoist account: no token, keyring or network.
    #[arg(long)]
    pub demo: bool,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Store, check or remove the Todoist API token.
    #[command(subcommand)]
    Auth(AuthCommand),
}

#[derive(Debug, Clone, Subcommand)]
pub enum AuthCommand {
    /// Prompt for a token, check it against Todoist and store it in the OS keyring.
    Login {
        /// Read the token from standard input instead of prompting.
        #[arg(long)]
        stdin: bool,
    },
    /// Show where the token comes from and which account it belongs to.
    Status,
    /// Remove ratatoist's keyring entry.
    Logout,
}
