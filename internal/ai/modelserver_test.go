package ai

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

type fixedScorer struct{}

func (fixedScorer) Name() string { return "fixed-scorer" }

func (fixedScorer) Score(_ context.Context, _ string, candidates []string) ([]float64, error) {
	scores := make([]float64, len(candidates))
	for i, c := range candidates {
		scores[i] = float64(len(c))
	}
	return scores, nil
}

type fixedEmbedder struct{}

func (fixedEmbedder) Embed(context.Context, string) ([]float32, error) { return []float32{1, 2}, nil }
func (fixedEmbedder) EmbedBatch(_ context.Context, texts []string) ([][]float32, error) {
	out := make([][]float32, len(texts))
	for i := range texts {
		out[i] = []float32{float32(i), 1}
	}
	return out, nil
}
func (fixedEmbedder) ModelName() string { return "fixed-embedder" }
func (fixedEmbedder) Dimensions() int   { return 2 }

// serveForTest starts a model server on the task's real socket path under a private
// global directory, the way the daemon's child process would, and waits for it.
func serveForTest(t *testing.T, task ModelTask, handle modelHandler) {
	t.Helper()
	// Unix socket paths are length-limited; keep the global directory short.
	global, err := os.MkdirTemp("", "gms")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = os.RemoveAll(global) })
	t.Setenv(globalDirEnv(), global)
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan error, 1)
	go func() { done <- serveModelSocket(ctx, ModelSocketPath(task), handle) }()
	t.Cleanup(func() {
		cancel()
		if err := <-done; err != nil {
			t.Errorf("model server: %v", err)
		}
		if _, err := os.Stat(ModelSocketPath(task)); !errors.Is(err, os.ErrNotExist) {
			t.Errorf("socket left behind after shutdown: %v", err)
		}
	})
	if !dialModelServer(task, 5*time.Second) {
		t.Fatal("model server never accepted connections")
	}
}

func globalDirEnv() string { return "GRAPHIT_GLOBAL_DIR" }

func TestModelServerRerankProxyScoresInOrder(t *testing.T) {
	serveForTest(t, ModelTaskRerank, rerankHandler(fixedScorer{}))

	adapter, err := ConfiguredReranker(context.Background())
	if err != nil {
		t.Fatalf("configuredRerankScorer: %v", err)
	}
	if _, ok := adapter.Scorer.(*proxyScorer); !ok {
		t.Fatalf("scorer = %T, want the model-server proxy while the server is up", adapter.Scorer)
	}
	scores, err := adapter.Scorer.Score(context.Background(), "q", []string{"a", "bbb", "cc"})
	if err != nil {
		t.Fatalf("Score: %v", err)
	}
	if len(scores) != 3 || scores[0] != 1 || scores[1] != 3 || scores[2] != 2 {
		t.Fatalf("scores = %v, want [1 3 2] in candidate order", scores)
	}
	if adapter.Name() != "fixed-scorer" {
		t.Fatalf("proxy name = %q, want the served model's name", adapter.Name())
	}
}

func TestModelServerEmbeddingProxyServesBatchesAndQueries(t *testing.T) {
	serveForTest(t, ModelTaskEmbedding, embeddingHandler(fixedEmbedder{}))

	client := newProxyEmbeddingClient()
	if client == nil {
		t.Fatal("no proxy client while the embedding server is up")
	}
	vecs, err := client.EmbedBatch(context.Background(), []string{"a", "b"})
	if err != nil || len(vecs) != 2 {
		t.Fatalf("EmbedBatch = %v, %v", vecs, err)
	}
	vec, err := client.EmbedQuery(context.Background(), "q")
	if err != nil || len(vec) != 2 {
		t.Fatalf("EmbedQuery = %v, %v", vec, err)
	}
}

func TestModelServerReportsScorerErrors(t *testing.T) {
	serveForTest(t, ModelTaskRerank, func(context.Context, []byte) any {
		return rerankResponse{Error: "model exploded"}
	})
	_, err := newProxyScorer().Score(context.Background(), "q", []string{"a"})
	if err == nil || !strings.Contains(err.Error(), "model exploded") {
		t.Fatalf("Score error = %v, want the server's error", err)
	}
}

// The daemon must never load a local model itself: without its model server, a local
// request fails instead of falling back to an in-process ONNX session.
func TestRequiredModelServerRefusesInProcessLocalModels(t *testing.T) {
	global, err := os.MkdirTemp("", "gms")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = os.RemoveAll(global) })
	t.Setenv(globalDirEnv(), global)
	SetModelServerRequired(true)
	t.Cleanup(func() { SetModelServerRequired(false) })
	restore := setModelServerWaitForTest(50 * time.Millisecond)
	t.Cleanup(restore)

	if _, err := NewEmbeddingClientFromConfig(); err == nil || !strings.Contains(err.Error(), "model server is not running") {
		t.Fatalf("NewEmbeddingClientFromConfig = %v, want the missing-server error", err)
	}
	if _, err := ConfiguredReranker(context.Background()); err == nil || !strings.Contains(err.Error(), "model server is not running") {
		t.Fatalf("configuredRerankScorer = %v, want the missing-server error", err)
	}
	if _, err := os.Stat(filepath.Join(global, "models")); err == nil {
		t.Fatal("a model directory appeared; the process tried to load a model itself")
	}
}
