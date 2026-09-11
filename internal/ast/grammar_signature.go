package ast

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// GrammarDirsFor returns the directories a grammar library can be installed
// into for one project. An empty projectDir yields only the global pair.
//
// This mirrors the search paths of both loaders — DynGrammarLoader for
// tree-sitter shared libraries and antlrGrammarSearchDirs for sidecar binaries —
// and exists so a caller outside this package does not have to restate them.
func GrammarDirsFor(projectDir string) []string {
	var dirs []string
	if projectDir != "" {
		dirs = append(dirs,
			filepath.Join(projectDir, brand.DotDir(), "grammars", "treesitter"),
			filepath.Join(projectDir, brand.DotDir(), "grammars", "antlr"),
		)
	}
	if global := brand.GlobalDir(); global != "" {
		dirs = append(dirs,
			filepath.Join(global, "grammars", "treesitter"),
			filepath.Join(global, "grammars", "antlr"),
		)
	}
	return dirs
}

// GrammarSignature summarises the grammar libraries installed for a project,
// changing whenever one is added, removed, or replaced.
//
// Query files are re-read while the process runs; grammar libraries are not, and
// deliberately so. resolveTreeSitterLang memoises each language for the life of
// the process — negative results included — because a *sitter.Language backs
// live parse state, and swapping one under a parse in flight is not something a
// mutex makes safe. The supported way to pick up a new library is to restart,
// which the daemon already knows how to do when the launcher stamp moves.
//
// So this is the input to that same decision, not to a hot reload.
func GrammarSignature(projectDir string) string {
	var parts []string
	for _, dir := range GrammarDirsFor(projectDir) {
		entries, err := os.ReadDir(dir)
		if err != nil {
			continue
		}
		for _, e := range entries {
			if e.IsDir() {
				continue
			}
			info, err := e.Info()
			if err != nil {
				continue
			}
			parts = append(parts, fmt.Sprintf("%s/%s:%d:%d",
				filepath.Base(dir), e.Name(), info.Size(), info.ModTime().UnixNano()))
		}
	}
	sort.Strings(parts)
	return strings.Join(parts, "|")
}
