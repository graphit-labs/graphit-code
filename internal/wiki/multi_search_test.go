//go:build lancedb

package wiki

import (
	"context"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/lancestore"
)

func setupMultiWikiDirs(t *testing.T) (string, string) {
	t.Helper()

	// Two compiled indexes, not two directories of pages: the index IS the wiki, and the
	// markdown scan these fixtures used to feed was deleted with the page output.
	dir1 := indexedWiki(t, []WikiChunk{
		{Slug: "Design", Title: "Design", Body: "The system follows clean architecture patterns.", DocType: "specification", WordCount: 6},
		{Slug: "API", Title: "API", Body: "REST endpoints for operations.", DocType: "specification", WordCount: 4},
	})
	dir2 := indexedWiki(t, []WikiChunk{
		{Slug: "Decisions", Title: "Decisions", Body: "We chose Go for performance reasons.", DocType: "decision", WordCount: 6},
		{Slug: "Conventions", Title: "Conventions", Body: "Use snake_case for file names.", DocType: "convention", WordCount: 5},
	})
	return dir1, dir2
}

// indexedWiki builds a compiled wiki in a temp directory and returns it.
func indexedWiki(t *testing.T, chunks []WikiChunk) string {
	t.Helper()
	dir := t.TempDir()
	if err := SyncDB(context.Background(), dir, chunks, nil, nil); err != nil {
		t.Fatalf("building the probe index: %v", err)
	}
	return dir
}

func TestSearchMultiWiki_NoSources(t *testing.T) {
	t.Parallel()
	client := &mockAIClient{responses: []string{"DONE: answer"}}
	_, err := SearchMultiWiki(context.Background(), client, "query", MultiWikiSearchConfig{})
	if err == nil {
		t.Error("expected error for no sources")
	}
}

func TestSearchMultiWiki_SingleSource(t *testing.T) {
	t.Parallel()
	dir1, _ := setupMultiWikiDirs(t)
	client := &mockAIClient{responses: []string{"DONE: Single source comprehensive answer about the design."}}

	result, err := SearchMultiWiki(context.Background(), client, "design", MultiWikiSearchConfig{
		Sources:  []WikiSource{{ID: "knowledge", Label: "Knowledge", Dir: dir1}},
		MaxTurns: 5,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.Answer == "" {
		t.Error("expected non-empty answer")
	}
}

func TestSearchMultiWiki_SingleMountedSourceUsesStoreConfig(t *testing.T) {
	t.Parallel()
	dir, _ := setupMultiWikiDirs(t)
	cfg := lancestore.Config{URI: WikiIndexPath(dir)}
	client := &mockAIClient{responses: []string{"DONE: Mounted source answer."}}

	result, err := SearchMultiWiki(context.Background(), client, "design", MultiWikiSearchConfig{
		Sources: []WikiSource{{
			ID:          "hub/design",
			Label:       "Published design",
			Dir:         "/path-that-must-not-be-opened",
			StoreConfig: &cfg,
		}},
	})
	if err != nil {
		t.Fatalf("mounted source: %v", err)
	}
	if result.Answer != "Mounted source answer." {
		t.Fatalf("answer = %q", result.Answer)
	}
}

func TestSearchMultiWiki_DirectAnswer(t *testing.T) {
	t.Parallel()
	dir1, dir2 := setupMultiWikiDirs(t)
	client := &mockAIClient{responses: []string{"DONE: Comprehensive answer from multiple wikis about architecture and decisions."}}

	result, err := SearchMultiWiki(context.Background(), client, "architecture", MultiWikiSearchConfig{
		Sources: []WikiSource{
			{ID: "knowledge", Label: "Knowledge", Dir: dir1},
			{ID: "memory", Label: "Memory", Dir: dir2},
		},
		MaxTurns: 5,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !strings.Contains(result.Answer, "Comprehensive") {
		t.Errorf("answer = %q, expected comprehensive response", result.Answer)
	}
}

func TestSearchMultiWiki_DirectAnswer_DoneSpace(t *testing.T) {
	t.Parallel()
	dir1, dir2 := setupMultiWikiDirs(t)
	client := &mockAIClient{responses: []string{"DONE Answer with space prefix comprehensive explanation."}}

	result, err := SearchMultiWiki(context.Background(), client, "query", MultiWikiSearchConfig{
		Sources: []WikiSource{
			{ID: "knowledge", Label: "Knowledge", Dir: dir1},
			{ID: "memory", Label: "Memory", Dir: dir2},
		},
		MaxTurns: 5,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.Answer == "" {
		t.Error("expected non-empty answer for DONE-space prefix")
	}
}

func TestSearchMultiWiki_WithPageRequests(t *testing.T) {
	t.Parallel()
	dir1, dir2 := setupMultiWikiDirs(t)
	client := &mockAIClient{responses: []string{
		"[knowledge]/Design",
		"[memory]/Decisions",
		"DONE: The design follows clean architecture and we chose Go for performance.",
	}}

	result, err := SearchMultiWiki(context.Background(), client, "design decisions", MultiWikiSearchConfig{
		Sources: []WikiSource{
			{ID: "knowledge", Label: "Knowledge", Dir: dir1},
			{ID: "memory", Label: "Memory", Dir: dir2},
		},
		MaxTurns: 5,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.Turns < 2 {
		t.Errorf("turns = %d, expected at least 2", result.Turns)
	}
}

func TestSearchMultiWiki_NoMatchingPages(t *testing.T) {
	t.Parallel()
	dir1, dir2 := setupMultiWikiDirs(t)
	client := &mockAIClient{responses: []string{"[knowledge]/nonexistent_page"}}

	result, err := SearchMultiWiki(context.Background(), client, "query", MultiWikiSearchConfig{
		Sources: []WikiSource{
			{ID: "knowledge", Label: "Knowledge", Dir: dir1},
			{ID: "memory", Label: "Memory", Dir: dir2},
		},
		MaxTurns: 5,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !strings.Contains(result.Answer, "no matching pages") {
		t.Errorf("answer = %q, expected no matching pages message", result.Answer)
	}
}

func TestSearchMultiWiki_AlreadyLoadedPages(t *testing.T) {
	t.Parallel()
	dir1, dir2 := setupMultiWikiDirs(t)
	client := &mockAIClient{responses: []string{
		"[knowledge]/Design",
		"[knowledge]/Design",
		"DONE: Final comprehensive answer about design patterns.",
	}}

	result, err := SearchMultiWiki(context.Background(), client, "design", MultiWikiSearchConfig{
		Sources: []WikiSource{
			{ID: "knowledge", Label: "Knowledge", Dir: dir1},
			{ID: "memory", Label: "Memory", Dir: dir2},
		},
		MaxTurns: 5,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.Turns < 2 {
		t.Errorf("expected multiple turns for already-loaded pages scenario")
	}
}

func TestSearchMultiWiki_DefaultMaxTurns(t *testing.T) {
	t.Parallel()
	dir1, dir2 := setupMultiWikiDirs(t)
	client := &mockAIClient{responses: []string{"DONE: Quick comprehensive answer."}}

	result, err := SearchMultiWiki(context.Background(), client, "query", MultiWikiSearchConfig{
		Sources: []WikiSource{
			{ID: "knowledge", Label: "Knowledge", Dir: dir1},
			{ID: "memory", Label: "Memory", Dir: dir2},
		},
		MaxTurns: 0,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.Answer == "" {
		t.Error("expected non-empty answer")
	}
}

func TestSearchMultiWiki_NoParsedPages(t *testing.T) {
	t.Parallel()
	dir1, dir2 := setupMultiWikiDirs(t)
	client := &mockAIClient{responses: []string{
		"This is a direct answer without DONE prefix providing a comprehensive overview.",
	}}

	result, err := SearchMultiWiki(context.Background(), client, "query", MultiWikiSearchConfig{
		Sources: []WikiSource{
			{ID: "knowledge", Label: "Knowledge", Dir: dir1},
			{ID: "memory", Label: "Memory", Dir: dir2},
		},
		MaxTurns: 5,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.Answer == "" {
		t.Error("expected non-empty answer")
	}
}

func TestSearchMultiWiki_BM25PreFilter(t *testing.T) {
	t.Parallel()
	dir1, dir2 := setupMultiWikiDirs(t)
	client := &mockAIClient{responses: []string{"DONE: BM25-filtered comprehensive answer about architecture."}}

	result, err := SearchMultiWiki(context.Background(), client, "architecture", MultiWikiSearchConfig{
		Sources: []WikiSource{
			{ID: "knowledge", Label: "Knowledge", Dir: dir1},
			{ID: "memory", Label: "Memory", Dir: dir2},
		},
		MaxTurns:          5,
		UseBM25:           true,
		BM25TopNPerSource: 3,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.Answer == "" {
		t.Error("expected non-empty answer with BM25")
	}
}

func TestSearchMultiWiki_PageRefRetry(t *testing.T) {
	t.Parallel()
	dir1, dir2 := setupMultiWikiDirs(t)
	client := &mockAIClient{responses: []string{
		"DONE: [[Design]]\n[[Decisions]]",
		"After analysis, the design uses clean architecture and Go was chosen for performance.",
	}}

	result, err := SearchMultiWiki(context.Background(), client, "overview", MultiWikiSearchConfig{
		Sources: []WikiSource{
			{ID: "knowledge", Label: "Knowledge", Dir: dir1},
			{ID: "memory", Label: "Memory", Dir: dir2},
		},
		MaxTurns: 5,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if strings.HasPrefix(result.Answer, "[[") {
		t.Errorf("expected retry to produce non-ref answer, got %q", result.Answer)
	}
}

func TestSearchMultiWiki_MissingIndex(t *testing.T) {
	t.Parallel()
	dir1 := t.TempDir()
	dir2 := t.TempDir()
	writeFile(t, dir2, "index.md", "# Wiki\n- [[Page]]")
	writeFile(t, dir2, "Page.md", "# Page\nContent.")

	client := &mockAIClient{responses: []string{"DONE: Answer from second wiki source."}}

	result, err := SearchMultiWiki(context.Background(), client, "query", MultiWikiSearchConfig{
		Sources: []WikiSource{
			{ID: "src1", Label: "Source 1", Dir: dir1},
			{ID: "src2", Label: "Source 2", Dir: dir2},
		},
		MaxTurns: 5,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.Answer == "" {
		t.Error("expected answer even with one missing index")
	}
}

func TestBM25SearchMulti(t *testing.T) {
	t.Parallel()
	dir1, dir2 := setupMultiWikiDirs(t)

	results := BM25SearchMulti(context.Background(), []WikiSource{
		{ID: "knowledge", Label: "Knowledge", Dir: dir1},
		{ID: "memory", Label: "Memory", Dir: dir2},
	}, "architecture design", 3)

	if len(results) == 0 {
		t.Error("expected BM25 results across multiple sources")
	}

	for _, r := range results {
		if r.SourceID == "" {
			t.Error("expected SourceID on each result")
		}
		if r.SourceLabel == "" {
			t.Error("expected SourceLabel on each result")
		}
	}
}

func TestBM25SearchMulti_NoResults(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	writeFile(t, dir, "index.md", "# Empty Wiki")

	results := BM25SearchMulti(context.Background(), []WikiSource{
		{ID: "empty", Label: "Empty", Dir: dir},
	}, "xyznonexistent", 5)

	if len(results) != 0 {
		t.Errorf("expected no results, got %d", len(results))
	}
}

func TestBm25PreFilterMulti(t *testing.T) {
	t.Parallel()
	dir1, dir2 := setupMultiWikiDirs(t)

	result := bm25PreFilterMulti(context.Background(), []WikiSource{
		{ID: "knowledge", Label: "Knowledge", Dir: dir1},
		{ID: "memory", Label: "Memory", Dir: dir2},
	}, "architecture design", 3)

	if result == "" {
		t.Error("expected non-empty BM25 pre-filter output")
	}
	if !strings.Contains(result, "multi-wiki") {
		t.Error("expected 'multi-wiki' in pre-filter header")
	}
}

func TestBm25PreFilterMulti_Empty(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	writeFile(t, dir, "index.md", "# Empty")

	result := bm25PreFilterMulti(context.Background(), []WikiSource{
		{ID: "empty", Label: "Empty", Dir: dir},
	}, "xyznonexistent", 5)

	if result != "" {
		t.Errorf("expected empty pre-filter for no results, got %q", result)
	}
}

func TestBuildMultiSearchSystemPrompt(t *testing.T) {
	t.Parallel()
	sources := []WikiSource{
		{ID: "knowledge", Label: "Knowledge Wiki"},
		{ID: "memory", Label: "Memory Wiki"},
	}

	prompt := buildMultiSearchSystemPrompt(sources)
	if !strings.Contains(prompt, "[knowledge]") {
		t.Error("expected source ID in prompt")
	}
	if !strings.Contains(prompt, "Knowledge Wiki") {
		t.Error("expected source label in prompt")
	}
	if !strings.Contains(prompt, "[memory]") {
		t.Error("expected memory source ID in prompt")
	}
	if !strings.Contains(prompt, "DONE:") {
		t.Error("expected DONE protocol in prompt")
	}
}

func TestParseMultiPageList(t *testing.T) {
	t.Parallel()
	sources := []WikiSource{
		{ID: "knowledge", Label: "Knowledge", Dir: "/tmp/k"},
		{ID: "memory", Label: "Memory", Dir: "/tmp/m"},
	}

	tests := []struct {
		name  string
		reply string
		want  int
	}{
		{"bracketed_source", "[knowledge]/Design\n[memory]/Decisions", 2},
		{"plain_source_slash", "knowledge/Design", 1},
		{"wikilink_format", "[knowledge]/[[Design]]", 1},
		{"numbered_list", "1. [knowledge]/Design\n2. [memory]/Conventions", 2},
		{"bullet_list", "- [knowledge]/API\n* [memory]/Decisions", 2},
		{"done_skipped", "DONE: answer", 0},
		{"empty_lines", "\n\n[knowledge]/Design\n\n", 1},
		{"with_md_suffix", "[knowledge]/Design.md", 1},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := parseMultiPageList(context.Background(), tt.reply, sources)
			if len(got) != tt.want {
				t.Errorf("parseMultiPageList() = %d results, want %d (for %q)", len(got), tt.want, tt.reply)
			}
		})
	}
}

// A bare page name with no [source-id] prefix is resolved by asking each wiki's INDEX whether it
// holds that page. It used to be resolved by an os.ReadFile of `<dir>/Design.md`, so this fixture
// moved from writing a page to building one.
func TestParseMultiPageList_FallbackSearch(t *testing.T) {
	t.Parallel()
	dir := indexedWiki(t, []WikiChunk{{
		Slug: "Design", Title: "Design", Body: "Content.", DocType: "document",
		WordCount: 1, ClusterID: -1,
	}})

	sources := []WikiSource{
		{ID: "knowledge", Label: "Knowledge", Dir: dir},
	}

	got := parseMultiPageList(context.Background(), "Design", sources)
	if len(got) != 1 {
		t.Errorf("expected fallback search to find Design, got %d results", len(got))
	}
}

func TestSearchMultiWiki_MaxTurns(t *testing.T) {
	t.Parallel()
	dir1, dir2 := setupMultiWikiDirs(t)
	client := &mockAIClient{responses: []string{
		"[knowledge]/Design",
		"[memory]/Decisions",
		"[knowledge]/API",
	}}

	result, err := SearchMultiWiki(context.Background(), client, "overview", MultiWikiSearchConfig{
		Sources: []WikiSource{
			{ID: "knowledge", Label: "Knowledge", Dir: dir1},
			{ID: "memory", Label: "Memory", Dir: dir2},
		},
		MaxTurns: 3,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.Turns < 3 {
		t.Errorf("turns = %d, expected to reach max turns", result.Turns)
	}
}
