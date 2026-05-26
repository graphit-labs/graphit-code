package ai

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

const (
	proxyModelTag = "proxy→daemon"
)

type proxyEmbeddingClient struct {
	baseURL    string
	httpClient *http.Client
	modelName  string
}

func newProxyEmbeddingClient() *proxyEmbeddingClient {
	port, err := readDaemonEmbedPort()
	if err != nil {
		return nil
	}

	client := &proxyEmbeddingClient{
		baseURL:    fmt.Sprintf("http://127.0.0.1:%d", port),
		httpClient: &http.Client{},
	}

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	req, err := http.NewRequestWithContext(ctx, "GET", client.baseURL+"/health", nil)
	if err != nil {
		return nil
	}

	resp, err := client.httpClient.Do(req)
	if err != nil {
		return nil
	}
	defer func() { _ = resp.Body.Close() }()

	if resp.StatusCode != http.StatusOK {
		return nil
	}

	var health struct {
		Status string `json:"status"`
		Model  string `json:"model"`
	}
	if json.NewDecoder(resp.Body).Decode(&health) != nil {
		return nil
	}

	client.modelName = health.Model + " (" + proxyModelTag + ")"
	return client
}

func (c *proxyEmbeddingClient) ModelName() string {
	return c.modelName
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

func (c *proxyEmbeddingClient) EmbedBatch(ctx context.Context, texts []string) ([][]float32, error) {
	if len(texts) == 0 {
		return nil, nil
	}

	body, err := json.Marshal(struct {
		Texts []string `json:"texts"`
	}{Texts: texts})
	if err != nil {
		return nil, err
	}

	req, err := http.NewRequestWithContext(ctx, "POST", c.baseURL+"/embed", bytes.NewReader(body))
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/json")

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("daemon embed request: %w", err)
	}
	defer func() { _ = resp.Body.Close() }()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("daemon embed: status %d", resp.StatusCode)
	}

	var result struct {
		Vectors [][]float32 `json:"vectors"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, fmt.Errorf("daemon embed decode: %w", err)
	}
	return result.Vectors, nil
}

func (c *proxyEmbeddingClient) EmbedQuery(ctx context.Context, query string) ([]float32, error) {
	body, err := json.Marshal(struct {
		Query string `json:"query"`
	}{Query: query})
	if err != nil {
		return nil, err
	}

	req, err := http.NewRequestWithContext(ctx, "POST", c.baseURL+"/embed/query", bytes.NewReader(body))
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/json")

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("daemon embed query: %w", err)
	}
	defer func() { _ = resp.Body.Close() }()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("daemon embed query: status %d", resp.StatusCode)
	}

	var result struct {
		Vector []float32 `json:"vector"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, fmt.Errorf("daemon embed query decode: %w", err)
	}
	return result.Vector, nil
}

func readDaemonEmbedPort() (int, error) {
	portFile := filepath.Join(brand.GlobalDir(), "daemon", "embed.port")
	data, err := os.ReadFile(portFile)
	if err != nil {
		return 0, err
	}
	port, err := strconv.Atoi(strings.TrimSpace(string(data)))
	if err != nil {
		return 0, fmt.Errorf("invalid port in %s: %w", portFile, err)
	}
	return port, nil
}
