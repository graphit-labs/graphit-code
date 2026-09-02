package uiserver

import (
	"context"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/wiki"
)

// A WIKI FIXTURE IS AN INDEX, NOT A DIRECTORY OF FILES.
//
// These tests used to build their subject with `os.WriteFile(dir, "<slug>.md", body)`, because the
// explorer read the pages off disk. It reads the index: the page body is a column, and so are the
// title, the type, the confidence and the provenance it used to parse out of frontmatter.
//
// indexPage keeps the call shape the fixtures had — same arguments, same returned error — so a test
// that only needs "a page called X exists with this text" reads the same as before.

// indexPage compiles one page into the index at dir, accumulating with whatever is already there.
//
// Accumulating matters: a rebuild replaces the whole chunk set, so writing two pages by calling this
// twice would otherwise leave only the second — which is not how two os.WriteFile calls behaved.
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

// indexChunk is indexPage for a test that cares about specific columns.
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
