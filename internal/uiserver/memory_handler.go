package uiserver

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"sort"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/store"
)

type memoryExplorerService interface {
	ListMemories() ([]memory.MemoryEntry, error)
	SearchMemories(context.Context, string, int, memory.SearchOptions) ([]memory.ChainResult, error)
	TraceMemory(context.Context, string) (memory.MemoryTrace, bool, error)
	AddMemory(string, string, memory.MemoryOpts) (string, error)
	UpdateMemoryTyped(string, string, string, string) error
	PromoteMemory(string) error
	DemoteMemory(string) error
	MarkMandatory(string) error
	UnmarkMandatory(string) error
	RemoveMemory(string) error
}

type MemoryHandler struct {
	defaultProjectDir string
	open              func(context.Context, string, string) (memoryExplorerService, error)
}

type MemoryScopeView struct {
	ID      string `json:"id"`
	Label   string `json:"label"`
	ScopeID string `json:"scope_id"`
	Kind    string `json:"kind"`
}

type MemoryCatalogItem struct {
	ID        string   `json:"id"`
	Title     string   `json:"title"`
	Type      string   `json:"type"`
	Tags      []string `json:"tags"`
	Important bool     `json:"important"`
	Mandatory bool     `json:"mandatory"`
	CreatedAt string   `json:"created_at"`
	UpdatedAt string   `json:"updated_at"`
	Revision  int      `json:"revision"`
	Snippet   string   `json:"snippet,omitempty"`
	Score     float64  `json:"score,omitempty"`
}

type MemoryCatalogResponse struct {
	Results []MemoryCatalogItem `json:"results"`
	Total   int                 `json:"total"`
	Types   []string            `json:"types"`
	Tags    []string            `json:"tags"`
}

type MemoryVersionView struct {
	Key         string   `json:"key"`
	ID          string   `json:"id"`
	RevisionID  string   `json:"revision_id,omitempty"`
	Status      string   `json:"status"`
	Title       string   `json:"title"`
	Body        string   `json:"body"`
	Type        string   `json:"type"`
	Tags        []string `json:"tags"`
	Important   bool     `json:"important"`
	Mandatory   bool     `json:"mandatory"`
	CreatedAt   string   `json:"created_at"`
	UpdatedAt   string   `json:"updated_at"`
	Revision    int      `json:"revision"`
	UpdatedBy   string   `json:"updated_by,omitempty"`
	Previous    string   `json:"previous,omitempty"`
	Next        string   `json:"next,omitempty"`
	Scope       string   `json:"scope"`
	ScopeID     string   `json:"scope_id"`
	ProjectID   string   `json:"project_id,omitempty"`
	ContentHash string   `json:"content_hash,omitempty"`
}

type MemoryTraceView struct {
	MemoryID  string              `json:"memory_id"`
	Current   *MemoryVersionView  `json:"current,omitempty"`
	Revisions []MemoryVersionView `json:"revisions"`
}

func NewMemoryHandler(defaultProjectDir string) *MemoryHandler {
	h := &MemoryHandler{defaultProjectDir: defaultProjectDir}
	h.open = h.openService
	return h
}

func (h *MemoryHandler) RegisterAPIRoutes(mux *http.ServeMux) {
	mux.HandleFunc("GET /api/memories/scopes", corsJSON(h.handleScopes))
	mux.HandleFunc("GET /api/memories", corsJSON(h.handleCatalog))
	mux.HandleFunc("POST /api/memories", corsJSON(h.handleCreate))
	mux.HandleFunc("GET /api/memories/{id}", corsJSON(h.handleDetail))
	mux.HandleFunc("PATCH /api/memories/{id}", corsJSON(h.handleUpdate))
	mux.HandleFunc("DELETE /api/memories/{id}", corsJSON(h.handleRemove))
}

func (h *MemoryHandler) projectDir(r *http.Request) string {
	projectDir := strings.TrimSpace(r.URL.Query().Get("project_dir"))
	if projectDir == "" {
		projectDir = h.defaultProjectDir
	}
	return projectDir
}

func memoryScope(r *http.Request) string {
	scope := strings.TrimSpace(r.URL.Query().Get("scope"))
	if scope == "" {
		return string(memory.MemoryScopeProject)
	}
	return scope
}

func (h *MemoryHandler) openService(ctx context.Context, projectDir, scope string) (memoryExplorerService, error) {
	memStore, err := memory.NewMemoryStore()
	if err != nil {
		return nil, err
	}
	switch memory.MemoryScope(scope) {
	case memory.MemoryScopeProject:
		projectID := store.ProjectID(projectDir)
		if projectID == "" {
			return nil, fmt.Errorf("project is not initialized")
		}
		return memory.NewMemoryService(memory.MemoryScopeProject, projectID, memStore).WithContext(ctx), nil
	case memory.MemoryScopeUser:
		userID, err := memory.UserScopeIDForContext(ctx)
		if err != nil {
			return nil, err
		}
		return memory.NewMemoryService(memory.MemoryScopeUser, userID, memStore).WithContext(ctx), nil
	default:
		return nil, fmt.Errorf("unsupported memory scope %q", scope)
	}
}

func (h *MemoryHandler) service(w http.ResponseWriter, r *http.Request) (memoryExplorerService, bool) {
	projectDir := h.projectDir(r)
	if projectDir == "" {
		writeMemoryError(w, http.StatusBadRequest, "project_dir is required")
		return nil, false
	}
	service, err := h.open(r.Context(), projectDir, memoryScope(r))
	if err != nil {
		writeMemoryError(w, http.StatusBadRequest, err.Error())
		return nil, false
	}
	return service, true
}

func (h *MemoryHandler) handleScopes(w http.ResponseWriter, r *http.Request) {
	projectDir := h.projectDir(r)
	projectID := store.ProjectID(projectDir)
	if projectID == "" {
		writeMemoryError(w, http.StatusBadRequest, "project is not initialized")
		return
	}
	userID, err := memory.UserScopeIDForContext(r.Context())
	if err != nil {
		writeMemoryError(w, http.StatusBadRequest, err.Error())
		return
	}
	writeJSON(w, []MemoryScopeView{
		{ID: "project", Label: projectDisplayName(projectDir), ScopeID: projectID, Kind: "project"},
		{ID: "user", Label: "User memory", ScopeID: userID, Kind: "user"},
	})
}

func (h *MemoryHandler) handleCatalog(w http.ResponseWriter, r *http.Request) {
	service, ok := h.service(w, r)
	if !ok {
		return
	}
	entries, err := service.ListMemories()
	if err != nil {
		writeMemoryError(w, http.StatusInternalServerError, err.Error())
		return
	}
	memory.SortMemoryEntries(entries)

	query := strings.TrimSpace(r.URL.Query().Get("query"))
	ranked := map[string]memory.ChainResult{}
	if query != "" {
		hits, err := service.SearchMemories(r.Context(), query, 0, memory.SearchOptions{Mode: "fts"})
		if err != nil {
			writeMemoryError(w, http.StatusInternalServerError, err.Error())
			return
		}
		for _, hit := range hits {
			ranked[hit.MemoryID] = hit
		}
	}

	typeFilter := strings.TrimSpace(r.URL.Query().Get("type"))
	tagFilter := strings.TrimSpace(r.URL.Query().Get("tag"))
	important, importantSet, valid := boolFilter(r.URL.Query().Get("important"))
	if !valid {
		writeMemoryError(w, http.StatusBadRequest, "important must be true or false")
		return
	}
	mandatory, mandatorySet, valid := boolFilter(r.URL.Query().Get("mandatory"))
	if !valid {
		writeMemoryError(w, http.StatusBadRequest, "mandatory must be true or false")
		return
	}

	types := map[string]struct{}{}
	tags := map[string]struct{}{}
	results := make([]MemoryCatalogItem, 0, len(entries))
	for _, entry := range entries {
		if entry.Type != "" {
			types[string(entry.Type)] = struct{}{}
		}
		for _, tag := range entry.Tags {
			if tag != "" {
				tags[tag] = struct{}{}
			}
		}
		hit, matched := ranked[entry.ID]
		if query != "" && !matched {
			continue
		}
		if typeFilter != "" && typeFilter != "all" && !strings.EqualFold(string(entry.Type), typeFilter) {
			continue
		}
		if tagFilter != "" && tagFilter != "all" && !containsFold(entry.Tags, tagFilter) {
			continue
		}
		if importantSet && entry.Important != important || mandatorySet && entry.Mandatory != mandatory {
			continue
		}
		results = append(results, MemoryCatalogItem{
			ID: entry.ID, Title: entry.Title, Type: string(entry.Type), Tags: entry.Tags,
			Important: entry.Important, Mandatory: entry.Mandatory, CreatedAt: entry.CreatedAt,
			UpdatedAt: entry.UpdatedAt, Revision: entry.Revision, Snippet: hit.Snippet, Score: hit.Score,
		})
	}
	writeJSON(w, MemoryCatalogResponse{Results: results, Total: len(results), Types: sortedKeys(types), Tags: sortedKeys(tags)})
}

func (h *MemoryHandler) handleDetail(w http.ResponseWriter, r *http.Request) {
	service, ok := h.service(w, r)
	if !ok {
		return
	}
	h.writeTrace(w, r, service, strings.TrimSpace(r.PathValue("id")), http.StatusOK)
}

func (h *MemoryHandler) handleCreate(w http.ResponseWriter, r *http.Request) {
	service, ok := h.service(w, r)
	if !ok {
		return
	}
	var body struct {
		Title     string   `json:"title"`
		Body      string   `json:"body"`
		Type      string   `json:"type"`
		Tags      []string `json:"tags"`
		Important bool     `json:"important"`
		Mandatory bool     `json:"mandatory"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		writeMemoryError(w, http.StatusBadRequest, "invalid JSON body")
		return
	}
	if strings.TrimSpace(body.Title) == "" || strings.TrimSpace(body.Body) == "" {
		writeMemoryError(w, http.StatusBadRequest, "title and body are required")
		return
	}
	if body.Type != "" && !memory.ValidMemoryType(body.Type) {
		writeMemoryError(w, http.StatusBadRequest, "invalid memory type")
		return
	}
	projectID := ""
	if memoryScope(r) == string(memory.MemoryScopeUser) {
		projectID = store.ProjectID(h.projectDir(r))
	}
	id, err := service.AddMemory(body.Title, body.Body, memory.MemoryOpts{
		ProjectID: projectID, Important: body.Important, Mandatory: body.Mandatory,
		Type: memory.MemoryType(body.Type), Tags: body.Tags,
	})
	if err != nil {
		writeMemoryError(w, http.StatusInternalServerError, err.Error())
		return
	}
	h.writeTrace(w, r, service, id, http.StatusCreated)
}

func (h *MemoryHandler) handleUpdate(w http.ResponseWriter, r *http.Request) {
	service, ok := h.service(w, r)
	if !ok {
		return
	}
	id := strings.TrimSpace(r.PathValue("id"))
	trace, found, err := service.TraceMemory(r.Context(), id)
	if err != nil {
		writeMemoryError(w, http.StatusInternalServerError, err.Error())
		return
	}
	if !found || trace.Current == nil {
		writeMemoryError(w, http.StatusNotFound, "memory not found")
		return
	}
	var body struct {
		Title     *string `json:"title"`
		Body      *string `json:"body"`
		Type      *string `json:"type"`
		Important *bool   `json:"important"`
		Mandatory *bool   `json:"mandatory"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		writeMemoryError(w, http.StatusBadRequest, "invalid JSON body")
		return
	}
	if body.Type != nil && !memory.ValidMemoryType(*body.Type) {
		writeMemoryError(w, http.StatusBadRequest, "invalid memory type")
		return
	}
	if body.Title != nil || body.Body != nil || body.Type != nil {
		title, content, memType := "", "", ""
		if body.Title != nil {
			title = *body.Title
		}
		if body.Body != nil {
			content = *body.Body
		}
		if body.Type != nil {
			memType = *body.Type
		}
		if err := service.UpdateMemoryTyped(id, title, content, memType); err != nil {
			writeMemoryError(w, http.StatusInternalServerError, err.Error())
			return
		}
	}
	if body.Important != nil && *body.Important != trace.Current.Important {
		if *body.Important {
			err = service.PromoteMemory(id)
		} else {
			err = service.DemoteMemory(id)
		}
		if err != nil {
			writeMemoryError(w, http.StatusInternalServerError, err.Error())
			return
		}
	}
	if body.Mandatory != nil && *body.Mandatory != trace.Current.Mandatory {
		if *body.Mandatory {
			err = service.MarkMandatory(id)
		} else {
			err = service.UnmarkMandatory(id)
		}
		if err != nil {
			writeMemoryError(w, http.StatusInternalServerError, err.Error())
			return
		}
	}
	h.writeTrace(w, r, service, id, http.StatusOK)
}

func (h *MemoryHandler) handleRemove(w http.ResponseWriter, r *http.Request) {
	if r.URL.Query().Get("confirm") != "true" {
		writeMemoryError(w, http.StatusConflict, "removal requires confirm=true")
		return
	}
	service, ok := h.service(w, r)
	if !ok {
		return
	}
	id := strings.TrimSpace(r.PathValue("id"))
	if err := service.RemoveMemory(id); err != nil {
		writeMemoryError(w, http.StatusInternalServerError, err.Error())
		return
	}
	writeJSON(w, map[string]any{"id": id, "removed": true})
}

func (h *MemoryHandler) writeTrace(w http.ResponseWriter, r *http.Request, service memoryExplorerService, id string, status int) {
	if id == "" {
		writeMemoryError(w, http.StatusBadRequest, "memory id is required")
		return
	}
	trace, found, err := service.TraceMemory(r.Context(), id)
	if err != nil {
		writeMemoryError(w, http.StatusInternalServerError, err.Error())
		return
	}
	if !found {
		writeMemoryError(w, http.StatusNotFound, "memory not found")
		return
	}
	view := MemoryTraceView{MemoryID: id, Revisions: make([]MemoryVersionView, 0, len(trace.Revisions))}
	if trace.Current != nil {
		current := memoryVersionView(*trace.Current)
		view.Current = &current
	}
	for _, revision := range trace.Revisions {
		view.Revisions = append(view.Revisions, memoryVersionView(revision))
	}
	w.WriteHeader(status)
	writeJSON(w, view)
}

func memoryVersionView(record memory.MemoryRecord) MemoryVersionView {
	status := "current"
	if record.Superseded || record.RevisionID != "" {
		status = "superseded"
	}
	return MemoryVersionView{
		Key: record.Key(), ID: record.ID, RevisionID: record.RevisionID, Status: status,
		Title: record.Title, Body: record.Body, Type: record.Type, Tags: record.Tags,
		Important: record.Important, Mandatory: record.Mandatory, CreatedAt: record.CreatedAt,
		UpdatedAt: record.UpdatedAt, Revision: record.Revision, UpdatedBy: record.UpdatedBy,
		Previous: record.Previous, Next: record.Next, Scope: record.Scope, ScopeID: record.ScopeID,
		ProjectID: record.ProjectID, ContentHash: record.ContentHash,
	}
}

func boolFilter(value string) (bool, bool, bool) {
	switch strings.TrimSpace(value) {
	case "", "all":
		return false, false, true
	case "true":
		return true, true, true
	case "false":
		return false, true, true
	default:
		return false, false, false
	}
}

func containsFold(values []string, want string) bool {
	for _, value := range values {
		if strings.EqualFold(value, want) {
			return true
		}
	}
	return false
}

func sortedKeys(values map[string]struct{}) []string {
	out := make([]string, 0, len(values))
	for value := range values {
		out = append(out, value)
	}
	sort.Strings(out)
	return out
}

func writeMemoryError(w http.ResponseWriter, status int, message string) {
	w.WriteHeader(status)
	writeJSON(w, map[string]string{"error": message})
}
