//go:build lancedb

package knowledge

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/lancestore"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

func TestKnowledgeSyncAddsLocalLayerToShallowClone(t *testing.T) {
	ctx := context.Background()
	source := t.TempDir()
	if err := os.MkdirAll(filepath.Join(source, "docs"), 0o755); err != nil {
		t.Fatal(err)
	}
	baseDoc := "# Architecture\n\nThe baseterm architecture is published.\n"
	if err := os.WriteFile(filepath.Join(source, "docs", "architecture.md"), []byte(baseDoc), 0o644); err != nil {
		t.Fatal(err)
	}
	baseWiki := filepath.Join(t.TempDir(), "base-wiki")
	cfg := IndexConfig{Scope: WikiScope{Subdir: "docs"}}
	if _, err := RunIndexPipeline(ctx, source, baseWiki, cfg); err != nil {
		t.Fatal(err)
	}

	cloneWiki := filepath.Join(t.TempDir(), "clone-wiki")
	remote, err := lancestore.Open(ctx, lancestore.Config{URI: wiki.WikiIndexPath(baseWiki)})
	if err != nil {
		t.Fatal(err)
	}
	local, err := lancestore.Open(ctx, lancestore.Config{URI: wiki.WikiIndexPath(cloneWiki), Writable: true})
	if err != nil {
		t.Fatal(err)
	}
	names, err := remote.TableNames(ctx)
	if err != nil {
		t.Fatal(err)
	}
	for _, name := range names {
		table, err := remote.OpenTable(ctx, name)
		if err != nil {
			t.Fatal(err)
		}
		version, err := table.CurrentVersion(ctx)
		if err != nil {
			t.Fatal(err)
		}
		if err := table.PutTag(ctx, "git-base", version); err != nil {
			t.Fatal(err)
		}
		if _, err := local.CloneTable(ctx, name, filepath.Join(wiki.WikiIndexPath(baseWiki), name+".lance"), lancestore.CloneOptions{SourceTag: "git-base"}); err != nil {
			t.Fatal(err)
		}
	}
	if err := local.Close(); err != nil {
		t.Fatal(err)
	}
	if err := remote.Close(); err != nil {
		t.Fatal(err)
	}

	clone := t.TempDir()
	if err := os.MkdirAll(filepath.Join(clone, "docs"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(clone, "docs", "architecture.md"), []byte(baseDoc), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, err := RunIndexPipeline(ctx, clone, cloneWiki, cfg); err != nil {
		t.Fatal(err)
	}
	assertWikiSearchCount(t, ctx, cloneWiki, "baseterm", 1)

	localDoc := "# Local Plan\n\nThe localterm design is only in this checkout.\n"
	if err := os.WriteFile(filepath.Join(clone, "docs", "local.md"), []byte(localDoc), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, err := RunIndexPipeline(ctx, clone, cloneWiki, cfg); err != nil {
		t.Fatal(err)
	}
	assertWikiSearchCount(t, ctx, cloneWiki, "baseterm", 1)
	assertWikiSearchCount(t, ctx, cloneWiki, "localterm", 1)
	assertWikiPage(t, ctx, cloneWiki, "Local Plan", "localterm", true)
	assertWikiPage(t, ctx, baseWiki, "Local Plan", "", false)
	if err := os.RemoveAll(filepath.Join(clone, "docs")); err != nil {
		t.Fatal(err)
	}
	if _, err := RunIndexPipeline(ctx, clone, cloneWiki, cfg); err != nil {
		t.Fatal(err)
	}
	assertWikiSearchCount(t, ctx, cloneWiki, "baseterm", 0)
	assertWikiSearchCount(t, ctx, cloneWiki, "localterm", 0)
}

func assertWikiSearchCount(t *testing.T, ctx context.Context, wikiDir, query string, want int) {
	t.Helper()
	db, err := wiki.OpenWikiDB(ctx, wikiDir)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = db.Close() }()
	results, err := db.Search(ctx, query, 10)
	if err != nil {
		t.Fatal(err)
	}
	if (want == 0 && len(results) != 0) || (want > 0 && len(results) < want) {
		t.Fatalf("search %q in %s returned %d, want at least %d", query, wikiDir, len(results), want)
	}
}

func assertWikiPage(t *testing.T, ctx context.Context, wikiDir, title, bodyTerm string, exists bool) {
	t.Helper()
	db, err := wiki.OpenWikiDB(ctx, wikiDir)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = db.Close() }()
	page, err := db.Chunk(ctx, wiki.SafeSlug(title))
	if !exists && errors.Is(err, wiki.ErrPageNotFound) {
		return
	}
	if err != nil {
		t.Fatal(err)
	}
	if !exists || !strings.Contains(page.Body, bodyTerm) {
		t.Fatalf("page %q in %s: exists=%v body=%q", title, wikiDir, exists, page.Body)
	}
}
