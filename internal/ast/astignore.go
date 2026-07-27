package ast

import (
	"github.com/graphit-labs/graphit-code/internal/brand"
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
var DefaultAstIgnorePatterns = []string{
	brand.DotDir() + "/",
}

func NewAstIgnoreChecker(rootPath string) *ignorer.IgnoreChecker {
	return ignorer.New(rootPath, rootPath, AstIgnoreFile, DefaultAstIgnorePatterns)
}
