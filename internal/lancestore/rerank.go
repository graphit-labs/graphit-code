package lancestore

import (
	"context"
	"sort"
)

// Reranker reorders candidates by judging each against the query.
//
// It receives what the engine retrieved, in the engine's order, and returns its own order. An
// implementation may drop nothing: filtering is the query's job, not the reranker's, and a
// reranker that shortens the list makes Limit mean two different things.
type Reranker interface {
	// Rerank returns hits ordered by its own relevance judgement. It must return the same set it
	// was given.
	Rerank(ctx context.Context, query string, hits []Hit) ([]Hit, error)
	// Name identifies the implementation for logs and for the Mode field on a Hit.
	Name() string
}

// RerankConfig is how a caller turns the second stage on.
type RerankConfig struct {
	// Reranker is the implementation. Nil means the stage does not run, which is the default.
	Reranker Reranker

	// CandidateLimit is how many rows the engine is asked for before reranking. Zero means
	// DefaultCandidateLimit.
	//
	// It exists because reranking only helps if the right answer is IN the candidate set: a
	// cross-encoder cannot promote what retrieval never returned. So the first stage widens and
	// the second stage narrows.
	CandidateLimit int
}

// DefaultCandidateLimit is how many candidates the first stage returns when reranking is on.
//
// Fifty is the usual shape for this pattern — wide enough that recall is retrieval's problem and
// not the reranker's, small enough that per-query inference stays bounded.
const DefaultCandidateLimit = 50

func (rc RerankConfig) enabled() bool { return rc.Reranker != nil }

func (rc RerankConfig) candidates(limit int) int {
	if !rc.enabled() {
		return limit
	}
	n := rc.CandidateLimit
	if n <= 0 {
		n = DefaultCandidateLimit
	}
	if n < limit {
		// Asking for fewer candidates than the caller wants results would cap the answer below
		// its own Limit, which is never what a widening stage should do.
		n = limit
	}
	return n
}

func (rc RerankConfig) apply(ctx context.Context, query string, hits []Hit, limit int) ([]Hit, error) {
	if !rc.enabled() || len(hits) == 0 {
		return trim(hits, limit), nil
	}
	reranked, err := rc.Reranker.Rerank(ctx, query, hits)
	if err != nil {
		return trim(hits, limit), err
	}
	if len(reranked) != len(hits) {
		return trim(hits, limit), errRerankerChangedSet
	}
	for i := range reranked {
		reranked[i].Mode = reranked[i].Mode + "+" + rc.Reranker.Name()
	}
	return trim(reranked, limit), nil
}

func trim(hits []Hit, limit int) []Hit {
	if limit > 0 && len(hits) > limit {
		return hits[:limit]
	}
	return hits
}

// SortByScoreDesc is a helper for a Reranker implementation: it orders hits by Score, highest
// first, breaking ties deterministically so two runs of the same query agree.
//
// Determinism matters more than it looks: a reranker that ties two candidates and orders them by
// map iteration makes the search flap between runs, which reads as a bug in whatever consumes it.
func SortByScoreDesc(hits []Hit) {
	sort.SliceStable(hits, func(i, j int) bool {
		if hits[i].Score != hits[j].Score {
			return hits[i].Score > hits[j].Score
		}
		return hits[i].String() < hits[j].String()
	})
}
