package uiserver

import (
	"context"
	"errors"
	"net/http"
	"strconv"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/pagination"
	graphtask "github.com/graphit-labs/graphit-code/internal/task"
)

type sessionExporter interface {
	SessionList(context.Context, graphtask.SessionListOptions) ([]graphtask.SessionSummary, error)
	SessionSearch(context.Context, string, int) ([]graphtask.SessionSearchResult, error)
	SessionGet(context.Context, string) (graphtask.SessionDetail, error)
}

type SessionHandler struct {
	defaultProjectDir string
	open              func(string) (sessionExporter, error)
	openRemote        func(context.Context, string) (sessionExporter, error)
}

func NewSessionHandler(defaultProjectDir string) *SessionHandler {
	return &SessionHandler{
		defaultProjectDir: defaultProjectDir,
		open: func(projectDir string) (sessionExporter, error) {
			return graphtask.Open(projectDir)
		},
		openRemote: func(ctx context.Context, projectID string) (sessionExporter, error) {
			return graphtask.OpenProject(ctx, projectID)
		},
	}
}

func (h *SessionHandler) RegisterAPIRoutes(mux *http.ServeMux) {
	mux.HandleFunc("GET /api/tasks/sessions", corsJSON(h.handleCatalog))
	mux.HandleFunc("GET /api/tasks/sessions/{id}", corsJSON(h.handleDetail))
}

// handleCatalog lists or, when query is set, searches sessions. Both paths return the same
// SessionSearchResult shape (score omitted for a plain list) so the frontend has one contract.
func (h *SessionHandler) handleCatalog(w http.ResponseWriter, r *http.Request) {
	scope, err := resolveProjectScope(r, h.defaultProjectDir)
	if err != nil {
		writeTaskError(w, http.StatusBadRequest, err.Error())
		return
	}
	pageSize := 0
	if value := strings.TrimSpace(r.URL.Query().Get("page_size")); value != "" {
		var err error
		pageSize, err = strconv.Atoi(value)
		if err != nil {
			writeTaskError(w, http.StatusBadRequest, "page_size must be an integer")
			return
		}
	}
	query := strings.TrimSpace(r.URL.Query().Get("query"))
	status := strings.TrimSpace(r.URL.Query().Get("status"))
	if status != "" && !graphtask.ValidStatus(status) {
		writeTaskError(w, http.StatusBadRequest, "invalid session status")
		return
	}
	active := strings.EqualFold(strings.TrimSpace(r.URL.Query().Get("active")), "true")
	window, err := pagination.Open(pagination.Spec{
		PageSize: pageSize,
		Cursor:   strings.TrimSpace(r.URL.Query().Get("cursor")),
		Bind: struct {
			Project, Query, Status string
			Active                 bool
		}{scope.Key(), query, status, active},
	})
	if err != nil {
		writeTaskError(w, http.StatusBadRequest, err.Error())
		return
	}
	service, err := h.openScope(r.Context(), scope)
	if err != nil {
		writeTaskError(w, taskOpenStatus(err), err.Error())
		return
	}
	var items []graphtask.SessionSearchResult
	if query != "" {
		items, err = service.SessionSearch(r.Context(), query, window.FetchLimit)
	} else {
		var summaries []graphtask.SessionSummary
		summaries, err = service.SessionList(r.Context(), graphtask.SessionListOptions{Status: status, Active: active})
		items = make([]graphtask.SessionSearchResult, len(summaries))
		for i, summary := range summaries {
			items[i] = graphtask.SessionSearchResult{SessionSummary: summary}
		}
	}
	if err != nil {
		writeTaskError(w, http.StatusInternalServerError, err.Error())
		return
	}
	writeJSON(w, pagination.Finish(window, items))
}

func (h *SessionHandler) handleDetail(w http.ResponseWriter, r *http.Request) {
	scope, err := resolveProjectScope(r, h.defaultProjectDir)
	if err != nil {
		writeTaskError(w, http.StatusBadRequest, err.Error())
		return
	}
	service, err := h.openScope(r.Context(), scope)
	if err != nil {
		writeTaskError(w, taskOpenStatus(err), err.Error())
		return
	}
	detail, err := service.SessionGet(r.Context(), strings.TrimSpace(r.PathValue("id")))
	if err != nil {
		status := http.StatusInternalServerError
		if errors.Is(err, graphtask.ErrSessionNotFound) {
			status = http.StatusNotFound
		}
		writeTaskError(w, status, err.Error())
		return
	}
	writeJSON(w, detail)
}

func (h *SessionHandler) openScope(ctx context.Context, scope ProjectScope) (sessionExporter, error) {
	if scope.Remote() {
		return h.openRemote(ctx, scope.ProjectID)
	}
	return h.open(scope.ProjectDir)
}
