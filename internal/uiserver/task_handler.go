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

type taskExporter interface {
	Export(context.Context, string) (graphtask.ExportDocument, error)
	Catalog(context.Context, graphtask.CatalogOptions) ([]graphtask.CatalogItem, error)
}

type TaskHandler struct {
	defaultProjectDir string
	open              func(string) (taskExporter, error)
}

func NewTaskHandler(defaultProjectDir string) *TaskHandler {
	return &TaskHandler{
		defaultProjectDir: defaultProjectDir,
		open: func(projectDir string) (taskExporter, error) {
			return graphtask.Open(projectDir)
		},
	}
}

func (h *TaskHandler) RegisterAPIRoutes(mux *http.ServeMux) {
	mux.HandleFunc("GET /api/tasks", corsJSON(h.handleCatalog))
	mux.HandleFunc("GET /api/tasks/export", corsJSON(h.handleExport))
}

func (h *TaskHandler) handleCatalog(w http.ResponseWriter, r *http.Request) {
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
	if status != "" && status != "blocked" && status != "flagged" && !graphtask.ValidStatus(status) {
		writeTaskError(w, http.StatusBadRequest, "invalid task status")
		return
	}
	window, err := pagination.Open(pagination.Spec{
		PageSize: pageSize,
		Cursor:   strings.TrimSpace(r.URL.Query().Get("cursor")),
		Bind: struct {
			ProjectDir string `json:"project_dir"`
			Query      string `json:"query"`
			Status     string `json:"status"`
		}{projectDir, query, status},
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
	items, err := service.Catalog(r.Context(), graphtask.CatalogOptions{Query: query, Status: status})
	if err != nil {
		writeTaskError(w, http.StatusInternalServerError, err.Error())
		return
	}
	writeJSON(w, pagination.Finish(window, items))
}

func (h *TaskHandler) handleExport(w http.ResponseWriter, r *http.Request) {
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
	document, err := service.Export(r.Context(), strings.TrimSpace(r.URL.Query().Get("id")))
	if err != nil {
		status := http.StatusInternalServerError
		if errors.Is(err, graphtask.ErrNotFound) {
			status = http.StatusNotFound
		}
		writeTaskError(w, status, err.Error())
		return
	}
	writeJSON(w, document)
}

func (h *TaskHandler) projectDir(r *http.Request) string {
	projectDir := strings.TrimSpace(r.URL.Query().Get("project_dir"))
	if projectDir == "" {
		projectDir = h.defaultProjectDir
	}
	return projectDir
}

func taskOpenStatus(err error) int {
	if errors.Is(err, graphtask.ErrDisabled) {
		return http.StatusNotFound
	}
	return http.StatusBadRequest
}

func writeTaskError(w http.ResponseWriter, status int, message string) {
	w.WriteHeader(status)
	writeJSON(w, map[string]string{"error": message})
}
