// Managed by Graphit: deterministic session-start memory protocol
const initializedSessions = new Set()
const startedSessions = new Set()

export const GraphitMemorySessionStart = async () => ({
  event: async ({ event }) => {
    if (event.type === "session.created") startedSessions.add(event.properties.info.id)
    if (event.type === "session.deleted") {
      startedSessions.delete(event.properties.info.id)
      initializedSessions.delete(event.properties.info.id)
    }
  },
  "experimental.chat.system.transform": async (input, output) => {
    if (!input.sessionID || initializedSessions.has(input.sessionID)) return
    initializedSessions.add(input.sessionID)
    startedSessions.delete(input.sessionID)
    const protocol = "Graphit deterministic session initialization: complete this protocol before responding to the first user request or taking any other action.\n1. Call `graphit_memory_mandatory` with no query and consume every mandatory memory it returns.\n2. Derive a contextual query from the current user request, then call `graphit_memory_search` with that query and `exclude_mandatory: true`.\n3. Select the relevant contextual results and read their full content with `graphit_wiki_source` using `wiki: \"memory\"` before acting.\nIf the memory store does not exist yet, proceed after the tool reports that condition. Execute this initialization exactly once for this session."
    if (output.system.length === 0) output.system.push(protocol)
    else output.system.splice(0, 1, `${output.system[0]}\n\n${protocol}`)
  },
})
