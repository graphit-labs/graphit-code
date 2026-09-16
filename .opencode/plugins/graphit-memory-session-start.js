// Managed by Graphit: deterministic session-start memory protocol
const initializedSessions = new Set()

export const GraphitLifecycle = async ({ directory }) => {
  const invariant = "Graphit invariant: reapply module routing before every new/resumed action. Read only the matching skill before first use; reuse it while present, reload after compaction if lost. Use Graphit MCP before native equivalents, with `ai_optimized: true` when supported. If a required tool is unavailable, use default native tools, never the Graphit CLI. Resume durable task state; throughout work, new questions trigger needed enabled Memory/Task recall. Reuse sufficient context. `project_dir` is call-local; persist project identity and relative paths, never a machine-specific checkout root."
  const loadBootstrap = () => {
    let bootstrap = "Graphit invariant: reapply module routing before every new/resumed action. Read only the matching skill before first use; reuse it while present, reload after compaction if lost. Use Graphit MCP before native equivalents, with `ai_optimized: true` when supported. If a required tool is unavailable, use default native tools, never the Graphit CLI. Resume durable task state; throughout work, new questions trigger needed enabled Memory/Task recall. Reuse sufficient context. `project_dir` is call-local; persist project identity and relative paths, never a machine-specific checkout root.\nIf module routing was not injected, use `graphit_mandates` once when available. Read matching installed skills first; fetch a missing skill with `graphit_module_skill`. Project instructions remain authoritative.\nGraphit session bootstrap:\n1. If available, read `graphit-memory`; call `graphit_memory_mandatory` with `ai_optimized: true` once per missing available scope (`project` for a resolved project, and `user`). Consume all standing context before acting.\n2. Whenever a question about the system, rationale or learned behavior needs context, including during work, read `graphit-memory`; query `graphit_memory_search` with `exclude_mandatory: true`, `top_k: 5`, `ai_optimized: true`, and the decision topic. Read selected ids with `graphit_memory_source`. Reuse sufficient context; new questions can require recall in the same session and scope.\n3. Before project work, read `graphit-task`; search `graphit_task_search` with `top_k: 5`, `ai_optimized: true`, focused on this request, or get an assigned id directly with `graphit_task_get`. Read the chosen task, parent specification and relevant dependencies/precedents. During work, new doubts also trigger focused search of prior investigations, decisions and evidence; do not wait for restart or a new plan. Follow `next_cursor` only while a relevant gap remains. Reuse recalled context.\n4. For multi-step work, persist specification, acceptance criteria, plan and dependency-ordered tasks before execution. Resume from recorded progress/evidence; revise affected tasks when scope changes. A single umbrella task is not an executable project plan."
    try {
      const result = Bun.spawnSync(["graphit", "_session-hook", "--format", "plain-context"], { cwd: directory })
      if (result.exitCode === 0) bootstrap = result.stdout.toString().trim() || bootstrap
    } catch {}
    return bootstrap
  }
  const dispatchFinalSync = () => {
    try {
      const subprocess = Bun.spawn(["graphit", "_session-hook", "--format", "no-output", "--sync"], { cwd: directory, stdout: "ignore", stderr: "ignore" })
      subprocess.unref()
    } catch {}
  }
  return {
  event: async ({ event }) => {
    if (event.type === "session.idle") dispatchFinalSync()
    if (event.type === "session.deleted") {
      dispatchFinalSync()
      initializedSessions.delete(event.properties.info.id)
    }
  },
  "tool.execute.after": async (_input, output) => {
    const reminder = "Graphit task checkpoint: on a completed work unit of a claimed task, call `graphit_task_progress` now with evidence and next step. Reads and task bookkeeping alone are not completed units. Keep task state in Graphit. Before completion, verify acceptance checks and affected code/documentation consistency in both directions; record inspected targets and evidence in Task. Resolve divergence before closing."
    if (typeof output.output === "string" && !output.output.includes(reminder)) output.output += `\n\n${reminder}`
  },
  "experimental.chat.system.transform": async (input, output) => {
    if (!input.sessionID) return
    const context = initializedSessions.has(input.sessionID) ? invariant : loadBootstrap()
    initializedSessions.add(input.sessionID)
    if (output.system.length === 0) output.system.push(context)
    else output.system.splice(0, 1, `${output.system[0]}\n\n${context}`)
  },
  "experimental.session.compacting": async (_input, output) => {
    let compactContext = invariant
    try {
      const result = Bun.spawnSync(["graphit", "_session-hook", "--format", "tool-context"], { cwd: directory })
      if (result.exitCode === 0) {
        const parsed = JSON.parse(result.stdout.toString())
        compactContext = parsed.additional_context || compactContext
      }
    } catch {}
    output.context.push(compactContext)
  },
  }
}
