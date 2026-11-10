import { spawn, spawnSync } from 'node:child_process';
import { delimiter, isAbsolute, join } from 'node:path';
import { accessSync, constants, statSync } from 'node:fs';

export const CLIENTS = ['claude-desktop', 'claude-code', 'cursor', 'vscode', 'windsurf', 'dsh'];
const MAX_BYTES = 256 * 1024;

function pathBinary() {
  // Resolve at activation, ignoring relative PATH entries. Windows otherwise
  // searches a call's workspace before PATH and could run a different binary.
  for (const directory of (process.env.PATH || '').split(delimiter)) {
    if (!isAbsolute(directory)) continue;
    const path = join(directory, process.platform === 'win32' ? 'mcp-hub.exe' : 'mcp-hub');
    try {
      accessSync(path, process.platform === 'win32' ? constants.F_OK : constants.X_OK);
      if (statSync(path).isFile()) return path;
    } catch { /* try the next absolute PATH directory */ }
  }
}

function stopTree(child) {
  if (!child.pid) return;
  if (process.platform === 'win32') {
    // Rust probes also own kill-on-close Job Objects on Windows.
    spawnSync(`${process.env.SystemRoot || 'C:\\Windows'}\\System32\\taskkill.exe`,
      ['/PID', String(child.pid), '/T', '/F'], { windowsHide: true, timeout: 5000, stdio: 'ignore' });
  } else {
    // Stdio probes create their own groups. Kill descendants as well as the
    // report group; killing only the report would leave those groups running.
    try { process.kill(child.pid, 'SIGSTOP'); } catch { /* already exited */ }
    const snapshot = spawnSync('ps', ['-A', '-o', 'pid=,ppid='], { encoding: 'utf8', timeout: 3000 });
    const rows = (snapshot.stdout || '').trim().split('\n').map(s => s.trim().split(/\s+/).map(Number));
    const descendants = new Set([child.pid]);
    for (let changed = true; changed;) {
      changed = false;
      for (const [pid, parent] of rows) {
        if (descendants.has(parent) && !descendants.has(pid)) { descendants.add(pid); changed = true; }
      }
    }
    for (const pid of [...descendants].reverse()) {
      try { process.kill(-pid, 'SIGKILL'); } catch { /* not a process group */ }
      try { process.kill(pid, 'SIGKILL'); } catch { /* already exited */ }
    }
  }
  child.kill('SIGKILL');
}

export function validateArgs(operation, args) {
  if (!['scan', 'audit', 'doctor'].includes(operation)) throw new Error('Unknown MCP Hub operation.');
  if (!args || typeof args !== 'object' || Array.isArray(args)) throw new Error('Expected an argument object.');
  const allowed = operation === 'doctor' ? ['client', 'server', 'timeout'] : ['client'];
  if (Object.keys(args).some(k => !allowed.includes(k))) throw new Error('Unknown MCP Hub argument.');
  if (args.client !== undefined && !CLIENTS.includes(args.client)) throw new Error('Unknown client.');
  if (operation === 'doctor') {
    if (typeof args.server !== 'string' || !args.server.trim() || args.server.length > 256 || /[\x00-\x1f]/.test(args.server)) {
      throw new Error('Doctor requires an exact server name from mcp_hub_scan.');
    }
    if (args.timeout !== undefined && (!Number.isInteger(args.timeout) || args.timeout < 1 || args.timeout > 30)) {
      throw new Error('timeout must be an integer from 1 to 30 seconds.');
    }
  }
}

export function validateReport(value, operation) {
  if (!value || value.api_version !== 1 || value.operation !== operation
      || !Array.isArray(value.results) || value.results.length > 200 || !Array.isArray(value.warnings)
      || !Number.isSafeInteger(value.servers_scanned) || value.servers_scanned < 0
      || !Number.isSafeInteger(value.invalid_configs) || value.invalid_configs < 0
      || !Number.isSafeInteger(value.total) || value.total < value.results.length || typeof value.truncated !== 'boolean') {
    throw new Error('Incompatible MCP Hub report. Install the Rust binary from the same checkout as this plugin.');
  }
  return value;
}

export function createRunner(config = {}) {
  if (!config || typeof config !== 'object' || Array.isArray(config)
      || Object.keys(config).some(k => k !== 'binary')) throw new Error('MCP Hub config accepts only binary.');
  const requested = config.binary ?? process.env.MCP_HUB_BIN ?? 'mcp-hub';
  const binary = requested === 'mcp-hub' ? pathBinary() : requested;
  if (typeof requested !== 'string' || !requested.trim() || /[\x00-\x1f]/.test(requested)) throw new Error('binary must be an executable path.');
  // Relative executable paths must never resolve inside a model-controlled workspace.
  if (requested !== 'mcp-hub' && !isAbsolute(requested)) throw new Error('Use an absolute binary path, or mcp-hub from PATH.');
  return async (operation, args, { signal, cwd } = {}) => {
    validateArgs(operation, args);
    if (!cwd || !isAbsolute(cwd)) throw new Error('MCP Hub requires an agent session with an absolute workspace path.');
    if (signal?.aborted) throw new Error('MCP Hub call cancelled.');
    if (!binary) throw new Error('Cannot start MCP Hub. Install the Rust binary and set MCP_HUB_BIN to its absolute path.');
    const argv = ['report', operation];
    if (args.client !== undefined) argv.push(`--client=${args.client}`);
    if (operation === 'doctor') argv.push(`--server=${args.server}`, `--timeout=${args.timeout ?? 5}`);
    return new Promise((resolve, reject) => {
      const child = spawn(binary, argv, { cwd, shell: false, windowsHide: true,
        detached: process.platform !== 'win32', stdio: ['ignore', 'pipe', 'pipe'] });
      const output = [];
      let size = 0, failure;
      const stop = message => { if (!failure) { failure = new Error(message); stopTree(child); } };
      const abort = () => stop('MCP Hub call cancelled.');
      const timer = setTimeout(() => stop('MCP Hub exceeded its 45 second deadline.'), 45_000);
      const cleanup = () => { clearTimeout(timer); signal?.removeEventListener('abort', abort); };
      signal?.addEventListener('abort', abort, { once: true });
      if (signal?.aborted) abort();
      child.stdout.on('data', chunk => {
        size += chunk.length;
        if (size > MAX_BYTES) stop('MCP Hub report exceeded 256 KiB. Narrow the client filter.');
        else output.push(chunk);
      });
      // Drain stderr but never echo it into model context: it can include secrets.
      child.stderr.resume();
      child.on('error', () => {
        failure ??= new Error('Cannot start MCP Hub. Install the Rust binary and set MCP_HUB_BIN to its absolute path.');
      });
      child.on('close', code => {
        cleanup();
        if (failure) return reject(failure);
        if (code !== 0) return reject(new Error('MCP Hub failed. Check that the binary supports report, the selected server is active and uniquely matches, and configs are readable.'));
        try { resolve(validateReport(JSON.parse(Buffer.concat(output).toString('utf8')), operation)); }
        catch { reject(new Error('Invalid MCP Hub report. Install the Rust binary from the same checkout as this plugin.')); }
      });
    });
  };
}
