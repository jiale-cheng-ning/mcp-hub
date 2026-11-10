// Optional integration with an installed official DSH runtime; no LLM/account needed.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
const plugin = await import(process.env.DSH_PLUGIN_ENTRY
  ? pathToFileURL(process.env.DSH_PLUGIN_ENTRY).href
  : '../../dsh/index.js');

if (!process.env.DSH_RUNTIME_ROOT) throw new Error('Set DSH_RUNTIME_ROOT to the installed @deepseek-ai/dsh directory.');
const require = createRequire(join(process.env.DSH_RUNTIME_ROOT, 'package.json'));
const load = async name => import(pathToFileURL(require.resolve(name)).href);
const { Context } = await load('@deepseek-ai/cordis');
const toolModule = await load('@deepseek-ai/dsh-tools');
const Tools = toolModule.default;
const ctx = new Context();
ctx.provide('systemPrompt', { tools: () => () => {} });
const runtime = ctx.plugin(Tools, {});
const work = await mkdtemp(join(tmpdir(), 'mcp-hub-dsh-runtime-'));
const previousHome = process.env.DSH_HOME;
process.env.DSH_HOME = work;
const fiber = ctx.plugin(plugin, { binary: process.env.MCP_HUB_TEST_BIN });
await new Promise(resolve => setTimeout(resolve, 50));
for (const name of ['mcp_hub_scan', 'mcp_hub_audit', 'mcp_hub_doctor']) {
  const def = ctx.tools.get(name);
  assert.ok(def, `${name} registered in the real DSH tool registry`);
  toolModule.assertSupportedJsonSchema(def.parameters);
  toolModule.assertSupportedJsonSchema(def.output.schema);
}
if (process.env.MCP_HUB_TEST_BIN) {
  await writeFile(join(work, 'cordis.patch.yml'), JSON.stringify([{ insert: [{ name: '@deepseek-ai/dsh-mcp-client',
    config: { serverName: 'fixture', command: process.env.MCP_HUB_TEST_BIN, args: ['--version'] } }] }]));
  for (const operation of ['scan', 'audit', 'doctor']) {
    const result = await ctx.tools.execute({ callId: `mcp-hub-smoke-${operation}`, name: `mcp_hub_${operation}`,
      arguments: { client: 'dsh', ...(operation === 'doctor' ? { server: 'fixture', timeout: 1 } : {}) },
      signal: new AbortController().signal, agent: { session: { header: { cwd: work } } } });
    assert.equal(result.isError, false, JSON.stringify(result));
    assert.ok(result.content.some(block => block.type === 'text' && block.text.includes(`MCP Hub ${operation}`)));
  }
}
await fiber.dispose();
assert.equal(ctx.tools.get('mcp_hub_scan'), undefined, 'unregisters on plugin disposal');
await runtime.dispose();
if (previousHome === undefined) delete process.env.DSH_HOME;
else process.env.DSH_HOME = previousHome;
await rm(work, { recursive: true, force: true });
console.log('Official DSH registry registration, schemas and disposal passed.');
