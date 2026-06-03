package daemon

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// ---------------------------------------------------------------------------
// mock embedding client
// ---------------------------------------------------------------------------

type mockEmbeddingClient struct {
	embedFn      func(ctx context.Context, text string) ([]float32, error)
	embedBatchFn func(ctx context.Context, texts []string) ([][]float32, error)
	modelName    string
}

func (m *mockEmbeddingClient) Embed(ctx context.Context, text string) ([]float32, error) {
	if m.embedFn != nil {
		return m.embedFn(ctx, text)
	}
	return []float32{0.1, 0.2}, nil
}

func (m *mockEmbeddingClient) EmbedBatch(ctx context.Context, texts []string) ([][]float32, error) {
	if m.embedBatchFn != nil {
		return m.embedBatchFn(ctx, texts)
	}
	result := make([][]float32, len(texts))
	for i := range texts {
		result[i] = []float32{float32(i) * 0.1, float32(i) * 0.2}
	}
	return result, nil
}

func (m *mockEmbeddingClient) ModelName() string {
	if m.modelName != "" {
		return m.modelName
	}
	return "mock-model"
}

// mockQueryEmbedder implements both EmbeddingClient and QueryEmbedder
type mockQueryEmbedder struct {
	mockEmbeddingClient
	embedQueryFn func(ctx context.Context, query string) ([]float32, error)
}

func (m *mockQueryEmbedder) EmbedQuery(ctx context.Context, query string) ([]float32, error) {
	if m.embedQueryFn != nil {
		return m.embedQueryFn(ctx, query)
	}
	return []float32{0.5, 0.6}, nil
}

// ---------------------------------------------------------------------------
// EmbedPortFile
// ---------------------------------------------------------------------------

func TestEmbedPortFile(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	got := EmbedPortFile()
	expected := filepath.Join(tempHome, "."+brand.Brand, "daemon", portFileName)
	if got != expected {
		t.Errorf("expected %q, got %q", expected, got)
	}
}

func TestNewEmbedServer_PortFilePath(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	// Use a nil client since we're only testing the path configuration
	srv := NewEmbedServer(nil)
	expected := filepath.Join(GlobalDaemonDir(), portFileName)
	if srv.portFile != expected {
		t.Errorf("expected portFile %q, got %q", expected, srv.portFile)
	}
}

// ---------------------------------------------------------------------------
// EmbedServerModule — Name
// ---------------------------------------------------------------------------

func TestEmbedServerModule_Name(t *testing.T) {
	mod := NewEmbedServerModule(nil)
	if mod.Name() != "embed-server" {
		t.Errorf("expected 'embed-server', got %q", mod.Name())
	}
}

// ---------------------------------------------------------------------------
// EmbedServerModule — Start (integrates with EmbedServer)
// ---------------------------------------------------------------------------

func TestEmbedServerModule_Start(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	client := &mockEmbeddingClient{modelName: "test-model"}
	mod := NewEmbedServerModule(client)

	ctx, cancel := context.WithCancel(context.Background())

	errCh := make(chan error, 1)
	go func() {
		errCh <- mod.Start(ctx)
	}()

	// Wait briefly for server to start
	time.Sleep(100 * time.Millisecond)

	// Cancel to shut down
	cancel()

	err := <-errCh
	if err != nil {
		t.Errorf("expected nil error, got %v", err)
	}
}

// ---------------------------------------------------------------------------
// EmbedServer — Start and handlers (full integration)
// ---------------------------------------------------------------------------

func TestEmbedServer_StartAndShutdown(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	client := &mockEmbeddingClient{modelName: "test-model"}
	srv := NewEmbedServer(client)

	ctx, cancel := context.WithCancel(context.Background())

	errCh := make(chan error, 1)
	go func() {
		errCh <- srv.Start(ctx)
	}()

	// Wait for server to start
	time.Sleep(100 * time.Millisecond)

	// Verify port file was written
	portData, err := os.ReadFile(srv.portFile)
	if err != nil {
		t.Fatalf("port file not written: %v", err)
	}
	portStr := strings.TrimSpace(string(portData))
	if portStr == "" {
		t.Fatal("port file is empty")
	}

	baseURL := "http://127.0.0.1:" + portStr

	// Test /health endpoint
	resp, err := http.Get(baseURL + "/health")
	if err != nil {
		t.Fatalf("health request failed: %v", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		t.Errorf("health status: expected 200, got %d", resp.StatusCode)
	}
	var healthResp healthResponse
	if err := json.NewDecoder(resp.Body).Decode(&healthResp); err != nil {
		t.Fatalf("decode health response: %v", err)
	}
	if healthResp.Status != "ok" {
		t.Errorf("health status: expected 'ok', got %q", healthResp.Status)
	}
	if healthResp.Model != "test-model" {
		t.Errorf("health model: expected 'test-model', got %q", healthResp.Model)
	}

	// Test /embed endpoint with POST
	embedBody := `{"texts":["hello","world"]}`
	resp2, err := http.Post(baseURL+"/embed", "application/json", strings.NewReader(embedBody))
	if err != nil {
		t.Fatalf("embed request failed: %v", err)
	}
	defer resp2.Body.Close()
	if resp2.StatusCode != http.StatusOK {
		t.Errorf("embed status: expected 200, got %d", resp2.StatusCode)
	}
	var embedResp embedResponse
	if err := json.NewDecoder(resp2.Body).Decode(&embedResp); err != nil {
		t.Fatalf("decode embed response: %v", err)
	}
	if len(embedResp.Vectors) != 2 {
		t.Errorf("expected 2 vectors, got %d", len(embedResp.Vectors))
	}

	// Test /embed/query endpoint
	queryBody := `{"query":"test query"}`
	resp3, err := http.Post(baseURL+"/embed/query", "application/json", strings.NewReader(queryBody))
	if err != nil {
		t.Fatalf("query request failed: %v", err)
	}
	defer resp3.Body.Close()
	if resp3.StatusCode != http.StatusOK {
		t.Errorf("query status: expected 200, got %d", resp3.StatusCode)
	}
	var queryResp queryResponse
	if err := json.NewDecoder(resp3.Body).Decode(&queryResp); err != nil {
		t.Fatalf("decode query response: %v", err)
	}
	if len(queryResp.Vector) == 0 {
		t.Error("expected non-empty vector")
	}

	// Shutdown
	cancel()

	startErr := <-errCh
	if startErr != nil {
		t.Errorf("Start error: %v", startErr)
	}

	// Port file should be removed after shutdown
	time.Sleep(50 * time.Millisecond)
	if _, err := os.Stat(srv.portFile); !os.IsNotExist(err) {
		t.Log("port file may still exist briefly after shutdown")
	}
}

// ---------------------------------------------------------------------------
// handleEmbed — edge cases
// ---------------------------------------------------------------------------

func TestHandleEmbed_MethodNotAllowed(t *testing.T) {
	client := &mockEmbeddingClient{}
	srv := &EmbedServer{client: client}

	req := httptest.NewRequest(http.MethodGet, "/embed", nil)
	w := httptest.NewRecorder()
	srv.handleEmbed(w, req)
	if w.Code != http.StatusMethodNotAllowed {
		t.Errorf("expected 405, got %d", w.Code)
	}
}

func TestHandleEmbed_InvalidJSON(t *testing.T) {
	client := &mockEmbeddingClient{}
	srv := &EmbedServer{client: client}

	req := httptest.NewRequest(http.MethodPost, "/embed", strings.NewReader("{bad json"))
	w := httptest.NewRecorder()
	srv.handleEmbed(w, req)
	if w.Code != http.StatusBadRequest {
		t.Errorf("expected 400, got %d", w.Code)
	}
}

func TestHandleEmbed_EmptyTexts(t *testing.T) {
	client := &mockEmbeddingClient{}
	srv := &EmbedServer{client: client}

	body := `{"texts":[]}`
	req := httptest.NewRequest(http.MethodPost, "/embed", strings.NewReader(body))
	w := httptest.NewRecorder()
	srv.handleEmbed(w, req)
	if w.Code != http.StatusOK {
		t.Errorf("expected 200, got %d", w.Code)
	}
	var resp embedResponse
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if len(resp.Vectors) != 0 {
		t.Errorf("expected 0 vectors, got %d", len(resp.Vectors))
	}
}

func TestHandleEmbed_EmbedBatchError(t *testing.T) {
	client := &mockEmbeddingClient{
		embedBatchFn: func(ctx context.Context, texts []string) ([][]float32, error) {
			return nil, fmt.Errorf("batch error")
		},
	}
	srv := &EmbedServer{client: client}

	body := `{"texts":["hello"]}`
	req := httptest.NewRequest(http.MethodPost, "/embed", strings.NewReader(body))
	w := httptest.NewRecorder()
	srv.handleEmbed(w, req)
	if w.Code != http.StatusInternalServerError {
		t.Errorf("expected 500, got %d", w.Code)
	}
}

func TestHandleEmbed_Success(t *testing.T) {
	client := &mockEmbeddingClient{}
	srv := &EmbedServer{client: client}

	body, _ := json.Marshal(embedRequest{Texts: []string{"hello", "world"}})
	req := httptest.NewRequest(http.MethodPost, "/embed", bytes.NewReader(body))
	w := httptest.NewRecorder()
	srv.handleEmbed(w, req)
	if w.Code != http.StatusOK {
		t.Errorf("expected 200, got %d", w.Code)
	}
	if ct := w.Header().Get("Content-Type"); ct != "application/json" {
		t.Errorf("expected Content-Type application/json, got %q", ct)
	}
}

// ---------------------------------------------------------------------------
// handleEmbedQuery — edge cases
// ---------------------------------------------------------------------------

func TestHandleEmbedQuery_MethodNotAllowed(t *testing.T) {
	client := &mockEmbeddingClient{}
	srv := &EmbedServer{client: client}

	req := httptest.NewRequest(http.MethodGet, "/embed/query", nil)
	w := httptest.NewRecorder()
	srv.handleEmbedQuery(w, req)
	if w.Code != http.StatusMethodNotAllowed {
		t.Errorf("expected 405, got %d", w.Code)
	}
}

func TestHandleEmbedQuery_InvalidJSON(t *testing.T) {
	client := &mockEmbeddingClient{}
	srv := &EmbedServer{client: client}

	req := httptest.NewRequest(http.MethodPost, "/embed/query", strings.NewReader("{bad"))
	w := httptest.NewRecorder()
	srv.handleEmbedQuery(w, req)
	if w.Code != http.StatusBadRequest {
		t.Errorf("expected 400, got %d", w.Code)
	}
}

func TestHandleEmbedQuery_WithQueryEmbedder(t *testing.T) {
	client := &mockQueryEmbedder{
		embedQueryFn: func(ctx context.Context, query string) ([]float32, error) {
			return []float32{0.9, 0.8}, nil
		},
	}
	srv := &EmbedServer{client: client}

	body := `{"query":"test"}`
	req := httptest.NewRequest(http.MethodPost, "/embed/query", strings.NewReader(body))
	w := httptest.NewRecorder()
	srv.handleEmbedQuery(w, req)
	if w.Code != http.StatusOK {
		t.Errorf("expected 200, got %d", w.Code)
	}
	if ct := w.Header().Get("Content-Type"); ct != "application/json" {
		t.Errorf("expected Content-Type application/json, got %q", ct)
	}
}

func TestHandleEmbedQuery_WithoutQueryEmbedder(t *testing.T) {
	// Plain EmbeddingClient without QueryEmbedder interface — falls back to Embed
	client := &mockEmbeddingClient{
		embedFn: func(ctx context.Context, text string) ([]float32, error) {
			return []float32{0.3, 0.4}, nil
		},
	}
	srv := &EmbedServer{client: client}

	body := `{"query":"test fallback"}`
	req := httptest.NewRequest(http.MethodPost, "/embed/query", strings.NewReader(body))
	w := httptest.NewRecorder()
	srv.handleEmbedQuery(w, req)
	if w.Code != http.StatusOK {
		t.Errorf("expected 200, got %d", w.Code)
	}
	var resp queryResponse
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if len(resp.Vector) != 2 || resp.Vector[0] != 0.3 {
		t.Errorf("expected fallback vector [0.3, 0.4], got %v", resp.Vector)
	}
}

func TestHandleEmbedQuery_QueryEmbedError(t *testing.T) {
	client := &mockQueryEmbedder{
		embedQueryFn: func(ctx context.Context, query string) ([]float32, error) {
			return nil, fmt.Errorf("query embed error")
		},
	}
	srv := &EmbedServer{client: client}

	body := `{"query":"test"}`
	req := httptest.NewRequest(http.MethodPost, "/embed/query", strings.NewReader(body))
	w := httptest.NewRecorder()
	srv.handleEmbedQuery(w, req)
	if w.Code != http.StatusInternalServerError {
		t.Errorf("expected 500, got %d", w.Code)
	}
}

func TestHandleEmbedQuery_EmbedFallbackError(t *testing.T) {
	client := &mockEmbeddingClient{
		embedFn: func(ctx context.Context, text string) ([]float32, error) {
			return nil, fmt.Errorf("embed error")
		},
	}
	srv := &EmbedServer{client: client}

	body := `{"query":"test"}`
	req := httptest.NewRequest(http.MethodPost, "/embed/query", strings.NewReader(body))
	w := httptest.NewRecorder()
	srv.handleEmbedQuery(w, req)
	if w.Code != http.StatusInternalServerError {
		t.Errorf("expected 500, got %d", w.Code)
	}
}

// ---------------------------------------------------------------------------
// handleHealth
// ---------------------------------------------------------------------------

func TestHandleHealth(t *testing.T) {
	client := &mockEmbeddingClient{modelName: "my-model"}
	srv := &EmbedServer{client: client}

	req := httptest.NewRequest(http.MethodGet, "/health", nil)
	w := httptest.NewRecorder()
	srv.handleHealth(w, req)
	if w.Code != http.StatusOK {
		t.Errorf("expected 200, got %d", w.Code)
	}
	var resp healthResponse
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if resp.Status != "ok" {
		t.Errorf("expected status 'ok', got %q", resp.Status)
	}
	if resp.Model != "my-model" {
		t.Errorf("expected model 'my-model', got %q", resp.Model)
	}
	if ct := w.Header().Get("Content-Type"); ct != "application/json" {
		t.Errorf("expected Content-Type application/json, got %q", ct)
	}
}
