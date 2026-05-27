package daemon

import (
	"context"
	"encoding/json"
	"fmt"
	"net"
	"net/http"
	"os"
	"path/filepath"

	"github.com/graphit-labs/graphit-code/internal/ai"
)

const portFileName = "embed.port"

type EmbedServer struct {
	client   ai.EmbeddingClient
	listener net.Listener
	server   *http.Server
	portFile string
}

func NewEmbedServer(client ai.EmbeddingClient) *EmbedServer {
	return &EmbedServer{
		client:   client,
		portFile: filepath.Join(GlobalDaemonDir(), portFileName),
	}
}

func EmbedPortFile() string {
	return filepath.Join(GlobalDaemonDir(), portFileName)
}

func (s *EmbedServer) Start(ctx context.Context) error {
	mux := http.NewServeMux()
	mux.HandleFunc("/embed", s.handleEmbed)
	mux.HandleFunc("/embed/query", s.handleEmbedQuery)
	mux.HandleFunc("/health", s.handleHealth)

	var err error
	s.listener, err = net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		return fmt.Errorf("embed server listen: %w", err)
	}

	port := s.listener.Addr().(*net.TCPAddr).Port

	if err := os.MkdirAll(filepath.Dir(s.portFile), 0o755); err != nil {
		_ = s.listener.Close()
		return fmt.Errorf("embed server: creating dir: %w", err)
	}
	if err := os.WriteFile(s.portFile, []byte(fmt.Sprintf("%d\n", port)), 0o644); err != nil {
		_ = s.listener.Close()
		return fmt.Errorf("embed server: writing port file: %w", err)
	}

	s.server = &http.Server{
		Handler: mux,
	}

	go func() {
		<-ctx.Done()
		_ = s.server.Shutdown(context.Background())
		_ = os.Remove(s.portFile)
	}()

	err = s.server.Serve(s.listener)
	if err == http.ErrServerClosed {
		return nil
	}
	return err
}

type embedRequest struct {
	Texts []string `json:"texts"`
}

type embedResponse struct {
	Vectors [][]float32 `json:"vectors"`
}

type queryRequest struct {
	Query string `json:"query"`
}

type queryResponse struct {
	Vector []float32 `json:"vector"`
}

type healthResponse struct {
	Status string `json:"status"`
	Model  string `json:"model"`
}

func (s *EmbedServer) handleEmbed(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "POST only", http.StatusMethodNotAllowed)
		return
	}

	var req embedRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	if len(req.Texts) == 0 {
		_ = json.NewEncoder(w).Encode(embedResponse{Vectors: [][]float32{}})
		return
	}

	vectors, err := s.client.EmbedBatch(r.Context(), req.Texts)
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(embedResponse{Vectors: vectors})
}

func (s *EmbedServer) handleEmbedQuery(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "POST only", http.StatusMethodNotAllowed)
		return
	}

	var req queryRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	var vec []float32
	var err error
	if qe, ok := s.client.(ai.QueryEmbedder); ok {
		vec, err = qe.EmbedQuery(r.Context(), req.Query)
	} else {
		vec, err = s.client.Embed(r.Context(), req.Query)
	}
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(queryResponse{Vector: vec})
}

func (s *EmbedServer) handleHealth(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(healthResponse{
		Status: "ok",
		Model:  s.client.ModelName(),
	})
}

type EmbedServerModule struct {
	client ai.EmbeddingClient
}

func NewEmbedServerModule(client ai.EmbeddingClient) *EmbedServerModule {
	return &EmbedServerModule{client: client}
}

func (m *EmbedServerModule) Name() string { return "embed-server" }

func (m *EmbedServerModule) Start(ctx context.Context) error {
	server := NewEmbedServer(m.client)
	return server.Start(ctx)
}
