//go:build lancedb

package memory

import (
	"context"
	"fmt"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
)

func tableAt(t *testing.T) *MemoryTable {
	t.Helper()
	tbl, err := OpenMemoryTable(context.Background(), t.TempDir())
	if err != nil {
		t.Fatalf("opening the memory store: %v", err)
	}
	t.Cleanup(func() { _ = tbl.Close() })
	return tbl
}

func TestOpenMemoryTableResetsAnIncompatibleDevelopmentSchema(t *testing.T) {
	ctx := context.Background()
	dir := t.TempDir()
	expected := memoryTableSchema(ai.ResolveConfiguredEmbeddingDimensions())
	legacy := lancestore.Schema{Fields: append([]lancestore.Field(nil), expected.Fields...)}
	legacy.Fields = legacy.Fields[:len(legacy.Fields)-1]
	store, err := lancestore.Open(ctx, lancestore.Config{URI: dir})
	if err != nil {
		t.Fatal(err)
	}
	old, err := store.CreateTable(ctx, memoryTableName, legacy)
	if err != nil {
		t.Fatal(err)
	}
	_ = old.Close()
	_ = store.Close()

	table, err := OpenMemoryTable(ctx, dir)
	if err != nil {
		t.Fatalf("opening incompatible store: %v", err)
	}
	defer func() { _ = table.Close() }()
	if !table.table.Schema().Equal(expected) {
		t.Fatalf("schema was not reset: %+v", table.table.Schema())
	}
}

func fullRecord() MemoryRecord {
	return MemoryRecord{
		ID:          "01ARZ3NDEKTSV4RRFFQ69G5FAV",
		Superseded:  false,
		Title:       "Storage: where every artifact lives",
		Body:        "What: the body.\nWhy: because.\n",
		Type:        "decision",
		Tags:        []string{"memory", "project", "decision", "storage"},
		Important:   true,
		Mandatory:   true,
		CreatedAt:   "2026-07-01T00:00:00Z",
		UpdatedAt:   "2026-08-15T12:30:00Z",
		Revision:    3,
		UpdatedBy:   "01UNITULID0000000000000000",
		Previous:    "history/01ARZ3NDEKTSV4RRFFQ69G5FAV/0002.md",
		Scope:       "project",
		ScopeID:     "01PROJECTULID000000000000",
		ProjectID:   "01OTHERPROJECT0000000000",
		ContentHash: "abcdef0123456789",
	}
}

// A title containing `: ` is the shape that made 47 memories unreadable, so the record has to survive
// it — and the renderer marshals, which is what makes that structural rather than lucky.
func TestARecordWithAHostileTitleRendersAndParsesBack(t *testing.T) {
	r := fullRecord()
	md := r.Markdown()
	fm, ok := ParseMemoryFrontmatterOK(md)
	if !ok {
		t.Fatalf("the rendered memory does not parse:\n%s", md)
	}
	if fm.Title != r.Title {
		t.Errorf("title = %q, want %q", fm.Title, r.Title)
	}
	if fm.UpdatedBy != r.UpdatedBy || fm.ProjectID != r.ProjectID {
		t.Errorf("a file-only field was lost: updated_by=%q project_id=%q", fm.UpdatedBy, fm.ProjectID)
	}
}

// An archived revision is addressed by `<id>/<revision_id>`, and a live memory by its id alone. That
// is what lets one chain hold several rows without them colliding.
func TestArchivedRevisionsAreSeparateRowsOfOneChain(t *testing.T) {
	ctx := context.Background()
	table := tableAt(t)

	live := fullRecord()
	first := live
	first.RevisionID = "0001"
	first.Superseded = true
	first.Revision = 1
	first.Next = "history/" + live.ID + "/0002.md"
	second := live
	second.RevisionID = "0002"
	second.Superseded = true
	second.Revision = 2
	second.Next = MemoryFileName(live.ID)

	if err := table.Put(ctx, live, first, second); err != nil {
		t.Fatalf("writing the chain: %v", err)
	}
	if live.Key() != live.ID {
		t.Errorf("a live memory's key = %q, want the bare id", live.Key())
	}
	if first.Key() != live.ID+"/0001" {
		t.Errorf("a revision's key = %q, want <id>/<revision_id>", first.Key())
	}

	n, err := table.Count(ctx)
	if err != nil {
		t.Fatalf("counting: %v", err)
	}
	if n != 3 {
		t.Fatalf("the chain holds %d rows, want 3 — the keys collided", n)
	}

	liveOnly, err := table.Live(ctx)
	if err != nil {
		t.Fatalf("listing live: %v", err)
	}
	if len(liveOnly) != 1 || liveOnly[0].RevisionID != "" {
		t.Errorf("Live() = %d record(s) %+v, want just the head", len(liveOnly), liveOnly)
	}

	revs, err := table.Revisions(ctx, live.ID)
	if err != nil {
		t.Fatalf("listing revisions: %v", err)
	}
	if len(revs) != 2 || revs[0].RevisionID != "0001" || revs[1].RevisionID != "0002" {
		t.Fatalf("revisions = %+v, want 0001 then 0002", revs)
	}
}

func TestPutReplacesAndDeleteRemoves(t *testing.T) {
	ctx := context.Background()
	table := tableAt(t)
	r := fullRecord()

	if err := table.Put(ctx, r); err != nil {
		t.Fatalf("writing: %v", err)
	}
	r.Title = "Rewritten"
	r.Revision = 4
	if err := table.Put(ctx, r); err != nil {
		t.Fatalf("rewriting: %v", err)
	}
	if n, err := table.Count(ctx); err != nil || n != 1 {
		t.Fatalf("after a rewrite: %d rows (err %v), want 1 — Put must replace, not append", n, err)
	}
	got, _, err := table.Get(ctx, r.Key())
	if err != nil {
		t.Fatalf("reading back: %v", err)
	}
	if got.Title != "Rewritten" || got.Revision != 4 {
		t.Errorf("the rewrite did not land: %+v", got)
	}

	if err := table.Delete(ctx, r.Key()); err != nil {
		t.Fatalf("deleting: %v", err)
	}
	if _, found, err := table.Get(ctx, r.Key()); err != nil || found {
		t.Errorf("after delete: found=%v err=%v", found, err)
	}
	if err := table.Delete(ctx, "never-existed"); err != nil {
		t.Errorf("deleting an absent key: %v", err)
	}
}

func TestListLiveAndRevisionsReadPastOneEnginePage(t *testing.T) {
	ctx := context.Background()
	table := tableAt(t)
	oldPageSize := memoryReadPageSize
	memoryReadPageSize = 2
	t.Cleanup(func() { memoryReadPageSize = oldPageSize })

	records := make([]MemoryRecord, 0, 7)
	for i := 0; i < 5; i++ {
		r := fullRecord()
		r.ID = fmt.Sprintf("memory-%02d", i)
		r.Title = r.ID
		records = append(records, r)
	}
	for i := 0; i < 2; i++ {
		r := fullRecord()
		r.RevisionID = fmt.Sprintf("%04d", i+1)
		r.Superseded = true
		records = append(records, r)
	}
	if err := table.Put(ctx, records...); err != nil {
		t.Fatalf("writing records: %v", err)
	}

	all, err := table.List(ctx)
	if err != nil || len(all) != 7 {
		t.Fatalf("List() returned %d records (err %v), want all 7", len(all), err)
	}
	live, err := table.Live(ctx)
	if err != nil || len(live) != 5 {
		t.Fatalf("Live() returned %d records (err %v), want all 5", len(live), err)
	}
	revisions, err := table.Revisions(ctx, fullRecord().ID)
	if err != nil || len(revisions) != 2 {
		t.Fatalf("Revisions() returned %d records (err %v), want both revisions", len(revisions), err)
	}
}

func TestMandatoryReadAndContextSearchExclusion(t *testing.T) {
	ctx := context.Background()
	table := tableAt(t)
	mandatory := fullRecord()
	mandatory.ID = "mandatory-memory"
	mandatory.Title = "Mandatory protocol"
	mandatory.Body = "session-protocol-token applies everywhere"
	mandatory.Mandatory = true
	ordinary := mandatory
	ordinary.ID = "ordinary-memory"
	ordinary.Title = "Ordinary detail"
	ordinary.Mandatory = false
	if err := table.Put(ctx, mandatory, ordinary); err != nil {
		t.Fatalf("writing memories: %v", err)
	}

	loaded, err := table.Mandatory(ctx)
	if err != nil || len(loaded) != 1 || loaded[0].ID != mandatory.ID {
		t.Fatalf("Mandatory() = %+v (err %v), want only %q", loaded, err, mandatory.ID)
	}

	wikiDir := filepath.Join(t.TempDir(), "wiki")
	if _, err := GenerateMemoryWikiFromTable(ctx, table, wikiDir); err != nil {
		t.Fatalf("building wiki: %v", err)
	}
	results := SearchChains(ctx, wikiDir, "session-protocol-token", 10, true)
	if len(results) != 1 || results[0].MemoryID != ordinary.ID {
		t.Fatalf("search excluding mandatory = %+v, want only %q", results, ordinary.ID)
	}
}

func TestMandatoryMarkAndUnmarkAreIndependentFromImportance(t *testing.T) {
	ctx := context.Background()
	tableDir := filepath.Join(t.TempDir(), "table")
	svc := NewMemoryService(MemoryScopeProject, "scope", &MemoryStore{})
	svc.tableURI = tableDir
	svc.wikiDir = filepath.Join(t.TempDir(), "wiki")
	id, err := svc.AddMemory("Foundation", "Always load this.", MemoryOpts{Important: true})
	if err != nil {
		t.Fatalf("adding memory: %v", err)
	}
	if err := svc.MarkMandatory(id); err != nil {
		t.Fatalf("marking mandatory: %v", err)
	}
	table, err := OpenMemoryTable(ctx, tableDir)
	if err != nil {
		t.Fatal(err)
	}
	record, found, err := table.Get(ctx, id)
	_ = table.Close()
	if err != nil || !found || !record.Mandatory || !record.Important {
		t.Fatalf("after mark: %+v found=%v err=%v", record, found, err)
	}
	if err := svc.UnmarkMandatory(id); err != nil {
		t.Fatalf("unmarking mandatory: %v", err)
	}
	table, err = OpenMemoryTable(ctx, tableDir)
	if err != nil {
		t.Fatal(err)
	}
	record, found, err = table.Get(ctx, id)
	_ = table.Close()
	if err != nil || !found || record.Mandatory || !record.Important {
		t.Fatalf("after unmark: %+v found=%v err=%v", record, found, err)
	}
}

// A record must survive the round trip with EVERY field, and the field list is the point.
//
// Six of these existed only in the markdown file and had no column anywhere in the wiki — `Scope`,
// `ScopeID`, `ProjectID`, `UpdatedBy`, `Tags`, and `UpdatedAt` (the wiki's `updated` is stamped with
// the COMPILE date, so a memory's real last-write time survived nowhere else). A schema that quietly
// dropped one of them would lose it on every write, not just once.
//
// This replaces a version that compared canonical content hashes through the migration's helper.
// The migration is retired; the guarantee is not, so the comparison is now field by field — which is
// stricter, and says which field was lost instead of only that one was.
func TestAMemoryRecordSurvivesTheRoundTripWithEveryField(t *testing.T) {
	t.Setenv("GRAPHIT_HUB_BUCKET", "")
	ctx := context.Background()
	tbl, err := OpenMemoryTable(ctx, filepath.Join(t.TempDir(), "table"))
	if err != nil {
		t.Fatalf("OpenMemoryTable: %v", err)
	}
	defer func() { _ = tbl.Close() }()

	want := MemoryRecord{
		ID:          "01ABCDEFGHIJKLMNOPQRSTUVWX",
		RevisionID:  "01ZZZZZZZZZZZZZZZZZZZZZZZZ",
		Superseded:  true,
		Title:       "A title with a colon: and a quote \" in it",
		Body:        "a body\nover two lines",
		Type:        "decision",
		Tags:        []string{"auth", "security"},
		Important:   true,
		CreatedAt:   "2026-08-01T00:00:00Z",
		UpdatedAt:   "2026-09-02T12:34:56Z",
		Revision:    3,
		UpdatedBy:   "01UNITUNITUNITUNITUNITUNIT",
		Previous:    "history/01ABCDEFGHIJKLMNOPQRSTUVWX/01YYYYYYYYYYYYYYYYYYYYYYYY.md",
		Next:        "01ABCDEFGHIJKLMNOPQRSTUVWX.md",
		Scope:       "project",
		ScopeID:     "01SCOPESCOPESCOPESCOPESCOP",
		ProjectID:   "01PROJPROJPROJPROJPROJPROJ",
		ContentHash: "deadbeefcafebabe",
	}
	if err := tbl.Put(ctx, want); err != nil {
		t.Fatalf("Put: %v", err)
	}

	got, ok, err := tbl.Get(ctx, want.Key())
	if err != nil || !ok {
		t.Fatalf("Get(%q): ok=%v err=%v", want.Key(), ok, err)
	}

	for _, c := range []struct {
		field     string
		want, got any
	}{
		{"ID", want.ID, got.ID},
		{"RevisionID", want.RevisionID, got.RevisionID},
		{"Superseded", want.Superseded, got.Superseded},
		{"Title", want.Title, got.Title},
		{"Body", want.Body, got.Body},
		{"Type", want.Type, got.Type},
		{"Important", want.Important, got.Important},
		{"Mandatory", want.Mandatory, got.Mandatory},
		{"CreatedAt", want.CreatedAt, got.CreatedAt},
		{"UpdatedAt", want.UpdatedAt, got.UpdatedAt},
		{"Revision", want.Revision, got.Revision},
		{"UpdatedBy", want.UpdatedBy, got.UpdatedBy},
		{"Previous", want.Previous, got.Previous},
		{"Next", want.Next, got.Next},
		{"Scope", want.Scope, got.Scope},
		{"ScopeID", want.ScopeID, got.ScopeID},
		{"ProjectID", want.ProjectID, got.ProjectID},
		{"ContentHash", want.ContentHash, got.ContentHash},
	} {
		if c.want != c.got {
			t.Errorf("%s = %v, want %v", c.field, c.got, c.want)
		}
	}
	if len(got.Tags) != len(want.Tags) {
		t.Fatalf("Tags = %v, want %v", got.Tags, want.Tags)
	}
	for i := range want.Tags {
		if got.Tags[i] != want.Tags[i] {
			t.Errorf("Tags[%d] = %q, want %q", i, got.Tags[i], want.Tags[i])
		}
	}

	if want.Key() != want.ID+"/"+want.RevisionID {
		t.Errorf("Key() = %q, want the id and revision joined", want.Key())
	}
}
