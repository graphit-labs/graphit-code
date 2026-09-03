package ast

import (
	"context"
	"fmt"

	"github.com/graphit-labs/graphit-code/internal/ai"
)

type SearchResult struct {
	Type           string
	Name           string
	Path           string
	Line           int
	Source         string
	Docstring      string
	IsDepend       bool
	SearchType     string
	RelevanceScore float64
	Distance       float64
}

type QueryService struct {
	db              GraphDB
	dbPath          string
	lacksFulltext   bool
	embeddingClient ai.EmbeddingClient
	searchIndex     *SearchIndex

	searchUnavailable error
}

func NewQueryService(db GraphDB) *QueryService {
	qs := &QueryService{
		db:            db,
		lacksFulltext: db.BackendType() != "neo4j",
	}
	if lb, ok := db.(*LadybugBackend); ok {
		qs.dbPath = lb.StoreDir()
		if si, err := OpenSearchIndex(context.Background(), lb.StoreDir()); err == nil {
			qs.searchIndex = si
		} else {
			qs.searchUnavailable = err
		}
	}
	return qs
}

func (q *QueryService) Close() {
	if q.searchIndex != nil {
		_ = q.searchIndex.Close()
		q.searchIndex = nil
	}
}

func (q *QueryService) SetEmbeddingClient(client ai.EmbeddingClient) {
	q.embeddingClient = client
}

func fitVectorWidth(vec []float32, dim int) []float32 {
	if len(vec) == dim {
		return vec
	}
	if len(vec) > dim {
		return vec[:dim]
	}
	padded := make([]float32, dim)
	copy(padded, vec)
	return padded
}

func (q *QueryService) FullTextSearch(ctx context.Context, query string, topK int) ([]SearchResult, error) {
	if q.searchIndex == nil {
		return nil, q.searchUnavailable
	}
	return q.searchIndex.Search(ctx, query, topK)
}

func (q *QueryService) SemanticSearch(ctx context.Context, query string, topK int, cluster string) ([]SearchResult, error) {
	if q.searchIndex == nil {
		return nil, q.searchUnavailable
	}
	if q.embeddingClient == nil {
		return nil, nil
	}

	var vec []float32
	var err error
	if qe, ok := q.embeddingClient.(ai.QueryEmbedder); ok {
		vec, err = qe.EmbedQuery(ctx, query)
	} else {
		vec, err = q.embeddingClient.Embed(ctx, query)
	}
	if err != nil {
		return nil, fmt.Errorf("embed query: %w", err)
	}

	if dim := q.embeddingClient.Dimensions(); len(vec) != dim {
		vec = fitVectorWidth(vec, dim)
	}

	results, err := q.searchIndex.SemanticSearch(ctx, vec, topK)
	if err != nil {
		return nil, err
	}

	if topK > 0 && len(results) > topK {
		results = results[:topK]
	}

	return results, nil
}

func (q *QueryService) HybridSearch(ctx context.Context, query string, topK int) ([]SearchResult, error) {
	if q.searchIndex == nil {
		return nil, q.searchUnavailable
	}

	var queryVec []float32
	if q.embeddingClient != nil {
		var err error
		if qe, ok := q.embeddingClient.(ai.QueryEmbedder); ok {
			queryVec, err = qe.EmbedQuery(ctx, query)
		} else {
			queryVec, err = q.embeddingClient.Embed(ctx, query)
		}
		if err != nil {
			queryVec = nil
		}

		if dim := q.embeddingClient.Dimensions(); queryVec != nil && len(queryVec) != dim {
			queryVec = fitVectorWidth(queryVec, dim)
		}
	}

	return q.searchIndex.HybridSearch(ctx, query, queryVec, topK)
}

type AIClient interface {
	Complete(ctx context.Context, systemPrompt, userPrompt string) (string, error)
}

type AICypherRequest struct {
	UserQuery  string
	RepoPath   string
	MaxResults int
	Backend    string
}

type AICypherResponse struct {
	Cypher            string
	Keywords          []string
	PreSearchEntities []string
	Result            *QueryResult
	Error             string
}

func GenerateAICypher(ctx context.Context, db GraphDB, aiClient AIClient, req AICypherRequest) (*AICypherResponse, error) {

	gen := newCypherGenerator(db, aiClient)
	return gen.generate(ctx, req)
}

type cypherGen struct {
	db GraphDB
	ai AIClient
}

func newCypherGenerator(db GraphDB, aiClient AIClient) *cypherGen {
	return &cypherGen{db: db, ai: aiClient}
}

func (g *cypherGen) generate(ctx context.Context, req AICypherRequest) (*AICypherResponse, error) {

	if generateFunc == nil {
		return nil, fmt.Errorf("AI Cypher generator not registered — this is a bug")
	}
	return generateFunc(ctx, g.db, g.ai, req)
}

type GenerateAICypherFunc func(ctx context.Context, db GraphDB, ai AIClient, req AICypherRequest) (*AICypherResponse, error)

var generateFunc GenerateAICypherFunc

func RegisterAICypherGenerator(fn GenerateAICypherFunc) {
	generateFunc = fn
}
