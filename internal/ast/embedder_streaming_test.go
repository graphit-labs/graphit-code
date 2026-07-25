package ast

import (
	"context"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ai"
)

func TestEmbedSourceSnippet(t *testing.T) {
	src := "l0\nl1\nl2\nl3"
	cases := []struct {
		name          string
		src           string
		line, endLine int
		want          string
	}{
		{"empty source", "", 2, 3, ""},
		{"line zero keeps full source", src, 0, 0, src},
		{"normal slice", src, 2, 3, "l1\nl2"},
		{"single line", src, 1, 1, "l0"},
		{"end beyond len clamps", src, 3, 999, "l2\nl3"},
		{"end le start clamps to end", src, 2, 1, "l1\nl2\nl3"},
		{"start beyond len empty", src, 99, 100, ""},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			if got := embedSourceSnippet(c.src, c.line, c.endLine); got != c.want {
				t.Errorf("embedSourceSnippet(%q,%d,%d) = %q, want %q", c.src, c.line, c.endLine, got, c.want)
			}
		})
	}
}

type fakeEmbClient struct{ texts []string }

func (f *fakeEmbClient) Embed(ctx context.Context, text string) ([]float32, error) {
	v, err := f.EmbedBatch(ctx, []string{text})
	if err != nil {
		return nil, err
	}
	return v[0], nil
}

func (f *fakeEmbClient) EmbedBatch(ctx context.Context, texts []string) ([][]float32, error) {
	f.texts = append(f.texts, texts...)
	out := make([][]float32, len(texts))
	for i := range texts {
		vec := make([]float32, ai.EmbeddingDimensions)
		vec[0] = 1
		out[i] = vec
	}
	return out, nil
}

func (f *fakeEmbClient) ModelName() string { return "fake" }

// TestEmbedderStreamingRunCycle exercises the single-pass streaming scan end to
// end against an on-disk shard cache: it verifies pending counting, that every
// embeddable entity gets a vector, that a warm cache reports zero pending, and
// (crucially) that the precomputed source snippet reaches the embedding text
// byte-identically to the old inline slicing.
func TestEmbedderStreamingRunCycle(t *testing.T) {
	dir := t.TempDir()
	pc, err := NewShardCache(dir)
	if err != nil {
		t.Fatal(err)
	}

	src := "package x\nfunc Foo() {\n\treturn\n}\nfunc Bar() {}\n"
	entry := &parseCacheEntry{
		RelPath:  "a.go",
		Language: "go",
		Source:   src,
		Entities: []cachedEntity{
			{Label: "Function", UID: "uid-foo", Name: "Foo", Path: "a.go", Line: 2, EndLine: 4},
			{Label: "Function", UID: "uid-bar", Name: "Bar", Path: "a.go", Line: 5, EndLine: 5},
			{Label: "Variable", UID: "uid-v", Name: "v", Path: "a.go", Line: 1, EndLine: 1},
			{Label: "Comment", UID: "uid-c", Name: "c", Path: "a.go", Line: 1, EndLine: 1}, // not embeddable
		},
	}
	if err := pc.Store("a.go", "hash1", entry); err != nil {
		t.Fatal(err)
	}
	// Persist so StreamEntries reloads from disk after eviction (mirrors the
	// production embedding loop, which opens a fresh cache from disk).
	if err := pc.FlushDirty(); err != nil {
		t.Fatal(err)
	}

	ec, err := NewShardEmbCache(dir, pc)
	if err != nil {
		t.Fatal(err)
	}

	fc := &fakeEmbClient{}
	cfg := DefaultEmbeddingConfig()
	cfg.ParseCache = pc
	cfg.EmbCache = ec
	e := NewEmbedder(fc, cfg)

	ctx := context.Background()

	// 3 embeddable entities pending (Comment is excluded).
	if got := e.CountPending(ctx); got != 3 {
		t.Fatalf("CountPending = %d, want 3", got)
	}

	n, err := e.RunCycle(ctx)
	if err != nil {
		t.Fatalf("RunCycle: %v", err)
	}
	if n != 3 {
		t.Errorf("RunCycle embedded %d, want 3", n)
	}

	for _, uid := range []string{"uid-foo", "uid-bar", "uid-v"} {
		if vec := ec.Get("a.go", uid, "hash1"); vec == nil {
			t.Errorf("no vector stored for %s", uid)
		}
	}
	if vec := ec.Get("a.go", "uid-c", "hash1"); vec != nil {
		t.Errorf("Comment entity should not have been embedded")
	}

	// Warm cache -> nothing pending.
	if got := e.CountPending(ctx); got != 0 {
		t.Errorf("CountPending after embedding = %d, want 0", got)
	}

	// The precomputed snippet (lines 2..4 of src) must appear in an embedding text.
	wantSnippet := "func Foo() {\n\treturn\n}"
	found := false
	for _, txt := range fc.texts {
		if strings.Contains(txt, wantSnippet) {
			found = true
			break
		}
	}
	if !found {
		t.Errorf("expected an embedding text containing Foo's source snippet %q; got %q", wantSnippet, fc.texts)
	}
}

// TestEmbedderDuplicateUIDFirstLabelWins guards the behavior that when one UID
// appears under two embeddable labels (the embedding cache is keyed on Path+UID
// without the label), the label earliest in embeddableLabels order wins — as the
// old per-label interleaved Get/Set flow did. "Class" precedes "Interface".
func TestEmbedderDuplicateUIDFirstLabelWins(t *testing.T) {
	dir := t.TempDir()
	pc, err := NewShardCache(dir)
	if err != nil {
		t.Fatal(err)
	}
	src := "class Point {}\n// merge\ninterface Point {}\n"
	entry := &parseCacheEntry{
		RelPath:  "a.ts",
		Language: "typescript",
		Source:   src,
		Entities: []cachedEntity{
			// Deliberately list Interface first to prove ordering follows
			// embeddableLabels, not slice/file order.
			{Label: "Interface", UID: "a.ts::Point", Name: "Point", Path: "a.ts", Line: 3, EndLine: 3},
			{Label: "Class", UID: "a.ts::Point", Name: "Point", Path: "a.ts", Line: 1, EndLine: 1},
		},
	}
	if err := pc.Store("a.ts", "h", entry); err != nil {
		t.Fatal(err)
	}
	if err := pc.FlushDirty(); err != nil {
		t.Fatal(err)
	}
	ec, err := NewShardEmbCache(dir, pc)
	if err != nil {
		t.Fatal(err)
	}
	fc := &fakeEmbClient{}
	cfg := DefaultEmbeddingConfig()
	cfg.ParseCache = pc
	cfg.EmbCache = ec
	e := NewEmbedder(fc, cfg)
	ctx := context.Background()

	if got := e.CountPending(ctx); got != 1 {
		t.Fatalf("CountPending = %d, want 1 (deduped by Path+UID)", got)
	}
	n, err := e.RunCycle(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if n != 1 {
		t.Errorf("embedded %d rows, want 1", n)
	}
	if len(fc.texts) != 1 {
		t.Fatalf("embedded %d texts, want 1: %q", len(fc.texts), fc.texts)
	}
	if !strings.HasPrefix(fc.texts[0], "[Class] ") {
		t.Errorf("first-label-wins violated: embedding text = %q, want [Class] prefix", fc.texts[0])
	}
}
