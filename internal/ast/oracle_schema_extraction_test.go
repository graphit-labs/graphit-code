package ast

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// The object names here are synthetic. These tests parse a fixture, not the corpus,
// so nothing about a real schema needs to appear in this repository.

// plsqlProject is a project directory carrying THIS repository's plsql.yaml.
//
// The query files are not embedded in the binary — they are read from the runtime
// directory the launcher extracts, with a project-level .graphit/ast/queries taking
// priority (see resolveQueriesForLang). A test that relied on the runtime copy would
// assert against whatever version happens to be installed, and would fail outright in
// a checkout that was never installed.
func plsqlProject(t *testing.T) string {
	t.Helper()

	proj := t.TempDir()
	queries := filepath.Join(proj, brand.DotDir(), "ast", "queries")
	if err := os.MkdirAll(queries, 0o755); err != nil {
		t.Fatalf("queries dir: %v", err)
	}
	yaml, err := os.ReadFile(filepath.Join("queries", "plsql.yaml"))
	if err != nil {
		t.Fatalf("read plsql.yaml: %v", err)
	}
	if err := os.WriteFile(filepath.Join(queries, "plsql.yaml"), yaml, 0o644); err != nil {
		t.Fatalf("write plsql.yaml: %v", err)
	}
	return proj
}

// plsqlParse writes src into the project and parses it with the native PL/SQL grammar.
//
// It calls parseWithConfig directly because the extension-keyed tables the public entry
// points consult are built from the runtime and user query directories only, so a
// project-local grammar is discoverable but not selectable through them.
func plsqlParse(t *testing.T, proj, name, src string) *ParsedFile {
	t.Helper()

	path := filepath.Join(proj, name)
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("mkdir %s: %v", name, err)
	}
	if err := os.WriteFile(path, []byte(src), 0o644); err != nil {
		t.Fatalf("write fixture: %v", err)
	}

	parser := &AntlrParser{projectDir: proj}
	cfg := &antlrLangConfig{
		Language:   "plsql",
		Grammar:    "antlr-plsql",
		Extensions: []string{".sql"},
		StartRule:  "sql_script",
	}
	pf, err := parser.parseWithConfig(path, ".sql", cfg, []byte(src), false, ParseOptions{IndexSource: true})
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	return pf
}

func plsqlFixture(t *testing.T, name, src string) *ParsedFile {
	t.Helper()
	return plsqlParse(t, plsqlProject(t), name, src)
}

// entitiesOfLabel collects the extracted entities carrying a graph label.
func entitiesOfLabel(pf *ParsedFile, label string) []Entity {
	var out []Entity
	for _, ents := range pf.Entities {
		for _, e := range ents {
			if e.GraphLabel == label {
				out = append(out, e)
			}
		}
	}
	return out
}

// TestCreateTableYieldsTableEntityWithItsColumns is the extraction half of the failure
// that aborted a 35358-file index.
//
// The Table query pointed at //create_table/tableview_name while create_table spells
// its name `(schema_name '.')? table_name`, so it matched nothing: zero Table entities
// alongside 75121 Table->Column containment edges, and a CONTAINS rel table group
// naming a node table that was never created ("Binder exception: Table Table does not
// exist"), which took the whole rebuild down with it.
func TestCreateTableYieldsTableEntityWithItsColumns(t *testing.T) {
	pf := plsqlFixture(t, "pedido.sql", `
CREATE TABLE "ACME"."PEDIDO"
   (	"ID_PEDIDO" NUMBER(10,0) NOT NULL ENABLE,
	"DT_PEDIDO" DATE NOT NULL ENABLE,
	"VL_TOTAL" NUMBER(12,2)
   ) ;
`)

	tables := entitiesOfLabel(pf, LabelTable)
	if len(tables) != 1 {
		t.Fatalf("got %d Table entities, want 1: %+v", len(tables), tables)
	}
	table := tables[0]
	if table.Name != "PEDIDO" {
		t.Errorf("table named %q — the schema qualifier is not the object name", table.Name)
	}
	// A declaration is not contained by itself; a self-context also spells the uid
	// path::PEDIDO.PEDIDO, which nothing else points at.
	if table.Context != "" {
		t.Errorf("table carries context %q/%q, want none", table.Context, table.ContextType)
	}

	columns := entitiesOfLabel(pf, LabelColumn)
	if len(columns) != 3 {
		t.Fatalf("got %d Column entities, want 3: %+v", len(columns), columns)
	}
	for _, c := range columns {
		if c.Context != "PEDIDO" || c.ContextType != LabelTable {
			t.Errorf("column %s belongs to %q/%q, want PEDIDO/Table", c.Name, c.Context, c.ContextType)
		}
	}
}

// TestCreateViewYieldsViewEntity: create_view names the view with a bare id_expression,
// so the tableview_name query matched nothing here either and a corpus of views produced
// only their comments.
func TestCreateViewYieldsViewEntity(t *testing.T) {
	pf := plsqlFixture(t, "vw_pedido.sql", `
CREATE OR REPLACE FORCE VIEW "ACME"."VW_PEDIDO" ("ID_PEDIDO", "VL_TOTAL") AS
  SELECT ID_PEDIDO, VL_TOTAL FROM ACME.PEDIDO WHERE VL_TOTAL > 0;
`)

	views := entitiesOfLabel(pf, LabelView)
	if len(views) != 1 {
		t.Fatalf("got %d View entities, want 1: %+v", len(views), views)
	}
	if views[0].Name != "VW_PEDIDO" {
		t.Errorf("view named %q, want VW_PEDIDO", views[0].Name)
	}
}

// TestQualifiedReferencesNameTheObject covers the other half: what a reference points
// at. Reading the first terminal of `identifier '.' id_expression` gave the SCHEMA, so
// every DML edge in a single-schema export converged on one node named after it.
func TestQualifiedReferencesNameTheObject(t *testing.T) {
	pf := plsqlFixture(t, "pck_venda.sql", `
CREATE OR REPLACE PACKAGE BODY "ACME"."PCK_VENDA" AS
  PROCEDURE FATURAR(P_ID NUMBER) IS
    V_TOTAL NUMBER;
  BEGIN
    SELECT VL_TOTAL INTO V_TOTAL FROM ACME.PEDIDO WHERE ID_PEDIDO = P_ID;
    UPDATE ACME.PEDIDO SET VL_TOTAL = 0 WHERE ID_PEDIDO = P_ID;
    INSERT INTO ACME.FATURA (ID_PEDIDO) VALUES (P_ID);
  END;
END PCK_VENDA;
`)

	byType := map[string][]string{}
	for _, ref := range pf.References {
		byType[ref.RelType] = append(byType[ref.RelType], ref.TargetName)
	}
	for _, want := range []struct{ relType, target string }{
		{"SELECTS", "PEDIDO"},
		{"UPDATES", "PEDIDO"},
		{"INSERTS", "FATURA"},
	} {
		found := false
		for _, got := range byType[want.relType] {
			if got == want.target {
				found = true
			}
			if got == "ACME" {
				t.Errorf("%s points at the schema ACME instead of an object", want.relType)
			}
		}
		if !found {
			t.Errorf("no %s reference to %s; got %v", want.relType, want.target, byType[want.relType])
		}
	}

	// The package body declares the procedure, and the procedure owns its local —
	// neither of which resolved to a real name before: a package-level body has no
	// "_name" child, so the context came out as the keyword PROCEDURE.
	procs := entitiesOfLabel(pf, LabelProcedure)
	if len(procs) != 1 || procs[0].Name != "FATURAR" {
		t.Fatalf("got %+v, want one Procedure named FATURAR", procs)
	}
	if procs[0].Context != "PCK_VENDA" || procs[0].ContextType != LabelPackage {
		t.Errorf("procedure belongs to %q/%q, want PCK_VENDA/Package", procs[0].Context, procs[0].ContextType)
	}
	for _, v := range entitiesOfLabel(pf, LabelVariable) {
		if v.Context != "FATURAR" {
			t.Errorf("variable %s belongs to %q, want FATURAR", v.Name, v.Context)
		}
	}
}

// TestCreateSequenceAndTriggerAreNamedAfterTheObject pins the two other declarations
// whose whole name came out as the schema: every Sequence in the reference export was
// named GC, and every trigger — plus the variables inside it — was attributed to GC.
func TestCreateSequenceAndTriggerAreNamedAfterTheObject(t *testing.T) {
	pf := plsqlFixture(t, "objetos.sql", `
CREATE SEQUENCE "ACME"."SEQ_PEDIDO" MINVALUE 1 INCREMENT BY 1 START WITH 1;

CREATE OR REPLACE TRIGGER "ACME"."TRG_PEDIDO_BI"
BEFORE INSERT ON ACME.PEDIDO FOR EACH ROW
DECLARE
  V_SEQ NUMBER;
BEGIN
  SELECT SEQ_PEDIDO.NEXTVAL INTO V_SEQ FROM DUAL;
  :NEW.ID_PEDIDO := V_SEQ;
END;
`)

	seqs := entitiesOfLabel(pf, LabelSequence)
	if len(seqs) != 1 || seqs[0].Name != "SEQ_PEDIDO" {
		t.Fatalf("got %+v, want one Sequence named SEQ_PEDIDO", seqs)
	}

	triggers := entitiesOfLabel(pf, LabelTrigger)
	if len(triggers) != 1 || triggers[0].Name != "TRG_PEDIDO_BI" {
		t.Fatalf("got %+v, want one Trigger named TRG_PEDIDO_BI", triggers)
	}
	for _, v := range entitiesOfLabel(pf, LabelVariable) {
		if v.Context != "TRG_PEDIDO_BI" {
			t.Errorf("variable %s belongs to %q, want TRG_PEDIDO_BI", v.Name, v.Context)
		}
	}
}
