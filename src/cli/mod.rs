use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "mcp-hub", version, about = "One TUI to manage all your MCP servers")]
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
            println!("scan command: client={:?}, json={}", client, json);
        }
        Some(Commands::Audit { json }) => {
            println!("audit command: json={}", json);
        }
        None => {
            println!("launching TUI...");
        }
    }
}
