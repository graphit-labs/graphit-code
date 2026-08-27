package daemon

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"net"
	"os"
	"path/filepath"

	"github.com/graphit-labs/graphit-code/internal/ai"
)

const sockFileName = "embed.sock"

type EmbedServer struct {
	client   ai.EmbeddingClient
	listener net.Listener
	sockFile string
}

func NewEmbedServer(client ai.EmbeddingClient) *EmbedServer {
	return &EmbedServer{
		client:   client,
		sockFile: filepath.Join(GlobalDaemonDir(), sockFileName),
	}
}

type embedRequest struct {
	Texts []string `json:"texts"`
	Query string   `json:"query"`
}

type embedResponse struct {
	Vectors [][]float32 `json:"vectors"`
	Error   string      `json:"error,omitempty"`
}

func (s *EmbedServer) Start(ctx context.Context) error {
	if err := os.MkdirAll(filepath.Dir(s.sockFile), 0o755); err != nil {
		return fmt.Errorf("embed server: creating dir: %w", err)
	}

	_ = os.Remove(s.sockFile)

	var err error
	s.listener, err = net.Listen("unix", s.sockFile)
	if err != nil {
		return fmt.Errorf("embed server listen: %w", err)
	}

	go func() {
		<-ctx.Done()
		_ = s.listener.Close()
		_ = os.Remove(s.sockFile)
	}()

	for {
		conn, err := s.listener.Accept()
		if err != nil {
			select {
			case <-ctx.Done():
				return nil
			default:
				return fmt.Errorf("embed server accept: %w", err)
			}
		}

		go s.handleConnection(ctx, conn)
	}
}

func (s *EmbedServer) handleConnection(ctx context.Context, conn net.Conn) {
	defer conn.Close()
	reader := bufio.NewReader(conn)

	for {
		line, err := reader.ReadBytes('\n')
		if err != nil {
			return // Connection closed
		}

		var req embedRequest
		if err := json.Unmarshal(line, &req); err != nil {
			s.sendError(conn, fmt.Sprintf("invalid json: %v", err))
			continue
		}

		var vectors [][]float32
		if len(req.Texts) > 0 {
			vectors, err = s.client.EmbedBatch(ctx, req.Texts)
		} else if req.Query != "" {
			var vec []float32
			if qe, ok := s.client.(ai.QueryEmbedder); ok {
				vec, err = qe.EmbedQuery(ctx, req.Query)
			} else {
				vec, err = s.client.Embed(ctx, req.Query)
			}
			vectors = [][]float32{vec}
		}

		if err != nil {
			s.sendError(conn, err.Error())
			continue
		}

		respBytes, _ := json.Marshal(embedResponse{Vectors: vectors})
		_, _ = conn.Write(append(respBytes, '\n'))
	}
}

func (s *EmbedServer) sendError(conn net.Conn, errMsg string) {
	respBytes, _ := json.Marshal(embedResponse{Error: errMsg})
	_, _ = conn.Write(append(respBytes, '\n'))
}
