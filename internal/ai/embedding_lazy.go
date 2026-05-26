package ai

import (
	"context"
	"sync"
)

type LazyEmbeddingClient struct {
	once   sync.Once
	client *localEmbeddingClient
	err    error
}

func NewLazyEmbeddingClient() *LazyEmbeddingClient {
	return &LazyEmbeddingClient{}
}

func (l *LazyEmbeddingClient) init() error {
	l.once.Do(func() {
		l.client, l.err = NewLocalEmbeddingClient()
	})
	return l.err
}

func (l *LazyEmbeddingClient) ModelName() string {
	if l.client != nil {
		return l.client.ModelName()
	}
	return localModelName + " (lazy, not loaded)"
}

func (l *LazyEmbeddingClient) Embed(ctx context.Context, text string) ([]float32, error) {
	if err := l.init(); err != nil {
		return nil, err
	}
	return l.client.Embed(ctx, text)
}

func (l *LazyEmbeddingClient) EmbedBatch(ctx context.Context, texts []string) ([][]float32, error) {
	if err := l.init(); err != nil {
		return nil, err
	}
	return l.client.EmbedBatch(ctx, texts)
}

func (l *LazyEmbeddingClient) EmbedQuery(ctx context.Context, query string) ([]float32, error) {
	if err := l.init(); err != nil {
		return nil, err
	}
	return l.client.EmbedQuery(ctx, query)
}
