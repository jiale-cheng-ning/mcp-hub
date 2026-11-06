use crate::config::discover::config_path_for;
use crate::config::model::ClientType;
use crate::preset;

const CLIENT_HELP: &str = "claude-desktop, claude-code, cursor, vscode, windsurf, dsh";

pub fn run(subcmd: &str, name: Option<&str>, target: Option<&str>, json: bool) {
    match subcmd {
        "list" => list_presets(json),
        "install" => {
            let preset_name = match name {
                Some(n) => n,
                None => {
                    eprintln!("Usage: mcp-hub preset install <name> --target <client>");
                    eprintln!("Run 'mcp-hub preset list' to see available presets.");
                    return;
                }
            };
            install_preset(preset_name, target);
        }
        _ => {
            eprintln!("Unknown subcommand '{}'. Use 'list' or 'install'.", subcmd);
        }
    }
}

fn list_presets(json: bool) {
    let presets = match preset::list_presets() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to load presets: {}", e);
            return;
        }
    };

    if json {
        let entries: Vec<serde_json::Value> = presets
            .iter()
            .map(|(name, desc)| serde_json::json!({"name": name, "description": desc}))
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&entries).unwrap_or_default()
        );
        return;
    }

    println!("Available presets:\n");
    for (name, desc) in &presets {
        println!("  {:<16} {}", name, desc);
    }
    println!("\nInstall with: mcp-hub preset install <name> --target <client>");
}

fn install_preset(name: &str, target: Option<&str>) {
    let preset = match preset::get_preset(name) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };

    let target_client = match target {
        Some(t) => match ClientType::from_str_loose(t) {
            Some(c) => c,
            None => {
                eprintln!("Unknown client '{}'. Use: {}", t, CLIENT_HELP);
                return;
            }
        },
        None => {
            println!("Preset '{}' — {}", name, preset.description);
            println!(
                "Servers: {}",
                preset
                    .servers
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            println!("\nNo --target specified. Available clients:");
            println!("  {}", CLIENT_HELP);
            println!("\nUsage: mcp-hub preset install {} --target <client>", name);
            return;
        }
    };

    let Some(path) = config_path_for(&target_client) else {
        eprintln!("Could not determine destination config path");
        return;
    };
    let servers = preset
        .servers
        .iter()
        .map(|(name, def)| {
            (
                name.clone(),
                crate::config::model::ExportedServer {
                    transport: crate::config::transport::Transport::Stdio,
                    command: "npx".into(),
                    args: def.args.clone(),
                    env: def
                        .env
                        .iter()
                        .map(|key| (key.clone(), format!("${{{}}}", key)))
                        .collect(),
                },
            )
        })
        .collect();
    let export = crate::config::model::ExportData {
        version: 2,
        exported_at: String::new(),
        servers,
    };
    match crate::config::write::merge(&path,&target_client,&export,false) {
        Ok((added,skipped))=>println!("Installed {} server(s), {} existing skipped. Configure referenced environment variables before use.",added,skipped),
        Err(error)=>eprintln!("Preset installation failed: {}",error),
    }
}
