package ai

import (
	"context"
)

const EmbeddingDimensions = 768

type EmbeddingClient interface {
	Embed(ctx context.Context, text string) ([]float32, error)

	EmbedBatch(ctx context.Context, texts []string) ([][]float32, error)

	ModelName() string
}

type QueryEmbedder interface {
	EmbedQuery(ctx context.Context, query string) ([]float32, error)
}

func NewEmbeddingClientFromConfig() (EmbeddingClient, error) {

	if proxy := newProxyEmbeddingClient(); proxy != nil {
		return proxy, nil
	}

	return NewLocalEmbeddingClient()
}
