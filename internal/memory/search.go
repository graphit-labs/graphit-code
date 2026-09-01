package memory

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/wiki"
)

// ChainResult is one memory in a search result, after the revision chain has been resolved.
//
// A chain is one memory across all of its revisions. The search index holds a page per revision,
// so a query can match several pages that are the same memory at different points in time — and
// answering with all of them would spend the caller's result budget re-describing one thing.
type ChainResult struct {
	wiki.BM25Result

	// MemoryID is the chain's identity: the id of its live memory, carried by every revision.
	MemoryID string `json:"memory_id,omitempty"`

	// Superseded marks a hit that is an archived revision rather than the current memory.
	Superseded bool `json:"superseded,omitempty"`

	// Current is the memory id to read for what the project believes NOW. Set only on a
	// superseded hit, because on a live one it is the hit itself.
	Current string `json:"current,omitempty"`

	// RevisionID addresses this revision inside its chain, for walking with previous/next.
	RevisionID string `json:"revision_id,omitempty"`
}

// chainOverFetch is how many extra hits are pulled so that dedup does not shrink the answer.
//
// Collapsing a chain removes results, so asking the index for exactly top_k and then deduping
// returns fewer than the caller asked for — and the missing ones are real memories that would
// have ranked. Fetching a multiple and trimming afterwards is what makes top_k mean "distinct
// memories" instead of "index rows".
const chainOverFetch = 4

// SearchChains searches the memory wiki and resolves the revision chain of every hit.
//
// Two rules, and they are the whole point of this function:
//
//   - when several hits belong to one chain, only the CURRENT revision is returned, with no
//     mention of the older ones. One memory occupies one result slot, and the caller is not
//     invited to read a revision the project has already moved past.
//   - when a hit is an archived revision whose current revision did NOT match, it is returned
//     with Superseded set and Current naming the live memory. The old text is what matched, so
//     hiding it would lose the answer; the annotation is what lets the agent decide whether to
//     read the revision, the current memory, or both.
func SearchChains(ctx context.Context, wikiDir, query string, topN int) []ChainResult {
	fetch := topN
	if fetch > 0 {
		fetch *= chainOverFetch
	}

	hits := wiki.BM25Search(ctx, wikiDir, query, fetch)
	resolved := resolveChains(wikiDir, hits)
	out := collapseChains(resolved)

	if topN > 0 && len(out) > topN {
		out = out[:topN]
	}
	return out
}

// resolveChains attaches the chain metadata each hit's page declares.
//
// It reads the page rather than the index because supersession lives in the page frontmatter —
// see the note on PageFieldSuperseded for why it is not a column. The cost is one small read per
// hit, and there are at most top_k * chainOverFetch of them.
func resolveChains(wikiDir string, hits []wiki.BM25Result) []ChainResult {
	out := make([]ChainResult, 0, len(hits))
	for _, hit := range hits {
		cr := ChainResult{BM25Result: hit}

		fields := pageChainFields(wikiDir, hit.Path)
		cr.MemoryID = fields[PageFieldMemoryID]
		cr.RevisionID = fields[PageFieldRevisionID]
		cr.Superseded = strings.EqualFold(fields[PageFieldSuperseded], "true")
		if cr.Superseded {
			cr.Current = fields[PageFieldCurrent]
			if cr.Current == "" {
				cr.Current = cr.MemoryID
			}
		}
		out = append(out, cr)
	}
	return out
}

// collapseChains keeps one result per chain, preferring the current revision.
//
// Order is preserved from the ranking: a chain takes the position of its best-ranked hit, so
// collapsing never promotes a weaker match above a stronger one.
func collapseChains(results []ChainResult) []ChainResult {
	kept := make([]ChainResult, 0, len(results))
	at := make(map[string]int, len(results))

	for _, r := range results {
		if r.MemoryID == "" {
			kept = append(kept, r)
			continue
		}
		i, seen := at[r.MemoryID]
		if !seen {
			at[r.MemoryID] = len(kept)
			kept = append(kept, r)
			continue
		}
		// The live revision replaces an archived one already held for this chain, keeping the
		// rank the chain earned. The reverse never happens: an archived hit adds nothing to a
		// chain whose current revision is already in the answer.
		if kept[i].Superseded && !r.Superseded {
			r.Score = kept[i].Score
			kept[i] = r
		}
	}
	return kept
}

// pageChainFields reads the chain frontmatter of one compiled page.
func pageChainFields(wikiDir, pagePath string) map[string]string {
	fields := map[string]string{}
	if pagePath == "" {
		return fields
	}

	full := filepath.Join(wikiDir, filepath.FromSlash(pagePath))
	if filepath.Ext(full) == "" {
		full += ".md"
	}
	if _, err := os.Stat(full); err != nil {
		return fields
	}

	for _, key := range []string{PageFieldMemoryID, PageFieldSuperseded, PageFieldCurrent, PageFieldRevisionID} {
		if v := wiki.ReadFrontmatterField(full, key); v != "" {
			fields[key] = v
		}
	}
	return fields
}

// FormatChainResultsTOON renders memory search results in the compact TOON shape.
//
// The `current` column appears only when some hit is a superseded revision, so a search whose
// answer is entirely current memories costs exactly what it did before the chain existed. When
// the column IS there, it is the instruction: the row is an old revision, and the id in `current`
// is what to read for what the project believes now.
func FormatChainResultsTOON(results []ChainResult, withPreview bool) string {
	if len(results) == 0 {
		return "results[0]{}:"
	}

	anySuperseded := false
	for _, r := range results {
		if r.Superseded {
			anySuperseded = true
			break
		}
	}
	if !anySuperseded {
		plain := make([]wiki.BM25Result, 0, len(results))
		for _, r := range results {
			plain = append(plain, r.BM25Result)
		}
		return wiki.FormatBM25ResultsTOON(plain, withPreview)
	}

	var sb strings.Builder
	header := "results[%d]{slug|title|type|score|superseded|current}:\n"
	if withPreview {
		header = "results[%d]{slug|title|type|score|superseded|current|preview}:\n"
	}
	fmt.Fprintf(&sb, header, len(results))

	for _, r := range results {
		slug := strings.TrimSuffix(r.Path, ".md")
		superseded := "-"
		current := "-"
		if r.Superseded {
			superseded = "yes"
			current = r.Current
		}
		if withPreview {
			fmt.Fprintf(&sb, "  %s|%s|%s|%.1f|%s|%s|%s\n",
				slug, r.Title, r.DocType, r.Score, superseded, current, previewCell(r.Snippet))
			continue
		}
		fmt.Fprintf(&sb, "  %s|%s|%s|%.1f|%s|%s\n", slug, r.Title, r.DocType, r.Score, superseded, current)
	}

	sb.WriteString("\nA row with superseded=yes is an OLD revision of the memory in `current`. " +
		"Read `current` for what the project believes now; read the row itself only when the old " +
		"wording is what you need. `previous` on the page walks further back.")
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
