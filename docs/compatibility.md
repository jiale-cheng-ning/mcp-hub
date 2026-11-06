# MCP compatibility and configuration migration

`doctor` and `bench` are diagnostic clients. They discover capabilities and list
tools/resources; they do not execute tools, service sampling/roots/elicitation,
or perform interactive OAuth login.

## Supported probe paths

| Transport | Protocol | Behavior |
| --- | --- | --- |
| stdio | 2026-07-28 | `server/discover`, then per-request `params._meta`; no initialize handshake |
| stdio | 2025-11-25, 2025-06-18, 2025-03-26, 2024-11-05 | Legacy initialize, returned version/capability validation, initialized notification |
| Streamable HTTP | 2026-07-28 | POST requests; both JSON and request-scoped SSE responses; no session ID or GET stream |
| Streamable HTTP | Legacy versions above | Legacy handshake and negotiated protocol header; preserves returned session ID |
| HTTP+SSE | Any | Configuration can be represented; active probing reports `unsupported` and audit reports transport deprecation |

Modern stdio discovery uses a disposable process because some old implementations
exit on a pre-initialize request. A fresh process performs the actual check.
On Windows the processes run without a console and are assigned to a kill-on-close
Job Object. On Unix they use a dedicated process group for cleanup.

HTTP probes do not follow redirects with credentials. `401`/`403` are authentication
errors; `5xx` and timeouts do not trigger legacy fallback. Only protocol/endpoint
evidence triggers fallback. Headers cannot override transport-owned fields such as
Host, content length, protocol version, method or session ID.

All responses are matched by JSON-RPC ID. Notifications and unrelated responses
cannot complete a request. Each check has one overall deadline, a 1 MiB response
limit, and bounded pagination (100 pages/10,000 entries). Only capabilities declared
by the server are queried. A legal empty list is successful; a failed or timed-out
declared capability is not. A repeated pagination cursor is an error.

`doctor --json` includes protocol, transport, and explicit `healthy`, `failed`,
`invalid`, `timeout`, `authentication_failed`, or `unsupported` status.
Benchmark summaries include successful/failed round counts and per-round status.
Latency statistics use successful rounds only. `spawn_ms` counts stdio process
creation (including the disposable probe); HTTP has no process-spawn measurement.
`init_ms` measures version/capability discovery, excluding process creation.

## Configuration formats

The internal transport model keeps HTTP URLs/headers separate from stdio
commands/arguments/environment. Import, export, sync, preset installation and
diagnostics share this model.

| Client | Configuration shape |
| --- | --- |
| Claude Code | `mcpServers`; `type: http/sse`, `url`, `headers`; stdio command/args/env |
| Cursor | `mcpServers`; `url`, `headers` or stdio command/args/env |
| VS Code | `servers`; explicit `type: stdio/http/sse` |
| Windsurf | `mcpServers`; `serverUrl`, `headers` or stdio command/args/env; legacy SSE migration is rejected |
| DeepSeek Harness | Cordis YAML operations with transport, URL/headers or stdio fields |
| Claude Desktop | Local stdio config only; remote connectors must be configured through the application |

Discovery includes user-level locations plus the current workspace's `.mcp.json`
and `.vscode/mcp.json`. Claude Code user config is `~/.claude.json`; VS Code user
config is the platform configuration directory's `Code/User/mcp.json`.
Other VS Code profiles, remote-development environments and Claude Code's nested
per-project local scopes are not automatically enumerated.

Unknown server fields such as working-directory, environment-file, or interactive
authentication options are explicitly reported as unsupported rather than silently
discarded during migration. Disabled/invalid entries cannot be silently migrated.
Existing destination entries are skipped by exact name; other destination settings
are preserved. Malformed or unreadable destinations are never treated as empty.
Writes retain backups and use atomic replacement; `sync --dry-run` is read-only.

Environment references are translated between portable `${NAME}` and `${env:NAME}`
syntax. DSH simple `!!js process.env.NAME` values are represented as environment
references internally and restored as tagged JS scalars when written. DSH requires
an entire env/header value to be a single reference; complex JS expressions or
embedded references that cannot be preserved are rejected. Literal `!!` inside
credentials is preserved. The tool never evaluates arbitrary JavaScript.

## Exported credentials

Export format 2 stores the explicit transport. Version 1 stdio exports can still be
read; unknown export versions are rejected. HTTP authentication headers, cookies,
secret environment fields and identifiable credential arguments are replaced.
Header placeholders are namespaced per server, e.g. `${MCP_<id>_HEADER_AUTHORIZATION}`.
Set the named environment variable to the **entire header value**, including a
`Bearer ` prefix if required, before importing. Missing references stop the import
before any destination write.

URL userinfo/query values are redacted and fragments removed. Replace redacted URLs
before import; move credentials to headers. Arguments with unidentifiable custom
secret formats cannot be reliably recognized: review an export before sharing it.
Scan JSON and the TUI use redacted endpoint/credential displays and do not expose
raw argument values. Raw in-memory configurations remain available to the checker
and direct sync so diagnostics use the configured credentials.

## Validation

Run with Rust stable and Python 3 on PATH (or set `MCP_HUB_TEST_PYTHON`):

```sh
cargo test --locked --offline
cargo clippy --locked --offline --all-targets -- -D warnings
cargo fmt --check
```

Protocol tests start only loopback HTTP fixtures and local stdio fixtures. They
cover modern/legacy negotiation, old servers exiting during discovery, protocol
metadata, JSON/SSE, session IDs, notifications, pagination, auth failures, tool
errors and deadlines. Configuration tests cover client-specific round trips,
credential redaction, v1 imports, DSH references, dry runs and invalid destinations.
They do not certify every third-party MCP server or interactive OAuth provider.

## References

- [MCP versioning](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/specification/2026-07-28/basic/versioning.mdx)
- [Modern discovery](https://modelcontextprotocol.io/specification/2026-07-28/server/discover)
- [Streamable HTTP](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http)
- [SDK protocol negotiation](https://ts.sdk.modelcontextprotocol.io/v2/protocol-versions)
- [Legacy lifecycle](https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle)
- [VS Code configuration](https://code.visualstudio.com/docs/agents/reference/mcp-configuration)
