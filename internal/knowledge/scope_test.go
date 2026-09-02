//go:build lancedb

package knowledge

import (
	"context"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

func writeDoc(t *testing.T, root, rel, body string) {
	t.Helper()
	abs := filepath.Join(root, filepath.FromSlash(rel))
	if err := os.MkdirAll(filepath.Dir(abs), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(abs, []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
}

// The wiki's scope is docs/ plus the root README. Everything else in the project
// is out — which is the whole point of the default no longer being ".".
func TestScopeForDefaults(t *testing.T) {
	root := t.TempDir()
	writeDoc(t, root, "README.md", "# Projeto\n")

	scope := ScopeFor(root, nil, nil)

	if scope.Subdir != config.DefaultDocsDir {
		t.Errorf("Subdir = %q; want %q", scope.Subdir, config.DefaultDocsDir)
	}
	if len(scope.ExtraFiles) != 1 || scope.ExtraFiles[0] != "README.md" {
		t.Errorf("ExtraFiles = %v; want [README.md]", scope.ExtraFiles)
	}
}

func TestScopeForRespectsConfig(t *testing.T) {
	root := t.TempDir()
	writeDoc(t, root, "README.md", "# Projeto\n")

	custom := config.ConfigMap{"knowledge": map[string]any{"docs_dir": "documentacao"}}
	if got := ScopeFor(root, custom, nil).Subdir; got != "documentacao" {
		t.Errorf("Subdir = %q; want %q", got, "documentacao")
	}

	noReadme := config.ConfigMap{"knowledge": map[string]any{"include_readme": "false"}}
	if extras := ScopeFor(root, noReadme, nil).ExtraFiles; len(extras) != 0 {
		t.Errorf("include_readme=false still carried %v", extras)
	}
}

// A project with no README at all must not name one, or the pipeline would stat a
// path that is not there on every rebuild.
func TestScopeForWithoutAReadme(t *testing.T) {
	root := t.TempDir()
	writeDoc(t, root, "docs/guia.md", "# Guia\n")

	if extras := ScopeFor(root, nil, nil).ExtraFiles; len(extras) != 0 {
		t.Errorf("a project with no README named %v", extras)
	}
}

// Casing and extension both vary in the wild, and enumerating their cross product
// as literal names is how one of them gets forgotten.
func TestRootReadmeVariants(t *testing.T) {
	exts := config.ResolveKnowledgeExtensions(nil, nil)

	for _, name := range []string{"README.md", "readme.md", "README.markdown", "Readme.rst", "README.txt"} {
		root := t.TempDir()
		writeDoc(t, root, name, "# x\n")
		if got := RootReadme(root, exts); got != name {
			t.Errorf("RootReadme did not find %q, got %q", name, got)
		}
	}

	// An extension the pipeline cannot chunk is not a document it can index.
	root := t.TempDir()
	writeDoc(t, root, "README.pdf", "%PDF-1.4\n")
	if got := RootReadme(root, exts); got != "" {
		t.Errorf("RootReadme returned %q for a README the wiki cannot read", got)
	}

	// A directory called README is not the README.
	dirRoot := t.TempDir()
	if err := os.MkdirAll(filepath.Join(dirRoot, "README.md"), 0o755); err != nil {
		t.Fatal(err)
	}
	if got := RootReadme(dirRoot, exts); got != "" {
		t.Errorf("RootReadme returned %q for a directory", got)
	}
}

// End to end: the scoped build indexes docs/ and the README, leaves the rest of
// the project alone, and reports every source path relative to the project root
// rather than to the docs directory.
func TestScopedBuildIndexesDocsAndReadmeOnly(t *testing.T) {
	root := t.TempDir()
	writeDoc(t, root, "README.md", "# Projeto\n\nA porta de entrada.\n")
	writeDoc(t, root, "docs/specs/feature.md", "# Feature\n\nO que ela faz.\n")
	writeDoc(t, root, "vendor/outro/LEIA.md", "# Vendored\n\nNao e nossa.\n")
	writeDoc(t, root, "internal/x/notas.md", "# Notas\n\nSoltas no codigo.\n")

	wikiDir := filepath.Join(t.TempDir(), "wiki")
	exts := config.ResolveKnowledgeExtensions(nil, nil)

	if _, err := GenerateKnowledgeWiki(context.Background(), root, wikiDir, exts, ScopeFor(root, nil, nil)); err != nil {
		t.Fatal(err)
	}

	sources := manifestSources(t, wikiDir)
	for _, want := range []string{"README.md", filepath.Join("docs", "specs", "feature.md")} {
		if !sources[want] {
			t.Errorf("%s is not in the wiki; indexed: %v", want, keysOf(sources))
		}
	}
	for _, unwanted := range []string{
		filepath.Join("vendor", "outro", "LEIA.md"),
		filepath.Join("internal", "x", "notas.md"),
	} {
		if sources[unwanted] {
			t.Errorf("%s was indexed — the scope is not holding", unwanted)
		}
	}
}

// docs_dir="." restores the old behaviour, README included exactly once rather
// than twice: the walk already found it, and a second entry would double every
// page it produces.
func TestDocsDirOfDotIndexesEverythingOnce(t *testing.T) {
	root := t.TempDir()
	writeDoc(t, root, "README.md", "# Projeto\n\nA porta de entrada.\n")
	writeDoc(t, root, "docs/guia.md", "# Guia\n\nConteudo.\n")

	wikiDir := filepath.Join(t.TempDir(), "wiki")
	exts := config.ResolveKnowledgeExtensions(nil, nil)
	inline := config.ConfigMap{"knowledge": map[string]any{"docs_dir": "."}}

	scope := ScopeFor(root, inline, nil)
	if _, err := GenerateKnowledgeWiki(context.Background(), root, wikiDir, exts, scope); err != nil {
		t.Fatal(err)
	}

	ic := NewKnowledgeIgnoreChecker(root)
	sources, err := enumerateKnowledgeSources(root, scope, exts, ic)
	if err != nil {
		t.Fatal(err)
	}
	var readmes int
	for _, s := range sources {
		if s.relPath == "README.md" {
			readmes++
		}
	}
	if readmes != 1 {
		t.Errorf("README.md enumerated %d times; want exactly 1", readmes)
	}

	got := manifestSources(t, wikiDir)
	for _, want := range []string{"README.md", filepath.Join("docs", "guia.md")} {
		if !got[want] {
			t.Errorf("%s is not in the wiki with docs_dir=.; indexed: %v", want, keysOf(got))
		}
	}
}

// A project whose docs tree does not exist yet still has a README, and the wiki
// has to come out with that page in it rather than empty.
func TestReadmeIsIndexedWithoutADocsTree(t *testing.T) {
	root := t.TempDir()
	writeDoc(t, root, "README.md", "# Projeto\n\nSem docs ainda.\n")

	wikiDir := filepath.Join(t.TempDir(), "wiki")
	exts := config.ResolveKnowledgeExtensions(nil, nil)

	result, err := GenerateKnowledgeWiki(context.Background(), root, wikiDir, exts, ScopeFor(root, nil, nil))
	if err != nil {
		t.Fatalf("a missing docs tree must not be an error: %v", err)
	}
	if result.ArticlesWritten == 0 {
		t.Error("no article was written for a project that has only a README")
	}
	if got := manifestSources(t, wikiDir); !got["README.md"] {
		t.Errorf("README.md is not in the wiki; indexed: %v", keysOf(got))
	}
}

// Scoping the build changed which directory ignore files are collected from, and
// that is worth pinning in both directions.
//
// Ignore files are collected by walking *up* from a start directory, and each is
// given a domain relative to the root. Handing the docs tree in as the root — the
// arrangement before WikiScope — put the project's own .gitignore and .wikiignore
// one level *above* the root, so `domainForFile` gave them the domain ".." and
// every pattern in them matched nothing. Passing the project as the root fixes
// that; passing the docs tree as the *start* directory is what keeps an ignore
// file kept inside the docs tree working.
func TestIgnoreFilesAtBothLevelsApply(t *testing.T) {
	build := func(t *testing.T, root string) map[string]bool {
		t.Helper()
		wikiDir := filepath.Join(t.TempDir(), "wiki")
		exts := config.ResolveKnowledgeExtensions(nil, nil)
		if _, err := GenerateKnowledgeWiki(context.Background(), root, wikiDir, exts,
			ScopeFor(root, nil, nil)); err != nil {
			t.Fatal(err)
		}
		return manifestSources(t, wikiDir)
	}

	t.Run("root gitignore", func(t *testing.T) {
		root := gitInit(t)
		writeDoc(t, root, ".gitignore", "docs/gerado/\n")
		writeDoc(t, root, "README.md", "# Projeto\n\nPorta.\n")
		writeDoc(t, root, "docs/guia.md", "# Guia\n\nConteudo.\n")
		writeDoc(t, root, "docs/gerado/auto.md", "# Auto\n\nGerado.\n")

		got := build(t, root)
		if got[filepath.Join("docs", "gerado", "auto.md")] {
			t.Errorf("the root .gitignore was not applied; indexed: %v", keysOf(got))
		}
		if !got[filepath.Join("docs", "guia.md")] {
			t.Errorf("the root .gitignore excluded too much; indexed: %v", keysOf(got))
		}
	})

	t.Run("root wikiignore", func(t *testing.T) {
		root := gitInit(t)
		writeDoc(t, root, ".wikiignore", "docs/rascunhos/\n")
		writeDoc(t, root, "README.md", "# Projeto\n\nPorta.\n")
		writeDoc(t, root, "docs/guia.md", "# Guia\n\nConteudo.\n")
		writeDoc(t, root, "docs/rascunhos/x.md", "# X\n\nRascunho.\n")

		got := build(t, root)
		if got[filepath.Join("docs", "rascunhos", "x.md")] {
			t.Errorf("the root .wikiignore was not applied; indexed: %v", keysOf(got))
		}
	})

	// This is the one the scoping change could have broken: the file is inside the
	// docs tree, which is no longer the root the patterns resolve against.
	t.Run("wikiignore inside the docs tree", func(t *testing.T) {
		root := gitInit(t)
		writeDoc(t, root, "README.md", "# Projeto\n\nPorta.\n")
		writeDoc(t, root, "docs/.wikiignore", "rascunho.md\n")
		writeDoc(t, root, "docs/guia.md", "# Guia\n\nConteudo.\n")
		writeDoc(t, root, "docs/rascunho.md", "# Rascunho\n\nNao publicar.\n")

		got := build(t, root)
		if got[filepath.Join("docs", "rascunho.md")] {
			t.Errorf("a .wikiignore inside the docs tree was not read; indexed: %v", keysOf(got))
		}
		if !got[filepath.Join("docs", "guia.md")] || !got["README.md"] {
			t.Errorf("it excluded more than it named; indexed: %v", keysOf(got))
		}
	})

	// A pattern in the docs tree is scoped to the docs tree, not reinterpreted
	// against the project root.
	t.Run("a docs-level pattern does not reach outside the docs tree", func(t *testing.T) {
		root := gitInit(t)
		writeDoc(t, root, "docs/.wikiignore", "README.md\n")
		writeDoc(t, root, "README.md", "# Projeto\n\nPorta.\n")
		writeDoc(t, root, "docs/README.md", "# Docs\n\nIndice.\n")

		got := build(t, root)
		if !got["README.md"] {
			t.Errorf("a docs-level pattern excluded the project README; indexed: %v", keysOf(got))
		}
		if got[filepath.Join("docs", "README.md")] {
			t.Errorf("the docs-level pattern did not apply inside docs/; indexed: %v", keysOf(got))
		}
	})
}

// gitInit makes a temp directory a git repository, because ignorer.New walks up to
// the git root to decide how far to collect ignore files from.
func gitInit(t *testing.T) string {
	t.Helper()
	root := t.TempDir()
	if out, err := exec.Command("git", "init", root).CombinedOutput(); err != nil {
		t.Skipf("git unavailable: %v: %s", err, out)
	}
	return root
}

func manifestSources(t *testing.T, wikiDir string) map[string]bool {
	t.Helper()
	chunks, err := wiki.IndexedChunks(context.Background(), wikiDir)
	if err != nil {
		t.Fatalf("reading indexed chunks: %v", err)
	}
	m := ManifestFromChunks(chunks)
	out := make(map[string]bool, len(m.SourceHashes))
	for path := range m.SourceHashes {
		out[path] = true
	}
	return out
}

func keysOf(m map[string]bool) string {
	out := make([]string, 0, len(m))
	for k := range m {
		out = append(out, k)
	}
	return strings.Join(out, ", ")
}
