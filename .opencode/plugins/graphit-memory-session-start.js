// Managed by Graphit: deterministic session-start memory protocol
const initializedSessions = new Set()

export const GraphitLifecycle = async ({ directory }) => {
  const invariant = "Graphit invariant: when a Graphit skill and MCP tool cover the current action, use them before native equivalents and load only that skill, once, at the moment it is needed. Resuming, re-entering, or continuing interrupted work reapplies this priority before the next action. If the required Graphit tool is unavailable in this agent, continue with its default native tools. Do not substitute the Graphit CLI for MCP."
  const loadBootstrap = () => {
    let bootstrap = "Graphit invariant: when a Graphit skill and MCP tool cover the current action, use them before native equivalents and load only that skill, once, at the moment it is needed. Resuming, re-entering, or continuing interrupted work reapplies this priority before the next action. If the required Graphit tool is unavailable in this agent, continue with its default native tools. Do not substitute the Graphit CLI for MCP.\nGraphit session bootstrap:\n1. Call `graphit_memory_mandatory` once and consume every result before acting.\n2. For the current request, call `graphit_memory_search` with `exclude_mandatory: true`, `ai_optimized: true`, and a focused query.\n3. Memory search returns titles and ids. Read only the relevant result(s) with `graphit_memory_source` before acting.\n4. Search prior and current Graphit tasks related to the current request with `graphit_task_search`, `ai_optimized: true`, and a focused query; follow `next_cursor` until the relevant task history is covered.\n5. Task search returns task identifiers and compact metadata. Read every relevant result with `graphit_task_get` before acting, and use its full specification, analytical results, progress, comments, evidence, and audit history as context."
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
    const reminder = "Graphit task checkpoint: if the action just finished the smallest independently reportable unit, call `graphit_task_progress` now with what landed and the exact next step. Do not write Markdown task state or defer the checkpoint until the end. Before completing a task, verify bidirectional code-documentation consistency: when code, configuration, or behavior changed, identify and update every affected current documentation/Knowledge surface; when only documentation changed, compare it with the authoritative code or behavior and correct whichever side is stale. Record the concrete targets inspected and evidence in Graphit Task; unresolved divergence blocks completion."
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
