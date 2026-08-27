package ast

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// The ANTLR adapter ignored value_capture entirely, so the declared defaults of an
// enterprise schema were parsed and then dropped: a column told you it existed,
// never that it is born 'ABERTO'; a PL/SQL constant never that it is 42; a COBOL
// data item never its VALUE clause, which is how COBOL declares a constant at all.
//
// value_capture on this side is a rule path, not a capture name, because an ANTLR
// match is a subtree rather than a capture list.

// stageAntlr stages an ANTLR query file into a fresh project.
//
// parseWithConfig is called directly, as plsqlParse already does: the
// extension-keyed tables the public entry points consult are built from the
// runtime and user query directories only, so a project-local grammar is
// discoverable but not selectable through them.
func stageAntlr(t *testing.T, queryFile string) string {
	t.Helper()
	body, err := os.ReadFile(filepath.Join("queries", queryFile))
	if err != nil {
		t.Skipf("no %s: %v", queryFile, err)
	}
	projectDir := t.TempDir()
	qdir := filepath.Join(projectDir, brand.DotDir(), "ast", "queries")
	if err := os.MkdirAll(qdir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(qdir, queryFile), body, 0o644); err != nil {
		t.Fatal(err)
	}
	InvalidateQueryCaches()
	t.Cleanup(InvalidateQueryCaches)
	return projectDir
}

func parseAntlrFixture(t *testing.T, proj, name, src string, cfg *antlrLangConfig) *ParsedFile {
	t.Helper()
	path := filepath.Join(proj, name)
	if err := os.WriteFile(path, []byte(src), 0o644); err != nil {
		t.Fatal(err)
	}
	pf, err := (&AntlrParser{projectDir: proj}).parseWithConfig(
		path, cfg.Extensions[0], cfg, []byte(src), false, ParseOptions{})
	if err != nil {
		t.Skipf("%s parse unavailable: %v", cfg.Language, err)
	}
	return pf
}

// antlrValues returns, for each entity name, the value on the entity, and the
// names of the Value nodes contained by it.
func antlrValues(pf *ParsedFile) (map[string]string, map[string][]string) {
	onEntity := map[string]string{}
	contained := map[string][]string{}
	for _, ents := range pf.Entities {
		for _, e := range ents {
			if v := e.Properties["value"]; v != "" {
				onEntity[e.Name] = v
			}
			if e.GraphLabel == "Value" && e.Context != "" {
				contained[e.Context] = append(contained[e.Context], e.Name)
			}
		}
	}
	return onEntity, contained
}

func TestAntlrDeclaredDefaultsBecomeNodes(t *testing.T) {
	cases := []struct {
		name, queryFile, file, source string
		cfg                           *antlrLangConfig
		want                          map[string]string
		// reject names that must NOT carry a value: a CHECK constraint is not a
		// default, and reading one as a default is what a descendant search does.
		reject []string
		// wantLabel pins the label a declaration lands under, which for Oracle
		// depends on a keyword the rule name does not carry.
		wantLabel map[string]string
	}{
		{
			name: "plsql", queryFile: "plsql.yaml", file: "t.sql",
			cfg: &antlrLangConfig{Language: "plsql", Grammar: "antlr-plsql",
				Extensions: []string{".sql"}, StartRule: "sql_script"},
			source: `CREATE TABLE PEDIDO (
  ID NUMBER(10) NOT NULL,
  STATUS VARCHAR2(10) DEFAULT 'ABERTO',
  VL NUMBER(12,2) CHECK (VL > 0)
);
CREATE OR REPLACE PACKAGE PKG AS
  C_MAX CONSTANT NUMBER := 42;
  V_URL VARCHAR2(100) := 'https://api.acme.com';
END PKG;
`,
			want: map[string]string{
				"STATUS": "ABERTO",
				"C_MAX":  "42",
				"V_URL":  "https://api.acme.com",
			},
			reject:    []string{"VL", "ID"},
			wantLabel: map[string]string{"C_MAX": "Constant", "V_URL": "Variable"},
		},
		{
			name: "tsql", queryFile: "tsql.yaml", file: "u.sql",
			cfg: &antlrLangConfig{Language: "tsql", Grammar: "antlr-tsql",
				Extensions: []string{".sql"}, StartRule: "tsql_file"},
			source: `CREATE TABLE Pedido (
  Id INT NOT NULL,
  Status VARCHAR(10) DEFAULT 'ABERTO'
);
`,
			want:   map[string]string{"Status": "ABERTO"},
			reject: []string{"Id"},
		},
		{
			name: "postgresql", queryFile: "postgresql.yaml", file: "p.sql",
			cfg: &antlrLangConfig{Language: "postgresql", Grammar: "antlr-postgresql",
				Extensions: []string{".sql"}, StartRule: "root"},
			source: `CREATE TABLE pedido (
  id integer NOT NULL,
  status varchar(10) DEFAULT 'ABERTO',
  vl numeric(12,2) CHECK (vl > 0)
);
`,
			want:   map[string]string{"status": "ABERTO"},
			reject: []string{"id", "vl"},
		},
		{
			name: "cobol85", queryFile: "cobol85.yaml", file: "t.cbl",
			cfg: &antlrLangConfig{Language: "cobol85", Grammar: "antlr-cobol85",
				Extensions: []string{".cbl"}, StartRule: "startRule"},
			source: `       IDENTIFICATION DIVISION.
       PROGRAM-ID. TESTE.
       DATA DIVISION.
       WORKING-STORAGE SECTION.
       01 WS-STATUS PIC X(10) VALUE 'ABERTO'.
       01 WS-MAX    PIC 9(4)  VALUE 42.
       01 WS-PLAIN  PIC X(10).
       PROCEDURE DIVISION.
           STOP RUN.
`,
			want:   map[string]string{"WS-STATUS": "ABERTO", "WS-MAX": "42"},
			reject: []string{"WS-PLAIN"},
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			projectDir := stageAntlr(t, tc.queryFile)
			pf := parseAntlrFixture(t, projectDir, tc.file, tc.source, tc.cfg)
			if pf == nil {
				t.Skipf("%s produced no parse", tc.name)
			}

			onEntity, contained := antlrValues(pf)
			for key, want := range tc.want {
				if got := onEntity[key]; got != want {
					t.Errorf("%s carries value %q, want %q", key, got, want)
				}
				if !containsStr(contained[key], want) {
					t.Errorf("no Value node %q under %q; got %v", want, key, contained[key])
				}
			}
			for _, key := range tc.reject {
				if got := onEntity[key]; got != "" {
					t.Errorf("%s has no default, yet carries value %q", key, got)
				}
			}

			labels := map[string]string{}
			for _, ents := range pf.Entities {
				for _, e := range ents {
					if e.GraphLabel != "" && e.GraphLabel != "Value" {
						labels[e.Name] = e.GraphLabel
					}
				}
			}
			for name, want := range tc.wantLabel {
				if got := labels[name]; got != want {
					t.Errorf("%s is labelled %q, want %q", name, got, want)
				}
			}
		})
	}
}
