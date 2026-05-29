#!/bin/bash
# Demo script for mcp-hub
# Usage: Record with asciinema or vhs, then convert to GIF
#
# Option 1 — asciinema + agg:
#   asciinema rec demo.cast -c "bash demo.sh"
#   agg demo.cast demo.gif
#
# Option 2 — vhs:
#   vhs demo.tape (see demo.tape in this directory)

set -e

# Backup existing config
BACKUP=$(mktemp)
CONFIG="$APPDATA/Claude/claude_desktop_config.json"
if [ -f "$CONFIG" ]; then
    cp "$CONFIG" "$BACKUP"
fi

# Create demo config
mkdir -p "$(dirname "$CONFIG")"
cat > "$CONFIG" << 'EOF'
{
  "mcpServers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/"],
      "env": {}
    },
    "github": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-github"],
      "env": {
        "GITHUB_PERSONAL_ACCESS_TOKEN": "ghp_xxxxxxxxxxxxxxxxxxxx"
      }
    },
    "brave-search": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-brave-search"],
      "env": {
        "BRAVE_API_KEY": "BSAxxxxxxxxxxxxxxxxxxxxxxx"
      }
    },
    "postgres": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-postgres", "postgresql://localhost/mydb"],
      "env": {}
    }
  }
}
EOF

# Build
cd "$(dirname "$0")"
cargo build --release 2>/dev/null

echo ""
echo "=== mcp-hub scan ==="
echo ""
./target/release/mcp-hub scan

echo ""
echo "=== mcp-hub audit ==="
echo ""
./target/release/mcp-hub audit

echo ""
echo "=== mcp-hub (TUI — press q to quit) ==="
echo ""
./target/release/mcp-hub

# Restore config
if [ -f "$BACKUP" ]; then
    cp "$BACKUP" "$CONFIG"
    rm "$BACKUP"
fi
