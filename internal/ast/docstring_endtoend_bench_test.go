package ast

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// BenchmarkParseGoFileEndToEnd measures a whole file parse, not just the
// docstring step.
//
// The component benchmarks above are misleading on their own: moving site
// collection out of a standalone traversal and into the query pass does not make
// the work free, it relocates it. Walking up from each captured name to its
// declaration still costs parent hops, and that cost now sits inside the query
// loop where the component benchmark cannot see it. Only a full parse shows what
// the change is actually worth.
func BenchmarkParseGoFileEndToEnd(b *testing.B) {
	lang, err := resolveTreeSitterLang("go", "tree-sitter-go")
	if err != nil || lang == nil {
		b.Skipf("go grammar unavailable: %v", err)
	}
	queryBody, err := os.ReadFile(filepath.Join("queries", "go.yaml"))
	if err != nil {
		b.Skipf("no go.yaml: %v", err)
	}

	projectDir := b.TempDir()
	qdir := filepath.Join(projectDir, brand.DotDir(), "ast", "queries")
	if err := os.MkdirAll(qdir, 0o755); err != nil {
		b.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(qdir, "go.yaml"), queryBody, 0o644); err != nil {
		b.Fatal(err)
	}

	var sb strings.Builder
	sb.WriteString("package p\n")
	for i := 0; i < 300; i++ {
		n := itoaBench(i)
		sb.WriteString("\n// F" + n + " does something worth documenting.\nfunc F" + n +
			"(a int) int {\n\tif a > 0 {\n\t\treturn a\n\t}\n\treturn 0\n}\n")
	}
	srcPath := filepath.Join(projectDir, "sample.go")
	if err := os.WriteFile(srcPath, []byte(sb.String()), 0o644); err != nil {
		b.Fatal(err)
	}

	restore, ok := tsExtMap[".go"]
	tsExtMap[".go"] = &tsLangConfig{Language: "go", Grammar: "tree-sitter-go", Extensions: []string{".go"}}
	b.Cleanup(func() {
		if ok {
			tsExtMap[".go"] = restore
		} else {
			delete(tsExtMap, ".go")
		}
	})

	p := &TreeSitterParser{projectDir: projectDir}
	if pf, err := p.Parse(srcPath, false, ParseOptions{}); err != nil || pf == nil || pf.EntityCount() == 0 {
		b.Skipf("parse produced nothing (err=%v) — queries or grammar unavailable", err)
	}

	b.ReportAllocs()
	b.SetBytes(int64(sb.Len()))
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		if _, err := p.Parse(srcPath, false, ParseOptions{}); err != nil {
			b.Fatal(err)
		}
	}
}
