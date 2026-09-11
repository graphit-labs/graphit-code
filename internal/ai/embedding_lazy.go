package ai

import (
	"context"
	"fmt"
	"sync"
)

// LazyEmbeddingClient holds construction of the active provider's embedder (local by default)
// until the first request, then
// memoises the result, including the failure, so a broken install or a missing API key is not
// retried on every call.
//
// This is what the daemon's EmbedServer wraps. It resolves through
// newDirectEmbeddingClientFromConfig — never NewEmbeddingClientFromConfig — because the daemon
// IS the thing the proxy client dials; going through the proxy path here would try to dial its
// own socket.
type LazyEmbeddingClient struct {
	mu     sync.Mutex
	key    string
	client EmbeddingClient
	err    error
}

func NewLazyEmbeddingClient() *LazyEmbeddingClient {
	return &LazyEmbeddingClient{}
}

func (l *LazyEmbeddingClient) init() error {
	key := activeEmbeddingConfigurationKey()
	l.mu.Lock()
	defer l.mu.Unlock()
	if l.key == key && (l.client != nil || l.err != nil) {
		return l.err
	}
	l.client, l.err = newDirectEmbeddingClientFromConfig()
	l.key = key
	return l.err
}

func (l *LazyEmbeddingClient) ModelName() string {
	l.mu.Lock()
	defer l.mu.Unlock()
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
	l.mu.Lock()
	defer l.mu.Unlock()
	if l.client != nil {
		return l.client.Dimensions()
	}
	return resolveActiveEmbeddingDimensions()
}

func activeEmbeddingConfigurationKey() string {
	provider, model, dimensions := ConfiguredEmbeddingIdentity()
	execution, executionErr := configuredONNXExecution(ModelTaskEmbedding)
	identity := provider + "\x00" + model + "\x00" + fmt.Sprint(dimensions) + "\x00" + fmt.Sprint(execution) + "\x00" + fmt.Sprint(executionErr)
	if snapshot, ok := activeAuthSnapshot(); ok {
		return snapshot.Provider.Name + "\x00" + fmt.Sprint(snapshot.Provider.Revision) + "\x00" + snapshot.Profile.Name + "\x00" + identity
	}
	return "local-default\x00" + identity
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
