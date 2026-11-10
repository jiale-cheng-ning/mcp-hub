import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, readFile, mkdir, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { apply, renderReport } from '../../dsh/index.js';
import { createRunner, validateArgs } from '../../dsh/runner.js';

const envelope = operation => ({ api_version: 1, operation, servers_scanned: 0, invalid_configs: 0, warnings: [], results: [], total: 0, truncated: false });
async function fake(t, mode = 'ok') {
  const cwd = await mkdtemp(join(tmpdir(), 'mcp-hub-plugin-'));
  t.after(() => rm(cwd, { recursive: true, force: true }));
  // Running `node report scan` exercises the actual subprocess runner on all OSes.
  await writeFile(join(cwd, 'report'), `
const fs = require('node:fs');
fs.writeFileSync('argv.json', JSON.stringify(process.argv.slice(2)));
const mode = ${JSON.stringify(mode)};
if (mode === 'hang' || mode === 'hang-tree') {
  if (mode === 'hang-tree') {
    const c = require('node:child_process').spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], { detached: true, stdio: 'ignore' });
    fs.writeFileSync('grandchild', String(c.pid));
  }
  fs.writeFileSync('pid', String(process.pid)); setInterval(() => {}, 1000);
}
else if (mode === 'oversize') process.stdout.write('x'.repeat(300000));
else if (mode === 'invalid') console.log('not-json');
else if (mode === 'old') console.log('[]');
else if (mode === 'fail') { console.error('secret-never-echo'); process.exit(3); }
else console.log(JSON.stringify({ ...${JSON.stringify(envelope('scan'))}, operation: process.argv[2] }));
`);
  return { cwd, run: createRunner({ binary: process.execPath }) };
}

test('registers three host tools without launching a process', async () => {
  const tools = [], handlers = [];
  apply({ tools: { register: def => tools.push(def) }, on: (event, callback) => handlers.push([event, callback]) });
  assert.deepEqual(tools.map(t => t.name), ['mcp_hub_scan', 'mcp_hub_audit', 'mcp_hub_doctor']);
  assert.equal(tools[0].isConcurrencySafe(), true);
  assert.equal(tools[2].isConcurrencySafe(), false);
  assert.deepEqual(tools[2].parameters.required, ['server']);
  assert.equal(handlers[0][0], 'dispose');
});

test('uses discrete arguments and the session workspace, including shell syntax in server names', async t => {
  const { run, cwd } = await fake(t);
  await run('doctor', { client: 'cursor', server: 'name;$(echo nope)', timeout: 3 }, { cwd });
  assert.deepEqual(JSON.parse(await readFile(join(cwd, 'argv.json'))), ['doctor', '--client=cursor', '--server=name;$(echo nope)', '--timeout=3']);
});

test('rejects unsafe or ambiguous arguments before launching', () => {
  for (const args of [{}, { server: '' }, { server: 'x', timeout: 0 }, { server: 'x', timeout: 31 }, { server: 'x', timeout: 1.5 }, { server: 'x', binary: 'evil' }, { server: 'x', client: 'unknown' }, { server: 'a\nb' }]) {
    assert.throws(() => validateArgs('doctor', args));
  }
  assert.throws(() => createRunner({ binary: './workspace-executable' }));
  assert.throws(() => createRunner({ binary: 'mcp-hub', arbitraryArgs: [] }));
});

for (const mode of ['invalid', 'old', 'fail', 'oversize']) {
  test(`fails explicitly on ${mode} output without echoing stderr`, async t => {
    const { run, cwd } = await fake(t, mode);
    await assert.rejects(run('scan', {}, { cwd }), e => !e.message.includes('secret-never-echo'));
  });
}

test('reports missing binaries and missing session workspaces', async t => {
  const { cwd } = await fake(t);
  await assert.rejects(createRunner({ binary: join(cwd, 'missing-binary') })('scan', {}, { cwd }), /Cannot start/);
  await assert.rejects(createRunner()('scan', {}), /workspace/);
});

test('cancellation kills the running subprocess tree and waits for closure', async t => {
  const { run, cwd } = await fake(t, 'hang-tree');
  const controller = new AbortController();
  const pending = assert.rejects(run('scan', {}, { cwd, signal: controller.signal }), /cancelled/);
  let pid;
  for (let i = 0; i < 150 && !pid; i++) {
    try { pid = Number(await readFile(join(cwd, 'pid'), 'utf8')); } catch { await new Promise(r => setTimeout(r, 20)); }
  }
  assert.ok(pid, 'fixture started');
  const grandchild = Number(await readFile(join(cwd, 'grandchild'), 'utf8'));
  controller.abort();
  await pending;
  assert.throws(() => process.kill(pid, 0), { code: 'ESRCH' });
  // A terminated Unix child can briefly remain as a zombie awaiting reaping.
  if (process.platform === 'win32') assert.throws(() => process.kill(grandchild, 0), { code: 'ESRCH' });
  else {
    const status = spawnSync('ps', ['-p', String(grandchild), '-o', 'stat='], { encoding: 'utf8' }).stdout.trim();
    assert.ok(!status || status.startsWith('Z'), `grandchild still running: ${status}`);
  }
});

test('plugin disposal cancels in-flight tools', async t => {
  const { cwd } = await fake(t, 'hang');
  let dispose;
  const tools = [];
  apply({ tools: { register: def => tools.push(def) }, on: (_event, cb) => { dispose = cb; } }, { binary: process.execPath });
  const pending = assert.rejects(tools[0].execute({}, { signal: new AbortController().signal, agent: { session: { header: { cwd } } } }), /cancelled/);
  dispose();
  await pending;
});

test('renderer distinguishes incomplete discovery and truncated results', () => {
  const report = { ...envelope('audit'), warnings: [{ client: 'Cursor', code: 'invalid_config' }], truncated: true };
  const text = renderReport({}, report)[0].text;
  assert.match(text, /Incomplete coverage/);
  assert.match(text, /truncated/);
  assert.match(text, /untrusted/);
});

const binary = process.env.MCP_HUB_TEST_BIN && resolve(process.env.MCP_HUB_TEST_BIN);
test('real Rust binary: scan, audit, diagnostics, exact doctor and bounded protocol probe', { skip: !binary }, async t => {
  const cwd = await mkdtemp(join(tmpdir(), 'mcp-hub-e2e-'));
  t.after(() => rm(cwd, { recursive: true, force: true }));
  const home = join(cwd, 'home');
  await mkdir(join(home, 'dsh'), { recursive: true });
  const config = join(home, 'dsh', 'cordis.patch.yml');
  const secret = 'private-fixture-token-never-disclose';
  // JSON is a YAML subset, so no test-only YAML dependency is needed.
  await writeFile(config, JSON.stringify([{ insert: [
    { name: '@deepseek-ai/dsh-mcp-client', config: { serverName: 'fixture-exact', command: binary, args: ['--version'], env: { API_TOKEN: secret } } },
    { name: '@deepseek-ai/dsh-mcp-client', config: { serverName: 'fixture-exact-extra', command: 'do-not-start-this-executable' } },
  ] }]));
  const env = { ...process.env, HOME: home, USERPROFILE: home, APPDATA: home, XDG_CONFIG_HOME: home, DSH_HOME: join(home, 'dsh') };
  const report = (op, extra = []) => spawnSync(binary, ['report', op, '--client=dsh', ...extra], { cwd, env, encoding: 'utf8', timeout: 15000 });
  const before = await readFile(config, 'utf8');
  const scan = report('scan');
  assert.equal(scan.status, 0, scan.stderr);
  assert.equal(JSON.parse(scan.stdout).servers_scanned, 2);
  const audit = report('audit');
  assert.equal(audit.status, 0, audit.stderr);
  assert.ok(JSON.parse(audit.stdout).results.some(f => f.rule === 'ENV_PLAINTEXT_SECRET'));
  assert.ok(!scan.stdout.includes(secret) && !audit.stdout.includes(secret));
  assert.notEqual(report('doctor').status, 0);
  const doctor = report('doctor', ['--server=fixture-exact', '--timeout=1']);
  assert.equal(doctor.status, 0, doctor.stderr);
  assert.equal(JSON.parse(doctor.stdout).results.length, 1);
  assert.equal(JSON.parse(doctor.stdout).results[0].server, 'fixture-exact');
  assert.notEqual(JSON.parse(doctor.stdout).results[0].status, 'healthy');
  assert.equal(await readFile(config, 'utf8'), before);
  const fixture = resolve('tests/fixtures/mcp_server.py');
  const healthyConfig = [{ insert: [{ name: '@deepseek-ai/dsh-mcp-client', config: {
    serverName: 'healthy', command: process.env.MCP_HUB_TEST_PYTHON || (process.platform === 'win32' ? 'python' : 'python3'),
    args: [fixture, 'stdio', 'modern'],
  } }] }];
  await writeFile(config, JSON.stringify(healthyConfig));
  const healthy = report('doctor', ['--server=healthy']);
  assert.equal(healthy.status, 0, healthy.stderr);
  assert.equal(JSON.parse(healthy.stdout).results[0].status, 'healthy');
  assert.equal(JSON.parse(healthy.stdout).results[0].tools_count, 1);
  await writeFile(config, JSON.stringify([{ insert: [...healthyConfig[0].insert, ...healthyConfig[0].insert] }]));
  assert.notEqual(report('doctor', ['--server=healthy']).status, 0, 'ambiguous names must not start probes');
  await writeFile(config, '{"API_TOKEN":"' + secret + '"');
  const bad = report('audit');
  assert.equal(bad.status, 0, bad.stderr);
  assert.equal(JSON.parse(bad.stdout).warnings[0].code, 'invalid_config');
  assert.ok(!bad.stdout.includes(secret));
  await writeFile(config, '[]');
  assert.deepEqual(JSON.parse(report('audit').stdout).results, []);
});
