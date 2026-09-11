package ast

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func hasName(hay []string, needle string) bool {
	for _, h := range hay {
		if h == needle {
			return true
		}
	}
	return false
}

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

func abbrevCorpusNamesOnly() []gateEntity {
	out := abbrevCorpus()
	for i := range out {
		out[i].docstring = ""
	}
	return out
}

func stageGrammar(t *testing.T, langName, grammar, ext, queryFile string) string {
	t.Helper()
	body, err := os.ReadFile(filepath.Join("queries", queryFile))
	if err != nil {
		t.Skipf("no %s: %v", queryFile, err)
	}
	return stageGrammarWithQueries(t, langName, grammar, ext, queryFile, string(body))
}

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
	parent  string
	pLabel  string
	value   string
	present bool
}

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
