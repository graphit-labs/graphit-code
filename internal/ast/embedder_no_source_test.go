//go:build lancedb

package ast

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func noSourceFixture(t *testing.T, relPath, body string, ents []cachedEntity) (*Embedder, string) {
	t.Helper()

	repoRoot := t.TempDir()
	stageEmbedLabelsIn(t, repoRoot, "Function", LabelComment)

	full := filepath.Join(repoRoot, relPath)
	if err := os.MkdirAll(filepath.Dir(full), 0o755); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	if err := os.WriteFile(full, []byte(body), 0o644); err != nil {
		t.Fatalf("write file: %v", err)
	}

	cacheDir := t.TempDir()
	cache, err := NewShardCache(cacheDir)
	if err != nil {
		t.Fatalf("shard cache: %v", err)
	}
	cache.SetRoot(repoRoot)
	t.Cleanup(func() { _ = cache.Close() })

	entry := &parseCacheEntry{RelPath: relPath, Language: embedLabelsTestLang, Entities: ents}
	if err := cache.Store(relPath, fileContentHash(full), entry); err != nil {
		t.Fatalf("store shard: %v", err)
	}
	if err := cache.FlushDirty(); err != nil {
		t.Fatalf("flush shard: %v", err)
	}

	idx, err := OpenSearchIndex(context.Background(), cacheDir)
	if err != nil {
		t.Fatalf("search index: %v", err)
	}
	t.Cleanup(func() { _ = idx.Close() })

	embCache, err := NewShardEmbCache(cacheDir, cache)
	if err != nil {
		t.Fatalf("emb cache: %v", err)
	}
	t.Cleanup(func() { _ = embCache.Close() })

	cfg := DefaultEmbeddingConfig()
	cfg.ParseCache = cache
	cfg.Index = idx
	cfg.EmbCache = embCache
	cfg.RepoRoot = repoRoot
	cfg.ProjectDir = repoRoot

	return NewEmbedder(nil, cfg), repoRoot
}

var noSourceBody = "package svc\n" +
	"\n" +
	"func ChargeCard(amount int) error {\n" +
	"\tgateway.Authorize(amount)\n" +
	"\treturn nil\n" +
	"}\n"

func noSourceEntities() []cachedEntity {
	return []cachedEntity{{
		UID:     "svc/pay.go::ChargeCard",
		Label:   "Function",
		Lang:    embedLabelsTestLang,
		Name:    "ChargeCard",
		Path:    "svc/pay.go",
		Line:    3,
		EndLine: 6,
	}}
}

// TestEmbeddingKeepsSourceSignalWithoutPersistingSource is the requirement behind
// ast.index_source: false.
//
// The flag says "do not keep a copy of the source", not "do not look at the source".
// An embedding is a vector, not recoverable text, so it can be computed from the file
// and persisted while the text never is. Before this, the snippet came only from
// entry.Source — empty under the flag — so semantic search silently lost the one part
// of the embedded text that describes what an entity DOES rather than what it is called.
func TestEmbeddingKeepsSourceSignalWithoutPersistingSource(t *testing.T) {
	e, _ := noSourceFixture(t, "svc/pay.go", noSourceBody, noSourceEntities())

	buckets := e.scanPending(true)
	rows := buckets["Function"]
	if len(rows) != 1 {
		t.Fatalf("got %d pending Function rows, want 1", len(rows))
	}

	if !strings.Contains(rows[0].Source, "gateway.Authorize(amount)") {
		t.Errorf("the snippet does not carry the body, so the embedding lost the "+
			"source signal:\n%q", rows[0].Source)
	}

	text := e.buildEmbeddingText(rows[0])
	for _, want := range []string{"ChargeCard", "gateway.Authorize(amount)"} {
		if !strings.Contains(text, want) {
			t.Errorf("embedding text is missing %q:\n%s", want, text)
		}
	}
}

// A shard never carries file text — not only when index_source is off. The text is a
// copy of a file that is already on disk, and its size is bounded by nothing but the
// size of the corpus, so reading a file for an embedding must not write it back.
// Asserted against the shard ON DISK, which is the artifact that would leak.
func TestShardOnDiskNeverCarriesFileText(t *testing.T) {
	const rel = "svc/pay.go"
	e, repoRoot := noSourceFixture(t, rel, noSourceBody, noSourceEntities())
	if rows := e.scanPending(true)["Function"]; len(rows) != 1 || rows[0].Source == "" {
		t.Fatalf("the fixture did not produce an embedded snippet: %+v", rows)
	}

	fullPath := filepath.Join(repoRoot, rel)
	entry := ConvertToCache(&ParsedFile{
		Path:     fullPath,
		Source:   noSourceBody,
		Language: embedLabelsTestLang,
	}, repoRoot, true, "service")
	if err := e.cfg.ParseCache.Store(rel, fileContentHash(fullPath), entry); err != nil {
		t.Fatalf("store source-indexed shard: %v", err)
	}
	if err := e.cfg.ParseCache.FlushDirty(); err != nil {
		t.Fatalf("flush: %v", err)
	}

	raw, err := os.ReadFile(filepath.Join(e.cfg.ParseCache.dir, "shards", rel+shardNodesSuffix))
	if err != nil {
		t.Fatalf("read shard: %v", err)
	}
	raw, err = shardZstdDecoder.DecodeAll(raw, nil)
	if err != nil {
		t.Fatalf("decompress shard: %v", err)
	}
	if strings.Contains(string(raw), "gateway.Authorize") {
		t.Error("the file body leaked into the shard JSON")
	}
	if strings.Contains(string(raw), "file_row") {
		t.Error("the legacy file_row tuple leaked into the shard JSON")
	}

	reloaded, err := NewShardCache(e.cfg.ParseCache.dir)
	if err != nil {
		t.Fatalf("reload shard cache: %v", err)
	}
	defer reloaded.Close()
	if got := reloaded.GetEntry(rel).Cluster; got != "service" {
		t.Errorf("cluster = %q, want service", got)
	}
}

// SAFETY: the embedding cache is keyed on the shard's content hash, so a snippet read
// from a file that no longer matches would cache a vector describing code the graph
// does not contain. A stale file must yield no snippet rather than the wrong one.
func TestEmbeddingSkipsSnippetWhenTheFileNoLongerMatches(t *testing.T) {
	e, repoRoot := noSourceFixture(t, "svc/pay.go", noSourceBody, noSourceEntities())

	if err := os.WriteFile(filepath.Join(repoRoot, "svc/pay.go"),
		[]byte("package svc\n\nfunc Something Else Entirely() {}\n"), 0o644); err != nil {
		t.Fatalf("rewrite file: %v", err)
	}

	rows := e.scanPending(true)["Function"]
	if len(rows) != 1 {
		t.Fatalf("got %d rows, want 1", len(rows))
	}
	if rows[0].Source != "" {
		t.Errorf("a file whose hash no longer matches the shard must not be read:\n%q",
			rows[0].Source)
	}
}

// With no root there is nothing to read, and that has to degrade rather than fail: the
// embedding falls back to name, docstring and context.
func TestEmbeddingWithoutRepoRootStillEmbedsTheEntity(t *testing.T) {
	e, _ := noSourceFixture(t, "svc/pay.go", noSourceBody, noSourceEntities())
	e.cfg.RepoRoot = ""
	e.cfg.ParseCache.SetRoot("")

	rows := e.scanPending(true)["Function"]
	if len(rows) != 1 {
		t.Fatalf("got %d rows, want 1", len(rows))
	}
	if rows[0].Source != "" {
		t.Errorf("no root means no snippet, got %q", rows[0].Source)
	}
	if !strings.Contains(e.buildEmbeddingText(rows[0]), "ChargeCard") {
		t.Error("the entity must still be embeddable by name")
	}
}

// The working tree is the ONLY source of the snippet. An in-memory entry that still
// carries text — the parser sets it on the way through — must not be able to answer
// with something the file does not say, because that text is never persisted and the
// next session would embed the file's version of the same entity.
func TestEmbeddingSnippetComesFromTheFileNotTheCachedEntry(t *testing.T) {
	const rel = "svc/pay.go"
	e, repoRoot := noSourceFixture(t, rel, noSourceBody, noSourceEntities())

	stale := strings.Replace(noSourceBody, "gateway.Authorize", "CACHED_MARKER", 1)
	if err := e.cfg.ParseCache.Store(rel, fileContentHash(filepath.Join(repoRoot, rel)),
		&parseCacheEntry{RelPath: rel, Language: embedLabelsTestLang, Source: stale,
			Entities: noSourceEntities()}); err != nil {
		t.Fatalf("store shard: %v", err)
	}

	rows := e.scanPending(true)["Function"]
	if len(rows) != 1 {
		t.Fatalf("got %d rows, want 1", len(rows))
	}
	if strings.Contains(rows[0].Source, "CACHED_MARKER") {
		t.Errorf("the snippet came from the entry's unpersisted text:\n%q", rows[0].Source)
	}
	if !strings.Contains(rows[0].Source, "gateway.Authorize") {
		t.Errorf("the snippet should be what the file on disk says:\n%q", rows[0].Source)
	}
}
