package ast

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// Fixture builders that touch no search index and no engine, so they must live in an UNTAGGED
// file. They used to sit beside the tests that use a search index, which are `//go:build
// lancedb` — and the untagged tests that depend on them then made the whole package fail to
// COMPILE without the tag. That reads as "the tests are broken" rather than "the tag is
// missing", and it hid every untagged test in the package behind one build error.

func hasName(hay []string, needle string) bool {
	for _, h := range hay {
		if h == needle {
			return true
		}
	}
	return false
}

// cacheFromCorpus builds a parse cache from a corpus of entities.
func cacheFromCorpus(t *testing.T, dir string, corpus []gateEntity) *ShardCache {
	t.Helper()
	pc, err := NewShardCache(dir)
	if err != nil {
		t.Fatalf("shard cache: %v", err)
	}
	byPath := map[string][]cachedEntity{}
	for _, e := range corpus {
		byPath[e.path] = append(byPath[e.path], cachedEntity{
			Label: e.entityType, UID: e.uid, Name: e.name,
			Path: e.path, Line: 1, EndLine: 1, Docstring: e.docstring,
		})
	}
	for p, ents := range byPath {
		if err := pc.Store(p, "h-"+p, &parseCacheEntry{
			RelPath: p, Language: "go", Source: "// " + p + "\n", Entities: ents,
		}); err != nil {
			t.Fatalf("store %s: %v", p, err)
		}
	}
	if err := pc.FlushDirty(); err != nil {
		t.Fatal(err)
	}
	return pc
}

// entityNames reduces results to entity names, dropping file hits, so an expectation
// about which ENTITY ranks first is not satisfied or spoiled by a file result.
func entityNames(res []SearchResult, topK int) []string {
	var out []string
	seen := map[string]bool{}
	for _, r := range res {
		if r.Type == "file" {
			continue
		}
		n := r.Name
		if i := strings.IndexByte(n, ' '); i > 0 {
			n = n[:i]
		}
		if seen[n] {
			continue
		}
		seen[n] = true
		out = append(out, n)
		if topK > 0 && len(out) == topK {
			break
		}
	}
	return out
}

// indexSearchNames returns the names of all results, files included, deduplicated and
// capped. Used where an expectation is about reachability rather than about which entity
// wins.

// abbrevCorpusNamesOnly is abbrevCorpus with the prose stripped. It exists to remove a
// confound: every entity in abbrevCorpus has a docstring containing "configuration", and on
// the FTS5 index the prefix pass made the query "config" match that word, so a hit proved
// nothing about whether the NAME was reachable. That particular route is gone — LadybugDB
// has no prefix matching — but the variant is kept because prose can still answer a query
// through the docstring index, and because TestAbbreviatedIdentifierSearch indexes names
// only, so only this variant is comparable to it.
func abbrevCorpusNamesOnly() []gateEntity {
	out := abbrevCorpus()
	for i := range out {
		out[i].docstring = ""
	}
	return out
}

// abbrevProbes are the three directions of partial match, shared by every measurement of
// abbreviation recall.

// stageGrammar is stageLang for languages whose grammar name is not derivable
// from the language name — yaml_lang uses tree-sitter-yaml.
func stageGrammar(t *testing.T, langName, grammar, ext, queryFile string) string {
	t.Helper()
	body, err := os.ReadFile(filepath.Join("queries", queryFile))
	if err != nil {
		t.Skipf("no %s: %v", queryFile, err)
	}
	return stageGrammarWithQueries(t, langName, grammar, ext, queryFile, string(body))
}

// stageGrammarWithQueries stages a query file the framework does not ship, which
// is how a project opts a registered grammar back in through ast.queries_dir.
// Reading from queries/ would skip instead of fail for these, and a skip is not
// how an opt-in that stopped working should report itself.

func parseFixture(t *testing.T, projectDir, name, source string) *ParsedFile {
	t.Helper()
	srcPath := filepath.Join(projectDir, name)
	if err := os.WriteFile(srcPath, []byte(source), 0o644); err != nil {
		t.Fatal(err)
	}
	pf, err := NewCompositeParser(projectDir, nil).Parse(srcPath, false, ParseOptions{})
	if err != nil {
		t.Fatalf("parse %s: %v", name, err)
	}
	return pf
}

// wantNode asserts a node exists with the given label and name, and that it is
// contained by the expected parent.

func nodesOf(pf *ParsedFile) map[[2]string]kvNode {
	got := make(map[[2]string]kvNode)
	for _, ents := range pf.Entities {
		for _, e := range ents {
			got[[2]string{e.GraphLabel, e.Name}] = kvNode{
				label: e.GraphLabel, name: e.Name,
				parent: e.Context, pLabel: e.ContextType,
				value: e.Properties["value"], present: true,
			}
		}
	}
	return got
}

func wantNode(t *testing.T, got map[[2]string]kvNode, label, name, parent, pLabel string) {
	t.Helper()
	n, ok := got[[2]string{label, name}]
	if !ok {
		t.Errorf("no %s node named %q", label, name)
		return
	}
	if n.parent != parent || n.pLabel != pLabel {
		t.Errorf("%s %q is contained by %s %q, want %s %q",
			label, name, n.pLabel, n.parent, pLabel, parent)
	}
}

type kvNode struct {
	label   string
	name    string
	parent  string // Context
	pLabel  string // ContextType
	value   string // Properties["value"]
	present bool
}

// Data formats are key/value languages, and until now only the key was indexed.
//
// An XML attribute produced one node named "env"; the "prod" it was set to lived
// nowhere. A JSON member produced a node named `"host"` — quotes included, because
// the capture spanned the string literal — and never the host itself. Meanwhile
// every capture in a pattern became an entity carrying the query's graph_label, so
// HCL's `resource "aws_instance" "web"` produced three Resource nodes: `resource`,
// `aws_instance` and `web`.
//
// Both halves of a pair are now nodes, the pair survives as a CONTAINS edge from
// key to value, and the value's name IS the value — which is what puts it in the
// search index, since that index reads name and docstring and never value.

func stageGrammarWithQueries(t *testing.T, langName, grammar, ext, queryFile, queries string) string {
	t.Helper()
	body := []byte(queries)
	if lang, err := resolveTreeSitterLang(langName, grammar); err != nil || lang == nil {
		t.Skipf("%s grammar unavailable: %v", langName, err)
	}

	projectDir := t.TempDir()
	qdir := filepath.Join(projectDir, brand.DotDir(), "ast", "queries")
	if err := os.MkdirAll(qdir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(qdir, queryFile), body, 0o644); err != nil {
		t.Fatal(err)
	}

	restore, had := tsExtMap[ext]
	extTablesMu.Lock()
	tsExtMap[ext] = &tsLangConfig{
		Language: langName, Grammar: grammar, Extensions: []string{ext},
	}
	extTablesMu.Unlock()
	t.Cleanup(func() {
		extTablesMu.Lock()
		if had {
			tsExtMap[ext] = restore
		} else {
			delete(tsExtMap, ext)
		}
		extTablesMu.Unlock()
	})
	return projectDir
}

// abbrevCorpus is the corpus TestAbbreviatedIdentifierSearch probes directly against a
// hand-built Ladybug FTS index, so the raw-versus-split-versus-trigram comparison and the
// whole-index measurements here run on identical input.
func abbrevCorpus() []gateEntity {
	return []gateEntity{
		{"a1", "coreConf", "Core configuration accessor.", "Function", "core.go"},
		{"a2", "CONF_MGR", "Configuration manager package.", "Package", "conf_mgr.sql"},
		{"a3", "CFG_LOAD", "Loads settings at startup.", "Procedure", "cfg_load.sql"},
		{"a4", "configLoader", "Loads the configuration.", "Class", "loader.go"},
		{"a5", "initConfiguration", "Initialises configuration state.", "Function", "init.go"},
		{"a6", "computeChecksum", "Computes a checksum.", "Function", "hash.go"},
		{"a7", "PKG_ACCOUNT_UPDATE", "Updates account rows.", "Package", "acct.sql"},
	}
}

// buildSearchIndexFrom seeds the search index with an arbitrary corpus.
