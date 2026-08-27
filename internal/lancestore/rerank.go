package lancestore

import (
	"context"
	"sort"
)

// Reranking: the second stage, and it is OPT-IN AND OFF BY DEFAULT.
//
// The engine already fuses the dense and the BM25 channel with its own reciprocal-rank-fusion
// reranker, which is score-based: it combines the rankings it was given. A CROSS-ENCODER is the
// other family — it discards those scores and reads each (query, candidate) pair to judge
// relevance directly. That is what LanceDB's own documentation points to for quality beyond
// fusion, and the Go binding exposes only RRF, so it is a gap this side fills.
//
// WHY OFF BY DEFAULT, and this is a measurement rather than caution:
//
//   - It costs a second model. The retrieval embedder is 137M/~132 MiB; the reranker chosen for
//     code is jina-reranker-v2-base-multilingual, which is the only small reranker with
//     published code-retrieval benchmarks, and it is roughly 1.1 GiB.
//   - It costs inference ON THE QUERY PATH. An embedding is computed once when a file is indexed
//     and cached by shard hash; a cross-encoder runs per query, over every candidate.
//   - And the gate it would have to justify itself against is SATURATED: 11/11 strict and 5/5
//     recall without it. On that evidence there is nothing to show, so shipping it enabled would
//     be repeating a best practice as a formula rather than applying it.
//
// So the seam exists, the implementation is pluggable, and turning it on is a decision someone
// makes against a harder evaluation set than the one that currently passes at 100%.

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

// apply runs the second stage and trims back to the caller's limit.
//
// A reranker that fails does NOT fail the search: the engine's own ranking is a good answer, and
// losing every result because a second-stage model could not load is worse than losing the
// reordering. The error is returned alongside the first-stage order so a caller can log it.
func (rc RerankConfig) apply(ctx context.Context, query string, hits []Hit, limit int) ([]Hit, error) {
	if !rc.enabled() || len(hits) == 0 {
		return trim(hits, limit), nil
	}
	reranked, err := rc.Reranker.Rerank(ctx, query, hits)
	if err != nil {
		return trim(hits, limit), err
	}
	if len(reranked) != len(hits) {
		// A reranker that returns a different set has broken its contract; the safe reading is
		// to distrust the reordering rather than to serve a truncated answer as if it were
		// ranked.
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
