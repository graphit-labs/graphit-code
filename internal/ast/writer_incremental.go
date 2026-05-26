package ast

import (
	"context"
	"fmt"
)

var allEdgeTypes = []string{
	"IMPORTS", "CALLS", "HAS_PARAMETER", "INHERITS", "IMPLEMENTS",
	"HAS_FIELD", "READS_FIELD", "WRITES_FIELD",
	"SELECTS", "INSERTS", "UPDATES", "DELETES",
	"CREATES", "ALTERS", "DROPS", "REFERENCES", "INCLUDES",
	"HAS_DECORATOR", "HAS_ANNOTATION", "HAS_ATTRIBUTE",
}

func (w *GraphWriter) getDeleteQueries(relPath string) []BatchQuery {
	var cmds []BatchQuery

	for _, edge := range allEdgeTypes {
		cmds = append(cmds, BatchQuery{
			Cypher: fmt.Sprintf(`MATCH (a)-[r:%s]->(b) WHERE r.source_file = $file DELETE r`, edge),
			Params: map[string]any{"file": relPath},
		})
	}

	cmds = append(cmds, BatchQuery{
		Cypher: `MATCH (f:File {path: $file})-[r:CONTAINS]->() DELETE r`,
		Params: map[string]any{"file": relPath},
	})
	cmds = append(cmds, BatchQuery{
		Cypher: `MATCH (f:File {path: $file}) DETACH DELETE f`,
		Params: map[string]any{"file": relPath},
	})

	return cmds
}

func (w *GraphWriter) WriteFileIncremental(ctx context.Context, pf *ParsedFile, repoPath string) error {
	return w.WriteChunkIncremental(ctx, []*ParsedFile{pf}, repoPath)
}
