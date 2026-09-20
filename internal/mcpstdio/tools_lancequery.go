package mcpstdio

import (
	"github.com/modelcontextprotocol/go-sdk/mcp"

	page "github.com/graphit-labs/graphit-code/internal/pagination"
	"github.com/graphit-labs/graphit-code/internal/toon"
)

// lanceQueryResult is the shared response shape for every structured-query tool over a LanceDB
// store: task_query, memory_query, knowledge_query and ast_fts_query.
//
// It is deliberately the same envelope taskSearchResult uses — the results array plus
// next_cursor — rather than a richer one carrying the table name and the applied projection.
// One shape for every paginated answer is worth more to a caller than per-tool metadata, and
// the projected column names are already the keys of each returned row.
func lanceQueryResult(value page.Page[map[string]any], optimized *bool) (*mcp.CallToolResult, any, error) {
	if aiOpt(optimized) {
		return textResult(paginationTOON(toon.FormatAny(value.Results), value.NextCursor))
	}
	return jsonResult(value)
}

// lanceSchemaResult answers a schema tool. Schemas are not paginated: a store holds a handful
// of tables, so the whole description is one small answer and a cursor would be ceremony.
func lanceSchemaResult(value any, optimized *bool) (*mcp.CallToolResult, any, error) {
	if aiOpt(optimized) {
		return toonResult(value)
	}
	return jsonResult(value)
}
