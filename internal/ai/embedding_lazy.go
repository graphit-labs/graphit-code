package ai

import (
	"context"
	"sync"
)

// LazyEmbeddingClient holds construction of the CONFIGURED embedder (ai.embedding.provider —
// local by default, otherwise whatever remote provider is set) until the first request, then
// memoises the result, including the failure, so a broken install or a missing API key is not
// retried on every call.
//
// This is what the daemon's EmbedServer wraps. It resolves through
// newDirectEmbeddingClientFromConfig — never NewEmbeddingClientFromConfig — because the daemon
// IS the thing the proxy client dials; going through the proxy path here would try to dial its
// own socket.
type LazyEmbeddingClient struct {
	once   sync.Once
	client EmbeddingClient
	err    error
}

func NewLazyEmbeddingClient() *LazyEmbeddingClient {
	return &LazyEmbeddingClient{}
}

func (l *LazyEmbeddingClient) init() error {
	l.once.Do(func() {
		l.client, l.err = newDirectEmbeddingClientFromConfig()
	})
	return l.err
}

func (l *LazyEmbeddingClient) ModelName() string {
	if l.client != nil {
		return l.client.ModelName()
	}
	return "embedder (lazy, not loaded)"
}

// Dimensions answers without forcing initialisation: a caller checking the vector schema
// width should not pay for a model load, or a network round trip to a remote provider, just
// to ask this question. Once the client IS loaded, its own answer is used instead — the
// authoritative one, in case the config-based guess and the constructed client ever disagree.
func (l *LazyEmbeddingClient) Dimensions() int {
	if l.client != nil {
		return l.client.Dimensions()
	}
	return resolveActiveEmbeddingDimensions()
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
	if qe, ok := l.client.(QueryEmbedder); ok {
		return qe.EmbedQuery(ctx, query)
	}
	return l.client.Embed(ctx, query)
}
