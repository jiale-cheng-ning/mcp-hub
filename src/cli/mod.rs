mod audit_cmd;
mod scan;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "mcp-hub",
    version,
    about = "One TUI to manage all your MCP servers"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// List all discovered MCP servers
    Scan {
        /// Filter by client name
        #[arg(long)]
        client: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Run security audit on all MCP server configs
    Audit {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

pub fn run() {
    let cli = Cli::parse();
    match cli.command {
        Some(Commands::Scan { client, json }) => {
            scan::run(client.as_deref(), json);
        }
        Some(Commands::Audit { json }) => {
            audit_cmd::run(json);
        }
        None => {
            let entries = crate::config::discover::discover().unwrap_or_else(|e| {
                eprintln!("Warning: {}", e);
                Vec::new()
            });
            if let Err(e) = crate::tui::run_tui(entries) {
                eprintln!("TUI error: {}", e);
            }
        }
    }
}
