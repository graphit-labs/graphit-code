package uiserver

import (
	"context"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/wiki"
)

func indexPage(t *testing.T, dir, name, content string) error {
	t.Helper()
	slug := strings.TrimSuffix(name, ".md")
	return indexChunk(t, dir, wiki.WikiChunk{
		Slug:      slug,
		Title:     firstHeadingOrSlug(content, slug),
		Body:      content,
		DocType:   "document",
		WordCount: len(strings.Fields(content)),
		ClusterID: -1,
	})
}

func indexChunk(t *testing.T, dir string, c wiki.WikiChunk) error {
	t.Helper()
	ctx := context.Background()

	existing := []wiki.WikiChunk{}
	if db, err := wiki.OpenWikiDB(ctx, dir); err == nil {
		if chunks, chunkErr := db.Chunks(ctx); chunkErr == nil {
			existing = chunks
		}
		_ = db.Close()
	}

	out := make([]wiki.WikiChunk, 0, len(existing)+1)
	for _, e := range existing {
		if e.Slug != c.Slug {
			out = append(out, e)
		}
	}
	out = append(out, c)
	return wiki.SyncDB(ctx, dir, out, nil, nil)
}

func firstHeadingOrSlug(content, slug string) string {
	for _, line := range strings.Split(content, "\n") {
		if after, ok := strings.CutPrefix(strings.TrimSpace(line), "# "); ok {
			return strings.TrimSpace(after)
		}
	}
	return slug
}
