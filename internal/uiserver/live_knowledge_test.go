//go:build lancedb

package uiserver

import (
	"context"
	"fmt"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/store"
	"os"
	"path/filepath"
	"testing"
)

func TestLiveKnowledgeResolvesOnlyInstalledSources(t *testing.T) {
	ctx := context.Background()
	workspace := t.TempDir()
	sources := map[string]string{}
	for i, name := range []string{"alpha", "alpha-deep", "beta"} {
		dir := t.TempDir()
		if err := os.WriteFile(filepath.Join(dir, brand.LockFileName()), []byte(fmt.Sprintf(`{"project":{"id":"01ARZ3NDEKTSV4RRFFQ69G5FA%d"}}`, i)), 0600); err != nil {
			t.Fatal(err)
		}
		sources[name] = knowledge.WikiDirFor(dir)
		if err := indexPage(t, sources[name], "rules.md", "# Rules\n\n**Evidence** from "+name); err != nil {
			t.Fatal(err)
		}
		if err := store.AddContext(workspace, store.KindKnowledge, store.ContextRecord{Name: name, Origin: "link", SourcePath: dir}); err != nil {
			t.Fatal(err)
		}
	}
	pages, err := liveKnowledgePage(ctx, workspace, "rules", "")
	if err != nil || len(pages) != 3 {
		t.Fatalf("ambiguity: %v %+v", err, pages)
	}
	pages, err = liveKnowledgePage(ctx, workspace, "alpha-deep/rules.md", "")
	if err != nil || len(pages) != 1 || pages[0].Context != "alpha-deep" {
		t.Fatalf("qualified: %v %+v", err, pages)
	}
	if _, err = liveKnowledgePage(ctx, workspace, "rules", "unselected"); err == nil {
		t.Fatal("unselected context allowed")
	}
	pages, err = liveKnowledgePage(ctx, workspace, "missing", "beta")
	if err != nil || len(pages) != 0 {
		t.Fatalf("missing: %v %+v", err, pages)
	}
	if err = os.RemoveAll(sources["beta"]); err != nil {
		t.Fatal(err)
	}
	if _, err = liveKnowledgePage(ctx, workspace, "rules", "beta"); err == nil {
		t.Fatal("removed source recreated")
	}
}
