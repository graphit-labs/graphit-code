package memory

import (
	"context"
	"math"
	"sort"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/wiki"
)

func loadMemoryVectors(ctx context.Context, wikiDir string) map[string][]float32 {
	if wikiDir == "" {
		return nil
	}
	db, err := wiki.OpenWikiDB(ctx, wikiDir)
	if err != nil {
		return nil
	}
	defer func() { _ = db.Close() }()

	bySource, err := db.VectorsBySource(ctx)
	if err != nil || len(bySource) == 0 {
		return nil
	}

	byID := make(map[string][]float32, len(bySource))
	for source, vec := range bySource {
		if isHistorySource(source) {
			continue
		}
		if id := memoryIDFromSource(source); id != "" {
			byID[id] = vec
		}
	}
	return byID
}

func isHistorySource(source string) bool {
	normalised := strings.ReplaceAll(source, "\\", "/")
	return strings.HasPrefix(normalised, HistoryDirName+"/") ||
		strings.Contains(normalised, "/"+HistoryDirName+"/")
}

func memoryIDFromSource(source string) string {
	base := source
	if i := strings.LastIndexAny(base, "/\\"); i >= 0 {
		base = base[i+1:]
	}
	return MemoryIDFromFileName(base)
}

// orderBySimilarity reorders memories so that semantically close ones sit next to each
// other, and returns them unchanged when there is nothing to order by.
//
// This is what makes batching survive a corpus that no longer fits one prompt. Batches
// are filled in order, so whatever is adjacent lands together — and in ID order, which
// is arrival order, adjacency means nothing. Two memories about the same thing written
// six months apart end up in different batches, are never put in front of the model
// together, and their duplication cannot be noticed. The report then says "nothing to
// do" about a pair it never compared.
//
// A greedy nearest-neighbour chain, which is O(n²) in a corpus of hundreds and costs
// microseconds. It is not an optimal clustering and does not need to be: the only
// property required is that near-duplicates are adjacent, and a duplicate is by
// definition its neighbour's nearest neighbour.
//
// Boundaries still exist — a chain has to be cut somewhere, and the two memories either
// side of a cut are separated. That is why CoverageNote keeps telling the truth about
// batched runs rather than claiming this solved it.
func orderBySimilarity(memories []memorySnapshot, vecs map[string][]float32) []memorySnapshot {
	if len(vecs) == 0 || len(memories) < 3 {
		return memories
	}

	var placeable []memorySnapshot
	var rest []memorySnapshot
	for _, m := range memories {
		if len(vecs[m.ID]) > 0 {
			placeable = append(placeable, m)
			continue
		}
		rest = append(rest, m)
	}
	if len(placeable) < 3 {
		return memories
	}

	norms := make([]float64, len(placeable))
	for i, m := range placeable {
		norms[i] = norm(vecs[m.ID])
	}

	sort.Slice(placeable, func(i, j int) bool { return placeable[i].ID < placeable[j].ID })

	used := make([]bool, len(placeable))
	ordered := make([]memorySnapshot, 0, len(placeable))
	current := 0
	used[0] = true
	ordered = append(ordered, placeable[0])

	for len(ordered) < len(placeable) {
		best, bestSim := -1, math.Inf(-1)
		cv := vecs[placeable[current].ID]
		cn := norms[current]
		for i := range placeable {
			if used[i] {
				continue
			}
			sim := cosine(cv, vecs[placeable[i].ID], cn, norms[i])
			if sim > bestSim {
				best, bestSim = i, sim
			}
		}
		if best < 0 {
			break
		}
		used[best] = true
		ordered = append(ordered, placeable[best])
		current = best
	}

	return append(ordered, rest...)
}

func norm(v []float32) float64 {
	var sum float64
	for _, x := range v {
		sum += float64(x) * float64(x)
	}
	return math.Sqrt(sum)
}

func cosine(a, b []float32, na, nb float64) float64 {
	if na == 0 || nb == 0 {
		return -1
	}
	n := len(a)
	if len(b) < n {
		n = len(b)
	}
	var dot float64
	for i := 0; i < n; i++ {
		dot += float64(a[i]) * float64(b[i])
	}
	return dot / (na * nb)
}
