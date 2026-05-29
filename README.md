<div align="center">

# mcp-hub

**One TUI to manage all your MCP servers.**

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Language-Rust-orange.svg)](https://www.rust-lang.org/)

Manage, monitor, and audit MCP server configurations across Claude Desktop, Cursor, VS Code, and more — from a single terminal dashboard.

</div>

---

## Features

- **TUI Dashboard** — Interactive terminal UI to browse and inspect all MCP servers
- **Auto-Discovery** — Automatically finds MCP configs from all installed AI clients
- **Security Audit** — Detects plaintext secrets, dangerous permissions, unpinned versions, duplicates
- **Health Checks** — See which servers are running and which are down
- **Single Binary** — One Rust binary, no runtime dependencies

## Installation

### From source (requires Rust)

```bash
cargo install --git https://github.com/jiale-cheng-ning/mcp-hub
```

### Build locally

```bash
git clone https://github.com/jiale-cheng-ning/mcp-hub.git
cd mcp-hub
cargo build --release
```

The binary will be at `target/release/mcp-hub`.

## Quick Start

```bash
# Launch the TUI dashboard
mcp-hub

# List all discovered MCP servers
mcp-hub scan

# List servers as JSON (for scripting)
mcp-hub scan --json

# Filter by client
mcp-hub scan --client cursor

# Run security audit
mcp-hub audit

# Audit output as JSON (for CI)
mcp-hub audit --json
```

## TUI Dashboard

Launch with `mcp-hub` (no arguments):

```
+------------------------- MCP Hub -------------------------+
|  [Servers] [Audit]                                        |
+-----------------------------------------------------------+
|                                                           |
|  +--------------+--------+--------+--------+-----------+  |
|  | Name         | Client | Command| Status |           |  |
|  +--------------+--------+--------+--------+-----------+  |
|  | filesystem   | Claude | npx    | running|           |  |
|  | github       | Claude | npx    | running|           |  |
|  | brave-search | Cursor | npx    | stopped|           |  |
|  +--------------+--------+--------+--------+-----------+  |
|                                                           |
|  -- Detail ------------------------------------------------|
|  Name: github                                             |
|  Command: npx -y @modelcontextprotocol/server-github      |
|  Env: GITHUB_PERSONAL_ACCESS_TOKEN=****                   |
|                                                           |
+-- Keys ----------------------------------------------------+
|  j/k: navigate  Tab: switch  q: quit                      |
+-----------------------------------------------------------+
```

### Keybindings

| Key | Action |
|-----|--------|
| `j` / `Down` | Move down |
| `k` / `Up` | Move up |
| `Tab` | Switch between Servers and Audit tabs |
| `q` / `Esc` | Quit |

## Security Audit

`mcp-hub audit` checks all discovered configs for:

| Rule | Severity | Description |
|------|----------|-------------|
| `ENV_PLAINTEXT_SECRET` | Warning | Env var name suggests a secret (TOKEN, KEY, etc.) stored in plaintext |
| `PERM_ROOT` | Warning | Server has unrestricted access to root filesystem |
| `PERM_HOME` | Warning | Server has unrestricted access to home directory |
| `NO_VERSION_PIN` | Info | npm package used without pinned version |
| `DUPLICATE_SERVER` | Info | Same server configured in multiple clients |

Example output:

```
WARNING (2)
  +-- filesystem: Server 'filesystem' has unrestricted access to root filesystem
  |   Fix: Restrict directory scope with a specific path
  +-- github: Potential secret 'GITHUB_PERSONAL_ACCESS_TOKEN' stored in plaintext config
      Fix: Use environment variable reference or secret manager

INFO (1)
  +-- github: Unpinned package version: '@modelcontextprotocol/server-github'
      Fix: Pin to a specific version (e.g., @scope/pkg@1.2.0)

Total findings: 3
```

## Supported Clients

| Client | Config Path |
|--------|------------|
| Claude Desktop | `%APPDATA%\Claude\claude_desktop_config.json` |
| Claude Code | `~/.claude/settings.json` |
| Cursor | `~/.cursor/mcp.json` |
| Windsurf | `~/.codeium/windsurf/mcp_config.json` |

## CLI Reference

| Command | Description | Flags |
|---------|-------------|-------|
| `mcp-hub` | Launch TUI dashboard | — |
| `mcp-hub scan` | List all servers | `--json`, `--client <name>` |
| `mcp-hub audit` | Run security audit | `--json` |
| `mcp-hub --help` | Show help | — |
| `mcp-hub --version` | Show version | — |

## Roadmap

- [x] Auto-discovery of MCP configs
- [x] TUI dashboard with server list and detail panel
- [x] Security audit (secrets, permissions, versions, duplicates)
- [x] Health checks (process detection)
- [ ] Config sync between clients
- [ ] Export/import configurations
- [ ] Preset server bundles (web-dev, data, fullstack)
- [ ] Real-time log viewer
- [ ] Resource monitoring (CPU/memory)

## Contributing

Contributions welcome! Please open an issue first to discuss what you'd like to change.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'feat: add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## License

MIT — see [LICENSE](LICENSE) for details.
