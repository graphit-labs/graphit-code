package ast

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"gopkg.in/yaml.v3"
)

func TestKeywordIsNotACallTarget(t *testing.T) {
	fragment := plsqlFixture(t, "fragment.sql",
		`PROCEDURE CGFK$CHK(p_field_level IN BOOLEAN) IS BEGIN IF 1 = 1 THEN `+
			`pck_pedido.pr_grava(1); END IF; COMMIT; END;`)
	assertNoKeywordCalls(t, fragment, "bare fragment")

	whole := plsqlFixture(t, "whole.sql", `
CREATE OR REPLACE PROCEDURE p_grava IS
BEGIN
  IF 1 = 1 THEN
    pck_pedido.pr_grava(1);
  END IF;
  COMMIT;
END;
`)
	assertNoKeywordCalls(t, whole, "parseable unit")

	var got []string
	for _, c := range whole.CallSites {
		got = append(got, c.Name)
	}
	found := false
	for _, name := range got {
		if strings.Contains(strings.ToLower(name), "pr_grava") {
			found = true
		}
	}
	if !found {
		t.Errorf("the real call is gone; calls: %v", got)
	}
}

func assertNoKeywordCalls(t *testing.T, pf *ParsedFile, what string) {
	t.Helper()
	forbidden := map[string]bool{
		"BEGIN": true, "begin": true, "DECLARE": true, "END": true, "IF": true,
		"if": true, "THEN": true, "COMMIT": true, "PROCEDURE": true, "procedure": true,
		"FUNCTION": true, "PACKAGE": true, ".": true,
	}
	var all []string
	for _, c := range pf.CallSites {
		all = append(all, c.Name)
	}
	for _, name := range all {
		if forbidden[name] {
			t.Errorf("[%s] %q was indexed as a call target; calls: %v", what, name, all)
		}
	}
}

// The filter is the GRAMMAR's, not the engine's: no list of words in Go. `end` is a
// keyword in PL/SQL and a fine function name in Ruby, so only the language can say.
func TestNameRejectIsDeclaredByTheGrammar(t *testing.T) {
	raw, err := os.ReadFile(filepath.Join("queries", "plsql.yaml"))
	if err != nil {
		t.Skipf("no plsql.yaml: %v", err)
	}
	var qf ExternalQueryFile
	if err := yaml.Unmarshal(raw, &qf); err != nil {
		t.Fatal(err)
	}

	calls := 0
	for _, q := range qf.Queries {
		if q.RelationType != RelCalls || q.DataKey != "calls" {
			continue
		}
		calls++
		if q.NameReject == "" {
			t.Errorf("the %q query at pattern %q declares no name_reject; a bare "+
				"identifier in statement position is a complete call in this grammar",
				q.DataKey, q.Pattern)
			continue
		}
		re := nameRejectMatcher(q.NameReject)
		if re == nil {
			t.Errorf("name_reject does not compile: %q", q.NameReject)
			continue
		}
		for _, word := range []string{"BEGIN", "begin", "IF", "PROCEDURE", "DECLARE",
			"FUNCTION", "RETURN", "COMMIT", "."} {
			if !re.MatchString(word) {
				t.Errorf("name_reject %q does not reject %q", q.NameReject, word)
			}
		}
		for _, name := range []string{"if_valida_cliente", "pr_begin_lote", "PCK_END_MES",
			"procedure_helper", "_privado"} {
			if re.MatchString(name) {
				t.Errorf("name_reject %q rejects the legitimate name %q — the expression "+
					"is not anchored", q.NameReject, name)
			}
		}
	}
	if calls == 0 {
		t.Fatal("no calls query found in plsql.yaml; this test has stopped testing anything")
	}
}

// A trigger is not callable in PL/SQL — it is fired by DML on its table, and no program
// invokes it by name. Declaring it as a possible CALLS target is what let every `begin`
// in the corpus resolve onto a real trigger whose name is the quoted identifier "BEGIN":
// 9092 false edges, zero true ones lost by removing it.
func TestPLSQLCallsCannotTargetATrigger(t *testing.T) {
	raw, err := os.ReadFile(filepath.Join("queries", "plsql.yaml"))
	if err != nil {
		t.Skipf("no plsql.yaml: %v", err)
	}
	var qf ExternalQueryFile
	if err := yaml.Unmarshal(raw, &qf); err != nil {
		t.Fatal(err)
	}
	rule, ok := qf.TargetRules[RelCalls]
	if !ok {
		t.Fatal("plsql.yaml declares no target_rules for CALLS")
	}
	for _, l := range rule.Labels {
		if l == LabelTrigger {
			t.Errorf("CALLS may target %s; labels = %v", LabelTrigger, rule.Labels)
		}
	}
	for _, want := range []string{LabelFunction, LabelProcedure, "Package"} {
		found := false
		for _, l := range rule.Labels {
			if l == want {
				found = true
			}
		}
		if !found {
			t.Errorf("CALLS can no longer target %s; labels = %v", want, rule.Labels)
		}
	}
}

// A filter that does not compile must be dropped at LOAD time, with a warning. Left in
// place it reads like protection and lets everything through — which is the failure mode
// this whole change is about.
func TestNameRejectThatDoesNotCompileIsDropped(t *testing.T) {
	projectDir := t.TempDir()
	qdir := filepath.Join(projectDir, brand.DotDir(), "ast", "queries")
	if err := os.MkdirAll(qdir, 0o755); err != nil {
		t.Fatal(err)
	}
	body := `language: probe_lang
extensions: [".probe"]
queries:
  - data_key: calls
    graph_label: ""
    pattern: "//call_statement/routine_name"
    name_capture: name
    type: relation
    relation_type: CALLS
    name_reject: '(?i)^(unclosed'
`
	if err := os.WriteFile(filepath.Join(qdir, "probe_lang.yaml"), []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}

	files, err := loadQueriesFromDir(qdir)
	if err != nil {
		t.Fatal(err)
	}
	if len(files) != 1 {
		t.Fatalf("got %d query files, want 1", len(files))
	}
	if got := files[0].Queries[0].NameReject; got != "" {
		t.Errorf("name_reject = %q, want it dropped: the expression does not compile", got)
	}
	if re := nameRejectMatcher("(?i)^(unclosed"); re != nil {
		t.Error("nameRejectMatcher compiled an invalid expression")
	}
}
