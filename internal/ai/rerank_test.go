package ai

import (
	"context"
	"errors"
	"math"
	"os"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
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
		"Validates the database schema before deployment.", "Function", "schema.go")

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

func TestLanceRerankerUsesReadableFieldsAndPreservesRows(t *testing.T) {
	scorer := &stubScorer{}
	ranker := lanceReranker{adapter: &RerankAdapter{Scorer: scorer}}
	hits := []lancestore.Hit{
		{Row: lancestore.Row{"name": "parseToken", "docstring": "unrelated"}, Score: 3},
		{Row: lancestore.Row{"name": "loadAccount", "docstring": "token account"}, Score: 1},
	}
	got, err := ranker.Rerank(context.Background(), "token account", hits)
	if err != nil {
		t.Fatal(err)
	}
	if got[0].Row["name"] != "loadAccount" || got[0].Score != 2 {
		t.Fatalf("reranked hits = %#v", got)
	}
	if len(got) != len(hits) || scorer.calls != 1 {
		t.Fatalf("len=%d calls=%d", len(got), scorer.calls)
	}
}

func TestLanceRerankTextSupportsWikiRows(t *testing.T) {
	text := lanceRerankText(lancestore.Row{"title": "Authentication", "summary": "OIDC and ACL", "body": "Broker details", "slug": "auth"})
	for _, want := range []string{"Authentication", "OIDC and ACL", "Broker details", "auth"} {
		if !strings.Contains(text, want) {
			t.Fatalf("rerank text %q does not contain %q", text, want)
		}
	}
}

// THE MODEL IS NEVER FETCHED UNLESS SOMEBODY ENABLED RERANKING. Present() answers from disk and
// must not create the directory, let alone reach the network: a user who never turns reranking on
// must not pay 1.04 GiB, at setup or ever.
func TestRerankModelIsNotFetchedOrCreatedByAsking(t *testing.T) {
	globalDir := t.TempDir()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), globalDir)

	model, err := LoadConfiguredModel(ModelTaskRerank)
	if err != nil {
		t.Fatalf("LoadConfiguredModel: %v", err)
	}
	if model.Present() {
		t.Error("Present() reported a model on a fresh HOME")
	}
	if _, err := os.Stat(model.Dir); !os.IsNotExist(err) {
		t.Errorf("asking about the model created %s — constructing a manager must touch nothing",
			model.Dir)
	}
}

// IfPresent returns (nil, nil) when the model is absent: "no reranking", not an error and not a
// download.
func TestNewCrossEncoderIfPresentDoesNotDownload(t *testing.T) {
	globalDir := t.TempDir()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), globalDir)

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
	globalDir := t.TempDir()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), globalDir)

	model, err := LoadConfiguredModel(ModelTaskRerank)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(model.Dir, 0o755); err != nil {
		t.Fatal(err)
	}
	for _, artifact := range model.Manifest.Artifacts {
		path, pathErr := model.ArtifactPath(artifact.Role)
		if pathErr != nil {
			t.Fatal(pathErr)
		}
		if err := os.WriteFile(path,
			[]byte("<html>404</html>"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	if model.Present() {
		t.Error("a 16-byte error page was accepted as the model bundle")
	}
}

func TestRerankScoreTransforms(t *testing.T) {
	positive := 1
	tests := []struct {
		name      string
		transform string
		class     *int
		logits    []float32
		want      float64
	}{
		{"auto binary", "auto", nil, []float32{-2, 3}, 3},
		{"none selected", "none", &positive, []float32{1, 4}, 4},
		{"sigmoid", "sigmoid", nil, []float32{0}, 0.5},
		{"softmax", "softmax", &positive, []float32{0, 0}, 0.5},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			reranker := CrossEncoderReranker{output: "scores", scoreTransform: tc.transform, positiveClass: tc.class}
			got, err := reranker.transformScore(tc.logits)
			if err != nil {
				t.Fatal(err)
			}
			if math.Abs(got-tc.want) > 1e-9 {
				t.Fatalf("score = %f, want %f", got, tc.want)
			}
		})
	}
	reranker := CrossEncoderReranker{output: "scores", scoreTransform: "none", positiveClass: func() *int { value := 2; return &value }()}
	if _, err := reranker.transformScore([]float32{1}); err == nil || !strings.Contains(err.Error(), "outside output width") {
		t.Fatalf("invalid positive class error = %v", err)
	}
}

type scorerFunc func(context.Context, string, []string) ([]float64, error)

func (f scorerFunc) Name() string { return "func" }
func (f scorerFunc) Score(ctx context.Context, q string, c []string) ([]float64, error) {
	return f(ctx, q, c)
}
