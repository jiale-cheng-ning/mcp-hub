mod audit_cmd;
mod bench_cmd;
mod doctor_cmd;
mod export_cmd;
mod import_cmd;
mod preset_cmd;
mod scan;
mod search_cmd;
mod sync_cmd;

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
    /// Export all MCP server configs to a single JSON file
    Export {
        /// Output file path (default: mcp-hub.json)
        #[arg(long, short)]
        output: Option<String>,
    },
    /// Import MCP server configs from a JSON file
    Import {
        /// Path to the export file
        file: String,
        /// Target client (claude-desktop, claude-code, cursor, vscode, windsurf)
        #[arg(long, short)]
        target: Option<String>,
    },
    /// Check real MCP protocol connectivity for all servers
    Doctor {
        /// Check only a specific server
        #[arg(long, short)]
        server: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
        /// Connection timeout in seconds
        #[arg(long, default_value = "5")]
        timeout: u64,
    },
    /// Search the official MCP Registry for servers
    Search {
        /// Search query
        query: String,
        /// Max results
        #[arg(long, default_value = "10")]
        limit: usize,
        /// Install a server from results
        #[arg(long)]
        install: bool,
        /// Target client for install
        #[arg(long, short)]
        target: Option<String>,
    },
    /// Benchmark MCP server performance
    Bench {
        /// Benchmark only a specific server
        #[arg(long, short)]
        server: Option<String>,
        /// Number of rounds
        #[arg(long, default_value = "3")]
        rounds: usize,
        /// Output as JSON
        #[arg(long)]
        json: bool,
        /// Connection timeout in seconds
        #[arg(long, default_value = "5")]
        timeout: u64,
    },
    /// Manage preset server bundles
    Preset {
        #[command(subcommand)]
        action: PresetAction,
    },
    /// Sync MCP server configs between clients
    Sync {
        /// Source client (claude-desktop, claude-code, cursor, vscode, windsurf)
        #[arg(long)]
        from: String,
        /// Target client
        #[arg(long, short)]
        to: String,
        /// Show what would be synced without writing
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
enum PresetAction {
    /// List available presets
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Install a preset to a client
    Install {
        /// Preset name (e.g. minimal, web-dev, backend, fullstack)
        name: String,
        /// Target client (claude-desktop, claude-code, cursor, vscode, windsurf)
        #[arg(long, short)]
        target: Option<String>,
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
        Some(Commands::Export { output }) => {
            export_cmd::run(output.as_deref());
        }
        Some(Commands::Import { file, target }) => {
            import_cmd::run(&file, target.as_deref());
        }
        Some(Commands::Doctor {
            server,
            json,
            timeout,
        }) => {
            doctor_cmd::run(server.as_deref(), json, timeout);
        }
        Some(Commands::Search {
            query,
            limit,
            install,
            target,
        }) => {
            search_cmd::run(&query, limit, install, target.as_deref());
        }
        Some(Commands::Bench {
            server,
            rounds,
            json,
            timeout,
        }) => {
            bench_cmd::run(server.as_deref(), rounds, json, timeout);
        }
        Some(Commands::Preset { action }) => match action {
            PresetAction::List { json } => {
                preset_cmd::run("list", None, None, json);
            }
            PresetAction::Install { name, target } => {
                preset_cmd::run("install", Some(&name), target.as_deref(), false);
            }
        },
        Some(Commands::Sync { from, to, dry_run }) => {
            sync_cmd::run(&from, &to, dry_run);
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
