package ast

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// Comments are entities in every language, not just in PL/SQL.
//
// PL/SQL already turned COMMENT ON statements into Comment entities whose name is
// the comment text, so "what does the documentation say" is answerable by search.
// Everywhere else a comment was only ever attached to a declaration as a
// Docstring field — which means a comment that documents nothing, a licence
// header, a note inside a function body, was not indexed at all.
//
// Each comment now also carries a REFERENCES edge: to the declaration it
// precedes when there is one, and to the file otherwise, so nothing is left
// unreachable for want of an owner.

func stageLang(t *testing.T, langName, ext, queryFile string) string {
	t.Helper()
	body, err := os.ReadFile(filepath.Join("queries", queryFile))
	if err != nil {
		t.Skipf("no %s: %v", queryFile, err)
	}
	if lang, err := resolveTreeSitterLang(langName, "tree-sitter-"+langName); err != nil || lang == nil {
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
		Language: langName, Grammar: "tree-sitter-" + langName, Extensions: []string{ext},
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

// commentsOf returns the Comment entities and the edge target of each.
//
// Matched by LINE, not by ReferenceInfo.SourceName == the comment's own text:
// SourceName carries a disambiguator (commentUIDName) instead of the raw text
// precisely so that two comments with identical text don't collide — see
// cache_convert.go's contentNamedUID / commentUIDName. Line is the value both the
// entity and its reference already carry independently and is what production
// code (cache_convert.go) also uses to make the two agree.
func commentsOf(t *testing.T, pf *ParsedFile) (map[string]bool, map[string]string) {
	t.Helper()
	names := map[string]bool{}
	lineToName := map[int]string{}
	for _, ents := range pf.Entities {
		for _, e := range ents {
			if e.GraphLabel == LabelComment {
				names[e.Name] = true
				lineToName[e.Line] = e.Name
			}
		}
	}
	targets := map[string]string{}
	for _, r := range pf.References {
		if r.RelType != "REFERENCES" {
			continue
		}
		if name, ok := lineToName[r.Line]; ok {
			targets[name] = r.TargetName
		}
	}
	return names, targets
}

func TestCommentsAreEntitiesInEveryLanguage(t *testing.T) {
	cases := []struct {
		lang, ext, queryFile, file, source string
		// wantComment -> the entity it must point at, or "" for the file
		want map[string]string
	}{
		{
			lang: "go", ext: ".go", queryFile: "go.yaml", file: "a.go",
			source: `package p

// This file's licence.

// Alfa does alfa.
func Alfa() {
	// a loose note in the body
	_ = 1
}
`,
			want: map[string]string{
				"This file's licence.":     "a.go",
				"Alfa does alfa.":          "Alfa",
				"a loose note in the body": "a.go",
			},
		},
		{
			lang: "python", ext: ".py", queryFile: "python.yaml", file: "b.py",
			source: `# cabeçalho do módulo

# beta faz beta.
def beta():
    # nota interna
    return 1
`,
			want: map[string]string{
				"cabeçalho do módulo": "b.py",
				"beta faz beta.":      "beta",
				"nota interna":        "b.py",
			},
		},
	}

	for _, tc := range cases {
		t.Run(tc.lang, func(t *testing.T) {
			projectDir := stageLang(t, tc.lang, tc.ext, tc.queryFile)
			srcPath := filepath.Join(projectDir, tc.file)
			if err := os.WriteFile(srcPath, []byte(tc.source), 0o644); err != nil {
				t.Fatal(err)
			}

			pf, err := NewCompositeParser(projectDir, nil).Parse(srcPath, false, ParseOptions{})
			if err != nil {
				t.Fatalf("parse: %v", err)
			}
			names, targets := commentsOf(t, pf)

			for want, wantTarget := range tc.want {
				if !names[want] {
					t.Errorf("comment %q was not indexed; got %v", want, keysOf(names))
					continue
				}
				if got := targets[want]; got != wantTarget {
					t.Errorf("comment %q points at %q, want %q", want, got, wantTarget)
				}
			}
		})
	}
}

// The comment marker must not survive into the name — the name is what a person
// reads in a search result.
func TestCommentNamesCarryNoMarkers(t *testing.T) {
	projectDir := stageLang(t, "python", ".py", "python.yaml")
	srcPath := filepath.Join(projectDir, "c.py")
	if err := os.WriteFile(srcPath, []byte(`def gama():
    """Docstring de uma linha."""
    return 2
`), 0o644); err != nil {
		t.Fatal(err)
	}

	pf, err := NewCompositeParser(projectDir, nil).Parse(srcPath, false, ParseOptions{})
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	for _, ents := range pf.Entities {
		for _, e := range ents {
			if e.Docstring == "" {
				continue
			}
			for _, marker := range []string{`"""`, "'''", "*/", "//", "#"} {
				if hasEdgeMarker(e.Docstring, marker) {
					t.Errorf("docstring of %q still carries %q: %q", e.Name, marker, e.Docstring)
				}
			}
		}
	}
}

func hasEdgeMarker(s, marker string) bool {
	return len(s) >= len(marker) &&
		(s[:len(marker)] == marker || s[len(s)-len(marker):] == marker)
}

func keysOf(m map[string]bool) []string {
	out := make([]string, 0, len(m))
	for k := range m {
		out = append(out, k)
	}
	return out
}

// The ANTLR engine reaches comments by a different route: they sit on the lexer's
// hidden channel and never enter the parse tree, so the driver reads them back
// out of the buffered token stream after the parse. This checks the whole route,
// from lexer channel to indexed entity with an edge.
func TestCommentsAreEntitiesInAntlrLanguages(t *testing.T) {
	drv := nativeAntlrDrivers["antlr-plsql"]
	if drv == nil {
		t.Skip("antlr-plsql driver not built into this binary")
	}

	src := []byte(`-- Cabeçalho do pacote.

/* Bloco explicativo
   em duas linhas. */
CREATE OR REPLACE FUNCTION SOMA_VALORES(a IN NUMBER, b IN NUMBER) RETURN NUMBER IS
BEGIN
  -- nota dentro do corpo
  RETURN a + b;
END;
/
`)

	tree, err := drv.Parse(src)
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	if tree == nil {
		t.Fatal("nil tree")
	}
	if len(tree.Comments) == 0 {
		t.Fatal("the driver recovered no comments — hidden-channel tokens are not being " +
			"read back from the stream")
	}

	got := map[string]bool{}
	for _, c := range tree.Comments {
		got[cleanDocstring(c.Text)] = true
	}
	for _, want := range []string{
		"Cabeçalho do pacote.",
		"nota dentro do corpo",
	} {
		if !got[want] {
			t.Errorf("comment %q not recovered; got %v", want, keysOf(got))
		}
	}

	// And the adapter turns them into entities with an edge.
	result := &ParsedFile{Path: "pkg.sql", Entities: map[string][]Entity{
		"functions": {{Name: "SOMA_VALORES", Line: 5, GraphLabel: "Function"}},
	}}
	extractCommentsAntlr(tree, result, "pkg.sql")

	names, targets := commentsOf(t, result)
	if len(names) == 0 {
		t.Fatal("no Comment entity produced from the ANTLR tree")
	}
	if !names["Cabeçalho do pacote."] {
		t.Errorf("header comment was not indexed; got %v", keysOf(names))
	}
	// The header is far from the function and belongs to the file; the block
	// comment sits directly above it and belongs to the function.
	if tgt := targets["Cabeçalho do pacote."]; tgt != "pkg.sql" {
		t.Errorf("header comment points at %q, want the file", tgt)
	}
	for name, tgt := range targets {
		t.Logf("  %-28q -> %s", name, tgt)
	}
}

// Two comments with identical text, at different positions, must both survive as
// distinct entities with their own REFERENCES edge — not collapse into one, the
// way keying extraction's dedup on text used to make them.
func TestRepeatedIdenticalCommentsAreBothIndexedAntlr(t *testing.T) {
	drv := nativeAntlrDrivers["antlr-plsql"]
	if drv == nil {
		t.Skip("antlr-plsql driver not built into this binary")
	}

	src := []byte(`-- nota repetida
CREATE OR REPLACE FUNCTION UM(a IN NUMBER) RETURN NUMBER IS
BEGIN
  RETURN a;
END;
/

-- nota repetida
CREATE OR REPLACE FUNCTION DOIS(a IN NUMBER) RETURN NUMBER IS
BEGIN
  RETURN a;
END;
/
`)

	tree, err := drv.Parse(src)
	if err != nil {
		t.Fatalf("parse: %v", err)
	}

	result := &ParsedFile{Path: "pkg.sql", Entities: map[string][]Entity{
		"functions": {
			{Name: "UM", Line: 2, GraphLabel: "Function"},
			{Name: "DOIS", Line: 9, GraphLabel: "Function"},
		},
	}}
	extractCommentsAntlr(tree, result, "pkg.sql")

	var comments []Entity
	for _, e := range result.Entities["comments"] {
		if e.GraphLabel == LabelComment {
			comments = append(comments, e)
		}
	}
	if len(comments) != 2 {
		t.Fatalf("expected 2 distinct Comment entities for the repeated note, got %d", len(comments))
	}
	if comments[0].Line == comments[1].Line {
		t.Fatalf("both comment entities report the same line: %d", comments[0].Line)
	}

	// Each must have its OWN reference, not one shared/dropped one — attachment
	// target (file vs. the following declaration) is a different, untouched piece
	// of logic; what this test guards is that neither comment's reference vanished.
	var refLines []int
	for _, r := range result.References {
		if r.RelType == "REFERENCES" {
			refLines = append(refLines, r.Line)
		}
	}
	if len(refLines) != 2 {
		t.Fatalf("expected 2 REFERENCES edges (one per comment), got %d: %v", len(refLines), refLines)
	}
	if refLines[0] == refLines[1] {
		t.Fatalf("both REFERENCES edges report the same line: %d — the second comment's edge was dropped or duplicated the first's", refLines[0])
	}
}
