package ast

import (
	"context"
	"os"
	"path/filepath"
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
// embeddable entity gets a vector — comments included, since their text is the
// prose semantic search exists for — that a warm cache reports zero pending, that
// the precomputed source snippet reaches the embedding text byte-identically to
// the old inline slicing, and that a comment is the one label that gets NO
// snippet, because its snippet is its own name a second time.
func TestEmbedderStreamingRunCycle(t *testing.T) {
	dir := t.TempDir()
	pc, err := NewShardCache(dir)
	if err != nil {
		t.Fatal(err)
	}

	lang := embedLabelsTestLang
	projectDir := stageEmbedLabelsGrammar(t, "Function", "Variable", LabelComment)

	src := "package x\n// documents Foo\nfunc Foo() {\n\treturn\n}\nfunc Bar() {}\n"
	// The snippet is read from the working tree, so the fixture needs a real file —
	// the shard no longer carries a copy of it.
	repoRoot := t.TempDir()
	if err := os.WriteFile(filepath.Join(repoRoot, "a.go"), []byte(src), 0o644); err != nil {
		t.Fatal(err)
	}
	srcHash := contentHashOf([]byte(src))
	pc.SetRoot(repoRoot)

	entry := &parseCacheEntry{
		RelPath:  "a.go",
		Language: lang,
		Entities: []cachedEntity{
			{Label: "Function", Lang: lang, UID: "uid-foo", Name: "Foo", Path: "a.go", Line: 3, EndLine: 5},
			{Label: "Function", Lang: lang, UID: "uid-bar", Name: "Bar", Path: "a.go", Line: 6, EndLine: 6},
			{Label: "Variable", Lang: lang, UID: "uid-v", Name: "v", Path: "a.go", Line: 1, EndLine: 1},
			{Label: "Comment", Lang: lang, UID: "uid-c", Name: "documents Foo", Path: "a.go", Line: 2, EndLine: 2},
		},
	}
	if err := pc.Store("a.go", srcHash, entry); err != nil {
		t.Fatal(err)
	}
	// Persist so StreamEntries reloads from disk after eviction (mirrors the
	// production embedding loop, which opens a fresh cache from disk).
	if err := pc.FlushDirty(); err != nil {
		t.Fatal(err)
	}

	idx, err := OpenSearchIndex(context.Background(), dir)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = idx.Close() })

	fc := &fakeEmbClient{}
	cfg := DefaultEmbeddingConfig()
	cfg.ParseCache = pc
	cfg.Index = idx
	cfg.ProjectDir = projectDir
	cfg.RepoRoot = repoRoot
	e := NewEmbedder(fc, cfg)

	ctx := context.Background()

	// All 4 entities are embeddable — the comment among them.
	if got := e.CountPending(ctx); got != 4 {
		t.Fatalf("CountPending = %d, want 4", got)
	}

	n, err := e.RunCycle(ctx)
	if err != nil {
		t.Fatalf("RunCycle: %v", err)
	}
	if n != 4 {
		t.Errorf("RunCycle embedded %d, want 4", n)
	}

	// The vector's only home is the entity's row in the search index.
	embedded, err := idx.EmbeddedUIDs(ctx)
	if err != nil {
		t.Fatalf("EmbeddedUIDs: %v", err)
	}
	for _, uid := range []string{"uid-foo", "uid-bar", "uid-v", "uid-c"} {
		if _, ok := embedded[uid]; !ok {
			t.Errorf("no vector stored for %s", uid)
		}
	}

	// Warm cache -> nothing pending.
	if got := e.CountPending(ctx); got != 0 {
		t.Errorf("CountPending after embedding = %d, want 0", got)
	}

	// The precomputed snippet (lines 3..5 of src) must appear in an embedding text.
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

	// The comment is embedded by its text, and ONLY by its text: the raw source
	// line ("// documents Foo") must not be appended on top of the name.
	commentText := ""
	for _, txt := range fc.texts {
		if strings.HasPrefix(txt, "["+LabelComment+"]") {
			commentText = txt
			break
		}
	}
	if commentText == "" {
		t.Fatalf("no embedding text for the comment; got %q", fc.texts)
	}
	if !strings.Contains(commentText, "documents Foo") {
		t.Errorf("comment embedding text lost its text: %q", commentText)
	}
	if strings.Contains(commentText, "// documents Foo") {
		t.Errorf("comment embedding text repeats its own source line: %q", commentText)
	}
}

// TestEmbedderDuplicateUIDFirstLabelWins guards the behavior that when one UID
// appears under two embeddable labels (the embedding cache is keyed on Path+UID
// without the label), the label its GRAMMAR listed first in embed_labels wins — as
// the old per-label interleaved Get/Set flow did. Here the grammar says Class,
// then Interface, so Class keeps the entity.
func TestEmbedderDuplicateUIDFirstLabelWins(t *testing.T) {
	lang := embedLabelsTestLang
	projectDir := stageEmbedLabelsGrammar(t, "Class", "Interface")

	dir := t.TempDir()
	pc, err := NewShardCache(dir)
	if err != nil {
		t.Fatal(err)
	}
	src := "class Point {}\n// merge\ninterface Point {}\n"
	entry := &parseCacheEntry{
		RelPath:  "a.ts",
		Language: lang,
		Source:   src,
		Entities: []cachedEntity{
			// Deliberately list Interface first to prove ordering follows the
			// grammar's embed_labels, not slice/file order.
			{Label: "Interface", Lang: lang, UID: "a.ts::Point", Name: "Point", Path: "a.ts", Line: 3, EndLine: 3},
			{Label: "Class", Lang: lang, UID: "a.ts::Point", Name: "Point", Path: "a.ts", Line: 1, EndLine: 1},
		},
	}
	if err := pc.Store("a.ts", "h", entry); err != nil {
		t.Fatal(err)
	}
	if err := pc.FlushDirty(); err != nil {
		t.Fatal(err)
	}
	idx, err := OpenSearchIndex(context.Background(), dir)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = idx.Close() })
	fc := &fakeEmbClient{}
	cfg := DefaultEmbeddingConfig()
	cfg.ParseCache = pc
	cfg.Index = idx
	cfg.ProjectDir = projectDir
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
