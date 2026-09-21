//go:build lancedb

package wiki

import (
	"context"
	"github.com/graphit-labs/graphit-code/internal/relations"
	"testing"
)

func TestPersistExplicitWikiReferencesAndRejectInvalidBeforeWrite(t *testing.T) {
	ctx := context.Background()
	dir := t.TempDir()
	open := func() *WikiDB {
		t.Helper()
		db, err := OpenWikiDB(ctx, dir)
		if err != nil {
			t.Fatal(err)
		}
		return db
	}
	db := open()
	chunks := []WikiChunk{{Slug: "Decision", Title: "Decision", Body: "---\nreferences: [{target: {type: memory, id: mem-id}, relation: supports}]\n---\n# Decision", ContentHash: "a"}}
	if err := db.Sync(ctx, chunks, nil, nil); err != nil {
		t.Fatal(err)
	}
	db.Close()
	db = open()
	defer db.Close()
	edges, complete, err := db.ReferenceEdges(ctx)
	if err != nil || !complete || len(edges) != 1 || edges[0].Target.ID != "mem-id" {
		t.Fatalf("%+v %v %v", edges, complete, err)
	}
	chunks[0].Body = "# Decision edited without replacing authored references"
	chunks[0].ContentHash = "omitted"
	if err := db.Sync(ctx, chunks, nil, nil); err != nil {
		t.Fatal(err)
	}
	if err := db.ReconcileReferences(ctx); err != nil {
		t.Fatal(err)
	}
	edges, _, _ = db.ReferenceEdges(ctx)
	if len(edges) != 1 {
		t.Fatal("omission lost existing reference")
	}
	chunks[0].Body = "---\nreferences: [{target: {type: wrong, id: x}}]\n---"
	if err := db.Sync(ctx, chunks, nil, nil); err == nil {
		t.Fatal("invalid input accepted")
	}
	edges, _, _ = db.ReferenceEdges(ctx)
	if len(edges) != 1 {
		t.Fatal("invalid input changed existing relation")
	}
	chunks[0].Body = "---\nreferences: []\n---\n# Decision"
	chunks[0].ContentHash = "b"
	if err := db.Sync(ctx, chunks, nil, nil); err != nil {
		t.Fatal(err)
	}
	if err := db.ReconcileReferences(ctx); err != nil {
		t.Fatal(err)
	}
	edges, complete, err = db.ReferenceEdges(ctx)
	if err != nil || !complete || len(edges) != 0 {
		t.Fatalf("%+v %v %v", edges, complete, err)
	}
}

func TestReconcileDoesNotPromoteOrphanReferenceGeneration(t *testing.T) {
	ctx := context.Background()
	db, err := OpenWikiDB(ctx, t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	defer db.Close()
	c := WikiChunk{Slug: "Current", Title: "Current", Body: "# Current", ContentHash: "current"}
	if err := db.Sync(ctx, []WikiChunk{c}, nil, nil); err != nil {
		t.Fatal(err)
	}
	source := relations.Entity{Type: "knowledge", ID: c.Slug}
	refs := []relations.Ref{{Target: relations.Entity{Type: "memory", ID: "orphan"}, Relation: "supports"}}
	if err := relations.Replace(ctx, db.store, source, 9000000000000000000, "record", refs, "uncommitted-head"); err != nil {
		t.Fatal(err)
	}
	if err := db.ReconcileReferences(ctx); err != nil {
		t.Fatal(err)
	}
	edges, complete, err := db.ReferenceEdges(ctx)
	if err != nil || !complete || len(edges) != 0 {
		t.Fatalf("orphan promoted: %+v %v %v", edges, complete, err)
	}
	c.Body = "# Updated without changing explicit references"
	c.ContentHash = "next"
	if err := db.Sync(ctx, []WikiChunk{c}, nil, nil); err != nil {
		t.Fatal(err)
	}
	edges, complete, err = db.ReferenceEdges(ctx)
	if err != nil || !complete || len(edges) != 0 {
		t.Fatalf("orphan promoted by Sync: %+v %v %v", edges, complete, err)
	}
}
