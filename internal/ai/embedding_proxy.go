package ai

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"net"
	"path/filepath"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

const (
	proxyModelTag = "proxy→daemon"
)

type proxyEmbeddingClient struct {
	sockFile  string
	modelName string
}

func newProxyEmbeddingClient() *proxyEmbeddingClient {
	sockFile := filepath.Join(brand.GlobalDir(), "daemon", "embed.sock")

	conn, err := net.Dial("unix", sockFile)
	if err != nil {
		return nil
	}
	_ = conn.Close()

	return &proxyEmbeddingClient{
		sockFile:  sockFile,
		modelName: "daemon-embedder (" + proxyModelTag + ")",
	}
}

func (c *proxyEmbeddingClient) ModelName() string {
	return c.modelName
}

// Dimensions reports the width of whatever ai.embedding.provider currently configures.
//
// It does not ask the daemon over the socket — ModelName() does not either, see modelName
// above — it reads the same config file the daemon reads. Both processes see the same
// answer because it is the same file on disk, and a config-only read is far cheaper than a
// round trip for something that changes only when the operator edits config.
func (c *proxyEmbeddingClient) Dimensions() int {
	return resolveActiveEmbeddingDimensions()
}

func (c *proxyEmbeddingClient) Embed(ctx context.Context, text string) ([]float32, error) {
	vecs, err := c.EmbedBatch(ctx, []string{text})
	if err != nil {
		return nil, err
	}
	if len(vecs) == 0 {
		return nil, fmt.Errorf("empty embedding response from daemon")
	}
	return vecs[0], nil
}

type embedRequest struct {
	Texts []string `json:"texts"`
	Query string   `json:"query"`
}

type embedResponse struct {
	Vectors [][]float32 `json:"vectors"`
	Error   string      `json:"error,omitempty"`
}

func (c *proxyEmbeddingClient) EmbedBatch(ctx context.Context, texts []string) ([][]float32, error) {
	if len(texts) == 0 {
		return nil, nil
	}

	reqBytes, err := json.Marshal(embedRequest{Texts: texts})
	if err != nil {
		return nil, err
	}

	return c.doRequest(reqBytes)
}

func (c *proxyEmbeddingClient) EmbedQuery(ctx context.Context, query string) ([]float32, error) {
	reqBytes, err := json.Marshal(embedRequest{Query: query})
	if err != nil {
		return nil, err
	}

	vecs, err := c.doRequest(reqBytes)
	if err != nil {
		return nil, err
	}
	if len(vecs) == 0 {
		return nil, fmt.Errorf("empty query embedding response from daemon")
	}
	return vecs[0], nil
}

func (c *proxyEmbeddingClient) doRequest(reqBytes []byte) ([][]float32, error) {
	var dialer net.Dialer
	conn, err := dialer.Dial("unix", c.sockFile)
	if err != nil {
		return nil, fmt.Errorf("daemon socket dial: %w", err)
	}
	defer conn.Close()

	if _, err := conn.Write(append(reqBytes, '\n')); err != nil {
		return nil, fmt.Errorf("daemon socket write: %w", err)
	}

	reader := bufio.NewReader(conn)
	respLine, err := reader.ReadBytes('\n')
	if err != nil {
		return nil, fmt.Errorf("daemon socket read: %w", err)
	}

	var resp embedResponse
	if err := json.Unmarshal(respLine, &resp); err != nil {
		return nil, fmt.Errorf("daemon socket decode: %w", err)
	}

	if resp.Error != "" {
		return nil, fmt.Errorf("daemon embed error: %s", resp.Error)
	}

	return resp.Vectors, nil
}
