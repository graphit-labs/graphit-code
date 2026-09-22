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

type fakeSessionExporter struct {
	summaries    []graphtask.SessionSummary
	searchResult []graphtask.SessionSearchResult
	detail       graphtask.SessionDetail
	listErr      error
	searchErr    error
	detailErr    error
	gotList      graphtask.SessionListOptions
	gotQuery     string
	gotLimit     int
	gotID        string
}

func (f *fakeSessionExporter) SessionList(_ context.Context, opts graphtask.SessionListOptions) ([]graphtask.SessionSummary, error) {
	f.gotList = opts
	return f.summaries, f.listErr
}

func (f *fakeSessionExporter) SessionSearch(_ context.Context, query string, limit int) ([]graphtask.SessionSearchResult, error) {
	f.gotQuery = query
	f.gotLimit = limit
	return f.searchResult, f.searchErr
}

func (f *fakeSessionExporter) SessionGet(_ context.Context, id string) (graphtask.SessionDetail, error) {
	f.gotID = id
	return f.detail, f.detailErr
}

func TestSessionHandlerPaginatesListAndBindsCursor(t *testing.T) {
	fake := &fakeSessionExporter{summaries: []graphtask.SessionSummary{
		{ID: "ses-newest", Title: "Newest"},
		{ID: "ses-middle", Title: "Middle"},
		{ID: "ses-oldest", Title: "Oldest"},
	}}
	handler := NewSessionHandler("/project")
	handler.open = func(string) (sessionExporter, error) { return fake, nil }
	mux := http.NewServeMux()
	handler.RegisterAPIRoutes(mux)

	first := httptest.NewRecorder()
	mux.ServeHTTP(first, httptest.NewRequest(http.MethodGet, "/api/tasks/sessions?page_size=2&status=open&active=true", nil))
	if first.Code != http.StatusOK {
		t.Fatalf("first status = %d, body = %s", first.Code, first.Body.String())
	}
	var firstPage struct {
		Results    []graphtask.SessionSearchResult `json:"results"`
		NextCursor string                          `json:"next_cursor"`
	}
	if err := json.NewDecoder(first.Body).Decode(&firstPage); err != nil {
		t.Fatal(err)
	}
	if len(firstPage.Results) != 2 || firstPage.Results[0].ID != "ses-newest" || firstPage.Results[1].ID != "ses-middle" || firstPage.NextCursor == "" {
		t.Fatalf("first page = %#v", firstPage)
	}
	if fake.gotList.Status != "open" || !fake.gotList.Active {
		t.Fatalf("list options = %#v", fake.gotList)
	}

	second := httptest.NewRecorder()
	mux.ServeHTTP(second, httptest.NewRequest(http.MethodGet, "/api/tasks/sessions?page_size=2&status=open&active=true&cursor="+firstPage.NextCursor, nil))
	if second.Code != http.StatusOK {
		t.Fatalf("second status = %d, body = %s", second.Code, second.Body.String())
	}
	var secondPage struct {
		Results    []graphtask.SessionSearchResult `json:"results"`
		NextCursor string                          `json:"next_cursor"`
	}
	if err := json.NewDecoder(second.Body).Decode(&secondPage); err != nil {
		t.Fatal(err)
	}
	if len(secondPage.Results) != 1 || secondPage.Results[0].ID != "ses-oldest" || secondPage.NextCursor != "" {
		t.Fatalf("second page = %#v", secondPage)
	}

	invalid := httptest.NewRecorder()
	mux.ServeHTTP(invalid, httptest.NewRequest(http.MethodGet, "/api/tasks/sessions?status=unknown", nil))
	if invalid.Code != http.StatusBadRequest {
		t.Fatalf("invalid status = %d", invalid.Code)
	}
}

func TestSessionHandlerOpensRemoteProjectByID(t *testing.T) {
	handler := NewSessionHandler("/local/default")
	handler.openRemote = func(_ context.Context, projectID string) (sessionExporter, error) {
		if projectID != remoteProjectID {
			t.Fatalf("project id = %q", projectID)
		}
		return &fakeSessionExporter{}, nil
	}
	handler.open = func(string) (sessionExporter, error) {
		t.Fatal("remote request fell back to a local project")
		return nil, nil
	}
	mux := http.NewServeMux()
	handler.RegisterAPIRoutes(mux)
	response := httptest.NewRecorder()
	mux.ServeHTTP(response, httptest.NewRequest(http.MethodGet, "/api/tasks/sessions?project_id="+remoteProjectID, nil))
	if response.Code != http.StatusOK {
		t.Fatalf("status = %d, body = %s", response.Code, response.Body.String())
	}
}

func TestSessionHandlerSearchesWhenQueryIsSet(t *testing.T) {
	fake := &fakeSessionExporter{searchResult: []graphtask.SessionSearchResult{
		{SessionSummary: graphtask.SessionSummary{ID: "ses-a", Title: "Archive export"}, Score: 0.9},
	}}
	handler := NewSessionHandler("/project")
	handler.open = func(string) (sessionExporter, error) { return fake, nil }
	mux := http.NewServeMux()
	handler.RegisterAPIRoutes(mux)

	response := httptest.NewRecorder()
	mux.ServeHTTP(response, httptest.NewRequest(http.MethodGet, "/api/tasks/sessions?query=archive", nil))
	if response.Code != http.StatusOK {
		t.Fatalf("status = %d, body = %s", response.Code, response.Body.String())
	}
	if fake.gotQuery != "archive" || fake.gotLimit <= 0 {
		t.Fatalf("search args = %q, %d", fake.gotQuery, fake.gotLimit)
	}
	var page struct {
		Results []graphtask.SessionSearchResult `json:"results"`
	}
	if err := json.NewDecoder(response.Body).Decode(&page); err != nil {
		t.Fatal(err)
	}
	if len(page.Results) != 1 || page.Results[0].ID != "ses-a" || page.Results[0].Score != 0.9 {
		t.Fatalf("page = %#v", page)
	}
}

func TestSessionHandlerReturnsDetail(t *testing.T) {
	fake := &fakeSessionExporter{detail: graphtask.SessionDetail{
		Session: graphtask.Session{ID: "ses-abcd", Title: "Detailed session"},
		Tasks:   []graphtask.CatalogItem{{ID: "tsk-1"}},
	}}
	handler := NewSessionHandler("/default/project")
	handler.open = func(projectDir string) (sessionExporter, error) {
		if projectDir != "/selected/project" {
			t.Fatalf("project dir = %q", projectDir)
		}
		return fake, nil
	}
	mux := http.NewServeMux()
	handler.RegisterAPIRoutes(mux)

	request := httptest.NewRequest(http.MethodGet, "/api/tasks/sessions/ses-abcd?project_dir=%2Fselected%2Fproject", nil)
	response := httptest.NewRecorder()
	mux.ServeHTTP(response, request)
	if response.Code != http.StatusOK {
		t.Fatalf("status = %d, body = %s", response.Code, response.Body.String())
	}
	if fake.gotID != "ses-abcd" {
		t.Fatalf("detail id = %q", fake.gotID)
	}
	var detail graphtask.SessionDetail
	if err := json.NewDecoder(response.Body).Decode(&detail); err != nil {
		t.Fatal(err)
	}
	if detail.Session.ID != "ses-abcd" || len(detail.Tasks) != 1 {
		t.Fatalf("detail = %#v", detail)
	}
}

func TestSessionHandlerMapsMissingSession(t *testing.T) {
	fake := &fakeSessionExporter{detailErr: graphtask.ErrSessionNotFound}
	handler := NewSessionHandler("/default/project")
	handler.open = func(string) (sessionExporter, error) { return fake, nil }
	mux := http.NewServeMux()
	handler.RegisterAPIRoutes(mux)

	request := httptest.NewRequest(http.MethodGet, "/api/tasks/sessions/ses-missing", nil)
	response := httptest.NewRecorder()
	mux.ServeHTTP(response, request)
	if response.Code != http.StatusNotFound {
		t.Fatalf("status = %d, body = %s", response.Code, response.Body.String())
	}
	var payload map[string]string
	if err := json.NewDecoder(response.Body).Decode(&payload); err != nil {
		t.Fatal(err)
	}
	if !errors.Is(fake.detailErr, graphtask.ErrSessionNotFound) || payload["error"] == "" {
		t.Fatalf("error payload = %#v", payload)
	}
}

func TestSessionHandlerRequiresProjectDir(t *testing.T) {
	handler := NewSessionHandler("")
	handler.open = func(string) (sessionExporter, error) { return &fakeSessionExporter{}, nil }
	mux := http.NewServeMux()
	handler.RegisterAPIRoutes(mux)

	catalog := httptest.NewRecorder()
	mux.ServeHTTP(catalog, httptest.NewRequest(http.MethodGet, "/api/tasks/sessions", nil))
	if catalog.Code != http.StatusBadRequest {
		t.Fatalf("catalog status = %d", catalog.Code)
	}
	detail := httptest.NewRecorder()
	mux.ServeHTTP(detail, httptest.NewRequest(http.MethodGet, "/api/tasks/sessions/ses-1", nil))
	if detail.Code != http.StatusBadRequest {
		t.Fatalf("detail status = %d", detail.Code)
	}
}
