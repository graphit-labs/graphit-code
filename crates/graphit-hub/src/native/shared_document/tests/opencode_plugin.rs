use super::*;
use crate::native::opencode_plugin::{
    OpenCodePluginInput, OpenCodePluginPlan, PluginPublicationError, publish_opencode_plugin,
    remove_opencode_plugin, render_opencode_plugin,
};
use graphit_core::artifact_store::ArtifactChange;
use std::process::Command;

fn directory_limits() -> NativeDocumentFileLimits {
    NativeDocumentFileLimits::new(64 * 1024, 16 * 1024, 128, 64 * 1024).unwrap()
}
fn peer(f: &Fixture, mode: &str) -> Command {
    if mode == "missing" {
        return Command::new(f.root.join("missing-graphit-executable"));
    }
    let path = f.root.join("peer.mjs");
    fs::write(
        &path,
        r#"
let bytes = '';
for await (const chunk of process.stdin) bytes += chunk;
const input = JSON.parse(bytes);
if (input.cwd !== process.cwd() || !input.hook_event_name) process.exit(9);
const mode = process.argv[2];
if (mode === 'slow') {
  process.on('SIGTERM', () => {});
  process.stdout.write('incomplete');
  const { writeFile } = await import('node:fs/promises');
  await writeFile('peer-ready', String(process.pid));
  await new Promise(resolve => setTimeout(resolve, 10000));
}
if (mode === 'lifecycle') {
  const { writeFile } = await import('node:fs/promises');
  await writeFile(input.hook_event_name, 'operational');
  // Keep idle work pending so deletion must cancel it before its own command runs.
  if (input.hook_event_name === 'session.idle') await new Promise(resolve=>setTimeout(resolve,10000));
}
if (mode === 'failure') { process.stdout.write('incomplete'); process.exit(7); }
if (mode === 'oversize') process.stdout.write('x'.repeat(65536));
else process.stdout.write('runtime-context:' + input.hook_event_name + ':' + process.argv[3]);
"#,
    )
    .unwrap();
    let mut command = Command::new("node");
    command.arg(path).arg(mode).arg("literal '$`\\\"; $(false)");
    command
}
fn plan(
    f: &Fixture,
    mode: &str,
    timeout: Duration,
    request: &RequestContext<'_>,
) -> OpenCodePluginPlan {
    let system = peer(f, mode);
    let compacting = peer(f, "normal");
    render_opencode_plugin(
        OpenCodePluginInput {
            identifier: "graphit-context",
            context: &system,
            compaction: &compacting,
            idle: None,
            deleted: None,
            timeout,
            max_input_bytes: 8192,
            max_output_bytes: 8192,
        },
        request,
    )
    .unwrap()
}
fn publish(f: &Fixture, plan: &OpenCodePluginPlan, request: &RequestContext<'_>) -> ArtifactChange {
    publish_opencode_plugin(&f.core, &f.root, plan, directory_limits(), request, || {
        Ok(())
    })
    .unwrap()
}
fn run_node(f: &Fixture, script: &str) -> serde_json::Value {
    let file = f.root.join("exercise.mjs");
    let prefix = format!(
        r#"import {{ pathToFileURL }} from 'node:url';
import {{ writeFile }} from 'node:fs/promises';
const directory = {};
await writeFile(directory + '/package.json', '{{"type":"module"}}');
const module = await import(pathToFileURL({}));
const hooks = {{}};
const queue = [];
let wake;
let streamStopped = false;
let delivered = 0;
let lookupStarted = false, lookupAborted = false;
let subscriptionFailure;
const ctx = {{
  location:{{directory}},
  session:{{
    get: async ({{sessionID}}, {{signal}}) => {{
      signal.throwIfAborted();
      if (sessionID.startsWith('lookup-')) {{
        lookupStarted = true;
        return await new Promise((resolve,reject) => {{
          signal.addEventListener('abort',()=>{{lookupAborted=true;reject(signal.reason);}},{{once:true}});
        }});
      }}
      return {{id:sessionID,location:{{directory:sessionID==='foreign'?directory+'/other':directory}}}}; }},
    hook: async (name, callback) => {{ hooks[name]=callback; return {{dispose:()=>{{delete hooks[name];}}}}; }},
  }},
  event:{{subscribe:async ({{signal}})=>{{
    signal.addEventListener('abort',()=>{{streamStopped=true;wake?.();}},{{once:true}});
    return {{async *[Symbol.asyncIterator](){{
      while(!streamStopped){{
        if(!queue.length) await new Promise(resolve=>{{wake=resolve;}});
        if(subscriptionFailure) throw subscriptionFailure;
        while(queue.length){{ yield queue.shift(); delivered++; }}
      }}
    }}}};
  }}}},
}};
const emit = async (event) => {{
  const target = delivered + queue.length + 1;
  queue.push(event);wake?.();
  while(delivered < target) await new Promise(resolve=>setImmediate(resolve));
}};
const cleanup = await module.default.setup(ctx);
if(module.default.id !== 'graphit-context' || typeof cleanup !== 'function') throw new Error('Invalid Plugin V2 export');
"#,
        serde_json::to_string(&f.root).unwrap(),
        serde_json::to_string(&f.root.join(".opencode/plugins/graphit-context.js")).unwrap()
    );
    fs::write(&file, prefix + script).unwrap();
    let result = Command::new("node")
        .arg(file)
        .current_dir(&f.root)
        .env_remove("NODE_OPTIONS")
        .env_remove("NODE_PATH")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "Node callback fixture failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}

#[test]
fn owned_plugin_executes_native_callbacks_with_literal_argv_and_preserves_arrays() {
    let cancel = Cancellation::default();
    let request = request(&cancel);
    let f = Fixture::new(&request);
    let mut oversized = Command::new("node");
    oversized.arg("x".repeat(request.content_limits().max_scalar_bytes + 1));
    let rejected = render_opencode_plugin(
        OpenCodePluginInput {
            identifier: "graphit-context",
            context: &oversized,
            compaction: &oversized,
            idle: None,
            deleted: None,
            timeout: Duration::from_secs(3),
            max_input_bytes: 8192,
            max_output_bytes: 8192,
        },
        &request,
    );
    assert!(matches!(rejected, Err(error) if error.kind() == io::ErrorKind::FileTooLarge));
    let context_command = peer(&f, "normal");
    let compaction_command = peer(&f, "normal");
    let lifecycle = peer(&f, "lifecycle");
    let plan = render_opencode_plugin(
        OpenCodePluginInput {
            identifier: "graphit-context",
            context: &context_command,
            compaction: &compaction_command,
            idle: Some(&lifecycle),
            deleted: Some(&lifecycle),
            timeout: Duration::from_secs(3),
            max_input_bytes: 8192,
            max_output_bytes: 8192,
        },
        &request,
    )
    .unwrap();
    assert!(!String::from_utf8_lossy(plan.bytes()).contains("runtime-context:"));
    assert_eq!(publish(&f, &plan, &request), ArtifactChange::Created);
    assert_eq!(publish(&f, &plan, &request), ArtifactChange::Unchanged);
    let output = run_node(
        &f,
        r#"
import { access, readFile } from 'node:fs/promises';
const messages = [{role:'user',parts:[]}], tools = {foreign:{}}, options = {foreign:true};
const system = {sessionID:'host-only',system:[{type:'text',text:'foreign-system'}],messages,tools,options};
const compaction = {sessionID:'host-only',system:[{type:'text',text:'foreign-summary'}],messages,tools,options};
await hooks.context(system);
await hooks.compaction(compaction);
let unavailable;
try { await hooks.context({sessionID:'foreign',system:[]}); }
catch(error) { unavailable = error.code; }
if(system.messages !== messages || system.tools !== tools || system.options !== options) throw new Error('Foreign context changed');
const override = {sessionID:'host-only',system:[],result:{summary:'foreign override'}};
try { await hooks.compaction(override); throw new Error('Override accepted'); }
catch(error) { if(error.code !== 'CONTEXT_UNAVAILABLE' || override.result.summary !== 'foreign override' || override.system.length) throw error; }
await emit({type:'session.idle',data:{sessionID:'other-project'},location:{directory:directory+'/other'}});
let foreignRan=false;
try { await access('session.idle'); foreignRan=true; } catch {}
if(foreignRan) throw new Error('Foreign location executed');
for(const type of ['session.idle','session.deleted']) {
  await emit({type,data:{sessionID:'host-only'},location:{directory}});
  let complete=false;
  for(let n=0;n<200;n++) {
    try { if(await readFile(type,'utf8')==='operational'){complete=true;break;} } catch {}
    await new Promise(resolve=>setTimeout(resolve,10));
  }
  if(!complete) throw new Error('Lifecycle command did not execute');
}
await cleanup();
console.log(JSON.stringify({system:system.system, context:compaction.system, unavailable}));
"#,
    );
    assert_eq!(output["system"][0]["text"], "foreign-system");
    assert_eq!(output["context"][0]["text"], "foreign-summary");
    assert_eq!(
        output["system"][1]["text"],
        "runtime-context:context:literal '$`\\\"; $(false)"
    );
    assert_eq!(
        output["context"][1]["text"],
        "runtime-context:compaction:literal '$`\\\"; $(false)"
    );
    assert_eq!(output["unavailable"], "LOCATION_UNAVAILABLE");
}

#[test]
fn plugin_nonzero_timeout_and_output_limit_never_apply_partial_context() {
    for (mode, timeout) in [
        ("missing", Duration::from_secs(3)),
        ("failure", Duration::from_secs(3)),
        ("oversize", Duration::from_secs(3)),
        ("slow", Duration::from_millis(200)),
    ] {
        let cancel = Cancellation::default();
        let request = request(&cancel);
        let f = Fixture::new(&request);
        let plan = plan(&f, mode, timeout, &request);
        publish(&f, &plan, &request);
        let output = run_node(
            &f,
            r#"
const output = {sessionID:'A',system:['foreign']};
let failed = false, code, signal;
try { await hooks.context(output); }
catch(error) { failed = true; code = error.code; signal = error.signal; }
await cleanup();
console.log(JSON.stringify({failed,code,signal,system:output.system}));
"#,
        );
        assert_eq!(output["failed"], true);
        assert_eq!(output["system"], serde_json::json!(["foreign"]));
        if mode == "failure" {
            assert_eq!(output["code"], 7);
        } else if mode == "missing" {
            assert_eq!(output["code"], "ENOENT");
        }
    }
    {
        let cancel = Cancellation::default();
        let request = request(&cancel);
        let f = Fixture::new(&request);
        let plan = plan(&f, "normal", Duration::from_millis(200), &request);
        publish(&f, &plan, &request);
        let output = run_node(
            &f,
            r#"
const output = {sessionID:'lookup-deadline',system:['foreign']};
let code;
const started = performance.now();
try { await hooks.context(output); } catch(error) { code=error.code; }
const elapsedMs=performance.now()-started;
await cleanup();
console.log(JSON.stringify({code,elapsedMs,lookupStarted,lookupAborted,system:output.system}));
"#,
        );
        assert_eq!(output["code"], "ETIMEDOUT");
        assert_eq!(output["lookupStarted"], true);
        assert_eq!(output["lookupAborted"], true);
        assert_eq!(output["system"], serde_json::json!(["foreign"]));
        assert!(output["elapsedMs"].as_f64().unwrap() < 3000.0);
    }
    {
        let cancel = Cancellation::default();
        let request = request(&cancel);
        let f = Fixture::new(&request);
        let context_command = peer(&f, "normal");
        let idle_command = peer(&f, "slow");
        let plan = render_opencode_plugin(
            OpenCodePluginInput {
                identifier: "graphit-context",
                context: &context_command,
                compaction: &context_command,
                idle: Some(&idle_command),
                deleted: None,
                timeout: Duration::from_secs(5),
                max_input_bytes: 8192,
                max_output_bytes: 8192,
            },
            &request,
        )
        .unwrap();
        publish(&f, &plan, &request);
        let output = run_node(
            &f,
            r#"
import { readFile } from 'node:fs/promises';
await emit({type:'session.idle',data:{sessionID:'A'},location:{directory}});
let pid;
for(let n=0;n<200;n++) {
  try { pid=Number(await readFile('peer-ready','utf8')); if(pid>0) break; } catch {}
  await new Promise(resolve=>setTimeout(resolve,10));
}
if(!(pid>0)) throw new Error('Lifecycle peer did not start');
subscriptionFailure=Object.assign(new Error('Stream unavailable'),{code:'STREAM_LOST'});
wake?.();
let exited=false;
for(let n=0;n<200;n++) {
  try { process.kill(pid,0); }
  catch(error) { if(error.code==='ESRCH') { exited=true; break; } throw error; }
  await new Promise(resolve=>setTimeout(resolve,10));
}
if(!exited) throw new Error('Lifecycle peer did not exit after stream failure');
// Let the native child-close phase and its promise handlers settle before cleanup.
await new Promise(resolve=>setImmediate(resolve));
await new Promise(resolve=>setImmediate(resolve));
let code;
try { await cleanup(); } catch(error) { code=error.code; }
console.log(JSON.stringify({code,exited}));
"#,
        );
        assert_eq!(output["code"], "STREAM_LOST");
        assert_eq!(output["exited"], true);
    }
}

#[test]
fn session_deletion_and_dispose_cancel_children_without_changing_native_outputs() {
    for action_kind in ["deleting", "aborting", "disposing", "lookup-cancelling"] {
        let cancel = Cancellation::default();
        let request = request(&cancel);
        let f = Fixture::new(&request);
        let plan = plan(&f, "slow", Duration::from_secs(5), &request);
        publish(&f, &plan, &request);
        let lookup = action_kind == "lookup-cancelling";
        let action = match action_kind {
            "deleting" => {
                "await emit({type:'session.deleted',data:{sessionID:'A'},location:{directory}});"
            }
            "aborting" => {
                "await emit({type:'session.execution.interrupted',data:{sessionID:'A',reason:'user'},location:{directory}});"
            }
            "lookup-cancelling" => {
                "await emit({type:'session.execution.interrupted',data:{sessionID:'lookup-A',reason:'user'},location:{directory}});"
            }
            _ => "await cleanup();",
        };
        let script = format!(
            r#"
import {{ access }} from 'node:fs/promises';
const output = {{sessionID:{lookup}?'lookup-A':'A',system:['foreign']}};
let cancellationCode;
const pending = hooks.context(output).then(() => false, error => {{ cancellationCode = error.code; return true; }});
let ready = false;
for (let n=0; n<200; n++) {{
  if ({lookup} && lookupStarted) {{ ready=true; break; }}
  try {{ await access('peer-ready'); ready=true; break; }} catch {{}}
  await new Promise(resolve=>setTimeout(resolve,10));
}}
if (!ready) throw new Error('Peer did not start');
const cancellingAt = performance.now();
{action}
const failed = await pending;
const cancellationMs = performance.now() - cancellingAt;
const compacting = {{sessionID:'B',system:['foreign']}};
let otherCompleted = false;
try {{ await hooks.compaction?.(compacting); if (!hooks.compaction) throw new Error('disposed'); otherCompleted=true; }} catch {{}}
await cleanup();
console.log(JSON.stringify({{failed,cancellationCode,cancellationMs,system:output.system,otherCompleted,context:compacting.system,lookupAborted}}));
"#
        );
        let output = run_node(&f, &script);
        assert_eq!(output["failed"], true);
        assert_eq!(output["system"], serde_json::json!(["foreign"]));
        assert_eq!(output["otherCompleted"], action_kind != "disposing");
        assert_eq!(output["cancellationCode"], "ABORT_ERR");
        if lookup {
            assert_eq!(output["lookupAborted"], true);
        }
        // The configured 5s deadline also bounds explicit cancellation.
        assert!(output["cancellationMs"].as_f64().unwrap() < 5000.0);
    }
}

#[test]
fn equal_foreign_plugin_is_not_adopted_and_owned_removal_preserves_other_plugins() {
    let cancel = Cancellation::default();
    let request = request(&cancel);
    let f = Fixture::new(&request);
    let plan = plan(&f, "normal", Duration::from_secs(3), &request);
    let destination = f.root.join(plan.relative_entry());
    fs::create_dir_all(destination.parent().unwrap()).unwrap();
    fs::write(&destination, plan.bytes()).unwrap();
    assert!(
        publish_opencode_plugin(
            &f.core,
            &f.root,
            &plan,
            directory_limits(),
            &request,
            || Ok(())
        )
        .is_err()
    );
    assert_eq!(fs::read(&destination).unwrap(), plan.bytes());
    fs::remove_file(&destination).unwrap();
    publish(&f, &plan, &request);
    fs::write(
        destination.parent().unwrap().join("foreign.mjs"),
        "export default async()=>({});",
    )
    .unwrap();
    assert_eq!(
        remove_opencode_plugin(&f.core, plan.identifier(), &request, || Ok(())).unwrap(),
        ArtifactChange::Removed
    );
    assert!(!destination.exists());
    assert!(destination.parent().unwrap().join("foreign.mjs").exists());
}

#[test]
fn plugin_reports_an_effect_before_post_publication_guard_failure_and_recovers() {
    let cancel = Cancellation::default();
    let request = request(&cancel);
    let f = Fixture::new(&request);
    let plan = plan(&f, "normal", Duration::from_secs(3), &request);
    let destination = f.root.join(plan.relative_entry());
    let error = publish_opencode_plugin(
        &f.core,
        &f.root,
        &plan,
        directory_limits(),
        &request,
        || {
            if destination.exists() {
                Err(io::ErrorKind::PermissionDenied.into())
            } else {
                Ok(())
            }
        },
    )
    .unwrap_err();
    assert!(
        matches!(error, PluginPublicationError::AfterPublication { change:ArtifactChange::Created, cause } if cause.kind()==io::ErrorKind::PermissionDenied)
    );
    assert_eq!(fs::read(&destination).unwrap(), plan.bytes());
    assert_eq!(publish(&f, &plan, &request), ArtifactChange::Unchanged);
    remove_opencode_plugin(&f.core, plan.identifier(), &request, || Ok(())).unwrap();
}
