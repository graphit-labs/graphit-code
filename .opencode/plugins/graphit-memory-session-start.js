// Managed by Graphit: deterministic session-start memory protocol
const initializedSessions = new Set()

export const GraphitLifecycle = async ({ directory }) => {
  const invariant = "Graphit invariant: prefer Graphit MCP — `graphit_ast_*` for code discovery/structure, `graphit_knowledge_*`/`graphit_wiki_*` for project knowledge, `graphit_hub_*` before model knowledge or web for external systems, and `graphit_memory_*` for durable decisions/corrections. Read only the matching skill when its domain becomes relevant. If the required Graphit tool is unavailable in this agent, continue with its default native tools. Do not substitute the Graphit CLI for MCP."
  const loadBootstrap = () => {
    let bootstrap = "Graphit invariant: prefer Graphit MCP — `graphit_ast_*` for code discovery/structure, `graphit_knowledge_*`/`graphit_wiki_*` for project knowledge, `graphit_hub_*` before model knowledge or web for external systems, and `graphit_memory_*` for durable decisions/corrections. Read only the matching skill when its domain becomes relevant. If the required Graphit tool is unavailable in this agent, continue with its default native tools. Do not substitute the Graphit CLI for MCP.\nGraphit session bootstrap:\n1. Call `graphit_memory_mandatory` once and consume every result before acting.\n2. For the current request, call `graphit_memory_search` with `exclude_mandatory: true` and a focused query.\n3. Search returns titles. Read only the relevant result(s) with `graphit_wiki_source` and `wiki: \"memory\"` before acting."
    try {
      const result = Bun.spawnSync(["/usr/local/bin/graphit", "_session-hook", "--format", "plain-context"], { cwd: directory })
      if (result.exitCode === 0) bootstrap = result.stdout.toString().trim() || bootstrap
    } catch {}
    return bootstrap
  }
  return {
  event: async ({ event }) => {
    if (event.type === "session.deleted") {
      initializedSessions.delete(event.properties.info.id)
    }
  },
  "experimental.chat.system.transform": async (input, output) => {
    if (!input.sessionID) return
    const context = initializedSessions.has(input.sessionID) ? invariant : loadBootstrap()
    initializedSessions.add(input.sessionID)
    if (output.system.length === 0) output.system.push(context)
    else output.system.splice(0, 1, `${output.system[0]}\n\n${context}`)
  },
  "experimental.session.compacting": async (_input, output) => {
    output.context.push(invariant)
  },
  }
}
