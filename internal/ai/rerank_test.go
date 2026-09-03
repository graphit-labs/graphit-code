package ai

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

type stubScorer struct {
	calls int
	fail  error
}

func (s *stubScorer) Name() string { return "stub" }

func (s *stubScorer) Score(_ context.Context, query string, candidates []string) ([]float64, error) {
	s.calls++
	if s.fail != nil {
		return nil, s.fail
	}
	out := make([]float64, len(candidates))
	for i, c := range candidates {
		for _, tok := range strings.Fields(strings.ToLower(query)) {
			if strings.Contains(strings.ToLower(c), tok) {
				out[i]++
			}
		}
	}
	return out, nil
}

// The adapter reorders and returns the SAME SET. A shortened list served as a ranked one would
// silently drop results the caller asked for.
func TestRerankAdapterReordersWithoutDroppingAnything(t *testing.T) {
	in := []RerankHit{
		{Text: "closeDatabase — closes the database connection", Index: 0},
		{Text: "retryPolicy — retry policy with exponential backoff", Index: 1},
		{Text: "parseConfig — parses the configuration file", Index: 2},
	}
	out, err := RerankAdapter{Scorer: &stubScorer{}}.Rank(context.Background(), "retry backoff", in)
	if err != nil {
		t.Fatalf("Rank: %v", err)
	}
	if len(out) != len(in) {
		t.Fatalf("Rank returned %d hits for %d candidates", len(out), len(in))
	}
	if !strings.HasPrefix(out[0].Text, "retryPolicy") {
		t.Errorf("top hit is %q, want retryPolicy", out[0].Text)
	}
	seen := map[int]int{}
	for _, h := range out {
		seen[h.Index]++
	}
	for i := range in {
		if seen[i] != 1 {
			t.Errorf("input %d appears %d times in the output", i, seen[i])
		}
	}
}

// A tie must break the same way every run. A ranker that flaps between runs reads as a bug in
// whatever consumes it.
func TestRerankAdapterIsDeterministicOnTies(t *testing.T) {
	in := []RerankHit{
		{Text: "alpha nothing matches", Index: 0},
		{Text: "beta nothing matches", Index: 1},
		{Text: "gamma nothing matches", Index: 2},
	}
	var first []int
	for run := 0; run < 5; run++ {
		out, err := RerankAdapter{Scorer: &stubScorer{}}.Rank(context.Background(), "zzz", in)
		if err != nil {
			t.Fatal(err)
		}
		order := make([]int, len(out))
		for i, h := range out {
			order[i] = h.Index
		}
		if run == 0 {
			first = order
			continue
		}
		for i := range order {
			if order[i] != first[i] {
				t.Fatalf("run %d ordered ties differently: %v then %v", run, first, order)
			}
		}
	}
}

// A scorer that returns the wrong number of scores has broken its contract, and the caller must
// hear about it rather than receive a silently mis-ranked list.
func TestRerankAdapterRefusesAScoreCountMismatch(t *testing.T) {
	bad := scorerFunc(func(context.Context, string, []string) ([]float64, error) {
		return []float64{1}, nil
	})
	in := []RerankHit{{Text: "a", Index: 0}, {Text: "b", Index: 1}}
	if _, err := (RerankAdapter{Scorer: bad}).Rank(context.Background(), "q", in); err == nil {
		t.Error("a mismatched score count was accepted")
	}
}

// A failing scorer returns the input order alongside the error, so the caller can degrade.
func TestRerankAdapterDegradesOnScorerFailure(t *testing.T) {
	in := []RerankHit{{Text: "first", Index: 0}, {Text: "second", Index: 1}}
	out, err := RerankAdapter{Scorer: &stubScorer{fail: errors.New("model gone")}}.
		Rank(context.Background(), "q", in)
	if err == nil {
		t.Error("the failure was not reported")
	}
	if len(out) != 2 || out[0].Index != 0 {
		t.Errorf("the input order was not preserved on failure: %v", out)
	}
}

// THE GRAM BAG MUST NOT REACH THE MODEL. It exists so BM25 can match a truncation; to a
// transformer trained on language it is hundreds of meaningless three-letter tokens that crowd out
// the sentence and eat the sequence budget. Feeding the indexed column straight in is the obvious
// thing and it is wrong.
func TestBuildRerankTextCarriesLanguageAndNotGrams(t *testing.T) {
	text := BuildRerankText(
		"validateSchema", "validate Schema",
		"Validates the database schema before migration.", "Function", "schema.go")

	for _, want := range []string{"validateSchema", "validate Schema", "Function",
		"Validates the database schema", "schema.go"} {
		if !strings.Contains(text, want) {
			t.Errorf("the reranker text is missing %q: %s", want, text)
		}
	}
	for _, gram := range []string{"val ali lid", "sch che hem"} {
		if strings.Contains(text, gram) {
			t.Errorf("a gram bag reached the reranker text: %s", text)
		}
	}
}

// An identical split adds nothing and is left out, so the sequence budget is not spent twice on
// the same word.
func TestBuildRerankTextSkipsARedundantSplit(t *testing.T) {
	text := BuildRerankText("Config", "Config", "Configuration for the parser.", "Struct", "config.go")
	if strings.Count(text, "Config —") > 1 {
		t.Errorf("the identifier was repeated when the split was identical: %s", text)
	}
}

// THE MODEL IS NEVER FETCHED UNLESS SOMEBODY ENABLED RERANKING. Present() answers from disk and
// must not create the directory, let alone reach the network: a user who never turns reranking on
// must not pay 1.04 GiB, at setup or ever.
func TestRerankModelIsNotFetchedOrCreatedByAsking(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)

	mgr, err := NewRerankModelManager()
	if err != nil {
		t.Fatalf("NewRerankModelManager: %v", err)
	}
	if mgr.Present() {
		t.Error("Present() reported a model on a fresh HOME")
	}
	if _, err := os.Stat(mgr.CacheDir()); !os.IsNotExist(err) {
		t.Errorf("asking about the model created %s — constructing a manager must touch nothing",
			mgr.CacheDir())
	}
}

// IfPresent returns (nil, nil) when the model is absent: "no reranking", not an error and not a
// download.
func TestNewCrossEncoderIfPresentDoesNotDownload(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)

	r, err := NewCrossEncoderRerankerIfPresent()
	if err != nil {
		t.Fatalf("IfPresent returned an error for an absent model: %v", err)
	}
	if r != nil {
		t.Error("IfPresent produced a reranker with no model on disk")
	}
}

// Present() is a size check, so a truncated download or an HTML error page saved under the model's
// name is not mistaken for a model.
func TestRerankPresentRejectsATruncatedBundle(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)

	mgr, err := NewRerankModelManager()
	if err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(mgr.CacheDir(), 0o755); err != nil {
		t.Fatal(err)
	}
	for _, name := range []string{modelFileName, tokenizerFileName} {
		if err := os.WriteFile(filepath.Join(mgr.CacheDir(), name),
			[]byte("<html>404</html>"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	if mgr.Present() {
		t.Error("a 16-byte error page was accepted as the model bundle")
	}
}

type scorerFunc func(context.Context, string, []string) ([]float64, error)

func (f scorerFunc) Name() string { return "func" }
func (f scorerFunc) Score(ctx context.Context, q string, c []string) ([]float64, error) {
	return f(ctx, q, c)
}
