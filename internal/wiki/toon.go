package wiki

import (
	"fmt"
	"strings"
)

// FormatSearchResultsTOON formats WikiSearchResult slices in compact TOON format.
// Example output:
//
//	results[3]{slug|title|type|score|summary}:
//	  auth-flow|Authentication Flow|specification|12.5|How auth works...
//	  db-schema|Database Schema|architecture|9.8|Schema design...
func FormatSearchResultsTOON(results []WikiSearchResult) string {
	if len(results) == 0 {
		return "results[0]{}:"
	}
	var sb strings.Builder
	fmt.Fprintf(&sb, "results[%d]{slug|title|type|score|summary}:\n", len(results))
	for _, r := range results {
		summary := r.Summary
		if len(summary) > 200 {
			summary = summary[:200] + "…"
		}
		summary = strings.ReplaceAll(summary, "|", "/")
		summary = strings.ReplaceAll(summary, "\n", " ")
		fmt.Fprintf(&sb, "  %s|%s|%s|%.1f|%s\n", r.Slug, r.Title, r.DocType, r.Score, summary)
	}
	return strings.TrimRight(sb.String(), "\n")
}

// FormatBrowseResultsTOON formats BrowseEntry slices in compact TOON format.
func FormatBrowseResultsTOON(entries []BrowseEntry) string {
	if len(entries) == 0 {
		return "results[0]{}:"
	}
	var sb strings.Builder
	fmt.Fprintf(&sb, "results[%d]{slug|title|type|confidence|words|summary}:\n", len(entries))
	for _, e := range entries {
		summary := e.Summary
		if len(summary) > 150 {
			summary = summary[:150] + "…"
		}
		summary = strings.ReplaceAll(summary, "|", "/")
		summary = strings.ReplaceAll(summary, "\n", " ")
		fmt.Fprintf(&sb, "  %s|%s|%s|%.1f|%d|%s\n", e.Slug, e.Title, e.DocType, e.Confidence, e.WordCount, summary)
	}
	return strings.TrimRight(sb.String(), "\n")
}

// FormatXRefResultsTOON formats XRefResult slices in compact TOON format.
func FormatXRefResultsTOON(query string, depth int, refs []XRefResult) string {
	if len(refs) == 0 {
		return fmt.Sprintf("xrefs[0]{%s|depth:%d}:", query, depth)
	}
	var sb strings.Builder
	fmt.Fprintf(&sb, "xrefs[%d]{slug|title|type|direction}:\n", len(refs))
	for _, r := range refs {
		fmt.Fprintf(&sb, "  %s|%s|%s|%s\n", r.Slug, r.Title, r.RefType, r.Direction)
	}
	return strings.TrimRight(sb.String(), "\n")
}

// FormatSyncLogTOON formats SyncLogEntry slices in compact TOON format.
func FormatSyncLogTOON(entries []SyncLogEntry) string {
	if len(entries) == 0 {
		return "log[0]{}:"
	}
	var sb strings.Builder
	fmt.Fprintf(&sb, "log[%d]{id|timestamp|docs|written|added|updated|deleted}:\n", len(entries))
	for _, e := range entries {
		fmt.Fprintf(&sb, "  %d|%s|%d|%d|%s|%s|%s\n",
			e.ID, e.Timestamp, e.TotalDocs, e.ArticlesWritten,
			strings.Join(e.Added, ","),
			strings.Join(e.Updated, ","),
			strings.Join(e.Deleted, ","))
	}
	return strings.TrimRight(sb.String(), "\n")
}
