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
}

func NewSessionHandler(defaultProjectDir string) *SessionHandler {
	return &SessionHandler{
		defaultProjectDir: defaultProjectDir,
		open: func(projectDir string) (sessionExporter, error) {
			return graphtask.Open(projectDir)
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
	projectDir := h.projectDir(r)
	if projectDir == "" {
		writeTaskError(w, http.StatusBadRequest, "project_dir is required")
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
			ProjectDir, Query, Status string
			Active                    bool
		}{projectDir, query, status, active},
	})
	if err != nil {
		writeTaskError(w, http.StatusBadRequest, err.Error())
		return
	}
	service, err := h.open(projectDir)
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
	projectDir := h.projectDir(r)
	if projectDir == "" {
		writeTaskError(w, http.StatusBadRequest, "project_dir is required")
		return
	}
	service, err := h.open(projectDir)
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

func (h *SessionHandler) projectDir(r *http.Request) string {
	projectDir := strings.TrimSpace(r.URL.Query().Get("project_dir"))
	if projectDir == "" {
		projectDir = h.defaultProjectDir
	}
	return projectDir
}
