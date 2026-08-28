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

	// searchUnavailable is why there is no index, when there is none. It is REPORTED rather
	// than swallowed — see NewQueryService.
	searchUnavailable error
}

func NewQueryService(db GraphDB) *QueryService {
	qs := &QueryService{
		db:            db,
		lacksFulltext: db.BackendType() != "neo4j",
	}
	if lb, ok := db.(*LadybugBackend); ok {
		qs.dbPath = lb.StoreDir()
		// A second store, and a second handle. Nothing here contends with the graph handle this
		// service already holds: Lance is multi-version, so this reader sees a consistent
		// snapshot while the daemon writes the same directory — which is what makes an in-place
		// update safe to read through.
		//
		// A failure is not fatal: a project with no index yet, or one whose index is mid-rebuild,
		// still answers structural queries. searchIndex stays nil and the search entry points
		// return nothing rather than an error.
		if si, err := OpenSearchIndex(context.Background(), lb.StoreDir()); err == nil {
			qs.searchIndex = si
		} else {
			// Kept, so the search entry points can tell "no index yet" from "this binary
			// cannot search at all". Returning no results for the second is the trap the
			// fts5 guard file existed to close: a build without the engine answered every
			// query with silence, and silence is indistinguishable from a correct empty
			// result.
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

	if len(vec) < ai.EmbeddingDimensions {
		padded := make([]float32, ai.EmbeddingDimensions)
		copy(padded, vec)
		vec = padded
	} else if len(vec) > ai.EmbeddingDimensions {
		vec = vec[:ai.EmbeddingDimensions]
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

		if queryVec != nil {
			if len(queryVec) < ai.EmbeddingDimensions {
				padded := make([]float32, ai.EmbeddingDimensions)
				copy(padded, queryVec)
				queryVec = padded
			} else if len(queryVec) > ai.EmbeddingDimensions {
				queryVec = queryVec[:ai.EmbeddingDimensions]
			}
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
