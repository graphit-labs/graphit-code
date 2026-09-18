package agent

import (
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

// Exercise the generated plugin, not just its source strings. Bun is mocked at
// the process boundary so identity, hook ordering and native output are tested
// without opening a real user's store or launching asynchronous sync.
func TestOpenCodeLifecycleForwardsNativeIdentity(t *testing.T) {
	node, err := exec.LookPath("node")
	if err != nil {
		t.Skip("Node.js required to execute the generated OpenCode plugin")
	}
	project := t.TempDir()
	if err := NewOpenCodeAdapter().syncSessionStartHook(project); err != nil {
		t.Fatal(err)
	}
	data, err := os.ReadFile(filepath.Join(project, ".opencode", "plugins", opencodeManagedHookFile))
	if err != nil {
		t.Fatal(err)
	}
	plugin := filepath.Join(project, "plugin.mjs")
	if err := os.WriteFile(plugin, data, 0o600); err != nil {
		t.Fatal(err)
	}
	harness := `
import assert from 'node:assert/strict';
import { GraphitLifecycle } from './plugin.mjs';
const calls = [];
let disabled = false;
globalThis.Bun = {
  spawnSync(args, options) {
    const payload = JSON.parse(new TextDecoder().decode(options.stdin));
    calls.push({args, options, payload});
    const format = args[3];
    const output = format === 'tool-context' ? JSON.stringify({additional_context:'restored'}) :
      format === 'plain-unit' ? (disabled ? '' : 'checkpoint') : 'bootstrap';
    return {exitCode:0, stdout:Buffer.from(output)};
  },
  spawn(args, options) {
    const payload = JSON.parse(new TextDecoder().decode(options.stdin));
    const call = {args, options, payload, detached:false};
    calls.push(call);
    return {unref(){ call.detached = true; }};
  }
};
const hooks = await GraphitLifecycle({directory:'/runtime/project'});
const system = {system:[]};
await hooks['experimental.chat.system.transform']({sessionID:'coordinator'}, system);
assert.deepEqual(system.system, ['bootstrap']);
await hooks['experimental.chat.system.transform']({sessionID:'coordinator'}, {system:[]});
assert.equal(calls.length, 1, 'ordinary turn must not repeat bootstrap');
const workerSystem = {system:[]};
await hooks['experimental.chat.system.transform']({sessionID:'worker'}, workerSystem);
const toolOutput = {output:'result'};
await hooks['tool.execute.after']({sessionID:'worker'}, toolOutput);
assert.equal(toolOutput.output, 'result\n\ncheckpoint');
disabled = true;
await hooks['tool.execute.after']({sessionID:'worker'}, {output:'unchanged'});
const suppressed = {output:'unchanged'};
await hooks['tool.execute.after']({sessionID:'worker'}, suppressed);
assert.equal(suppressed.output, 'unchanged', 'disabled Task must not fall back to a task reminder');
const compact = {context:[]};
await hooks['experimental.session.compacting']({sessionID:'coordinator'}, compact);
assert.deepEqual(compact.context, ['restored']);
await hooks.event({event:{type:'session.idle',properties:{sessionID:'worker'}}});
await hooks.event({event:{type:'session.deleted',properties:{info:{id:'coordinator'}}}});
await hooks['experimental.chat.system.transform']({sessionID:'coordinator'}, {system:[]});
await hooks.event({event:{type:'session.idle'}});
assert.deepEqual(calls.map(c=>c.payload.sessionID), ['coordinator','worker','worker','worker','worker','coordinator','worker','coordinator','coordinator',undefined]);
for (const call of calls) {
  assert.equal(call.options.cwd, '/runtime/project');
  assert.equal(call.payload.cwd, '/runtime/project');
  assert.ok(!call.args.some(a=>a.includes('complete')), 'native stop must not complete a durable session');
  if (call.args.includes('--sync')) assert.equal(call.detached, true);
}
` // Missing native ID stays absent; it never borrows the last active session.
	script := filepath.Join(project, "harness.mjs")
	if err := os.WriteFile(script, []byte(harness), 0o600); err != nil {
		t.Fatal(err)
	}
	if output, err := exec.Command(node, script).CombinedOutput(); err != nil {
		t.Fatalf("generated OpenCode lifecycle failed: %v\n%s", err, output)
	}
}
