// Managed by Graphit: deterministic session-start memory protocol
const initializedSessions = new Set()

export const GraphitLifecycle = async ({ directory }) => {
  const invariant = "Graphit invariant: reapply module routing before every new/resumed action. Read matching skills before first use; reuse per target/overrides, reload on their change; reload after compaction if lost. Cluster neighbors remain Graphit-managed: use their returned `dir` as `project_dir` for target MCP reads, not native discovery. Use Graphit MCP before native equivalents; `ai_optimized: true` when supported. If a required tool is unavailable, use default native tools, never the Graphit CLI. With Task enabled, resume durable session/task state; revise changed intent before acting; close delivered sessions explicitly. New questions trigger needed Memory/Task recall; reuse evidence. `project_dir` is call-local; persist identity and relative paths, never checkout roots."
  const nativeInput = (sessionID) => new TextEncoder().encode(JSON.stringify({ cwd: directory, sessionID }))
  const runHook = (format, sessionID) => Bun.spawnSync(["graphit", "_session-hook", "--format", format], { cwd: directory, stdin: nativeInput(sessionID) })
  const loadBootstrap = (sessionID) => {
    let bootstrap = "Graphit invariant: reapply module routing before every new/resumed action. Read matching skills before first use; reuse per target/overrides, reload on their change; reload after compaction if lost. Cluster neighbors remain Graphit-managed: use their returned `dir` as `project_dir` for target MCP reads, not native discovery. Use Graphit MCP before native equivalents; `ai_optimized: true` when supported. If a required tool is unavailable, use default native tools, never the Graphit CLI. With Task enabled, resume durable session/task state; revise changed intent before acting; close delivered sessions explicitly. New questions trigger needed Memory/Task recall; reuse evidence. `project_dir` is call-local; persist identity and relative paths, never checkout roots.\nIf module routing was not injected, use `graphit_mandates` once when available. Read matching installed skills first; fetch a missing skill with `graphit_module_skill`. Project instructions remain authoritative.\nGraphit session bootstrap:\n1. If available, read `graphit-memory`; call `graphit_memory_mandatory` with `ai_optimized: true` once per missing available scope (`project` for a resolved project, and `user`). Consume all standing context before acting.\n2. Whenever a question about the system, rationale or learned behavior needs context, including during work, read `graphit-memory`; query `graphit_memory_search` with `exclude_mandatory: true`, `top_k: 5`, `ai_optimized: true`, and the decision topic. Read selected ids with `graphit_memory_source`. Reuse sufficient context; new questions can require recall in the same session and scope.\n3. Read `graphit-task` before session work. Use `graphit_task_session_list` for active sessions and `graphit_task_session_search` for relevant history; `graphit_task_session_get` reads the chosen description, strategy, checkpoints and linked task IDs. Then decide between continuing one and opening a new one: an open session whose demand this request continues is the one to resume, even across interruption or compaction; changed scope revises it rather than starting a second; only a different demand justifies a new session. With no match, `graphit_task_session_create` records the detailed user request, scope, constraints and strategy, then claim coordination. Link all agent-created tasks to that durable session_id; native host session IDs are not Graphit session IDs. Delegated workers receive session_id and task IDs, read them, claim only their task and never take or close the coordinator's session.\n4. At meaningful progress, problems or decisions the coordinator calls `graphit_task_session_checkpoint` with evidence, rationale, strategy and exact next step. For added requests or changed direction, use `graphit_task_session_revise` before affected work and reconcile its tasks. Before reporting delivery, explicitly `graphit_task_session_complete` with a final summary after linked tasks are terminal; explain cancelled scope. Interrupted work needs a descriptive checkpoint and explicit release, not completion. Stop hooks never close sessions; absent or uncorrelated host identity cannot release ownership safely, so do not rely on them for handoff.\n5. Before project work, read `graphit-task`; search `graphit_task_search` with `top_k: 5`, `ai_optimized: true`, focused on this request, or get an assigned id directly with `graphit_task_get`. Read the chosen task, parent specification and relevant dependencies/precedents. During work, new doubts also trigger focused search of prior investigations, decisions and evidence; do not wait for restart or a new plan. Follow `next_cursor` only while a relevant gap remains. Reuse recalled context.\n6. For multi-step work, persist specification, acceptance criteria, plan and dependency-ordered tasks before execution. Resume from recorded progress/evidence; revise affected tasks when scope changes. A single umbrella task is not an executable project plan."
    try {
      const result = runHook("plain-context", sessionID)
      if (result.exitCode === 0) bootstrap = result.stdout.toString().trim() || bootstrap
    } catch {}
    return bootstrap
  }
  const dispatchFinalSync = (sessionID) => {
    try {
      const subprocess = Bun.spawn(["graphit", "_session-hook", "--format", "no-output", "--sync"], { cwd: directory, stdin: nativeInput(sessionID), stdout: "ignore", stderr: "ignore" })
      subprocess.unref()
    } catch {}
  }
  return {
    event: async ({ event }) => {
      if (event.type === "session.idle") dispatchFinalSync(event.properties?.sessionID)
      if (event.type === "session.deleted") {
        const sessionID = event.properties?.info?.id
        dispatchFinalSync(sessionID)
        initializedSessions.delete(sessionID)
      }
    },
    "tool.execute.after": async (input, output) => {
      let reminder = "Graphit task checkpoint (if enabled): after meaningful work use `graphit_task_progress`; coordinator: `graphit_task_session_checkpoint` with evidence, problems, decisions, strategy, next step. Reads/bookkeeping alone need none. Revise changed intent; close delivered sessions explicitly. Verify acceptance checks and code/documentation consistency in both directions; record targets/evidence. Resolve divergence before closing."
      try {
        const result = runHook("plain-unit", input.sessionID)
        if (result.exitCode === 0) reminder = result.stdout.toString().trim()
      } catch {}
      if (reminder && typeof output.output === "string" && !output.output.includes(reminder)) output.output += "\n\n" + reminder
    },
    "experimental.chat.system.transform": async (input, output) => {
      if (!input.sessionID) return
      const context = initializedSessions.has(input.sessionID) ? invariant : loadBootstrap(input.sessionID)
      initializedSessions.add(input.sessionID)
      if (output.system.length === 0) output.system.push(context)
      else output.system.splice(0, 1, output.system[0] + "\n\n" + context)
    },
    "experimental.session.compacting": async (input, output) => {
      let compactContext = invariant
      try {
        const result = runHook("tool-context", input.sessionID)
        if (result.exitCode === 0) {
          const parsed = JSON.parse(result.stdout.toString())
          compactContext = parsed.additional_context || compactContext
        }
      } catch {}
      output.context.push(compactContext)
    },
  }
}
