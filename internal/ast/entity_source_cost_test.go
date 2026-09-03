package ast

import (
	"os"
	"path/filepath"
	"runtime"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	sitter "github.com/tree-sitter/go-tree-sitter"
)

func goCorpusFor(tb testing.TB, limit int) (projectDir string, files []string, lang *sitter.Language) {
	tb.Helper()

	lang, err := resolveTreeSitterLang("go", "tree-sitter-go")
	if err != nil || lang == nil {
		tb.Skipf("go grammar unavailable: %v", err)
	}
	queryBody, err := os.ReadFile(filepath.Join("queries", "go.yaml"))
	if err != nil {
		tb.Skipf("no go.yaml: %v", err)
	}

	projectDir = tb.TempDir()
	qdir := filepath.Join(projectDir, brand.DotDir(), "ast", "queries")
	if err := os.MkdirAll(qdir, 0o755); err != nil {
		tb.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(qdir, "go.yaml"), queryBody, 0o644); err != nil {
		tb.Fatal(err)
	}

	all, err := filepath.Glob("*.go")
	if err != nil {
		tb.Fatal(err)
	}
	for _, f := range all {
		if len(files) >= limit {
			break
		}
		if len(f) > 8 && f[len(f)-8:] == "_test.go" {
			continue
		}
		files = append(files, f)
	}
	if len(files) == 0 {
		tb.Skip("no corpus files")
	}
	return projectDir, files, lang
}

func declTextBytes(tb testing.TB, projectDir, path string, lang *sitter.Language) (fileBytes, entities, retained int) {
	tb.Helper()

	src, err := os.ReadFile(path)
	if err != nil {
		tb.Fatal(err)
	}
	p := sitter.NewParser()
	defer p.Close()
	if err := p.SetLanguage(lang); err != nil {
		tb.Fatal(err)
	}
	tree := p.Parse(src, nil)
	if tree == nil {
		return len(src), 0, 0
	}
	defer tree.Close()

	for _, ce := range compiledQueriesFor(projectDir, "go", ".go", lang) {
		qc := sitter.NewQueryCursor()
		matches := qc.Matches(ce.Query, tree.RootNode(), src)
		for {
			m := matches.Next()
			if m == nil {
				break
			}
			for ci := range m.Captures {
				parent := m.Captures[ci].Node.Parent()
				if parent == nil {
					continue
				}
				entities++
				retained += len(parent.Utf8Text(src))
			}
		}
		qc.Close()
	}
	return len(src), entities, retained
}

func TestEntitySourceRetainedBytes(t *testing.T) {
	projectDir, files, lang := goCorpusFor(t, 40)

	var totalFile, totalEnt, totalRetained int
	worstRatio, worstFile := 0.0, ""
	for _, f := range files {
		fb, ents, ret := declTextBytes(t, projectDir, f, lang)
		totalFile += fb
		totalEnt += ents
		totalRetained += ret
		if fb > 0 {
			if r := float64(ret) / float64(fb); r > worstRatio {
				worstRatio, worstFile = r, f
			}
		}
	}
	if totalEnt == 0 {
		t.Skip("queries produced no entities — nothing to measure")
	}

	ratio := float64(totalRetained) / float64(totalFile)
	t.Logf("corpus: %d files, %d KB of source, %d entities", len(files), totalFile/1024, totalEnt)
	t.Logf("Entity.Source would retain: %d KB", totalRetained/1024)
	t.Logf("that is %.2fx the size of the source it was parsed from", ratio)
	t.Logf("worst single file: %s at %.2fx", worstFile, worstRatio)
	t.Logf("mean per entity: %d bytes", totalRetained/totalEnt)

	if totalRetained <= totalFile {
		t.Logf("NOTE: retention did not exceed source size on this corpus; the " +
			"overlapping-copies effect needs deeper nesting to show")
	}
}

func TestEntitySourceLiveHeap(t *testing.T) {
	projectDir, files, lang := goCorpusFor(t, 40)

	type entityWithSource struct {
		Entity
		Source string
	}

	build := func(retain bool) ([]entityWithSource, int) {
		var ents []entityWithSource
		bytesHeld := 0
		for _, f := range files {
			src, err := os.ReadFile(f)
			if err != nil {
				t.Fatal(err)
			}
			p := sitter.NewParser()
			if err := p.SetLanguage(lang); err != nil {
				t.Fatal(err)
			}
			tree := p.Parse(src, nil)
			if tree == nil {
				p.Close()
				continue
			}
			for _, ce := range compiledQueriesFor(projectDir, "go", ".go", lang) {
				qc := sitter.NewQueryCursor()
				matches := qc.Matches(ce.Query, tree.RootNode(), src)
				for {
					m := matches.Next()
					if m == nil {
						break
					}
					for ci := range m.Captures {
						parent := m.Captures[ci].Node.Parent()
						if parent == nil {
							continue
						}
						text := parent.Utf8Text(src)
						e := entityWithSource{Entity: Entity{
							Name:           m.Captures[ci].Node.Utf8Text(src),
							ModifierExport: ModifierExportVerdict("modifier", text, nil, nil),
						}}
						if retain {
							e.Source = text
							bytesHeld += len(text)
						}
						ents = append(ents, e)
					}
				}
				qc.Close()
			}
			tree.Close()
			p.Close()
		}
		return ents, bytesHeld
	}

	liveHeapWith := func(retain bool) (uint64, int, int) {
		ents, held := build(retain)
		runtime.GC()
		runtime.GC()
		var ms runtime.MemStats
		runtime.ReadMemStats(&ms)
		live := ms.HeapAlloc
		runtime.KeepAlive(ents)
		return live, len(ents), held
	}

	withoutHeap, n1, _ := liveHeapWith(false)
	withHeap, n2, held := liveHeapWith(true)

	if n1 == 0 || n1 != n2 {
		t.Skipf("corpus produced %d vs %d entities — not comparable", n1, n2)
	}

	t.Logf("entities built: %d over %d files", n1, len(files))
	t.Logf("live heap, text discarded (current): %d KB", withoutHeap/1024)
	t.Logf("live heap, text retained (old field): %d KB", withHeap/1024)
	if withHeap > withoutHeap {
		t.Logf("difference: %d KB held by the removed field", (withHeap-withoutHeap)/1024)
	}
	t.Logf("body bytes the field pinned: %d KB across %d entities", held/1024, n1)

	if held == 0 {
		t.Error("the retaining arm held nothing — the measurement is not exercising the field")
	}
}
