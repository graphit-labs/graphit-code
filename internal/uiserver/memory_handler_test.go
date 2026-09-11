package uiserver

import (
	"bytes"
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"reflect"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/memory"
)

type fakeMemoryExplorer struct {
	entries      []memory.MemoryEntry
	hits         []memory.ChainResult
	trace        memory.MemoryTrace
	query        string
	addedTitle   string
	addedBody    string
	addedOpts    memory.MemoryOpts
	updatedID    string
	updatedTitle string
	updatedBody  string
	updatedType  string
	promoted     bool
	mandatory    bool
	removedID    string
	listErr      error
	traceFound   bool
}

func (f *fakeMemoryExplorer) ListMemories() ([]memory.MemoryEntry, error) {
	return f.entries, f.listErr
}

func (f *fakeMemoryExplorer) SearchMemories(_ context.Context, query string, _ int, _ memory.SearchOptions) ([]memory.ChainResult, error) {
	f.query = query
	return f.hits, nil
}

func (f *fakeMemoryExplorer) TraceMemory(_ context.Context, _ string) (memory.MemoryTrace, bool, error) {
	return f.trace, f.traceFound || f.trace.Current != nil || len(f.trace.Revisions) > 0, nil
}

func (f *fakeMemoryExplorer) AddMemory(title, body string, opts memory.MemoryOpts) (string, error) {
	f.addedTitle, f.addedBody, f.addedOpts = title, body, opts
	record := memory.MemoryRecord{ID: "01NEW", Title: title, Body: body, Type: string(opts.Type), Tags: opts.Tags, Revision: 1, Scope: "project", ScopeID: "01PROJECT"}
	f.trace = memory.MemoryTrace{Current: &record, Revisions: []memory.MemoryRecord{}}
	f.traceFound = true
	return record.ID, nil
}

func (f *fakeMemoryExplorer) UpdateMemoryTyped(id, title, body, memType string) error {
	f.updatedID, f.updatedTitle, f.updatedBody, f.updatedType = id, title, body, memType
	if f.trace.Current != nil {
		previous := *f.trace.Current
		previous.RevisionID = "01REV"
		previous.Superseded = true
		previous.Next = id + ".md"
		f.trace.Revisions = append(f.trace.Revisions, previous)
		if title != "" {
			f.trace.Current.Title = title
		}
		if body != "" {
			f.trace.Current.Body = body
		}
		if memType != "" {
			f.trace.Current.Type = memType
		}
		f.trace.Current.Revision++
		f.trace.Current.Previous = "history/" + id + "/01REV.md"
	}
	return nil
}

func (f *fakeMemoryExplorer) PromoteMemory(string) error {
	f.promoted = true
	if f.trace.Current != nil {
		f.trace.Current.Important = true
	}
	return nil
}
func (f *fakeMemoryExplorer) DemoteMemory(string) error {
	f.promoted = false
	if f.trace.Current != nil {
		f.trace.Current.Important = false
	}
	return nil
}
func (f *fakeMemoryExplorer) MarkMandatory(string) error {
	f.mandatory = true
	if f.trace.Current != nil {
		f.trace.Current.Mandatory = true
	}
	return nil
}
func (f *fakeMemoryExplorer) UnmarkMandatory(string) error {
	f.mandatory = false
	if f.trace.Current != nil {
		f.trace.Current.Mandatory = false
	}
	return nil
}
func (f *fakeMemoryExplorer) RemoveMemory(id string) error { f.removedID = id; return nil }

func memoryTestHandler(t *testing.T, fake *fakeMemoryExplorer) http.Handler {
	t.Helper()
	handler := NewMemoryHandler("/project")
	handler.open = func(_ context.Context, projectDir, scope string) (memoryExplorerService, error) {
		if projectDir != "/project" {
			t.Fatalf("project dir = %q", projectDir)
		}
		if scope != "project" && scope != "user" {
			t.Fatalf("scope = %q", scope)
		}
		return fake, nil
	}
	mux := http.NewServeMux()
	handler.RegisterAPIRoutes(mux)
	return mux
}

func TestMemoryHandlerCatalogUsesMemorySearchAndDomainFilters(t *testing.T) {
	fake := &fakeMemoryExplorer{
		entries: []memory.MemoryEntry{
			{ID: "01A", Title: "Storage decision", Type: memory.MemoryTypeDecision, Tags: []string{"memory", "project", "storage"}, Important: true, UpdatedAt: "2026-09-02T00:00:00Z", Revision: 2},
			{ID: "01B", Title: "UI convention", Type: memory.MemoryTypeConvention, Tags: []string{"memory", "project", "ui"}, UpdatedAt: "2026-09-03T00:00:00Z", Revision: 1},
		},
		hits: []memory.ChainResult{{MemoryID: "01A", Title: "Storage decision", Snippet: "authoritative table", Score: 9.5}},
	}
	response := httptest.NewRecorder()
	request := httptest.NewRequest(http.MethodGet, "/api/memories?query=storage&type=decision&tag=storage&important=true&mandatory=false", nil)
	memoryTestHandler(t, fake).ServeHTTP(response, request)
	if response.Code != http.StatusOK {
		t.Fatalf("status = %d, body = %s", response.Code, response.Body.String())
	}
	var payload MemoryCatalogResponse
	if err := json.NewDecoder(response.Body).Decode(&payload); err != nil {
		t.Fatal(err)
	}
	if fake.query != "storage" || payload.Total != 1 || payload.Results[0].ID != "01A" {
		t.Fatalf("catalog = %#v, query = %q", payload, fake.query)
	}
	if payload.Results[0].Snippet != "authoritative table" || payload.Results[0].Revision != 2 {
		t.Fatalf("catalog metadata = %#v", payload.Results[0])
	}
}

func TestMemoryHandlerCatalogOrdersPriorityThenNewestEvenForSearch(t *testing.T) {
	fake := &fakeMemoryExplorer{
		entries: []memory.MemoryEntry{
			{ID: "normal", Title: "Normal", UpdatedAt: "2026-09-06T00:00:00Z"},
			{ID: "important-old", Title: "Important old", Important: true, UpdatedAt: "2026-09-03T00:00:00Z"},
			{ID: "mandatory-old", Title: "Mandatory old", Mandatory: true, UpdatedAt: "2026-09-02T00:00:00Z"},
			{ID: "important-new", Title: "Important new", Important: true, UpdatedAt: "2026-09-04T00:00:00Z"},
			{ID: "mandatory-new", Title: "Mandatory new", Mandatory: true, UpdatedAt: "2026-09-05T00:00:00Z"},
		},
		hits: []memory.ChainResult{
			{MemoryID: "normal", Score: 100},
			{MemoryID: "important-old", Score: 90},
			{MemoryID: "mandatory-old", Score: 1},
			{MemoryID: "important-new", Score: 2},
			{MemoryID: "mandatory-new", Score: 3},
		},
	}
	response := httptest.NewRecorder()
	request := httptest.NewRequest(http.MethodGet, "/api/memories?query=match", nil)
	memoryTestHandler(t, fake).ServeHTTP(response, request)
	if response.Code != http.StatusOK {
		t.Fatalf("status = %d, body = %s", response.Code, response.Body.String())
	}
	var payload MemoryCatalogResponse
	if err := json.NewDecoder(response.Body).Decode(&payload); err != nil {
		t.Fatal(err)
	}
	got := make([]string, len(payload.Results))
	for i, item := range payload.Results {
		got[i] = item.ID
	}
	want := []string{"mandatory-new", "mandatory-old", "important-new", "important-old", "normal"}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("catalog order = %v, want %v", got, want)
	}
}

func TestMemoryHandlerDetailReturnsTheAuthoritativeRevisionChain(t *testing.T) {
	current := memory.MemoryRecord{
		ID: "01A", Title: "Current", Body: "current body", Type: "decision", Tags: []string{"trace"},
		Revision: 2, UpdatedBy: "unit-current", Previous: "history/01A/01REV.md", Scope: "project",
		ScopeID: "01PROJECT", ContentHash: "current-hash",
	}
	revision := current
	revision.Title, revision.Body, revision.Revision = "Original", "old body", 1
	revision.RevisionID, revision.Superseded, revision.Next = "01REV", true, "01A.md"
	revision.Previous, revision.UpdatedBy, revision.ContentHash = "", "unit-original", "old-hash"
	fake := &fakeMemoryExplorer{trace: memory.MemoryTrace{Current: &current, Revisions: []memory.MemoryRecord{revision}}}

	response := httptest.NewRecorder()
	request := httptest.NewRequest(http.MethodGet, "/api/memories/01A?scope=project", nil)
	memoryTestHandler(t, fake).ServeHTTP(response, request)
	if response.Code != http.StatusOK {
		t.Fatalf("status = %d, body = %s", response.Code, response.Body.String())
	}
	var payload MemoryTraceView
	if err := json.NewDecoder(response.Body).Decode(&payload); err != nil {
		t.Fatal(err)
	}
	if payload.Current == nil || payload.Current.Status != "current" || payload.Current.ContentHash != "current-hash" {
		t.Fatalf("current = %#v", payload.Current)
	}
	if len(payload.Revisions) != 1 || payload.Revisions[0].Status != "superseded" || payload.Revisions[0].Next != "01A.md" {
		t.Fatalf("revisions = %#v", payload.Revisions)
	}
}

func TestMemoryHandlerCreatesAndUpdatesThroughTheMemoryService(t *testing.T) {
	fake := &fakeMemoryExplorer{}
	handler := memoryTestHandler(t, fake)

	created := httptest.NewRecorder()
	createBody := []byte(`{"title":"New decision","body":"Use the direct table","type":"decision","tags":["storage"],"important":true}`)
	handler.ServeHTTP(created, httptest.NewRequest(http.MethodPost, "/api/memories?scope=project", bytes.NewReader(createBody)))
	if created.Code != http.StatusCreated || fake.addedTitle != "New decision" || fake.addedOpts.Type != memory.MemoryTypeDecision {
		t.Fatalf("create status = %d, fake = %#v, body = %s", created.Code, fake, created.Body.String())
	}

	updated := httptest.NewRecorder()
	updateBody := []byte(`{"title":"Refined decision","body":"Use one direct table","type":"decision","mandatory":true}`)
	handler.ServeHTTP(updated, httptest.NewRequest(http.MethodPatch, "/api/memories/01NEW?scope=project", bytes.NewReader(updateBody)))
	if updated.Code != http.StatusOK || fake.updatedID != "01NEW" || fake.updatedTitle != "Refined decision" || !fake.mandatory {
		t.Fatalf("update status = %d, fake = %#v, body = %s", updated.Code, fake, updated.Body.String())
	}
	var payload MemoryTraceView
	if err := json.NewDecoder(updated.Body).Decode(&payload); err != nil {
		t.Fatal(err)
	}
	if payload.Current == nil || payload.Current.Revision != 2 || len(payload.Revisions) != 1 {
		t.Fatalf("updated trace = %#v", payload)
	}
}

func TestMemoryHandlerRequiresExplicitRemovalConfirmation(t *testing.T) {
	fake := &fakeMemoryExplorer{}
	handler := memoryTestHandler(t, fake)

	unconfirmed := httptest.NewRecorder()
	handler.ServeHTTP(unconfirmed, httptest.NewRequest(http.MethodDelete, "/api/memories/01A?scope=project", nil))
	if unconfirmed.Code != http.StatusConflict || fake.removedID != "" {
		t.Fatalf("unconfirmed status = %d, removed = %q", unconfirmed.Code, fake.removedID)
	}

	confirmed := httptest.NewRecorder()
	handler.ServeHTTP(confirmed, httptest.NewRequest(http.MethodDelete, "/api/memories/01A?scope=project&confirm=true", nil))
	if confirmed.Code != http.StatusOK || fake.removedID != "01A" {
		t.Fatalf("confirmed status = %d, removed = %q", confirmed.Code, fake.removedID)
	}
}
