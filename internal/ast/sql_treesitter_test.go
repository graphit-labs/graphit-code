package ast

import (
	"testing"
)

// The tree-sitter SQL grammar extracted ALMOST nothing, and nobody saw it, because two
// camadas escondiam:
//
//  1. The `tables` pattern required a `name:` field on `create_table`, and that node
//     atribui campo a nenhum filho. Um pattern assim COMPILA — os nomes de campo
//     exist somewhere in the grammar, so TestEveryShippedQueryPatternCompiles passes —
//     and it matches zero times. A pattern that compiles and never matches is a silent
//     no-op.
//  2. `CompositeParser` trata `.sql` como tendo os dois backends: tenta tree-sitter,
//     sees zero entities and falls back to ANTLR. An isolated `create table` seemed
//     because the `Table` came from there.
//
// That is why EVERY test here calls `parseSource` directly, which is pure tree-sitter.
// A test written over `parseFixture`/`CompositeParser` would pass with the defect in
// place, which is exactly how it survived until now.
func parseSQLWithTreeSitter(t *testing.T, source string) *ParsedFile {
	t.Helper()
	projectDir := stageGrammar(t, "sql", "tree-sitter-sql", ".sql", "sql.yaml")
	cfg, ok := tsLangConfigByName(projectDir, "sql")
	if !ok {
		t.Skip("sql language not resolvable")
	}
	p := &TreeSitterParser{projectDir: projectDir}
	pf, err := p.parseSource("a.sql", ".sql", cfg, []byte(source), 0, 0, false,
		ParseOptions{IndexSource: true})
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	return pf
}

func TestSQLCreateTableIsExtracted(t *testing.T) {
	pf := parseSQLWithTreeSitter(t, `create table cliente (
  id integer not null,
  nome varchar(60)
);
`)
	wantEntityLine(t, pf, "Table", "cliente", 1)
}

func TestSQLCreateViewAndFunctionAreExtracted(t *testing.T) {
	pf := parseSQLWithTreeSitter(t, `create view v_ativos as select id from cliente;

create function f_total(x int) returns int as $$ select 1 $$;
`)
	wantEntityLine(t, pf, "View", "v_ativos", 1)
	wantEntityLine(t, pf, "Function", "f_total", 3)
}

// A table without its columns is half a table, and the ANTLR side always had them.
// This is also what makes the grammar useful as the INNER language of an embedded
// block, which is where the defect was discovered.
func TestSQLColumnsAreExtractedAndBelongToTheirTable(t *testing.T) {
	pf := parseSQLWithTreeSitter(t, `create table pedido (
  id integer,
  cliente_id integer,
  total numeric(10,2)
);
`)
	for _, c := range []struct {
		name string
		line int
	}{{"id", 2}, {"cliente_id", 3}, {"total", 4}} {
		e, ok := entityAt(pf, "Column", c.name)
		if !ok {
			t.Errorf("no Column %q; entities: %v", c.name, entityLabelsOf(pf))
			continue
		}
		if e.Line != c.line {
			t.Errorf("Column %q at line %d, want %d", c.name, e.Line, c.line)
		}
		if e.Context != "pedido" || e.ContextType != "Table" {
			t.Errorf("Column %q is contained by %s %q, want Table \"pedido\"",
				c.name, e.ContextType, e.Context)
		}
	}
}

// `select * from xpto` is the case that drove all of this — the body of an <execute> in
// XML. Without this a SELECT contributes nothing, and since tree-sitter is the DEFAULT
// parser for `.sql` that would hold for most of a project's SQL files.
//
// The edge types are the same ones the ANTLR dialects produce, on purpose: "who reads
// this table" has to be one question, regardless of which backend parsed the file.
func TestSQLDMLProducesTheSameEdgeTypesAsTheANTLRDialects(t *testing.T) {
	pf := parseSQLWithTreeSitter(t, `select nome from cliente;

select p.id from pedido p join fatura c on c.id = p.id;

insert into auditoria (id) values (1);

update estoque set qtd = 0;

delete from carrinho where id = 1;

alter table cliente add column obs varchar(10);
`)
	// A set of pairs, not a map keyed by table: the SAME table carries edges
	// of different types — `cliente` is read by a SELECT and altered by an ALTER —
	// and a map keyed by name would hide exactly that.
	got := map[[2]string]bool{}
	for _, r := range pf.References {
		got[[2]string{r.TargetName, r.RelType}] = true
	}
	for _, want := range [][2]string{
		{"cliente", "SELECTS"},
		{"cliente", "ALTERS"},
		{"pedido", "SELECTS"},
		{"fatura", "SELECTS"},
		{"auditoria", "INSERTS"},
		{"estoque", "UPDATES"},
		{"carrinho", "DELETES"},
	} {
		if !got[want] {
			var all []string
			for k := range got {
				all = append(all, k[1]+"->"+k[0])
			}
			t.Errorf("no %s edge to %q; all: %v", want[1], want[0], all)
		}
	}
	// A referenced table does not become a Table node: it is used here, not declared,
	// and a Table node would make it look like a declaration.
	for _, name := range []string{"pedido", "auditoria", "estoque", "carrinho"} {
		if _, ok := entityAt(pf, "Table", name); ok {
			t.Errorf("referenced table %q became a Table entity", name)
		}
	}
}

func TestSQLCreateIndexIsExtracted(t *testing.T) {
	pf := parseSQLWithTreeSitter(t, "create index ix_pedido_id on pedido (id);\n")
	wantEntityLine(t, pf, "Index", "ix_pedido_id", 1)
}

// The missing net: the compilation test does not catch a pattern that compiles and
// never matches. This one exercises every sql.yaml query against a source it MUST
// match.
func TestEverySQLQueryMatchesSomething(t *testing.T) {
	pf := parseSQLWithTreeSitter(t, `create table cliente (id integer);

create view v as select id from cliente;

create function f() returns int as $$ select 1 $$;

select id from pedido;
`)
	for _, want := range [][2]string{
		{"Table", "cliente"},
		{"Column", "id"},
		{"View", "v"},
		{"Function", "f"},
	} {
		if _, ok := entityAt(pf, want[0], want[1]); !ok {
			t.Errorf("no %s %q — a query in sql.yaml matches nothing; entities: %v",
				want[0], want[1], entityLabelsOf(pf))
		}
	}
	if len(pf.References) == 0 {
		t.Error("no DML edges from the SELECT statements")
	}
}
