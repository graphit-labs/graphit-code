package daemon

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"net"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

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

func (m *mockEmbeddingClient) Dimensions() int { return 2 }

func TestNewEmbedServer_SockFilePath(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	srv := NewEmbedServer(nil)
	expected := filepath.Join(GlobalDaemonDir(), sockFileName)
	if srv.sockFile != expected {
		t.Errorf("expected sockFile %q, got %q", expected, srv.sockFile)
	}
}

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

	time.Sleep(100 * time.Millisecond)

	if _, err := os.Stat(srv.sockFile); os.IsNotExist(err) {
		t.Fatalf("sock file not created")
	}

	conn, err := net.Dial("unix", srv.sockFile)
	if err != nil {
		t.Fatalf("dial failed: %v", err)
	}
	defer conn.Close()

	embedBody := `{"texts":["hello","world"]}` + "\n"
	_, err = conn.Write([]byte(embedBody))
	if err != nil {
		t.Fatalf("write failed: %v", err)
	}

	reader := bufio.NewReader(conn)
	respBytes, err := reader.ReadBytes('\n')
	if err != nil {
		t.Fatalf("read failed: %v", err)
	}

	var embedResp embedResponse
	if err := json.Unmarshal(respBytes, &embedResp); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if len(embedResp.Vectors) != 2 {
		t.Errorf("expected 2 vectors, got %d", len(embedResp.Vectors))
	}

	queryBody := `{"query":"test query"}` + "\n"
	_, err = conn.Write([]byte(queryBody))
	if err != nil {
		t.Fatalf("write failed: %v", err)
	}

	respBytes2, err := reader.ReadBytes('\n')
	if err != nil {
		t.Fatalf("read failed: %v", err)
	}

	var queryResp embedResponse
	if err := json.Unmarshal(respBytes2, &queryResp); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if len(queryResp.Vectors) != 1 {
		t.Errorf("expected 1 vector, got %d", len(queryResp.Vectors))
	}

	cancel()

	startErr := <-errCh
	if startErr != nil {
		t.Errorf("Start error: %v", startErr)
	}

	time.Sleep(50 * time.Millisecond)
	if _, err := os.Stat(srv.sockFile); !os.IsNotExist(err) {
		t.Log("sock file may still exist briefly after shutdown")
	}
}

func TestHandleConnection_InvalidJSON(t *testing.T) {
	client := &mockEmbeddingClient{}
	srv := &EmbedServer{client: client}

	serverConn, clientConn := net.Pipe()
	defer clientConn.Close()

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	go srv.handleConnection(ctx, serverConn)

	_, err := clientConn.Write([]byte("{bad\n"))
	if err != nil {
		t.Fatalf("write failed: %v", err)
	}

	reader := bufio.NewReader(clientConn)
	respBytes, err := reader.ReadBytes('\n')
	if err != nil {
		t.Fatalf("read failed: %v", err)
	}

	var resp embedResponse
	if err := json.Unmarshal(respBytes, &resp); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if !strings.Contains(resp.Error, "invalid json") {
		t.Errorf("expected invalid json error, got %q", resp.Error)
	}
}

func TestHandleConnection_EmbedError(t *testing.T) {
	client := &mockEmbeddingClient{
		embedBatchFn: func(ctx context.Context, texts []string) ([][]float32, error) {
			return nil, fmt.Errorf("embed error")
		},
	}
	srv := &EmbedServer{client: client}

	serverConn, clientConn := net.Pipe()
	defer clientConn.Close()

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	go srv.handleConnection(ctx, serverConn)

	_, err := clientConn.Write([]byte(`{"texts":["hello"]}` + "\n"))
	if err != nil {
		t.Fatalf("write failed: %v", err)
	}

	reader := bufio.NewReader(clientConn)
	respBytes, err := reader.ReadBytes('\n')
	if err != nil {
		t.Fatalf("read failed: %v", err)
	}

	var resp embedResponse
	if err := json.Unmarshal(respBytes, &resp); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if resp.Error != "embed error" {
		t.Errorf("expected 'embed error', got %q", resp.Error)
	}
}
