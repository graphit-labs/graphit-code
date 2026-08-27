package ast

import (
	"strings"
	"testing"

	sitter "github.com/tree-sitter/go-tree-sitter"
)

func benchDocstringTree(b *testing.B) (*sitter.Node, []byte, *ExternalQueryFile, *sitter.Language, []Entity) {
	b.Helper()
	lang, err := resolveTreeSitterLang("go", "tree-sitter-go")
	if err != nil || lang == nil {
		b.Skipf("go grammar unavailable: %v", err)
	}
	var sb strings.Builder
	sb.WriteString("package p\n")
	ents := make([]Entity, 0, 300)
	line := 2
	for i := 0; i < 300; i++ {
		sb.WriteString("\n// Doc comment for symbol.\nfunc F" + itoaBench(i) + "() { x := 1; _ = x }\n")
		line += 3
		ents = append(ents, Entity{Name: "F" + itoaBench(i), Line: line, GraphLabel: "Function"})
	}
	src := []byte(sb.String())
	p := sitter.NewParser()
	_ = p.SetLanguage(lang)
	tree := p.Parse(src, nil)
	b.Cleanup(func() { tree.Close(); p.Close() })
	return tree.RootNode(), src, &ExternalQueryFile{DeclarationTypes: []string{"function_declaration"}}, lang, ents
}

func itoaBench(i int) string {
	if i == 0 {
		return "0"
	}
	var b [8]byte
	pos := len(b)
	for i > 0 {
		pos--
		b[pos] = byte('0' + i%10)
		i /= 10
	}
	return string(b[pos:])
}

// BenchmarkExtractDocstringsTS measures what the query pass now pays: the sites
// are already in hand, so only they are examined.
func BenchmarkExtractDocstringsTS(b *testing.B) {
	root, src, cfg, lang, ents := benchDocstringTree(b)
	m := newDocstringMatchers(cfg, lang)
	sites := collectDeclSites(root, m)
	b.ReportAllocs()
	b.SetBytes(int64(len(src)))
	for i := 0; i < b.N; i++ {
		cp := make([]Entity, len(ents))
		copy(cp, ents)
		pf := &ParsedFile{Entities: map[string][]Entity{"functions": cp}}
		attachDocstringsTS(sites, src, pf, m)
	}
}

// BenchmarkExtractDocstringsTSWithScan adds back the cost the old code paid on
// every file: scanning the whole tree to find those same sites.
func BenchmarkExtractDocstringsTSWithScan(b *testing.B) {
	root, src, cfg, lang, ents := benchDocstringTree(b)
	m := newDocstringMatchers(cfg, lang)
	b.ReportAllocs()
	b.SetBytes(int64(len(src)))
	for i := 0; i < b.N; i++ {
		cp := make([]Entity, len(ents))
		copy(cp, ents)
		pf := &ParsedFile{Entities: map[string][]Entity{"functions": cp}}
		attachDocstringsTS(collectDeclSites(root, m), src, pf, m)
	}
}

func BenchmarkExtractDocstringsTSLegacy(b *testing.B) {
	root, src, cfg, _, ents := benchDocstringTree(b)
	b.ReportAllocs()
	b.SetBytes(int64(len(src)))
	for i := 0; i < b.N; i++ {
		cp := make([]Entity, len(ents))
		copy(cp, ents)
		pf := &ParsedFile{Entities: map[string][]Entity{"functions": cp}}
		legacyExtractDocstringsTS(root, src, pf, cfg)
	}
}
