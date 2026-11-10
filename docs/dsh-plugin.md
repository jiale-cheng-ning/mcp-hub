# DeepSeek Harness plugin / DSH 插件

The Rust CLI remains a standalone tool. The repository also ships a host-only
DSH bundle, `@jiale-cheng-ning/dsh-mcp-hub`, exposing three tools in DSH conversations.
Results appear as readable lists in the normal tool result, with structured
values available to programmatic tool calls. There is
no separate Web settings panel.

保留原有 Rust 命令行工具，同时提供 DSH 插件入口。在 DSH 对话里可以请求：

- “用 MCP Hub 扫描我的 MCP 配置。” → `mcp_hub_scan`
- “用 MCP Hub 检查 MCP 配置的安全问题。” → `mcp_hub_audit`
- “检查 Cursor 中名为 github 的 MCP 服务能否连接。” → `mcp_hub_doctor`

## Install / 安装

Requirements: Node.js 20.11+, DSH with a `tools` service, pnpm for DSH plugin
installation, and **the Rust binary built from this checkout**. Older releases
without the `report` subcommand cannot serve the plugin. The plugin reports a
clear error for an unavailable or incompatible binary.

需要 Node.js 20.11+、提供 `tools` 服务的 DSH，以及用于安装插件的 pnpm。
**必须使用本次源码构建的 Rust 程序**；旧版二进制没有 `report` 接口。

From this repository / 在仓库目录执行：

```sh
cargo install --path . --locked
npm pack
```

`npm pack` produces `jiale-cheng-ning-dsh-mcp-hub-0.1.0.tgz`. Install the local
archive into the DSH profile you use / 将生成的压缩包安装到使用的 DSH profile：

```sh
dsh plugin --profile web add ./jiale-cheng-ning-dsh-mcp-hub-0.1.0.tgz
```

Restart that profile after installation. `web` is an example; use `sdk`,
`headless` or your own profile name as needed. The package declares
`dsh.bundle.patch` and inserts the `mcp-hub` host plugin. It has no npm
dependencies, install scripts or build steps. It does not download or install
the Rust executable automatically.

安装后重启对应 profile。`web` 只是示例，也可以选择 `sdk`、`headless` 或自定义
profile。插件本身不需要构建，没有 npm 依赖和安装脚本；Rust 程序需单独安装。

If the binary isn't on DSH's PATH, set its absolute path **before starting DSH**:

```powershell
# Windows: use the actual path reported by cargo install.
$env:MCP_HUB_BIN = "$env:USERPROFILE\.cargo\bin\mcp-hub.exe"
dsh web
```

```sh
# Linux / macOS
export MCP_HUB_BIN="$HOME/.cargo/bin/mcp-hub"
dsh web
```

The plugin also accepts a host configuration object `{ "binary": "/absolute/path/to/mcp-hub" }`.
Explicit configuration takes priority over `MCP_HUB_BIN`, then the activation-time
PATH lookup. Relative executable paths and relative PATH entries are refused.
The model cannot supply a binary path, shell command or working directory.

## Tools / 功能

| Tool | Arguments | Behaviour |
| --- | --- | --- |
| `mcp_hub_scan` | optional `client` | Names, clients, transports and configuration status; starts no servers. |
| `mcp_hub_audit` | optional `client` | Static configuration rules, severity and descriptions; starts no servers. |
| `mcp_hub_doctor` | required `server`, optional `client`, `timeout` | Live protocol probe of exactly one active server; timeout defaults to 5 seconds, range 1–30. |

Clients: `claude-desktop`, `claude-code`, `cursor`, `vscode`, `windsurf`, `dsh`.
Omitting `client` scans the supported configuration locations for all clients.
Project files are resolved relative to the **calling agent session's workspace**,
not the DSH profile installation directory. A call without a session workspace fails.

Doctor uses an exact server name from scan, rather than a substring match. If
multiple configurations have that name, select `client`; if it remains ambiguous,
give the configurations distinct names. Disabled and invalid configurations are
never launched. A failed connection is returned as a domain status such as
`timeout`, `authentication_failed` or `invalid_response`, rather than `healthy`.

扫描和审计只读取配置。健康检查会执行所选配置中的 stdio 命令，或连接其 HTTP
地址；只在用户请求实际检查时调用。它不调用服务器提供的业务工具，也不修改配置。
同名服务需要指定客户端；仍然重名时，应先区分服务名称。

The CLI discovers the paths documented in [compatibility.md](compatibility.md).
It does not enumerate arbitrary third-party plugin stores or every generated
bundle layer in DSH. An empty result means no entries were found in those known
locations, not that the running DSH tool registry has no MCP servers.

## Result contract and limits / 输出与限制

The bridge calls `mcp-hub report <scan|audit|doctor>`, with argument arrays and
no shell. Its version-1 JSON envelope contains `operation`, `servers_scanned`,
`invalid_configs`, `warnings`, `results`, `total` and `truncated`.

- Raw environment values, headers, URLs, commands, command arguments, parser
  snippets and server-provided tool/resource text are omitted from these reports.
  Service names and client names remain visible. Audit uses static rule
  descriptions instead of CLI finding messages that can quote arguments.
- Unreadable or malformed files produce structured warnings. Invalid server
  entries are counted separately. The renderer calls out incomplete coverage.
- Scan and audit return at most 200 rows, with the full count and truncation flag.
  The bridge refuses output over 256 KiB and has its own 45-second deadline.
- DSH cancellation and plugin disposal stop the report process and its process
  tree. Raw stderr is drained and never copied into conversation results.
- Audits use the CLI's bundled rules and package data. They are heuristic checks,
  not a full vulnerability scan or proof that a server is safe.

## Validation / 验证

```sh
cargo build --locked
# Set this to target/debug/mcp-hub.exe on Windows.
MCP_HUB_TEST_BIN="$PWD/target/debug/mcp-hub" npm test
```

Tests use temporary configurations and local fixtures. They cover actual Rust
reports, successful/failed protocol probes, exact-name selection, malformed
configuration warnings, secret omission, subprocess errors, output bounds,
cancellation and plugin disposal. CI runs these tests on Windows, Linux and macOS.

An optional smoke test uses an installed official DSH tool registry without
starting an LLM session or changing your profile:

```powershell
$env:DSH_RUNTIME_ROOT = 'C:\path\to\node_modules\@deepseek-ai\dsh'
$env:MCP_HUB_TEST_BIN = "$PWD\target\debug\mcp-hub.exe"
npm run test:dsh-runtime
```

Locally verified against DSH `0.1.7-rc.1`: registration, schema acceptance,
execution of all three tools through `ctx.tools.execute`, rendering and disposal.
The packed archive was also installed offline into an isolated DSH profile and
its bundle composition verified. Current official source uses the same raw tool
registration contract; a full 0.2.1 runtime test is still
needed before claiming that exact version has been tested.

## Listing / 收录

Once this code is published and installation is verified, add the `dsh-plugin`
GitHub topic and submit **one** file to awesome-dsh-plugin:
`data/plugins/jiale-cheng-ning__mcp-hub.yml`. Suggested entry:

```yaml
url: https://github.com/jiale-cheng-ning/mcp-hub
name: jiale-cheng-ning/mcp-hub
category: security
description:
  en: 'DSH tools for discovering MCP configurations across clients, checking static security rules and probing one named server, using a separately installed mcp-hub Rust binary.'
  zh: '通过单独安装的 mcp-hub Rust 程序，在 DSH 对话中跨客户端发现 MCP 配置、检查静态安全规则并验证指定服务的连通性。'
```

Maintainers decide whether to accept the listing. Release automation also packs
`dsh-mcp-hub.tgz` for future GitHub Releases; a release archive is not published
merely by editing these files.

References: [DSH tool authoring](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/cookbook/adding-a-tool.md),
[DSH CLI](https://github.com/deepseek-ai/deepseek-harness/blob/master/apps/cli/README.md),
[listing requirements](https://github.com/awesome-dsh-plugin/awesome-dsh-plugin/blob/main/contributing.md).
