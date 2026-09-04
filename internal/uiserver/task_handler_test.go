package uiserver

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"testing"

	graphtask "github.com/graphit-labs/graphit-code/internal/task"
)

type fakeTaskExporter struct {
	document   graphtask.ExportDocument
	catalog    []graphtask.CatalogItem
	err        error
	catalogErr error
	gotID      string
	gotCatalog graphtask.CatalogOptions
}

func (f *fakeTaskExporter) Export(_ context.Context, id string) (graphtask.ExportDocument, error) {
	f.gotID = id
	return f.document, f.err
}

func (f *fakeTaskExporter) Catalog(_ context.Context, opts graphtask.CatalogOptions) ([]graphtask.CatalogItem, error) {
	f.gotCatalog = opts
	return f.catalog, f.catalogErr
}

func TestTaskHandlerPaginatesCatalogAndBindsCursor(t *testing.T) {
	fake := &fakeTaskExporter{catalog: []graphtask.CatalogItem{
		{ID: "tsk-a", Title: "First"},
		{ID: "tsk-b", Title: "Second"},
		{ID: "tsk-c", Title: "Third"},
	}}
	handler := NewTaskHandler("/project")
	handler.open = func(string) (taskExporter, error) { return fake, nil }
	mux := http.NewServeMux()
	handler.RegisterAPIRoutes(mux)

	first := httptest.NewRecorder()
	mux.ServeHTTP(first, httptest.NewRequest(http.MethodGet, "/api/tasks?page_size=2&query=work&status=open", nil))
	if first.Code != http.StatusOK {
		t.Fatalf("first status = %d, body = %s", first.Code, first.Body.String())
	}
	var firstPage struct {
		Results    []graphtask.CatalogItem `json:"results"`
		NextCursor string                  `json:"next_cursor"`
	}
	if err := json.NewDecoder(first.Body).Decode(&firstPage); err != nil {
		t.Fatal(err)
	}
	if len(firstPage.Results) != 2 || firstPage.NextCursor == "" {
		t.Fatalf("first page = %#v", firstPage)
	}
	if fake.gotCatalog.Query != "work" || fake.gotCatalog.Status != "open" {
		t.Fatalf("catalog options = %#v", fake.gotCatalog)
	}

	second := httptest.NewRecorder()
	mux.ServeHTTP(second, httptest.NewRequest(http.MethodGet, "/api/tasks?page_size=2&query=work&status=open&cursor="+firstPage.NextCursor, nil))
	if second.Code != http.StatusOK {
		t.Fatalf("second status = %d, body = %s", second.Code, second.Body.String())
	}
	var secondPage struct {
		Results    []graphtask.CatalogItem `json:"results"`
		NextCursor string                  `json:"next_cursor"`
	}
	if err := json.NewDecoder(second.Body).Decode(&secondPage); err != nil {
		t.Fatal(err)
	}
	if len(secondPage.Results) != 1 || secondPage.Results[0].ID != "tsk-c" || secondPage.NextCursor != "" {
		t.Fatalf("second page = %#v", secondPage)
	}

	mismatch := httptest.NewRecorder()
	mux.ServeHTTP(mismatch, httptest.NewRequest(http.MethodGet, "/api/tasks?page_size=2&query=changed&status=open&cursor="+firstPage.NextCursor, nil))
	if mismatch.Code != http.StatusBadRequest {
		t.Fatalf("mismatched cursor status = %d", mismatch.Code)
	}
	invalid := httptest.NewRecorder()
	mux.ServeHTTP(invalid, httptest.NewRequest(http.MethodGet, "/api/tasks?status=unknown", nil))
	if invalid.Code != http.StatusBadRequest {
		t.Fatalf("invalid status = %d", invalid.Code)
	}
}

func TestTaskHandlerExportsCanonicalDocument(t *testing.T) {
	fake := &fakeTaskExporter{document: graphtask.ExportDocument{
		SchemaVersion: graphtask.ExportSchemaVersion,
		ProjectID:     "project-1",
		TaskID:        "tsk-abcd",
		Tasks:         []graphtask.Task{{ID: "tsk-abcd", Title: "Exported task"}},
		Dependencies:  []graphtask.DependencyRecord{},
		Checks:        []graphtask.CheckRecord{},
		Events:        []graphtask.Event{},
		Comments:      []graphtask.Comment{},
		SpecRevisions: []graphtask.SpecRevision{},
	}}
	handler := NewTaskHandler("/default/project")
	handler.open = func(projectDir string) (taskExporter, error) {
		if projectDir != "/selected/project" {
			t.Fatalf("project dir = %q", projectDir)
		}
		return fake, nil
	}
	mux := http.NewServeMux()
	handler.RegisterAPIRoutes(mux)

	request := httptest.NewRequest(http.MethodGet, "/api/tasks/export?project_dir=%2Fselected%2Fproject&id=tsk-abcd", nil)
	response := httptest.NewRecorder()
	mux.ServeHTTP(response, request)
	if response.Code != http.StatusOK {
		t.Fatalf("status = %d, body = %s", response.Code, response.Body.String())
	}
	if fake.gotID != "tsk-abcd" {
		t.Fatalf("export id = %q", fake.gotID)
	}
	var document graphtask.ExportDocument
	if err := json.NewDecoder(response.Body).Decode(&document); err != nil {
		t.Fatal(err)
	}
	if document.SchemaVersion != graphtask.ExportSchemaVersion || len(document.Tasks) != 1 {
		t.Fatalf("document = %#v", document)
	}
}

func TestTaskHandlerUsesDefaultProjectAndMapsMissingTask(t *testing.T) {
	fake := &fakeTaskExporter{err: graphtask.ErrNotFound}
	handler := NewTaskHandler("/default/project")
	handler.open = func(projectDir string) (taskExporter, error) {
		if projectDir != "/default/project" {
			t.Fatalf("project dir = %q", projectDir)
		}
		return fake, nil
	}
	mux := http.NewServeMux()
	handler.RegisterAPIRoutes(mux)

	request := httptest.NewRequest(http.MethodGet, "/api/tasks/export?id=tsk-missing", nil)
	response := httptest.NewRecorder()
	mux.ServeHTTP(response, request)
	if response.Code != http.StatusNotFound {
		t.Fatalf("status = %d, body = %s", response.Code, response.Body.String())
	}
	var payload map[string]string
	if err := json.NewDecoder(response.Body).Decode(&payload); err != nil {
		t.Fatal(err)
	}
	if !errors.Is(fake.err, graphtask.ErrNotFound) || payload["error"] == "" {
		t.Fatalf("error payload = %#v", payload)
	}
}
