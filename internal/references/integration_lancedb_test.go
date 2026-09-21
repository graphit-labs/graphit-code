//go:build lancedb

package references

import (
	"context"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/relations"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/task"
	"github.com/graphit-labs/graphit-code/internal/testsupport/testenv"
	"github.com/graphit-labs/graphit-code/internal/wiki"
	"os"
	"path/filepath"
	"testing"
)

func TestMain(m *testing.M) { testenv.Run(m) }
func TestCrossDomainPersistedReferencesReopenAndProjectIsolation(t *testing.T) {
	ctx := context.Background()
	project := func(id string) string {
		t.Helper()
		dir := t.TempDir()
		if err := os.WriteFile(filepath.Join(dir, brand.LockFileName()), []byte(`{"project":{"id":"`+id+`"}}`), 0600); err != nil {
			t.Fatal(err)
		}
		return dir
	}
	dir := project("01ARZ3NDEKTSV4RRFFQ69G5FAV")
	other := project("01ARZ3NDEKTSV4RRFFQ69G5FAW")
	ms, err := memory.NewMemoryStore()
	if err != nil {
		t.Fatal(err)
	}
	mem := memory.NewMemoryService(memory.MemoryScopeProject, store.ProjectID(dir), ms).WithContext(ctx)
	memoryID, err := mem.AddMemory("Decision", "Durable engineering decision", memory.MemoryOpts{})
	if err != nil {
		t.Fatal(err)
	}
	svc, err := task.Open(dir)
	if err != nil {
		t.Fatal(err)
	}
	refs := []relations.Ref{{Target: relations.Entity{Type: "memory", ID: memoryID}, Relation: "supports"}}
	session, err := svc.SessionCreate(relations.WithInputs(ctx, &refs), task.SessionCreateInput{Title: "Request", Description: "Implement and verify typed relations", Strategy: "Persist then reopen", Actor: "author"})
	if err != nil {
		t.Fatal(err)
	}
	record, err := svc.Create(relations.WithInputs(ctx, &refs), task.CreateInput{Title: "Delivery", Description: "Persist relations", SessionID: session.ID, Actor: "author", AcceptanceCriteria: []string{"Must preserve relationships"}, Tests: []string{"Reopen the database and read typed links"}})
	if err != nil {
		t.Fatal(err)
	}
	db, err := wiki.OpenWikiDB(ctx, knowledge.ReadDirIn(dir, ""))
	if err != nil {
		t.Fatal(err)
	}
	body := "---\nreferences: [{target: {type: task, id: " + record.ID + "}, relation: documents}]\n---\n# Contract"
	if err = db.Sync(ctx, []wiki.WikiChunk{{Slug: "Contract", Title: "Contract", Body: body, ContentHash: "v1"}}, nil, nil); err != nil {
		t.Fatal(err)
	}
	db.Close()
	snapshot := Read(ctx, dir, false)
	if !snapshot.Complete {
		t.Fatalf("incomplete: %v", snapshot.Warnings)
	}
	if len(snapshot.Records) != 4 {
		t.Fatalf("records=%+v", snapshot.Records)
	}
	if got := Select(snapshot.Edges, Filter{TargetType: "memory", TargetID: memoryID, TargetScopeID: store.ProjectID(dir)}); len(got) != 2 {
		t.Fatalf("backlinks=%+v", got)
	}
	if got := Select(snapshot.Edges, Filter{SourceType: "knowledge", SourceID: "Contract", TargetType: "task", TargetID: record.ID}); len(got) != 1 {
		t.Fatalf("knowledge relations=%+v", got)
	}
	if failures := Reconcile(ctx, dir, false); len(failures) != 0 {
		t.Fatal(failures)
	}
	after := Read(ctx, dir, false)
	if len(after.Edges) != len(snapshot.Edges) {
		t.Fatal("reconciliation changed explicit edges")
	}
	if err := mem.WithContext(ctx).RemoveMemory(memoryID); err != nil {
		t.Fatal(err)
	}
	deleted := Read(ctx, dir, false)
	for _, r := range deleted.Records {
		if r.Entity.Type == "memory" && r.Entity.ID == memoryID {
			t.Fatal("deleted memory remains navigable")
		}
	}
	if len(Select(deleted.Edges, Filter{TargetType: "memory", TargetID: memoryID})) != 2 {
		t.Fatal("deleting target erased source provenance")
	}
	foreign := Read(ctx, other, false)
	if got := Select(foreign.Edges, Filter{TargetID: memoryID}); len(got) != 0 {
		t.Fatal("another project acquired these edges")
	}
}
