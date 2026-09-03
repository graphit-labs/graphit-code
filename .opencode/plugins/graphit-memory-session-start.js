// Managed by Graphit: deterministic session-start memory protocol
const initializedSessions = new Set()

export const GraphitLifecycle = async ({ directory }) => {
  const invariant = "Graphit invariant: when a Graphit skill and MCP tool cover the current action, use them before native equivalents and load only that skill, once, at the moment it is needed. Resuming, re-entering, or continuing interrupted work reapplies this priority before the next action. If the required Graphit tool is unavailable in this agent, continue with its default native tools. Do not substitute the Graphit CLI for MCP."
  const loadBootstrap = () => {
    let bootstrap = "Graphit invariant: when a Graphit skill and MCP tool cover the current action, use them before native equivalents and load only that skill, once, at the moment it is needed. Resuming, re-entering, or continuing interrupted work reapplies this priority before the next action. If the required Graphit tool is unavailable in this agent, continue with its default native tools. Do not substitute the Graphit CLI for MCP.\nGraphit session bootstrap:\n1. Call `graphit_memory_mandatory` once and consume every result before acting.\n2. For the current request, call `graphit_memory_search` with `exclude_mandatory: true` and a focused query.\n3. Search returns titles. Read only the relevant result(s) with `graphit_wiki_source` and `wiki: \"memory\"` before acting."
    try {
      const result = Bun.spawnSync(["/usr/local/bin/graphit", "_session-hook", "--format", "plain-context"], { cwd: directory })
      if (result.exitCode === 0) bootstrap = result.stdout.toString().trim() || bootstrap
    } catch {}
    return bootstrap
  }
  const dispatchFinalSync = () => {
    try {
      const subprocess = Bun.spawn(["/usr/local/bin/graphit", "_session-hook", "--format", "no-output", "--sync"], { cwd: directory, stdout: "ignore", stderr: "ignore" })
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
    const reminder = "Graphit task checkpoint: if the action that just finished completed the smallest independently reportable unit of the current task, update the active task manager and task log now with what landed and what comes next. Do not defer that update until the end."
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
      const result = Bun.spawnSync(["/usr/local/bin/graphit", "_session-hook", "--format", "tool-context"], { cwd: directory })
      if (result.exitCode === 0) {
        const parsed = JSON.parse(result.stdout.toString())
        compactContext = parsed.additional_context || compactContext
      }
    } catch {}
    output.context.push(compactContext)
  },
  }
}
