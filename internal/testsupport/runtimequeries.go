// Package testsupport holds helpers that only test binaries use. Nothing in the
// shipped binary imports it.
package testsupport

import (
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/version"
)

// seedErr records what init's seeding did, so a TestMain can fail loudly on it.
var seedErr error

// init seeds at init time because that is the only moment early enough.
//
// internal/ast builds its extension tables in a package init() of its own
// (treesitter_adapter.go), reading the runtime queries directory exactly once and
// caching the result — including an empty one. A TestMain runs after every package
// init, so seeding there lands after the tables were already built from an empty
// directory, and the languages never appear.
//
// Init order is what makes this work: a package's dependencies are initialised before
// it is. So importing this package from internal/ast's test files puts this init ahead
// of internal/ast's, and internal/brand's HOME isolation — a dependency of this
// package — ahead of both, which is the order the seeding needs.
func init() {
	if !testing.Testing() {
		return
	}
	seedErr = seedRuntimeQueries()
}

// SeedError reports whether the init-time seeding of the grammar query files
// succeeded. Call it from TestMain: a package whose tests parse code cannot do
// anything meaningful without languages, and the failure it produces otherwise is an
// empty graph, which points at the wrong bug.
func SeedError() error { return seedErr }

// seedRuntimeQueries puts this repository's grammar query files where the AST loader
// looks for the installed ones.
//
// The language definitions are NOT compiled into the binary. They travel
// internal/ast/queries/*.yaml → cmd/launcher/runtime/ast/queries (copied by the
// Makefile) → embedded in the launcher → extracted at install time into
// ~/<brand dir>/runtime/<version>/ast/queries. rebuildExtTables reads only that last
// location, so a test binary — which no installer ever ran for — sees no languages at
// all: embeddedLangConfig resolves nothing, every .sql parse fails with "unknown ANTLR
// grammar", and the graph comes out empty.
//
// Before HOME was isolated for tests this went unnoticed, because the loader found the
// runtime of whatever version the developer happened to have installed. That made a
// green suite depend on the machine it ran on: a contributor who had never run the
// installer, and CI, would see failures that reproduce nowhere else. Seeding from the
// repository is both hermetic and more honest — the files under test are the ones in
// this checkout, not the ones from some earlier install.
//
// It exercises the production load path rather than bypassing it, so the loader,
// the merge order and the extension tables are all still under test.
func seedRuntimeQueries() error {
	root, err := repoRoot()
	if err != nil {
		return err
	}
	src := filepath.Join(root, "internal", "ast", "queries")
	if _, err := os.Stat(src); err != nil {
		return fmt.Errorf("grammar queries not found at %s: %w", src, err)
	}

	runtimeDir := brand.RuntimeDir(version.Version)
	if runtimeDir == "" {
		return errors.New("no runtime directory: the brand global dir did not resolve")
	}
	dst := filepath.Join(runtimeDir, "ast", "queries")
	if err := os.MkdirAll(filepath.Dir(dst), 0o755); err != nil {
		return err
	}

	// A copy, not a symlink. A symlink would make the repository the single copy and
	// pick up mid-session edits — worth nothing here, because every test binary gets a
	// fresh home and reseeds anyway — while turning any write into that directory into
	// a silent edit of tracked files in this checkout. Nothing writes there today; the
	// copy means nothing has to keep being true for that to stay safe. It is 45 small
	// files.
	return copyDir(src, dst)
}

// repoRoot walks up from the working directory to the module root. A test binary runs
// with its own package directory as the working directory, so the depth differs per
// package and cannot be a fixed number of "..".
func repoRoot() (string, error) {
	dir, err := os.Getwd()
	if err != nil {
		return "", err
	}
	for {
		if _, err := os.Stat(filepath.Join(dir, "go.mod")); err == nil {
			return dir, nil
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			return "", fmt.Errorf("no go.mod above %s", dir)
		}
		dir = parent
	}
}

func copyDir(src, dst string) error {
	entries, err := os.ReadDir(src)
	if err != nil {
		return err
	}
	if err := os.MkdirAll(dst, 0o755); err != nil {
		return err
	}
	for _, e := range entries {
		if e.IsDir() {
			continue
		}
		if err := copyFile(filepath.Join(src, e.Name()), filepath.Join(dst, e.Name())); err != nil {
			return err
		}
	}
	return nil
}

func copyFile(src, dst string) error {
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer func() { _ = in.Close() }()

	out, err := os.Create(dst)
	if err != nil {
		return err
	}
	if _, err := io.Copy(out, in); err != nil {
		_ = out.Close()
		return err
	}
	return out.Close()
}
