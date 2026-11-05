import assert from 'node:assert/strict';
import { describe, test } from 'node:test';
import { mkdtemp, readFile, realpath, rm, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

// Exercise the production runtime, rather than a second implementation or a
// provider installation. Rust separately checks the producer's configuration
// envelope and exact inclusion of this template.
const template = await readFile(new URL('../crates/graphit-hub/src/native/opencode_plugin.mjs', import.meta.url), 'utf8');
const literal = "literal '$`\\\"; $(false), espaços é";
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};

async function fixture(t, { mode = 'normal', timeout = 5000, idle, deleted, maxInput = 8192 } = {}) {
  const directory = await realpath(await mkdtemp(join(tmpdir(), 'graphit plugin é ')));
  const connections = new Set(), children = new Set(), arrivals = new Map(), waiting = new Map();
  let cleanup;
  t.after(async () => {
    try { await cleanup?.(); } finally {
      for (const pid of children) {
        try { process.kill(pid, 'SIGKILL'); } catch (error) { if (error.code !== 'ESRCH') throw error; }
      }
      for (const socket of connections) socket.destroy();
      await new Promise(resolve => server.close(resolve));
      await rm(directory, { recursive: true, force: true });
    }
  });
  const server = createServer(socket => {
    connections.add(socket);
    const closed = deferred();
    let pid;
    socket.once('close', () => { connections.delete(socket); children.delete(pid); closed.resolve(); });
    let input = '';
    socket.on('data', chunk => {
      input += chunk;
      const line = input.indexOf('\n');
      if (line < 0) return;
      const ready = JSON.parse(input.slice(0, line));
      ready.closed = closed.promise;
      input = input.slice(line + 1);
      pid = ready.pid;
      children.add(pid);
      const pending = waiting.get(ready.event);
      if (pending) { waiting.delete(ready.event); pending.resolve(ready); }
      else arrivals.set(ready.event, ready);
    });
  });
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  const port = server.address().port;
  const peer = join(directory, 'peer.mjs');
  await writeFile(peer, `
import { createConnection } from 'node:net';
let bytes = '';
for await (const chunk of process.stdin) bytes += chunk;
const input = JSON.parse(bytes);
if (input.cwd !== process.cwd() || !input.hook_event_name) process.exit(9);
const mode = process.argv[2];
if (mode === 'slow' || mode === 'lifecycle') {
  const socket = createConnection({host:'127.0.0.1',port:Number(process.argv[4])});
  await new Promise((resolve,reject)=>{socket.once('connect',resolve);socket.once('error',reject);});
  socket.write(JSON.stringify({event:input.hook_event_name,pid:process.pid})+'\\n');
  if (mode === 'slow' || input.hook_event_name === 'session.idle') {
    process.on('SIGTERM',()=>{});
    process.stdout.write('incomplete');
    // The live socket is the readiness barrier and keeps the peer alive until
    // the plugin cancels it. No arbitrary timeout creates the pending state.
    await new Promise(resolve=>socket.once('close',resolve));
  } else socket.end();
}
if (mode === 'failure') { process.stdout.write('incomplete'); process.exit(7); }
if (mode === 'oversize') process.stdout.write('x'.repeat(65536));
else if (mode === 'invalid-encoding') process.stdout.write(Buffer.from([0xff]));
else if (mode !== 'empty') process.stdout.write('runtime-context:'+input.hook_event_name+':'+process.argv[3]);
`);
  const command = kind => ({ program: kind === 'missing' ? join(directory, 'missing-executable') : process.execPath, args: [peer, kind, literal, String(port)] });
  const configuration = {
    identifier: 'graphit-context', context: command(mode), compaction: command('normal'),
    idle: idle ? command(idle) : null, deleted: deleted ? command(deleted) : null,
    timeout, max_input_bytes: maxInput, max_output_bytes: 8192,
  };
  const file = join(directory, 'plugin.mjs');
  await writeFile(file, `import { execFile } from 'node:child_process';\nimport { resolve } from 'node:path';\nconst configuration = ${JSON.stringify(configuration)};\n${template}`);
  const plugin = (await import(pathToFileURL(file))).default;
  const hooks = {}, queue = [];
  const lookup = deferred();
  let lookupAborted = false, streamStopped = false, streamFailure, waking;
  const ctx = {
    location: { directory },
    session: {
      get: async ({sessionID}, {signal}) => {
        signal.throwIfAborted();
        if (sessionID.startsWith('lookup-')) {
          lookup.resolve();
          return new Promise((resolve, reject) => {
            signal.addEventListener('abort', () => { lookupAborted = true; reject(signal.reason); }, {once: true});
          });
        }
        return {id: sessionID, location: {directory: sessionID === 'foreign' ? join(directory, 'other') : directory}};
      },
      hook: async (name, callback) => {
        hooks[name] = callback;
        return {dispose: () => { delete hooks[name]; }};
      },
    },
    event: {subscribe: async ({signal}) => {
      signal.addEventListener('abort', () => { streamStopped = true; waking?.(); }, {once: true});
      return {async *[Symbol.asyncIterator]() {
        while (!streamStopped) {
          if (!queue.length && !streamFailure) await new Promise(resolve => { waking = resolve; });
          if (streamFailure) throw streamFailure;
          while (queue.length) {
            const entry = queue.shift();
            yield entry.event;
            entry.done.resolve();
          }
        }
      }};
    }},
  };
  cleanup = await plugin.setup(ctx);
  assert.equal(plugin.id, 'graphit-context');
  assert.equal(typeof cleanup, 'function');
  return {
    directory, hooks, lookup: lookup.promise, lookupAborted: () => lookupAborted,
    ready(event) {
      if (arrivals.has(event)) { const value = arrivals.get(event); arrivals.delete(event); return Promise.resolve(value); }
      assert.equal(waiting.has(event), false, 'only one readiness waiter per event');
      const pending = deferred(); waiting.set(event, pending); return pending.promise;
    },
    async emit(type, sessionID = 'A', location = directory) {
      const done = deferred(); queue.push({event: {type, data: {sessionID}, location: {directory: location}}, done});
      waking?.(); await done.promise;
    },
    failStream() { streamFailure = Object.assign(new Error('stream unavailable'), {code:'STREAM_LOST'}); waking?.(); },
    async dispose() { await cleanup(); },
    // Avoid reporting the same known failure twice from t.after.
    async disposeFailed() { const current = cleanup; cleanup = undefined; await current(); },
  };
}

const options = { timeout: 10000 };
describe('OpenCode 2 production plugin runtime', { concurrency: true }, () => {
  test('context/compaction preserve foreign arrays, literal argv and location boundaries', options, async t => {
    const f = await fixture(t, {idle:'lifecycle', deleted:'lifecycle'});
    const messages = [{role:'user',parts:[]}], tools = {foreign:{}}, extra = {foreign:true};
    const system = {sessionID:'host-only',system:[{type:'text',text:'foreign-system'}],messages,tools,options:extra};
    const compacting = {sessionID:'host-only',system:[{type:'text',text:'foreign-summary'}],messages,tools,options:extra};
    await f.hooks.context(system);
    await f.hooks.compaction(compacting);
    assert.deepEqual(system.system, [{type:'text',text:'foreign-system'},{type:'text',text:'runtime-context:context:'+literal}]);
    assert.deepEqual(compacting.system, [{type:'text',text:'foreign-summary'},{type:'text',text:'runtime-context:compaction:'+literal}]);
    for (const output of [system, compacting]) {
      assert.equal(output.messages, messages); assert.equal(output.tools, tools); assert.equal(output.options, extra);
    }
    await assert.rejects(f.hooks.context({sessionID:'foreign',system:[]}), {code:'LOCATION_UNAVAILABLE'});
    const override = {sessionID:'host-only',system:[],result:{summary:'foreign override'}};
    await assert.rejects(f.hooks.compaction(override), {code:'CONTEXT_UNAVAILABLE'});
    assert.deepEqual(override, {sessionID:'host-only',system:[],result:{summary:'foreign override'}});
    await f.emit('session.idle', 'foreign', join(f.directory, 'other'));
    await f.emit('session.idle', 'host-only');
    await f.ready('session.idle');
    await f.emit('session.deleted', 'host-only');
    await f.ready('session.deleted');
  });

  for (const [mode, expected] of [['missing','ENOENT'],['failure',7],['oversize','ERR_CHILD_PROCESS_STDIO_MAXBUFFER'],['invalid-encoding','INVALID_ENCODING'],['empty','EMPTY_CONTEXT']]) {
    test(`${mode} never applies partial context`, options, async t => {
      const f = await fixture(t, {mode});
      const output = {sessionID:'A',system:['foreign']};
      await assert.rejects(f.hooks.context(output), {code:expected});
      assert.deepEqual(output.system, ['foreign']);
    });
  }

  test('timeout terminates a started peer without applying its partial output', options, async t => {
    const f = await fixture(t, {mode:'slow',timeout:2000});
    const output = {sessionID:'A',system:['foreign']};
    const pending = f.hooks.context(output);
    const rejected = assert.rejects(pending, {code:'ETIMEDOUT'});
    const {pid} = await f.ready('context');
    await rejected;
    assert.deepEqual(output.system, ['foreign']);
    assert.throws(() => process.kill(pid, 0), {code:'ESRCH'});
  });

  test('session lookup has the same deadline and is aborted', options, async t => {
    const f = await fixture(t, {timeout:100});
    const output = {sessionID:'lookup-deadline',system:['foreign']};
    const rejected = assert.rejects(f.hooks.context(output), {code:'ETIMEDOUT'});
    await f.lookup;
    await rejected;
    assert.equal(f.lookupAborted(), true);
    assert.deepEqual(output.system, ['foreign']);
  });

  test('stream failure terminates lifecycle child and is reported during cleanup', options, async t => {
    const f = await fixture(t, {idle:'slow'});
    await f.emit('session.idle');
    const {pid, closed} = await f.ready('session.idle');
    f.failStream();
    await closed;
    await assert.rejects(f.disposeFailed(), {code:'STREAM_LOST'});
    assert.throws(() => process.kill(pid, 0), {code:'ESRCH'});
  });

  for (const action of ['deleting','aborting','disposing','lookup-cancelling']) {
    test(`${action} cancels only the relevant operation`, options, async t => {
      const f = await fixture(t, {mode:'slow'});
      const lookup = action === 'lookup-cancelling';
      const output = {sessionID:lookup ? 'lookup-A' : 'A',system:['foreign']};
      const rejected = assert.rejects(f.hooks.context(output), {code:'ABORT_ERR'});
      const ready = lookup ? (await f.lookup, undefined) : await f.ready('context');
      if (action === 'disposing') await f.dispose();
      else await f.emit(action === 'deleting' ? 'session.deleted' : 'session.execution.interrupted', output.sessionID);
      await rejected;
      assert.deepEqual(output.system, ['foreign']);
      if (lookup) assert.equal(f.lookupAborted(), true);
      else assert.throws(() => process.kill(ready.pid, 0), {code:'ESRCH'});
      if (action === 'disposing') assert.equal(f.hooks.compaction, undefined);
      else {
        const compacting = {sessionID:'B',system:['foreign']};
        await f.hooks.compaction(compacting);
        assert.deepEqual(compacting.system, ['foreign',{type:'text',text:'runtime-context:compaction:'+literal}]);
      }
    });
  }

  test('invalid context and input limits reject without mutating output', options, async t => {
    const f = await fixture(t, {maxInput:1});
    await assert.rejects(f.hooks.context({sessionID:'A',system:[]}), {code:'INPUT_LIMIT'});
    await assert.rejects(f.hooks.context({sessionID:'',system:[]}), {code:'INVALID_EVENT'});
    await assert.rejects(f.hooks.context({sessionID:'A',system:'wrong'}), {code:'INVALID_OUTPUT'});
  });
});
