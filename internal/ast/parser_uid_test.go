package ast

import (
	"strings"
	"testing"
)

func assertDistinctParsedUIDs(t *testing.T, first, second *parseCacheEntry, label, name string) {
	t.Helper()
	find := func(entry *parseCacheEntry) string {
		for _, entity := range entry.Entities {
			if entity.UID == "" {
				t.Fatalf("%s parser emitted an entity without uid: %+v", entry.Language, entity)
			}
			if entity.Label == label && strings.EqualFold(entity.Name, name) {
				return entity.UID
			}
		}
		t.Fatalf("%s parser did not emit %s %s: %+v", entry.Language, label, name, entry.Entities)
		return ""
	}
	a, b := find(first), find(second)
	if a == b {
		t.Fatalf("homonymous %s entities share uid %q", label, a)
	}
}

func TestTreeSitterHomonymsKeepDistinctUIDs(t *testing.T) {
	project := stageGrammar(t, "go", "tree-sitter-go", ".go", "go.yaml")
	a := ConvertToCache(parseFixture(t, project, "a.go", "package demo\nfunc Run() {}\n"), project, false, "")
	b := ConvertToCache(parseFixture(t, project, "b.go", "package demo\nfunc Run() {}\n"), project, false, "")
	assertDistinctParsedUIDs(t, a, b, "Function", "Run")
}

func TestANTLRHomonymsKeepDistinctUIDs(t *testing.T) {
	project := stageAntlr(t, "plsql.yaml")
	cfg := &antlrLangConfig{Language: "plsql", Grammar: "antlr-plsql", Extensions: []string{".sql"}, StartRule: "sql_script"}
	source := "CREATE TABLE PEDIDO (ID NUMBER(10));\n"
	a := ConvertToCache(parseAntlrFixture(t, project, "a.sql", source, cfg), project, false, "")
	b := ConvertToCache(parseAntlrFixture(t, project, "b.sql", source, cfg), project, false, "")
	assertDistinctParsedUIDs(t, a, b, "Table", "PEDIDO")
}
