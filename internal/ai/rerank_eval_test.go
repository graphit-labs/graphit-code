//go:build rerankeval

package ai

import (
	"context"
	"fmt"
	"math"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"testing"
	"time"
)

type evalDoc struct {
	name, etype, doc, path string
}

func (d evalDoc) text() string {
	return BuildRerankText(d.name, splitForEval(d.name), d.doc, d.etype, d.path)
}

func splitForEval(s string) string {
	var out []rune
	for i, r := range s {
		if i > 0 && r >= 'A' && r <= 'Z' {
			out = append(out, ' ')
		}
		if r == '_' {
			out = append(out, ' ')
			continue
		}
		out = append(out, r)
	}
	return string(out)
}

func evalCorpus() []evalDoc {
	return []evalDoc{
		{"sortRelsLargestFirst", "Function",
			"Orders the relationship tables by descending row count, which is the order they are created in. The engine bounds every alternative of a type-alternatives pattern by the row count of the first alternative.",
			"internal/ladybugstore/icebug.go"},
		{"writeIndptr", "Function",
			"Writes the N+1 row-pointer offsets of a CSR, including a source node with no outgoing edge.",
			"internal/ladybugstore/icebug.go"},
		{"writeParquet", "Function",
			"Writes a whole table as one Parquet row group. Every call to FileWriter.Write starts a new row group, so batching the rows produced dozens where the reference tool produces one.",
			"internal/ladybugstore/icebug.go"},
		{"sanitizeUTF8", "Function",
			"Makes a value safe for a Parquet string column, reporting whether it had to change anything. A Parquet STRING column is UTF-8 by definition and the engine rejects a file whose string column is not.",
			"internal/ladybugstore/icebug.go"},
		{"foldLabels", "Function",
			"Reads every label's nodes into one table. The column set is the union across labels: a property a label does not have is null for its nodes.",
			"internal/ladybugstore/icebug.go"},
		{"Publish", "Method",
			"Uploads the scope's memories and deletes the ones removed since it was opened. The upload runs in the background so writing a memory never waits on the network.",
			"internal/memory/memory_s3_store.go"},
		{"Pull", "Method",
			"Brings the remote's memories into the raw directory, merging. Nothing local is deleted to match the remote, because a memory just written and not yet uploaded would be the thing deleted.",
			"internal/memory/memory_s3_store.go"},
		{"Prune", "Method",
			"Removes a scope's raw directory and deregisters it. It does not delete the remote prefix: pruning reclaims local disk for a scope this unit no longer tracks.",
			"internal/memory/memory_s3_store.go"},
		{"UnitID", "Function",
			"Identifies this installation of the framework. The default is a generated identifier persisted in the global configuration, so it works with no setup and needs no other tool present.",
			"internal/config/unit.go"},
		{"projectRootDir", "Function",
			"Finds the project the working directory belongs to by walking up for the lockfile, which is the framework's own marker.",
			"internal/memory/identity.go"},
		{"SyncRegistry", "Method",
			"Refreshes the local mirror of the registry from the bucket. It replaces the whole mirror rather than merging, so a document deleted remotely disappears locally too.",
			"internal/hub/s3_store.go"},
		{"WriteEventFile", "Method",
			"Uploads one telemetry event in the background. It never returns an error and never blocks: telemetry that can fail or slow a user's command is worse than telemetry that is missing.",
			"internal/hub/s3_store.go"},
		{"stageEvent", "Method",
			"Keeps a telemetry event that failed to upload so the next flush can retry it, bounded so a broken remote cannot grow the directory without limit.",
			"internal/hub/s3_store.go"},
		{"findBoundary", "Function",
			"Decides how far up the tree ignore files are collected from. Collection must never pass the project, because a pattern from above it gets a domain of parent segments that can never match.",
			"internal/ignorer/ignorer.go"},
		{"quoteIdent", "Function",
			"Renders a column name with backticks. The filter dialect treats a double-quoted name as a string literal, so the predicate matches nothing and the delete silently removes no rows.",
			"internal/lancestore/store_lancedb.go"},
		{"DeleteByKey", "Method",
			"Removes the rows whose key column matches any of the given values, quoted and batched because a caller's keys are arbitrary text.",
			"internal/lancestore/store_lancedb.go"},
		{"Upsert", "Method",
			"Replaces the rows sharing a key and appends the rest. It is delete-then-append, and the order matters so a re-indexed file cannot leave a stale copy of itself behind.",
			"internal/lancestore/store_lancedb.go"},
		{"Search", "Method",
			"Runs a full-text, semantic or hybrid query. When both text and vector are set the engine runs both passes and fuses them with its own reciprocal-rank-fusion reranker.",
			"internal/lancestore/store_lancedb.go"},
		{"BuildRerankText", "Function",
			"Renders a candidate for the cross-encoder: the identifier, its split form and the docstring, and not the gram bag, which to a transformer is meaningless three-letter tokens.",
			"internal/ai/rerank_adapter.go"},
		{"Score", "Method",
			"Returns one relevance score per candidate. A candidate that cannot be tokenised scores the lowest possible value rather than failing the batch.",
			"internal/ai/rerank_local.go"},
		{"gramBag", "Function",
			"Emits the two- and three-grams of a string, so a truncated query can reach the identifier it abbreviates.",
			"internal/lancestore/probe.go"},
		{"remotePrefix", "Function",
			"The key prefix holding one memory scope. A leading path element is stripped first so it cannot be doubled by a caller that passes the path either way.",
			"internal/memory/memory_s3_store.go"},
		{"evictOldestStaged", "Method",
			"Drops the oldest staged telemetry events until there is room for one more.",
			"internal/hub/s3_store.go"},
		{"EnsureIndexes", "Method",
			"Builds the given indexes, skipping any that already exist. A full-text query needs an inverted index on its column and returns nothing without one.",
			"internal/lancestore/store_lancedb.go"},
	}
}

type evalQuery struct {
	q    string
	want string
	why  string
}

func evalQueries() []evalQuery {
	return []evalQuery{
		{q: "why does the order of table creation matter for queries with alternatives",
			want: "sortRelsLargestFirst", why: "the only entity about creation order bounding alternatives"},
		{q: "how do I stop the parquet writer from making many row groups",
			want: "writeParquet", why: "the only entity about row group count"},
		{q: "what handles bytes that are not valid text before writing a column",
			want: "sanitizeUTF8", why: "the only entity about invalid UTF-8"},
		{q: "where do I make sure a memory written locally is not lost when syncing",
			want: "Pull", why: "merging rather than mirroring is what protects the unpublished write"},
		{q: "how is telemetry kept from slowing down a user command",
			want: "WriteEventFile", why: "the background upload that never blocks"},
		{q: "what keeps a broken remote from filling the disk with retries",
			want: "evictOldestStaged", why: "the bound on the retry directory"},
		{q: "why did my delete remove no rows without reporting an error",
			want: "quoteIdent", why: "the double-quote-as-literal trap"},
		{q: "how does the framework know which installation it is running on",
			want: "UnitID", why: "the installation identity"},
		{q: "how is the project root found without a version control system",
			want: "projectRootDir", why: "walking up for the lockfile"},
		{q: "how far up the directory tree are ignore patterns collected",
			want: "findBoundary", why: "the collection boundary"},
		{q: "what makes a reindexed file not leave an old copy behind",
			want: "Upsert", why: "delete-then-append ordering"},
		{q: "what is fed to the relevance model for each candidate",
			want: "BuildRerankText", why: "the candidate rendering"},
		{q: "how does a truncated word reach the longer identifier it abbreviates",
			want: "gramBag", why: "the n-gram expansion"},
		{q: "why does my full text query return nothing at all",
			want: "EnsureIndexes", why: "the missing inverted index"},
		{q: "what reclaims local disk for a scope this machine stopped using",
			want: "Prune", why: "local prune that leaves the remote alone"},
		{q: "how are both keyword and vector results combined into one ranking",
			want: "Search", why: "the engine's own fusion"},
	}
}

func lexicalRank(query string, docs []evalDoc) []int {
	terms := strings.Fields(strings.ToLower(query))
	df := make(map[string]int, len(terms))
	for _, t := range terms {
		for _, d := range docs {
			if strings.Contains(strings.ToLower(d.text()), t) {
				df[t]++
			}
		}
	}
	type scored struct {
		idx int
		s   float64
	}
	out := make([]scored, len(docs))
	for i, d := range docs {
		low := strings.ToLower(d.text())
		var s float64
		for _, t := range terms {
			if df[t] == 0 {
				continue
			}
			n := float64(strings.Count(low, t))
			if n == 0 {
				continue
			}
			idf := math.Log(float64(len(docs)) / float64(df[t]))
			s += n * idf
		}
		out[i] = scored{i, s}
	}
	sort.SliceStable(out, func(a, b int) bool {
		if out[a].s != out[b].s {
			return out[a].s > out[b].s
		}
		return out[a].idx < out[b].idx
	})
	order := make([]int, len(out))
	for i, s := range out {
		order[i] = s.idx
	}
	return order
}

func rankOf(order []int, docs []evalDoc, want string) int {
	for pos, idx := range order {
		if docs[idx].name == want {
			return pos + 1
		}
	}
	return 0
}

func mrrOf(rank int) float64 {
	if rank == 0 {
		return 0
	}
	return 1 / float64(rank)
}

func ndcgAt(rank, k int) float64 {
	if rank == 0 || rank > k {
		return 0
	}
	return 1 / math.Log2(float64(rank)+1)
}

func TestRerankEvalMeasuresTheRealModel(t *testing.T) {
	dir := os.Getenv("GRAPHIT_RERANK_MODEL_DIR")
	if dir == "" {
		t.Skip("set GRAPHIT_RERANK_MODEL_DIR to a directory holding model.onnx and tokenizer.json")
	}
	modelPath := filepath.Join(dir, "model.onnx")
	tokenizerPath := filepath.Join(dir, "tokenizer.json")
	for _, p := range []string{modelPath, tokenizerPath} {
		if _, err := os.Stat(p); err != nil {
			t.Fatalf("missing %s: %v", p, err)
		}
	}

	rr, err := newCrossEncoderFrom(modelPath, tokenizerPath)
	if err != nil {
		t.Fatalf("load the reranker from %s: %v", dir, err)
	}
	defer rr.Close()

	docs := evalCorpus()
	queries := evalQueries()
	adapter := RerankAdapter{Scorer: rr}
	ctx := context.Background()

	var baseMRR, rankMRR, baseNDCG, rankNDCG float64
	var baseTop1, rankTop1, improved, worsened int
	var totalInference time.Duration
	var outsideWindow int
	var fullCorpusMRR float64

	t.Logf("%-58s %-24s %-8s %s", "query", "expected", "lexical", "reranked")
	t.Logf("%s", strings.Repeat("-", 108))

	for _, q := range queries {
		order := lexicalRank(q.q, docs)

		const candidates = 10

		baseRank := rankOf(order, docs, q.want)
		fullCorpusMRR += mrrOf(baseRank)

		if baseRank > candidates {
			baseRank = 0
			outsideWindow++
		}

		top := order
		if len(top) > candidates {
			top = top[:candidates]
		}
		hits := make([]RerankHit, len(top))
		for i, idx := range top {
			hits[i] = RerankHit{Text: docs[idx].text(), Index: idx}
		}

		start := time.Now()
		ranked, err := adapter.Rank(ctx, q.q, hits)
		totalInference += time.Since(start)
		if err != nil {
			t.Fatalf("rerank %q: %v", q.q, err)
		}

		rankedOrder := make([]int, len(ranked))
		for i, h := range ranked {
			rankedOrder[i] = h.Index
		}
		newRank := rankOf(rankedOrder, docs, q.want)

		baseMRR += mrrOf(baseRank)
		rankMRR += mrrOf(newRank)
		baseNDCG += ndcgAt(baseRank, 10)
		rankNDCG += ndcgAt(newRank, 10)
		if baseRank == 1 {
			baseTop1++
		}
		if newRank == 1 {
			rankTop1++
		}
		switch {
		case newRank != 0 && (baseRank == 0 || newRank < baseRank):
			improved++
		case baseRank != 0 && (newRank == 0 || newRank > baseRank):
			worsened++
		}

		mark := func(r int) string {
			if r == 0 {
				return "-"
			}
			return fmt.Sprint(r)
		}
		q58 := q.q
		if len(q58) > 56 {
			q58 = q58[:56]
		}
		t.Logf("%-58s %-24s %-8s %s", q58, q.want, mark(baseRank), mark(newRank))
	}

	n := float64(len(queries))
	t.Logf("%s", strings.Repeat("-", 108))
	t.Logf("model dir:        %s", dir)
	t.Logf("top-1:            lexical %d/%d  ->  reranked %d/%d", baseTop1, len(queries), rankTop1, len(queries))
	t.Logf("MRR:              lexical %.3f    ->  reranked %.3f", baseMRR/n, rankMRR/n)
	t.Logf("nDCG@10:          lexical %.3f    ->  reranked %.3f", baseNDCG/n, rankNDCG/n)
	t.Logf("queries improved: %d   worsened: %d   unchanged: %d",
		improved, worsened, len(queries)-improved-worsened)
	t.Logf("first-stage miss: %d/%d answers were outside the %d-candidate window (MRR over the "+
		"whole corpus is %.3f — the gap to the windowed baseline is what retrieval costs, not the reranker)",
		outsideWindow, len(queries), 10, fullCorpusMRR/n)
	t.Logf("inference:        %s total, %s per query over %d candidates",
		totalInference.Round(time.Millisecond),
		(totalInference / time.Duration(len(queries))).Round(time.Millisecond), 10)

	if baseTop1 == len(queries) {
		t.Errorf("the lexical baseline already answers every query first — this set has no headroom "+
			"and cannot measure a reranker (%d/%d)", baseTop1, len(queries))
	}
}
