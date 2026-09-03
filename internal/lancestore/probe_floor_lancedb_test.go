//go:build lancedb

package lancestore

import (
	"context"
	"fmt"
	"sort"
	"strings"
	"testing"
	"unicode"
)

type probeEntity struct {
	uid, name, doc, etype, path string
}

func probeCorpus() []probeEntity {
	return []probeEntity{
		{"u1", "parseConfig", "Parses the configuration file into a Config struct.", "Function", "config.go"},
		{"u2", "Config", "Configuration for the parser.", "Struct", "config.go"},
		{"u3", "loadUserConfig", "Loads user level configuration overrides.", "Function", "user.go"},
		{"u4", "validateSchema", "Validates the database schema before migration.", "Function", "schema.go"},
		{"u5", "SchemaValidator", "Validates schemas.", "Class", "schema.go"},
		{"u6", "connectDatabase", "Opens a connection to the database.", "Function", "db.go"},
		{"u7", "closeDatabase", "Closes the database connection.", "Function", "db.go"},
		{"u8", "retryPolicy", "Retry policy with exponential backoff for network calls.", "Struct", "retry.go"},
		{"u9", "computeChecksum", "Computes a checksum of the payload.", "Function", "hash.go"},
		{"u10", "parseSQL", "Parses a SQL statement into an AST.", "Function", "sql.go"},
		{"a1", "coreConf", "Core configuration accessor.", "Function", "core.go"},
		{"a2", "CONF_MGR", "Configuration manager package.", "Package", "conf_mgr.sql"},
		{"a3", "CFG_LOAD", "Loads settings at startup.", "Procedure", "cfg_load.sql"},
		{"a4", "configLoader", "Loads the configuration.", "Class", "loader.go"},
		{"a5", "initConfiguration", "Initialises configuration state.", "Function", "init.go"},
		{"a7", "PKG_ACCOUNT_UPDATE", "Updates account rows.", "Package", "acct.sql"},
		{"p1", "XPTO_EXTRAIR_ABCD01_DOC_LOTE", "Extrai lote de DOC.", "Procedure", "xpto.sql"},
		{"p2", "PKG_VALIDACAO_PAGAMENTO", "Valida pagamento.", "Package", "pkg_val.sql"},
		{"p3", "TRG_AUDITORIA_CLIENTE", "Auditoria de cliente.", "Trigger", "trg_aud.sql"},
	}
}

type probe struct {
	query  string
	want   string
	recall bool
	why    string
}

func probeSet() []probe {
	return []probe{
		{query: "parseConfig", want: "parseConfig"},
		{query: "checksum", want: "computeChecksum"},
		{query: "retry backoff", want: "retryPolicy"},
		{query: "parse sql", want: "parseSQL"},
		{query: "conf", want: "CONF_MGR"},
		{query: "compu", want: "computeChecksum"},
		{query: "retr", want: "retryPolicy"},
		{query: "connect", want: "connectDatabase"},
		{query: "audit", want: "TRG_AUDITORIA_CLIENTE"},
		{query: "extrair", want: "XPTO_EXTRAIR_ABCD01_DOC_LOTE"},
		{query: "cf", want: "CFG_LOAD"},

		{query: "configuration", want: "parseConfig", recall: true,
			why: "seven entities carry it, and initConfiguration carries it in the name"},
		{query: "schema", want: "validateSchema", recall: true,
			why: "validateSchema and SchemaValidator both carry it in the name"},
		{query: "config", want: "configLoader", recall: true,
			why: "Config is the entity literally named that"},
		{query: "valid", want: "validateSchema", recall: true,
			why: "the case the project itself excluded: a prefix of validate and validacao"},
		{query: "valida", want: "PKG_VALIDACAO_PAGAMENTO", recall: true,
			why: "also a prefix of both validate and validacao"},
	}
}

const recallTopN = 5

func normalizeGrams(s string) string {
	var b strings.Builder
	for _, r := range strings.ToLower(s) {
		if unicode.IsLetter(r) || unicode.IsDigit(r) {
			b.WriteRune(r)
		}
	}
	return b.String()
}

func gramBag(s string) []string {
	r := []rune(normalizeGrams(s))
	var out []string
	for _, n := range []int{2, 3} {
		for i := 0; i+n <= len(r); i++ {
			out = append(out, string(r[i:i+n]))
		}
	}
	return out
}

func splitIdentifier(s string) string {
	var parts []string
	var cur strings.Builder
	flush := func() {
		if cur.Len() > 0 {
			parts = append(parts, cur.String())
			cur.Reset()
		}
	}
	rs := []rune(s)
	for i, r := range rs {
		switch {
		case r == '_' || r == '.' || r == '-' || r == ' ' || r == '/':
			flush()
		case unicode.IsUpper(r):
			if i > 0 && (unicode.IsLower(rs[i-1]) ||
				(i+1 < len(rs) && unicode.IsLower(rs[i+1]) && unicode.IsUpper(rs[i-1]))) {
				flush()
			}
			cur.WriteRune(r)
		default:
			cur.WriteRune(r)
		}
	}
	flush()
	return strings.Join(parts, " ")
}

func dedup(in []string) []string {
	seen := make(map[string]bool, len(in))
	out := make([]string, 0, len(in))
	for _, s := range in {
		if s != "" && !seen[s] {
			seen[s] = true
			out = append(out, s)
		}
	}
	return out
}

func documentText(e probeEntity) string {
	split := splitIdentifier(e.name)
	grams := gramBag(e.name)
	for _, w := range strings.Fields(split) {
		grams = append(grams, gramBag(w)...)
	}
	return strings.Join([]string{
		e.name, split, strings.ToLower(split), strings.ToLower(e.name),
		e.doc, e.etype,
		strings.Join(dedup(grams), " "),
	}, " ")
}

func queryText(q string) string {
	out := []string{strings.ToLower(q)}
	for _, t := range strings.Fields(strings.ToLower(q)) {
		out = append(out, t)
		out = append(out, gramBag(t)...)
	}
	return strings.Join(dedup(out), " ")
}

func buildProbeTable(t *testing.T, params indexTuning) *Table {
	t.Helper()
	ctx := context.Background()
	st := openLocal(t)

	schema := Schema{Fields: []Field{
		{Name: "uid", Type: FieldString},
		{Name: "name", Type: FieldString},
		{Name: "path", Type: FieldString},
		{Name: "etype", Type: FieldString},
		{Name: "body", Type: FieldString},
	}}
	tbl, err := st.CreateTable(ctx, "probe", schema)
	if err != nil {
		t.Fatalf("create: %v", err)
	}

	build := params.text
	if build == nil {
		build = plainText
	}
	rows := make([]Row, 0, len(probeCorpus()))
	for _, e := range probeCorpus() {
		rows = append(rows, Row{
			"uid": e.uid, "name": e.name, "path": e.path, "etype": e.etype,
			"body": build(e),
		})
	}
	if err := tbl.Append(ctx, rows); err != nil {
		t.Fatalf("append: %v", err)
	}
	if err := params.build(ctx, tbl, "body"); err != nil {
		t.Fatalf("index: %v", err)
	}
	return tbl
}

func score(t *testing.T, tbl *Table) (strictHits, strictTotal, recallHits, recallTotal, empty int) {
	t.Helper()
	ctx := context.Background()

	t.Logf("%-16s %-8s %-30s %s", "query", "kind", "expected", "got")
	t.Logf("%s", strings.Repeat("-", 96))

	for _, p := range probeSet() {
		hits, err := tbl.Search(ctx, Query{
			Text: queryText(p.query), TextColumn: "body", Limit: 5})
		if err != nil {
			t.Errorf("%q: %v", p.query, err)
			continue
		}
		got := make([]string, 0, len(hits))
		for _, h := range hits {
			got = append(got, fmt.Sprintf("%v", h.Row["name"]))
		}
		if len(got) == 0 {
			empty++
		}

		kind := "top-1"
		ok := len(got) > 0 && got[0] == p.want
		if p.recall {
			kind = "recall"
			ok = false
			for i, n := range got {
				if n == p.want && i < recallTopN {
					ok = true
					break
				}
			}
			recallTotal++
			if ok {
				recallHits++
			}
		} else {
			strictTotal++
			if ok {
				strictHits++
			}
		}

		mark := strings.Join(got, ", ")
		if len(mark) > 46 {
			mark = mark[:46]
		}
		status := "  "
		if ok {
			status = "OK"
		}
		t.Logf("%-16s %-8s %-30s %s %s", p.query, kind, p.want, status, mark)
	}
	t.Logf("%s", strings.Repeat("-", 96))
	return
}

type indexTuning struct {
	label string
	text  func(probeEntity) string
	opts  TextIndexOptions
}

func (it indexTuning) build(ctx context.Context, tbl *Table, column string) error {
	return tbl.EnsureIndexes(ctx, Index{Column: column, Kind: IndexInvertedText, Text: it.opts})
}

func plainText(e probeEntity) string {
	return strings.Join([]string{e.name, e.doc, e.etype}, " ")
}

func u32p(v uint32) *uint32 { return &v }
func boolp(v bool) *bool    { return &v }

// THE GATE. Retrieval and scoring are the engine's; the only Go-side work is expanding the
// document and the query at write and read time, which is preprocessing rather than ranking.
func TestSearchQualityGate(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping heavy LanceDB quality gate in -short mode")
	}
	tbl := buildProbeTable(t, chosenTuning())
	strict, strictN, recall, recallN, empty := score(t, tbl)

	t.Logf("strict top-1: %d/%d   recall@%d: %d/%d   empty: %d",
		strict, strictN, recallTopN, recall, recallN, empty)

	if strict < strictN {
		t.Errorf("QUALITY GATE: %d/%d probes with a single defensible answer ranked it first",
			strict, strictN)
	}
	if recall < recallN {
		t.Errorf("RECALL GATE: %d/%d ambiguous probes reached the named entity in the top %d",
			recall, recallN, recallTopN)
	}
	if empty > 0 {
		t.Errorf("QUALITY GATE: %d queries returned nothing", empty)
	}
}

func chosenTuning() indexTuning {
	return indexTuning{
		label: "Go gram expansion, engine default tokenizer",
		text:  documentText,
	}
}

func TestSearchTuningSweep(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping heavy LanceDB quality gate in -short mode")
	}
	candidates := []indexTuning{
		{label: "engine default"},
		{label: "engine stem+ascii", opts: TextIndexOptions{
			Language: "English", Stem: boolp(true), ASCIIFolding: boolp(true)}},
		{label: "engine ngram 2-4", opts: TextIndexOptions{
			BaseTokenizer: "ngram", NgramMin: u32p(2), NgramMax: u32p(4)}},
		{label: "engine ngram 2-4 +ascii", opts: TextIndexOptions{
			BaseTokenizer: "ngram", NgramMin: u32p(2), NgramMax: u32p(4), ASCIIFolding: boolp(true)}},
		{label: "engine ngram 2-5 +ascii", opts: TextIndexOptions{
			BaseTokenizer: "ngram", NgramMin: u32p(2), NgramMax: u32p(5), ASCIIFolding: boolp(true)}},
		{label: "engine ngram 2-4 prefixonly", opts: TextIndexOptions{
			BaseTokenizer: "ngram", NgramMin: u32p(2), NgramMax: u32p(4),
			NgramPrefixOnly: boolp(true), ASCIIFolding: boolp(true)}},
		{label: "GO grams, engine default", text: documentText},
		{label: "GO grams + engine ngram", text: documentText, opts: TextIndexOptions{
			BaseTokenizer: "ngram", NgramMin: u32p(2), NgramMax: u32p(4)}},
	}

	type result struct {
		label                  string
		strict, strictN        int
		recall, recallN, empty int
	}
	var out []result
	for _, c := range candidates {
		t.Run(strings.ReplaceAll(c.label, " ", "_"), func(t *testing.T) {
			tbl := buildProbeTable(t, c)
			s, sn, r, rn, e := score(t, tbl)
			out = append(out, result{c.label, s, sn, r, rn, e})
		})
	}

	sort.Slice(out, func(i, j int) bool {
		li := out[i].strict + out[i].recall - out[i].empty*10
		lj := out[j].strict + out[j].recall - out[j].empty*10
		return li > lj
	})
	t.Logf("%-32s %-12s %-12s %s", "configuration", "strict", "recall@5", "empty")
	for _, r := range out {
		t.Logf("%-32s %d/%-10d %d/%-10d %d",
			r.label, r.strict, r.strictN, r.recall, r.recallN, r.empty)
	}
}
