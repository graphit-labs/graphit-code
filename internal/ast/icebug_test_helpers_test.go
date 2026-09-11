package ast

import (
	"fmt"
	"path/filepath"
)

func syntheticCorpus(files int) map[string]*parseCacheEntry {
	const entitiesPerFile = 12
	entries := make(map[string]*parseCacheEntry, files)
	for fileIndex := 0; fileIndex < files; fileIndex++ {
		rel := fmt.Sprintf("internal/pkg%d/module%d/file%d.go", fileIndex%97, fileIndex%53, fileIndex)
		entry := &parseCacheEntry{
			RelPath:  rel,
			Language: "go",
			Cluster:  "core",
			DirPaths: []string{filepath.Dir(rel), fmt.Sprintf("internal/pkg%d", fileIndex%97)},
		}
		for entityIndex := 0; entityIndex < entitiesPerFile; entityIndex++ {
			uid := fmt.Sprintf("%s:Symbol%d", rel, entityIndex)
			entry.Entities = append(entry.Entities, cachedEntity{
				Label: "Function", UID: uid, Name: fmt.Sprintf("Symbol%d_%d", fileIndex, entityIndex),
				Path: rel, Line: entityIndex * 9, EndLine: entityIndex*9 + 7,
				Docstring: "Handles one step of the pipeline and returns the normalized result.",
				Lang:      "go", Complexity: 3, Context: "pkg", ContextType: "package",
				IsExported: entityIndex%2 == 0,
			})
			entry.ContainsEdges = append(entry.ContainsEdges, cachedContainsEdge{
				ParentUID: rel, ChildUID: uid, ParentLabel: "File", ChildLabel: "Function",
			})
			entry.Calls = append(entry.Calls, cachedCall{
				CallerUID: uid, CalleeUID: fmt.Sprintf("Symbol%d_%d", (fileIndex+1)%files, entityIndex),
				SourceType: "Function", Line: entityIndex * 9, Path: rel, Lang: "go",
			})
		}
		entries[rel] = entry
	}
	return entries
}
