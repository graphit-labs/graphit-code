package ast

import (
	"context"
	"fmt"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ai"
)

// NOTE: probe identifiers in this file are synthetic, and should stay that way.
// These tests seed their own database, so any identifier of the right shape
// serves the purpose — the measurement is whether a fragment of a compound name
// finds it. Keeping them synthetic also keeps the tests independent of whatever
// corpus GRAPHIT_E2E_SQL_DIR happens to point at.

// Verification of the search index.
//
// These tests were written to compare a LadybugDB implementation against the SQLite one,
// fed from the same ShardCache so any difference had to be the implementation rather than
// the input. The migration was reverted — LadybugDB does not maintain an FTS index on
// insert, which forced an O(corpus) rebuild per edit, and it intermittently stored invalid
// UTF-8 — so the tests now run against SQLite alone, keeping the requirements they encode.

// cacheFromCorpus builds a parse cache from a corpus of entities.
func cacheFromCorpus(t *testing.T, dir string, corpus []gateEntity) *ShardCache {
	t.Helper()
	pc, err := NewShardCache(dir)
	if err != nil {
		t.Fatalf("shard cache: %v", err)
	}
	byPath := map[string][]cachedEntity{}
	for _, e := range corpus {
		byPath[e.path] = append(byPath[e.path], cachedEntity{
			Label: e.entityType, UID: e.uid, Name: e.name,
			Path: e.path, Line: 1, EndLine: 1, Docstring: e.docstring,
		})
	}
	for p, ents := range byPath {
		if err := pc.Store(p, "h-"+p, &parseCacheEntry{
			RelPath: p, Language: "go", Source: "// " + p + "\n", Entities: ents,
		}); err != nil {
			t.Fatalf("store %s: %v", p, err)
		}
	}
	if err := pc.FlushDirty(); err != nil {
		t.Fatal(err)
	}
	return pc
}

func buildSearchIndex(t *testing.T, dir string, cache *ShardCache,
	embLookup func(relPath, uid string) []float32) *SearchIndex {
	t.Helper()
	si, err := OpenSearchIndex(context.Background(), filepath.Join(dir, "ladybugdb"))
	if err != nil {
		t.Fatalf("open search index: %v", err)
	}
	t.Cleanup(func() { _ = si.Close() })
	if err := si.RebuildFromCache(context.Background(), cache, embLookup); err != nil {
		t.Fatalf("rebuild search index: %v", err)
	}
	return si
}

// entityNames reduces results to entity names, dropping file hits, so an expectation
// about which ENTITY ranks first is not satisfied or spoiled by a file result.
func entityNames(res []SearchResult, topK int) []string {
	var out []string
	seen := map[string]bool{}
	for _, r := range res {
		if r.Type == "file" {
			continue
		}
		n := r.Name
		if i := strings.IndexByte(n, ' '); i > 0 {
			n = n[:i]
		}
		if seen[n] {
			continue
		}
		seen[n] = true
		out = append(out, n)
		if topK > 0 && len(out) == topK {
			break
		}
	}
	return out
}

// indexSearchNames returns the names of all results, files included, deduplicated and
// capped. Used where an expectation is about reachability rather than about which entity
// wins.
func indexSearchNames(t *testing.T, si *SearchIndex, query string, topK int) []string {
	t.Helper()
	res, err := si.Search(context.Background(), query, topK)
	if err != nil {
		t.Logf("  search %q failed: %v", query, err)
		return nil
	}
	var out []string
	seen := map[string]bool{}
	for _, r := range res {
		n := r.Name
		if i := strings.IndexByte(n, ' '); i > 0 {
			n = n[:i]
		}
		if seen[n] {
			continue
		}
		seen[n] = true
		out = append(out, n)
	}
	return out
}

// TestSearchIndexQualityFloor is the gate the migration had to clear, kept as a floor now
// that the index it was compared against no longer exists.
//
// The recorded numbers are from the differential run on this exact corpus and probe set:
// Ladybug placed the expected entity first 14 times out of 16, SQLite 12, with neither
// returning nothing for any query. The assertion is therefore against SQLite's 12 — the
// bar is "no worse than what was replaced", not "never regress by one position", so a
// ranking tweak does not fail the suite while a real regression does.
// TestSearchIndexQualityFloor is the re-derived gate: STRICT where one answer is defensible,
// RECALL where more than one is.
//
// THE OLD FLOOR OF 13/16 WAS MEASURING TIE-BREAKS, and that finding is the reason this test looks
// like this. Five of the sixteen probes have no single defensible answer by the rule this project
// already wrote down in truncated_query_test.go — "a probe with no defensible answer measures
// nothing":
//
//	configuration -> expected parseConfig, but initConfiguration is at least as good
//	schema        -> expected validateSchema, but a file named schema.go answers it
//	config        -> expected configLoader, but an entity literally named Config answers it better
//	valid         -> validateSchema and SchemaValidator are the same claim
//	valida        -> PKG_VALIDACAO_PAGAMENTO and SchemaValidator, likewise
//
// Those five encoded which of two right answers the old engine's ranking happened to prefer. A
// previous session read the resulting 11/16 as a quality deficit and was one step from building a
// cross-encoder to close a gap that was not there. So they became recall probes, and the strict
// floor became all eleven of the probes that have one answer. Same corpus, same queries, a
// question that can be answered wrongly.
func TestSearchIndexQualityFloor(t *testing.T) {
	dir := t.TempDir()
	corpus := prefixCorpus()
	cache := cacheFromCorpus(t, filepath.Join(dir, "cache"), corpus)
	si := buildSearchIndex(t, dir, cache, nil)

	// Probes with exactly one defensible answer. The floor is all of them.
	cases := []struct{ query, wantTop string }{
		{"parseConfig", "parseConfig"},
		{"checksum", "computeChecksum"},
		{"retry backoff", "retryPolicy"},
		{"parse sql", "parseSQL"},
		{"conf", "CONF_MGR"},
		{"compu", "computeChecksum"},
		{"retr", "retryPolicy"},
		{"connect", "connectDatabase"},
		{"audit", "TRG_AUDITORIA_CLIENTE"},
		{"extrair", "XPTO_EXTRAIR_ABCD01_DOC_LOTE"},
		{"cf", "CFG_LOAD"},
	}
	const baselineTop1 = 11

	// Probes with more than one defensible answer: the expected entity has to be REACHABLE.
	recall := []struct{ query, wantAny string }{
		{"configuration", "parseConfig"},
		{"schema", "validateSchema"},
		{"config", "configLoader"},
		{"valid", "validateSchema"},
		{"valida", "PKG_VALIDACAO_PAGAMENTO"},
	}

	t.Logf("%-16s | %-30s | %s", "query", "expected top-1", "got")
	t.Logf("%s", strings.Repeat("-", 84))

	var hits, empty int
	for _, c := range cases {
		res, err := si.Search(context.Background(), c.query, 5)
		if err != nil {
			t.Errorf("search %q: %v", c.query, err)
			continue
		}
		names := entityNames(res, 5)
		top := ""
		if len(names) > 0 {
			top = names[0]
		} else {
			empty++
		}
		if top == c.wantTop {
			hits++
		}
		mark := top
		if mark == "" {
			mark = "(vazio)"
		} else if top == c.wantTop {
			mark += " OK"
		}
		t.Logf("%-16s | %-30s | %s", c.query, c.wantTop, mark)
	}

	// recallAt5 is the window the old gate itself used: it called Search(query, 5), so five is
	// what "reachable" has always meant here rather than a number chosen now to fit.
	const recallAt5 = 5
	var reached int
	for _, c := range recall {
		res, err := si.Search(context.Background(), c.query, recallAt5)
		if err != nil {
			t.Errorf("search %q: %v", c.query, err)
			continue
		}
		names := entityNames(res, recallAt5)
		found := false
		for _, n := range names {
			if n == c.wantAny {
				found = true
			}
		}
		if found {
			reached++
		}
		t.Logf("%-16s | %-30s | recall@%d: %v %v", c.query, c.wantAny, recallAt5, found, names)
		if !found {
			t.Errorf("QUALITY FLOOR: %q does not reach %q anywhere in the top %d: %v",
				c.query, c.wantAny, recallAt5, names)
		}
	}

	t.Logf("%s", strings.Repeat("-", 84))
	t.Logf("strict top-1: %d/%d   recall@%d: %d/%d   empty: %d",
		hits, len(cases), recallAt5, reached, len(recall), empty)

	if hits < baselineTop1 {
		t.Errorf("QUALITY FLOOR: the expected entity ranked first %d/%d times, below the measured "+
			"%d/%d on the same corpus and probes", hits, len(cases), baselineTop1, len(cases))
	}
	if empty > 0 {
		t.Errorf("QUALITY FLOOR: %d queries returned nothing; the measured baseline had none", empty)
	}
}

// TestSearchIndexRebuildIsIdempotent guards the atomic swap: rebuilding must
// replace the contents, not accumulate them, and must leave the index queryable.
func TestSearchIndexRebuildIsIdempotent(t *testing.T) {
	dir := t.TempDir()
	cache := cacheFromCorpus(t, filepath.Join(dir, "cache"), gateCorpus())
	lb := buildSearchIndex(t, dir, cache, nil)

	first, err := lb.Search(context.Background(), "config", 20)
	if err != nil {
		t.Fatalf("search: %v", err)
	}
	if len(first) == 0 {
		t.Fatal("first rebuild produced an index that matches nothing")
	}

	for i := 0; i < 3; i++ {
		if err := lb.RebuildFromCache(context.Background(), cache, nil); err != nil {
			t.Fatalf("rebuild %d: %v", i+2, err)
		}
		got, err := lb.Search(context.Background(), "config", 20)
		if err != nil {
			t.Fatalf("search after rebuild %d: %v", i+2, err)
		}
		if len(got) != len(first) {
			t.Errorf("rebuild %d changed the result count: %d -> %d (duplicates or losses)",
				i+2, len(first), len(got))
		}
		if fingerprint(got) != fingerprint(first) {
			t.Errorf("rebuild %d changed results:\n first: %v\n got:   %v",
				i+2, fingerprint(first), fingerprint(got))
		}
	}
}

func fingerprint(res []SearchResult) string {
	parts := make([]string, 0, len(res))
	for _, r := range res {
		parts = append(parts, fmt.Sprintf("%s:%s:%s:%d", r.Type, r.Path, r.Name, r.Line))
	}
	return strings.Join(parts, "|")
}

// TestSearchIndexIncremental exercises the incremental update path: stale rows go, new rows
// arrive, and repeating the same update does not duplicate anything.
func TestSearchIndexIncremental(t *testing.T) {
	dir := t.TempDir()
	corpus := gateCorpus()
	cachePath := filepath.Join(dir, "cache")
	cache := cacheFromCorpus(t, cachePath, corpus)
	lb := buildSearchIndex(t, dir, cache, nil)

	find := func(query, want string) bool {
		res, err := lb.Search(context.Background(), query, 20)
		if err != nil {
			t.Fatalf("search %q: %v", query, err)
		}
		for _, n := range entityNames(res, 0) {
			if n == want {
				return true
			}
		}
		return false
	}

	if !find("checksum", "computeChecksum") {
		t.Fatal("baseline: computeChecksum not found before the incremental update")
	}

	// Replace hash.go's contents with a differently named entity, and delete db.go.
	if err := cache.Store("hash.go", "h2", &parseCacheEntry{
		RelPath: "hash.go", Language: "go", Source: "// hash.go v2\n",
		Entities: []cachedEntity{{
			Label: "Function", UID: "u9b", Name: "renamedDigest",
			Path: "hash.go", Line: 1, EndLine: 1, Docstring: "Computes a digest.",
		}},
	}); err != nil {
		t.Fatalf("store changed file: %v", err)
	}
	cache.Remove("db.go")
	if err := cache.FlushDirty(); err != nil {
		t.Fatal(err)
	}

	if err := lb.UpdateIncremental(context.Background(), cache, []string{"hash.go"}, []string{"db.go"}, nil); err != nil {
		t.Fatalf("incremental update: %v", err)
	}

	if find("checksum", "computeChecksum") {
		t.Error("computeChecksum survived after its file was rewritten — stale rows are not removed")
	}
	if !find("digest", "renamedDigest") {
		t.Error("renamedDigest not found after the incremental update — new rows are not indexed")
	}
	if find("database", "connectDatabase") {
		t.Error("connectDatabase survived after db.go was deleted")
	}

	// Re-running the same update must not duplicate anything.
	if err := lb.UpdateIncremental(context.Background(), cache, []string{"hash.go"}, []string{"db.go"}, nil); err != nil {
		t.Fatalf("repeated incremental update: %v", err)
	}
	res, err := lb.Search(context.Background(), "digest", 20)
	if err != nil {
		t.Fatalf("search: %v", err)
	}
	// The index stores the identifier together with its split ("renamedDigest renamed
	// Digest"), so the raw Name never equals the identifier; entityNames normalises it.
	count := 0
	for _, n := range entityNames(res, 0) {
		if n == "renamedDigest" {
			count++
		}
	}
	if count != 1 {
		t.Errorf("renamedDigest appears %d times after a repeated update, want 1 (delete-then-insert is not idempotent)", count)
	}
}

// TestSearchIndexSemantic covers the vector half end to end with the real
// embedder: vectors written through RebuildFromCache must be retrievable, and hybrid
// search must fuse them with the lexical passes.
func TestSearchIndexSemantic(t *testing.T) {
	client, err := ai.NewEmbeddingClientFromConfig()
	if err != nil {
		if strings.Contains(err.Error(), "API version") {
			t.Fatalf("ONNX Runtime rejects the binding's API version — Makefile ORT_VERSION is out of "+
				"step with go.mod onnxruntime_go: %v", err)
		}
		t.Skipf("embedding client unavailable: %v", err)
	}

	ctx := context.Background()
	dir := t.TempDir()
	corpus := abbrevCorpusNamesOnly()
	cache := cacheFromCorpus(t, filepath.Join(dir, "cache"), corpus)

	// Embed the identifier alone, so a hit is attributable to the name rather than
	// to prose — the confound isolated in TestAbbreviatedIdentifierSearchSQLite.
	byUID := make(map[string][]float32, len(corpus))
	names := make([]string, 0, len(corpus))
	uids := make([]string, 0, len(corpus))
	for _, e := range corpus {
		names = append(names, e.name)
		uids = append(uids, e.uid)
	}
	vecs, err := client.EmbedBatch(ctx, names)
	if err != nil {
		t.Skipf("embedding unavailable: %v", err)
	}
	for i, uid := range uids {
		byUID[uid] = vecs[i]
	}

	lb := buildSearchIndex(t, dir, cache, func(_, uid string) []float32 {
		return byUID[uid]
	})

	qe, ok := client.(ai.QueryEmbedder)
	if !ok {
		t.Skip("client does not implement QueryEmbedder")
	}
	qv, err := qe.EmbedQuery(ctx, "config")
	if err != nil {
		t.Fatalf("embed query: %v", err)
	}

	sem, err := lb.SemanticSearch(context.Background(), qv, 5)
	if err != nil {
		t.Fatalf("semantic search: %v", err)
	}
	if len(sem) == 0 {
		t.Fatal("semantic search returned nothing although vectors were written")
	}
	semNames := entityNames(sem, 5)
	t.Logf("semantic top-5 for \"config\": %v", semNames)

	// CFG_LOAD shares no trigram with "config", so a lexical pass cannot reach it.
	// The whole point of carrying vectors is that this one does.
	found := false
	for _, n := range semNames {
		if n == "CFG_LOAD" {
			found = true
		}
	}
	if !found {
		t.Errorf("semantic search did not surface CFG_LOAD for \"config\" (got %v) — "+
			"vectors are stored but not usefully retrievable", semNames)
	}

	hyb, err := lb.HybridSearch(context.Background(), "config", qv, 10)
	if err != nil {
		t.Fatalf("hybrid search: %v", err)
	}
	if len(hyb) == 0 {
		t.Fatal("hybrid search returned nothing")
	}
	hybNames := entityNames(hyb, 10)
	t.Logf("hybrid top-10 for \"config\": %v", hybNames)

	// Fusion must not lose what either half found on its own.
	lex, err := lb.Search(context.Background(), "config", 10)
	if err != nil {
		t.Fatalf("lexical search: %v", err)
	}
	inHybrid := map[string]bool{}
	for _, n := range hybNames {
		inHybrid[n] = true
	}
	for _, n := range entityNames(lex, 10) {
		if !inHybrid[n] {
			t.Errorf("hybrid search dropped %q, which the lexical pass alone found", n)
		}
	}
	for _, n := range semNames {
		if !inHybrid[n] {
			t.Errorf("hybrid search dropped %q, which the semantic pass alone found", n)
		}
	}
}

// TestSearchIndexIncrementalRepeated runs consecutive incremental updates, which is what a
// daemon does all day and what two updates do not exercise.
//
// Two was not enough: with the FTS indexes recreated AFTER the writes, the fourth update's
// DELETE hit an index that no longer knew the node it was deleting —
//
//	FTS index 'sf_source' is inconsistent: document for node offset 3002 is missing during
//	delete. Drop and recreate the FTS index.
//
// — so UpdateIncremental returned early and the update was silently skipped. The end-to-end
// benchmark read that as a speed-up (48 ms instead of 1.9 s) because a failing no-op is fast,
// and the error was being discarded at the call site. Both are fixed; this is the test that
// would have caught it.
func TestSearchIndexIncrementalRepeated(t *testing.T) {
	dir := t.TempDir()
	cache := cacheFromCorpus(t, filepath.Join(dir, "cache"), prefixCorpus())
	lb := buildSearchIndex(t, dir, cache, nil)

	const rounds = 8
	for round := 1; round <= rounds; round++ {
		name := fmt.Sprintf("renamedRound%d", round)
		if err := cache.Store("hash.go", fmt.Sprintf("h%d", round), &parseCacheEntry{
			RelPath: "hash.go", Language: "go", Source: "// hash.go\n",
			Entities: []cachedEntity{{
				Label: "Function", UID: "u9r", Name: name,
				Path: "hash.go", Line: 1, EndLine: 1,
				Docstring: "Round " + fmt.Sprint(round) + " marker.",
			}},
		}); err != nil {
			t.Fatalf("round %d store: %v", round, err)
		}
		if err := cache.FlushDirty(); err != nil {
			t.Fatalf("round %d flush: %v", round, err)
		}

		if err := lb.UpdateIncremental(context.Background(), cache, []string{"hash.go"}, nil, nil); err != nil {
			t.Fatalf("round %d update: %v", round, err)
		}

		// The new name must be searchable and the previous one gone.
		res, err := lb.Search(context.Background(), name, 20)
		if err != nil {
			t.Fatalf("round %d search: %v", round, err)
		}
		found := false
		for _, n := range entityNames(res, 0) {
			if n == name {
				found = true
			}
		}
		if !found {
			t.Errorf("round %d: %q is not searchable after its incremental update", round, name)
		}
		if round > 1 {
			prev := fmt.Sprintf("renamedRound%d", round-1)
			res, err := lb.Search(context.Background(), prev, 20)
			if err != nil {
				t.Fatalf("round %d stale search: %v", round, err)
			}
			for _, n := range entityNames(res, 0) {
				if n == prev {
					t.Errorf("round %d: %q survived after being replaced", round, prev)
				}
			}
		}
	}
}

// TestSearchResultsCarryCleanNames pins down what a result shows.
//
// The index stores an identifier together with its split so both spellings match, and that
// used to live in the same column the result displayed — so search returned
// "parseConfig parse Config" and "config.go config go" as names, to the agent consuming it
// over MCP and to every test, which had to strip the suffix before comparing. The split now
// lives in name_split and the displayed column holds the identifier alone.
func TestSearchResultsCarryCleanNames(t *testing.T) {
	dir := t.TempDir()
	cache := cacheFromCorpus(t, filepath.Join(dir, "cache"), gateCorpus())
	si := buildSearchIndex(t, dir, cache, nil)

	res, err := si.Search(context.Background(), "config", 20)
	if err != nil {
		t.Fatalf("search: %v", err)
	}
	if len(res) == 0 {
		t.Fatal("no results — the test would pass vacuously")
	}

	for _, r := range res {
		t.Logf("%-8s %-14s %q", r.Type, r.Path, r.Name)
		if strings.ContainsRune(r.Name, ' ') {
			t.Errorf("result name %q contains a space: the indexed split is leaking into the "+
				"displayed name", r.Name)
		}
	}
}

// TestHybridSearchDelegatesToKeywordsWhenTheIndexHasNoEmbeddings guards the degrade.
//
// The engine's RRF rank consumes the vector channel's `_distance` column, which no row can
// produce when every embedding is NULL — the failure surfaces as a query-planner error about
// a missing column, not as an actionable message. So the binary question (has vectors, yes
// or no) is recorded at build time, and a hybrid query against a vector-less index must
// answer via keywords instead.
func TestHybridSearchDelegatesToKeywordsWhenTheIndexHasNoEmbeddings(t *testing.T) {
	dir := t.TempDir()
	cache := hybridScaleFixture(t)
	si := buildSearchIndex(t, dir, cache, nil)

	vec := targetVector()
	res, err := si.HybridSearch(context.Background(), "evictOldestStaged", vec, 10)
	if err != nil {
		t.Fatalf("hybrid search with no embeddings errored instead of degrading: %v", err)
	}
	if len(res) == 0 {
		t.Fatal("degraded hybrid search returned nothing the keyword half would have found")
	}
	found := false
	for _, r := range res {
		if r.Name == "evictOldestStaged" {
			found = true
			break
		}
	}
	if !found {
		t.Errorf("degraded hybrid search did not return the sought entity: %+v", res)
	}

	// Same input against the keyword half alone must agree: degrade is Search, not luck.
	keyword, err := si.Search(context.Background(), "evictOldestStaged", 10)
	if err != nil {
		t.Fatalf("keyword search: %v", err)
	}
	if len(res) != len(keyword) {
		t.Errorf("hybrid-degraded had %d results, keyword-only %d — the degrade must reproduce Search",
			len(res), len(keyword))
	}
}
