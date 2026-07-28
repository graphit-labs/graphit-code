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
func commentsOf(t *testing.T, pf *ParsedFile) (map[string]bool, map[string]string) {
	t.Helper()
	names := map[string]bool{}
	for _, ents := range pf.Entities {
		for _, e := range ents {
			if e.GraphLabel == LabelComment {
				names[e.Name] = true
			}
		}
	}
	targets := map[string]string{}
	for _, r := range pf.References {
		if r.RelType == "REFERENCES" && names[r.SourceName] {
			targets[r.SourceName] = r.TargetName
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

// Licença deste arquivo.

// Alfa faz alfa.
func Alfa() {
	// nota solta no corpo
	_ = 1
}
`,
			want: map[string]string{
				"Licença deste arquivo.": "a.go",
				"Alfa faz alfa.":         "Alfa",
				"nota solta no corpo":    "a.go",
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
