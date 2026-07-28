package ast

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// The differential in docstring_equiv_test.go feeds attachDocstringsTS the sites
// a whole-tree scan finds, which pins the pairing rule but says nothing about the
// change that actually happened: production no longer scans, it collects the
// declaration around each captured name.
//
// This parses real source through the real query pipeline, so a capture whose
// declaration sits more than one level up — or which declSiteFor fails to reach
// at all — shows up as a missing docstring here and nowhere else.
//
// The queries are staged into the project's own .graphit/ast/queries, which takes
// priority over the installed runtime, and the extension is registered by hand.
// That second step is not test scaffolding for its own sake: initTsExtMap builds
// the extension table at package init from the installed runtime alone, so a
// project-level query file can describe a language that the parser then refuses
// to open. Without the injection this test would only run on a machine where the
// launcher had already unpacked a runtime.
func TestDocstringsSurviveTheRealQueryPipeline(t *testing.T) {
	repoQueries, err := filepath.Abs(filepath.Join("queries"))
	if err != nil {
		t.Fatal(err)
	}

	cases := []struct {
		lang, queryFile, ext, fileName, source string
		want                                   map[string]string
	}{
		{
			lang: "go", queryFile: "go.yaml", ext: ".go", fileName: "sample.go",
			source: `package p

// Alpha does alpha things.
func Alpha() {}

// Widget is a documented type.
type Widget struct{}

func Undocumented() {}
`,
			want: map[string]string{
				"Alpha": "Alpha does alpha things.",
				// KNOWN DEFECT, predates the change this file was written for —
				// verified identical against the previous implementation. The query
				// captures the type name, whose nearest declaration ancestor is
				// type_spec, but the doc comment is a sibling of the enclosing
				// type_declaration. Neither the old whole-tree scan nor the current
				// site collection bridges that gap, so exported Go types carry no
				// documentation into the index.
				"Widget":       "",
				"Undocumented": "",
			},
		},
		{
			lang: "python", queryFile: "python.yaml", ext: ".py", fileName: "sample.py",
			source: `def alpha():
    """Alpha docstring."""
    pass


# beta comment
def beta(x):
    return x
`,
			want: map[string]string{
				// cleanDocstring used to strip only opening markers, so this came
				// back as `Alpha docstring."""`. It now strips closing ones too.
				"alpha": "Alpha docstring.",
				"beta":  "beta comment",
			},
		},
	}

	for _, tc := range cases {
		t.Run(tc.lang, func(t *testing.T) {
			qsrc := filepath.Join(repoQueries, tc.queryFile)
			body, err := os.ReadFile(qsrc)
			if err != nil {
				t.Skipf("no query file %s: %v", tc.queryFile, err)
			}

			if lang, err := resolveTreeSitterLang(tc.lang, "tree-sitter-"+tc.lang); err != nil || lang == nil {
				t.Skipf("%s grammar unavailable: %v", tc.lang, err)
			}
			restore, ok := tsExtMap[tc.ext]
			tsExtMap[tc.ext] = &tsLangConfig{
				Language: tc.lang, Grammar: "tree-sitter-" + tc.lang, Extensions: []string{tc.ext},
			}
			t.Cleanup(func() {
				if ok {
					tsExtMap[tc.ext] = restore
				} else {
					delete(tsExtMap, tc.ext)
				}
			})

			projectDir := t.TempDir()
			qdir := filepath.Join(projectDir, brand.DotDir(), "ast", "queries")
			if err := os.MkdirAll(qdir, 0o755); err != nil {
				t.Fatal(err)
			}
			if err := os.WriteFile(filepath.Join(qdir, tc.queryFile), body, 0o644); err != nil {
				t.Fatal(err)
			}

			srcPath := filepath.Join(projectDir, tc.fileName)
			if err := os.WriteFile(srcPath, []byte(tc.source), 0o644); err != nil {
				t.Fatal(err)
			}

			pf, err := NewCompositeParser(projectDir, nil).Parse(srcPath, false, ParseOptions{})
			if err != nil {
				t.Fatalf("parse: %v", err)
			}
			if pf == nil || pf.EntityCount() == 0 {
				t.Skipf("no entities extracted for %s — grammar unavailable on this machine", tc.lang)
			}

			got := map[string]string{}
			for _, ents := range pf.Entities {
				for _, e := range ents {
					if _, interesting := tc.want[e.Name]; interesting {
						got[e.Name] = e.Docstring
					}
				}
			}

			documented := 0
			for name, want := range tc.want {
				have, ok := got[name]
				if !ok {
					t.Errorf("%s: entity %q was not extracted at all", tc.lang, name)
					continue
				}
				if have != want {
					t.Errorf("%s: %q docstring = %q, want %q", tc.lang, name, have, want)
				}
				if want != "" {
					documented++
				}
			}
			if documented == 0 {
				t.Errorf("%s: no docstring was expected by this case — it would not catch a regression", tc.lang)
			}
			t.Logf("%s: %d documented entities matched through the real pipeline", tc.lang, documented)
		})
	}
}
