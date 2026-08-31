package uiserver

import (
	"encoding/json"
	"fmt"
	"net/http"

	"github.com/graphit-labs/graphit-code/internal/backlog"
)

type DaemonBacklogHandler struct{}

func NewDaemonBacklogHandler() *DaemonBacklogHandler {
	return &DaemonBacklogHandler{}
}

func (h *DaemonBacklogHandler) RegisterAPIRoutes(mux *http.ServeMux) {
	mux.HandleFunc("GET /api/backlog", corsJSON(h.handleBacklogList))
	mux.HandleFunc("POST /api/backlog/item", corsJSON(h.handleBacklogAdd))
	mux.HandleFunc("DELETE /api/backlog/item/{slug}", corsJSON(h.handleBacklogRemove))
}

func (h *DaemonBacklogHandler) handleBacklogList(w http.ResponseWriter, r *http.Request) {
	projectDir := r.URL.Query().Get("project_dir")
	if projectDir == "" {
		http.Error(w, "project_dir required", http.StatusBadRequest)
		return
	}

	items, err := backlog.List(projectDir)
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
	if items == nil {
		items = []backlog.Item{}
	}

	writeJSON(w, items)
}

func (h *DaemonBacklogHandler) handleBacklogAdd(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "POST only", http.StatusMethodNotAllowed)
		return
	}

	projectDir := r.URL.Query().Get("project_dir")
	if projectDir == "" {
		http.Error(w, "project_dir required", http.StatusBadRequest)
		return
	}

	var body struct {
		Title string `json:"title"`
		Body  string `json:"body"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		http.Error(w, "invalid request body", http.StatusBadRequest)
		return
	}

	if body.Title == "" {
		http.Error(w, "title is required", http.StatusBadRequest)
		return
	}

	item, err := backlog.Add(projectDir, body.Title, body.Body)
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}

	writeJSON(w, item)
}

func (h *DaemonBacklogHandler) handleBacklogRemove(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodDelete {
		http.Error(w, "DELETE only", http.StatusMethodNotAllowed)
		return
	}

	projectDir := r.URL.Query().Get("project_dir")
	if projectDir == "" {
		http.Error(w, "project_dir required", http.StatusBadRequest)
		return
	}

	slug := r.PathValue("slug")
	if slug == "" {
		http.Error(w, "slug is required", http.StatusBadRequest)
		return
	}

	if err := backlog.Remove(projectDir, slug); err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}

	writeJSON(w, map[string]any{"success": true, "message": fmt.Sprintf("Backlog item %q removed.", slug)})
}
