package ast

import (
	"path"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/ignorer"
)

const AstIgnoreFile = ".astignore"

// DefaultAstIgnorePatterns is applied before any of the project's own ignore
// files.
//
// The brand directory holds this tool's output — the database, the manifest and
// one shard per source file — and it lives inside the project being indexed, so
// every watcher over that project sees those writes. Handing them back to the
// pipeline amplifies rather than merely wastes: a shard is a .json file and
// .json has a parser, so indexing a shard emits a shard for the shard, and each
// round produces more files than the last.
//
// `graphit init` does add the directory to .gitignore and the checker honours
// that, but the injection is best-effort and the file belongs to the user, so
// the guard cannot rest on it. The knowledge side has carried the same entry in
// DefaultKnowledgeIgnorePatterns for a while; this is the AST side catching up.
//
// The lockfile is the same output written outside that directory: it is the
// framework's own state, it is a .json file so the JSON grammar claims it, and
// it changes on every install, sync and config write — so it churns the graph
// with Pair and Value nodes describing the indexer to itself. Anchored with a
// leading slash because only the project's own lockfile is ours; a file of that
// name inside a fixture or a nested checkout is that project's business.
var DefaultAstIgnorePatterns = []string{
	brand.DotDir() + "/",
	"/" + brand.LockFileName(),
}

func NewAstIgnoreChecker(rootPath string) *ignorer.IgnoreChecker {
	return ignorer.New(rootPath, rootPath, AstIgnoreFile, AstIgnorePatternsFor(rootPath))
}

// AstIgnorePatternsFor returns the defaults for a specific project: the
// unconditional ones plus, unless ast.index_docs says otherwise, the knowledge
// module's documentation tree.
func AstIgnorePatternsFor(rootPath string) []string {
	patterns := DefaultAstIgnorePatterns
	if p := DocsIgnorePatternFor(rootPath); p != "" {
		patterns = append(append([]string(nil), patterns...), p)
	}
	return patterns
}

// DocsIgnorePatternFor returns the pattern that keeps the documentation tree out
// of the code graph, or "" when the tree should be indexed after all.
//
// The docs tree belongs to the knowledge wiki. Indexing it here too produced a
// Heading node per section and a File node per page in the code graph, which is
// noise in every structural query and, on a large docs tree, a real share of the
// index. Set ast.index_docs to "true" to opt back in.
//
// Two configurations return "" rather than a pattern, because excluding them
// would be wrong rather than merely opinionated:
//
//   - knowledge.docs_dir is "." — the docs tree is the project, so the pattern
//     would exclude everything and the graph would be empty.
//   - the path escapes the project (absolute, or starting with "..") — the
//     ignore checker matches project-relative paths, so no pattern it could
//     build would describe that directory.
//
// rootPath is where the lockfile is read from, so a scoped index of a
// subdirectory (`ast index --path internal/auth`) sees no project config and no
// exclusion. That is the intended escape hatch: an explicit path is an explicit
// request, and `ast index --path docs` indexes the docs tree.
func DocsIgnorePatternFor(rootPath string) string {
	projectCfg := config.LoadProjectConfig(rootPath)
	if config.ResolveAstIndexDocs(nil, projectCfg) {
		return ""
	}

	docsDir := config.ResolveDocsDir(nil, projectCfg)
	if filepath.IsAbs(docsDir) {
		return ""
	}
	clean := path.Clean(filepath.ToSlash(strings.TrimSpace(docsDir)))
	if clean == "." || clean == "" || clean == "/" ||
		clean == ".." || strings.HasPrefix(clean, "../") {
		return ""
	}

	// Anchored to the project root: knowledge.docs_dir is resolved from there, so
	// a bare "docs/" — which gitignore reads as "any directory named docs, at any
	// depth" — would also swallow internal/x/docs/, which no one asked for.
	return "/" + strings.TrimPrefix(clean, "/") + "/"
}
