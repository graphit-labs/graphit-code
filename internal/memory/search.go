package memory

import (
	"context"
	"fmt"
	"sort"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/lancestore"
)

// ChainResult is one authoritative memory search result after revision-chain collapse.
type ChainResult struct {
	Path      string  `json:"path"`
	Title     string  `json:"title"`
	DocType   string  `json:"doc_type,omitempty"`
	Score     float64 `json:"score"`
	Snippet   string  `json:"snippet,omitempty"`
	MemoryID  string  `json:"memory_id,omitempty"`
	Important bool    `json:"important"`
	Mandatory bool    `json:"mandatory"`
	CreatedAt string  `json:"created_at,omitempty"`
	UpdatedAt string  `json:"updated_at,omitempty"`

	Superseded bool   `json:"superseded,omitempty"`
	Current    string `json:"current,omitempty"`
	RevisionID string `json:"revision_id,omitempty"`
}

type SearchOptions struct {
	ExcludeMandatory bool
	// Mode is "fts", "semantic", or "hybrid". Empty means fts.
	Mode   string
	Vector []float32
}

// SearchMemories searches this scope's authoritative table. No wiki or derived store is opened.
func (m *MemoryService) SearchMemories(ctx context.Context, query string, topN int, opts SearchOptions) ([]ChainResult, error) {
	tbl, err := m.openTable(ctx)
	if err != nil {
		return nil, err
	}
	defer func() { _ = tbl.Close() }()
	return tbl.SearchChains(ctx, query, topN, opts)
}

// SearchChains searches current and archived records and returns at most one result per chain.
func (t *MemoryTable) SearchChains(ctx context.Context, query string, topN int, opts SearchOptions) ([]ChainResult, error) {
	if topN < 0 {
		return nil, fmt.Errorf("top_k cannot be negative")
	}
	mode := opts.Mode
	if mode == "" {
		mode = "fts"
	}
	if mode != "fts" && mode != "semantic" && mode != "hybrid" {
		return nil, fmt.Errorf("unknown memory search mode %q", mode)
	}
	if (mode == "semantic" || mode == "hybrid") && len(opts.Vector) == 0 {
		if mode == "semantic" {
			return nil, fmt.Errorf("semantic memory search requires a query vector")
		}
		mode = "fts"
	}

	if err := t.RefreshIndexes(ctx); err != nil {
		return nil, fmt.Errorf("preparing authoritative memory indexes: %w", err)
	}
	total, err := t.Count(ctx)
	if err != nil {
		return nil, err
	}
	if total == 0 {
		return nil, nil
	}

	ranked, err := t.searchRankings(ctx, query, int(total), mode, opts)
	if err != nil {
		return nil, err
	}
	out := collapseChains(ranked)
	live, err := t.Live(ctx)
	if err != nil {
		return nil, err
	}
	current := make(map[string]MemoryRecord, len(live))
	for _, record := range live {
		current[record.ID] = record
	}
	filtered := out[:0]
	for i := range out {
		if record, ok := current[out[i].MemoryID]; ok {
			out[i].Important = record.Important
			out[i].Mandatory = record.Mandatory
			out[i].CreatedAt = record.CreatedAt
			out[i].UpdatedAt = record.UpdatedAt
		}
		if opts.ExcludeMandatory && out[i].Mandatory {
			continue
		}
		filtered = append(filtered, out[i])
	}
	SortChainResults(filtered)
	if topN > 0 && len(filtered) > topN {
		filtered = filtered[:topN]
	}
	return filtered, nil
}

func (t *MemoryTable) searchRankings(ctx context.Context, query string, limit int, mode string, opts SearchOptions) ([]ChainResult, error) {
	text := strings.TrimSpace(query)
	if text == "" && mode != "semantic" {
		return nil, nil
	}
	filter := ""

	var queries []lancestore.Query
	switch mode {
	case "semantic":
		queries = append(queries, lancestore.Query{
			Vector: opts.Vector, VectorColumn: "embedding", Filter: filter, Limit: limit,
		})
	case "hybrid":
		queries = append(queries, lancestore.Query{
			Text: text, TextColumn: "body", Vector: opts.Vector, VectorColumn: "embedding",
			Filter: filter, Limit: limit,
		})
		for _, column := range []string{"title", "tags_json", "type"} {
			queries = append(queries, lancestore.Query{Text: text, TextColumn: column, Filter: filter, Limit: limit})
		}
	default:
		for _, column := range []string{"body", "title", "tags_json", "type"} {
			queries = append(queries, lancestore.Query{Text: text, TextColumn: column, Filter: filter, Limit: limit})
		}
	}

	const rrfK = 60.0
	type fused struct {
		result ChainResult
		score  float64
	}
	byKey := make(map[string]*fused)
	for _, q := range queries {
		hits, err := t.table.Search(ctx, q)
		if err != nil {
			return nil, err
		}
		for rank, hit := range hits {
			rec := recordFromRow(hit.Row)
			key := rec.Key()
			entry := byKey[key]
			if entry == nil {
				entry = &fused{result: chainResultFromRecord(rec)}
				byKey[key] = entry
			}
			entry.score += 1000 / (rrfK + float64(rank+1))
		}
	}

	out := make([]ChainResult, 0, len(byKey))
	for _, entry := range byKey {
		entry.result.Score = entry.score
		out = append(out, entry.result)
	}
	sort.Slice(out, func(i, j int) bool {
		if out[i].Score != out[j].Score {
			return out[i].Score > out[j].Score
		}
		return out[i].Path < out[j].Path
	})
	return out, nil
}

func chainResultFromRecord(rec MemoryRecord) ChainResult {
	current := ""
	if rec.Superseded {
		current = rec.ID
	}
	return ChainResult{
		Path: rec.Key(), Title: rec.Title, DocType: rec.Type, Snippet: memorySnippet(rec.Body),
		MemoryID: rec.ID, Important: rec.Important, Mandatory: rec.Mandatory,
		CreatedAt: rec.CreatedAt, UpdatedAt: rec.UpdatedAt,
		RevisionID: rec.RevisionID, Superseded: rec.Superseded, Current: current,
	}
}

func memorySnippet(body string) string {
	body = strings.TrimSpace(strings.Join(strings.Fields(body), " "))
	if len(body) > 240 {
		return body[:240] + "…"
	}
	return body
}

// collapseChains keeps one result per chain, preferring the current revision.
func collapseChains(results []ChainResult) []ChainResult {
	kept := make([]ChainResult, 0, len(results))
	at := make(map[string]int, len(results))
	for _, r := range results {
		i, seen := at[r.MemoryID]
		if !seen {
			at[r.MemoryID] = len(kept)
			kept = append(kept, r)
			continue
		}
		if kept[i].Superseded && !r.Superseded {
			r.Score = kept[i].Score
			kept[i] = r
		}
	}
	return kept
}

func FormatChainResultsTOON(results []ChainResult, withPreview bool) string {
	if len(results) == 0 {
		return "results[0]{}:"
	}
	anySuperseded := false
	for _, r := range results {
		anySuperseded = anySuperseded || r.Superseded
	}

	var sb strings.Builder
	header := "results[%d]{slug|title|type|priority|updated_at|score}:\n"
	if withPreview {
		header = "results[%d]{slug|title|type|priority|updated_at|score|preview}:\n"
	}
	if anySuperseded {
		header = "results[%d]{slug|title|type|priority|updated_at|score|superseded|current}:\n"
		if withPreview {
			header = "results[%d]{slug|title|type|priority|updated_at|score|superseded|current|preview}:\n"
		}
	}
	fmt.Fprintf(&sb, header, len(results))
	for _, r := range results {
		if !anySuperseded {
			if withPreview {
				fmt.Fprintf(&sb, "  %s|%s|%s|%s|%s|%.1f|%s\n", r.Path, r.Title, r.DocType, PriorityLabel(r.Mandatory, r.Important), r.UpdatedAt, r.Score, previewCell(r.Snippet))
			} else {
				fmt.Fprintf(&sb, "  %s|%s|%s|%s|%s|%.1f\n", r.Path, r.Title, r.DocType, PriorityLabel(r.Mandatory, r.Important), r.UpdatedAt, r.Score)
			}
			continue
		}
		superseded, current := "-", "-"
		if r.Superseded {
			superseded, current = "yes", r.Current
		}
		if withPreview {
			fmt.Fprintf(&sb, "  %s|%s|%s|%s|%s|%.1f|%s|%s|%s\n", r.Path, r.Title, r.DocType, PriorityLabel(r.Mandatory, r.Important), r.UpdatedAt, r.Score, superseded, current, previewCell(r.Snippet))
		} else {
			fmt.Fprintf(&sb, "  %s|%s|%s|%s|%s|%.1f|%s|%s\n", r.Path, r.Title, r.DocType, PriorityLabel(r.Mandatory, r.Important), r.UpdatedAt, r.Score, superseded, current)
		}
	}
	if anySuperseded {
		sb.WriteString("\nA row with superseded=yes is an OLD revision of the memory in `current`. " +
			"Read `current` for what the project believes now; read the row itself only when the old wording is needed.")
	}
	return sb.String()
}

func previewCell(s string) string {
	s = strings.ReplaceAll(s, "|", "/")
	s = strings.ReplaceAll(s, "\n", " ")
	if len(s) > 150 {
		s = s[:150] + "…"
	}
	return s
}
