package ast

import (
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/ide"
)

// ASTRuleContent teaches decisions the hook cannot make. Tool schemas already
// describe arguments, so the skill avoids duplicating their reference manuals.
func ASTRuleContent() string {
	return strings.Join([]string{
		"# Graphit AST",
		"",
		"Use this skill for code discovery, structure, relationships, impact analysis, or source held by a Graphit context. Graphit AST precedes native search; it does not prohibit focused reads after the graph has located current-project code.",
		"",
		"## Workflow",
		"",
		"1. Identify the target: current project (`project_dir`) or installed AST context (`context`). Resolve another repository through the Hub; never guess its path.",
		"2. Before the first Cypher for that target, call `" + brand.MCPToolName("ast", "schema") + "`. Reuse that schema until the target changes.",
		"3. If the question is exploratory, pair one exact `" + brand.MCPToolName("ast", "query") + "` with one `" + brand.MCPToolName("ast", "search") + "` on the same topic. If the question is already exact, query alone is enough.",
		"4. Read only selected code with `" + brand.MCPToolName("ast", "source") + "` using an entity, line slice, or pattern. Imported source is readable only through this tool.",
		"5. Before editing an entity, query its definition, callers/dependents, and test references. Expand only when the result shows wider impact.",
		"",
		"## Query discipline",
		"",
		"Use only labels, properties, and relationships returned by the schema. Common nodes include `File`, `Function`, `Method`, `Class`, and `Struct`; common edges include `CONTAINS`, `CALLS`, `IMPORTS`, `INHERITS`, and `REFERENCES`, but the live schema is authoritative. A planner rejection or missing property is a reason to reread the schema, not guess another field.",
		"",
		"Search grounds the query; it is not evidence by itself. Query results establish structure. Source establishes behavior. Keep `project_dir` absolute and do not mix results from different targets.",
		"",
		"## Fallbacks and freshness",
		"",
		"Retry a database-open error once; it is commonly a transient lock. If the graph is absent, use `" + brand.MCPToolName("daemon", "status") + "` and then `" + brand.MCPToolName("ast", "index") + "`. Use `" + brand.MCPToolName("ast", "embed") + "` only when semantic embeddings are missing. Use native discovery when the required Graphit tool is unavailable to this agent or for unsupported/unindexed current-project text, and record the limitation. Native tools cannot read an imported context.",
		"",
		"The daemon normally indexes edits. Call `" + brand.MCPToolName("sync") + "` only when a decision requires proven freshness. The adapter stop hook dispatches completion sync asynchronously; do not duplicate it, wait for it, or sync after every edit.",
		"",
		"## Administrative tools",
		"",
		"`" + brand.MCPToolName("ast", "list") + "` lists contexts; `" + brand.MCPToolName("ast", "install") + "` and `" + brand.MCPToolName("ast", "remove") + "` manage them; `" + brand.MCPToolName("ast", "export") + "` exports a graph; `" + brand.MCPToolName("cluster", "projects") + "` resolves ecosystem projects. These mutate or move artifacts only when the user task requires it.",
		"",
		"Tool index: `graphit_ast_search`, `graphit_ast_query`, `graphit_ast_schema`, `graphit_ast_source`, `graphit_ast_list`, `graphit_ast_index`, `graphit_ast_embed`, `graphit_ast_export`, `graphit_ast_install`, `graphit_ast_remove`, `graphit_cluster_projects`, `graphit_daemon_status`, `graphit_sync`.",
	}, "\n") + "\n"
}

func MandateTrigger() string {
	return ide.ModuleMandateTrigger(
		"AST Code Exploration",
		astSkillName,
		"code discovery or structural analysis",
		"",
		[]string{
			"locating or understanding code, a symbol, callers/callees, imports, inheritance, tests, complexity, or change impact",
			"using grep, glob, find, semantic search, code symbols, or file-by-file reads to discover code",
			"editing an entity whose dependents and test reach are not yet known",
			"reading code from another repository or a named AST context",
			"writing Cypher, recovering from a graph/schema failure, or requiring a provably fresh graph",
		},
		[]string{"ast_search", "ast_query", "ast_schema", "ast_source"},
	)
}
