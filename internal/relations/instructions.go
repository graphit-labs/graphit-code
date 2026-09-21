package relations

import "strings"

// AgentContract is shared by installed module skills and their lifecycle mandates.
const AgentContract = "When a task, session, memory or knowledge page relies on another record, explicitly send `references` with each target's `type` and `id`, plus `relation` (for example supports, implements, derived_from or relates_to). Resolve real IDs through the corresponding read tools; never invent them. Mentioning an ID in prose or making a Markdown link does not create a database relation. Send the complete intended explicit list on reference changes; omit it to preserve existing relations, and send [] to clear explicit relations. Structural task/session links remain managed by their existing fields. Qualify cross-project/user targets with `scope` and `scope_id`, and imported knowledge with `context`; use logical identities, never checkout paths."

const AgentExample = "Example write input: `references: [{\"target\": {\"type\": \"memory\", \"id\": \"<verified memory ID>\"}, \"relation\": \"supports\"}]`. The source is the record being written; do not repeat a source ID. Read its existing references before replacing the list so unrelated links survive."

const KnowledgeContract = "For Knowledge, declare the same typed references in the authoritative Markdown YAML frontmatter under `references`, as a list of objects with `target: {type: memory, id: <verified ID>, scope: project, scope_id: <project ID>}` and `relation: derived_from`. Indexing persists that metadata. Keep existing frontmatter and reference entries when editing; an explicit `references: []` clears authored relations. Ordinary wiki links remain a separate persisted wiki-link relationship."

const AgentMandate = "Explicitly send typed references (target type/id and relation) when citing records in writes; prose alone creates no database relation. Read the module skill for scope and update semantics."

const queryContract = "Use `graphit_references_query` with source_type/source_id for outgoing relations or target_type/target_id for backlinks. Scope filters retain identity; include_user explicitly adds personal memory. If the result reports incomplete projections, `graphit_references_reconcile` repairs writable local module projections from existing structured data; it never invents relations from prose."

func QueryContract(queryTool, reconcileTool string) string {
	return strings.NewReplacer("graphit_references_query", queryTool, "graphit_references_reconcile", reconcileTool).Replace(queryContract)
}
