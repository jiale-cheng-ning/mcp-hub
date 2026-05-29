<div align="center">

# mcp-hub

**One TUI to manage all your MCP servers.**

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Language-Rust-orange.svg)](https://www.rust-lang.org/)

You installed MCP servers in Claude Desktop, Cursor, VS Code, and Claude Code.
They're scattered across different config files.
Some are broken. Some have security issues. You can't see them all at once.

**mcp-hub fixes that.** One terminal dashboard. All your servers. Health checks included.

<img src="assets/mcp-hub-demo.svg" alt="mcp-hub TUI demo" width="720"/>

</div>

---

## What it does

| Feature | Description |
|---------|-------------|
| **TUI Dashboard** | Interactive terminal UI — browse, inspect, and filter all MCP servers |
| **Auto-Discovery** | Scans Claude Desktop, Claude Code, Cursor, Windsurf configs automatically |
| **Security Audit** | Finds plaintext secrets, dangerous permissions, unpinned packages, duplicates |
| **Health Checks** | Shows which servers are running and which are down |
| **Single Binary** | One Rust binary. No runtime. No dependencies. `cargo install` and go. |

## Install

```bash
# From GitHub (requires Rust)
cargo install --git https://github.com/jiale-cheng-ning/mcp-hub

# Or clone and build
git clone https://github.com/jiale-cheng-ning/mcp-hub.git
cd mcp-hub
cargo build --release
# binary: target/release/mcp-hub
```

## Usage

```bash
mcp-hub              # Launch TUI dashboard
mcp-hub scan         # List all servers in a table
mcp-hub scan --json  # JSON output for scripting
mcp-hub audit        # Run security audit
mcp-hub audit --json # JSON output for CI
```

### TUI keybindings

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `Tab` | Switch between Servers and Audit tabs |
| `q` / `Esc` | Quit |

### Audit rules

| Rule | Severity | What it catches |
|------|----------|-----------------|
| `ENV_PLAINTEXT_SECRET` | Warning | API keys / tokens stored as plaintext in config |
| `PERM_ROOT` / `PERM_HOME` | Warning | Filesystem servers with unrestricted access |
| `NO_VERSION_PIN` | Info | npm packages without pinned versions |
| `DUPLICATE_SERVER` | Info | Same server configured in multiple clients |

### Example: `mcp-hub audit`

```
🟡 WARNING (3)
  ├─ filesystem: Server 'filesystem' has unrestricted access to root filesystem
  │  Fix: Restrict directory scope with a specific path
  ├─ github: Potential secret 'GITHUB_PERSONAL_ACCESS_TOKEN' stored in plaintext config
  │  Fix: Use environment variable reference or secret manager
  ├─ brave-search: Potential secret 'BRAVE_API_KEY' stored in plaintext config
  │  Fix: Use environment variable reference or secret manager

ℹ️  INFO (4)
  ├─ filesystem: Unpinned package version: '@modelcontextprotocol/server-filesystem'
  │  Fix: Pin to a specific version (e.g., @scope/pkg@1.2.0)
  ...

Total findings: 7
```

## Supported clients

| Client | Config location |
|--------|----------------|
| Claude Desktop | `%APPDATA%\Claude\claude_desktop_config.json` |
| Claude Code | `~/.claude/settings.json` |
| Cursor | `~/.cursor/mcp.json` |
| Windsurf | `~/.codeium/windsurf/mcp_config.json` |

## Roadmap

- [x] Auto-discovery of MCP configs
- [x] TUI dashboard with server list and detail panel
- [x] Security audit (secrets, permissions, versions, duplicates)
- [x] Health checks (process detection)
- [ ] Config sync between clients
- [ ] Export/import configurations (Git-friendly)
- [ ] Preset server bundles (`mcp-hub preset install web-dev`)
- [ ] Real-time log viewer
- [ ] Resource monitoring (CPU/memory)

## Contributing

Contributions welcome. Open an issue first to discuss what you'd like to change.

## License

MIT — see [LICENSE](LICENSE).
