package uiserver

import (
	"context"
	"fmt"
	"net/http"
	"net/url"
	"sort"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/references"
	"github.com/graphit-labs/graphit-code/internal/relations"
	"github.com/graphit-labs/graphit-code/internal/store"
)

type referenceTarget struct {
	Kind      string `json:"kind"`
	ID        string `json:"id"`
	Title     string `json:"title"`
	ProjectID string `json:"project_id"`
	Scope     string `json:"scope,omitempty"`
	ScopeID   string `json:"scope_id,omitempty"`
	Context   string `json:"context,omitempty"`
	Href      string `json:"href,omitempty"`
}
type referenceEdge struct {
	Source   referenceTarget `json:"source"`
	Target   referenceTarget `json:"target"`
	Relation string          `json:"relation"`
	Fields   []string        `json:"fields"`
}
type referenceView struct {
	Source   referenceTarget `json:"source"`
	Outgoing []referenceEdge `json:"outgoing"`
	Incoming []referenceEdge `json:"incoming"`
	Complete bool            `json:"complete"`
	Warnings []string        `json:"warnings"`
}

func referenceHref(kind, id, scope, contextName string) string {
	switch kind {
	case "task":
		return "/task/explorer/" + url.PathEscape(id)
	case "session":
		return "/task/sessions/" + url.PathEscape(id)
	case "memory":
		return "/memory/explorer/" + url.PathEscape(scope) + "/" + url.PathEscape(id)
	default:
		route := "/knowledge/explorer"
		if contextName != "" && contextName != "project" {
			route += "/" + url.PathEscape(contextName)
		}
		return route + "?page=" + url.QueryEscape(id)
	}
}
func referenceIdentity(v referenceTarget) relations.Entity {
	contextName := v.Context
	if contextName == "project" {
		contextName = ""
	}
	return relations.Entity{Type: v.Kind, ID: v.ID, Scope: v.Scope, ScopeID: v.ScopeID, Context: contextName}
}

// ReadRecordRelations joins persisted edges with an authorized current catalogue.
// No Markdown/prose parser or relation write runs in this read path. Unresolved
// targets retain their identity for analysis, but receive no navigable URL.
func ReadRecordRelations(ctx context.Context, dir string) ([]referenceTarget, []relations.Edge, []string) {
	snapshot := references.Read(ctx, dir, true)
	records := []referenceTarget{}
	for _, r := range snapshot.Records {
		e := r.Entity
		contextName := e.Context
		if e.Type == "knowledge" && contextName == "" {
			contextName = "project"
		}
		records = append(records, referenceTarget{e.Type, e.ID, r.Title, store.ProjectID(dir), e.Scope, e.ScopeID, contextName, referenceHref(e.Type, e.ID, e.Scope, contextName)})
	}
	return records, snapshot.Edges, snapshot.Warnings
}

func projectReferences(records []referenceTarget, persisted []relations.Edge, selected referenceTarget, warnings []string) referenceView {
	view := referenceView{Source: selected, Outgoing: []referenceEdge{}, Incoming: []referenceEdge{}, Complete: len(warnings) == 0, Warnings: warnings}
	catalog := map[string]referenceTarget{}
	for _, r := range records {
		catalog[referenceIdentity(r).Key()] = r
	}
	selectedKey := referenceIdentity(selected).Key()
	resolve := func(e relations.Entity) referenceTarget {
		if r, ok := catalog[e.Key()]; ok {
			return r
		}
		return referenceTarget{Kind: e.Type, ID: e.ID, Scope: e.Scope, ScopeID: e.ScopeID, Context: e.Context, Title: e.ID}
	}
	for _, edge := range persisted {
		source, ok := catalog[edge.Source.Key()]
		if !ok {
			continue
		}
		item := referenceEdge{source, resolve(edge.Target), edge.Relation, []string{edge.Field}}
		if edge.Source.Key() == selectedKey {
			view.Outgoing = append(view.Outgoing, item)
		}
		if edge.Target.Key() == selectedKey {
			view.Incoming = append(view.Incoming, item)
		}
	}
	sort.Slice(view.Outgoing, func(i, j int) bool { return view.Outgoing[i].Target.Title < view.Outgoing[j].Target.Title })
	sort.Slice(view.Incoming, func(i, j int) bool { return view.Incoming[i].Source.Title < view.Incoming[j].Source.Title })
	return view
}
func (h *TaskHandler) handleReferences(w http.ResponseWriter, r *http.Request) {
	q := r.URL.Query()
	dir := h.projectDir(r)
	kind, id := q.Get("kind"), q.Get("id")
	if kind == "knowledge" {
		id = strings.TrimSuffix(id, ".md")
	}
	if dir == "" || id == "" || (kind != "task" && kind != "session" && kind != "memory" && kind != "knowledge") {
		writeTaskError(w, 400, "project_dir, kind and id are required")
		return
	}
	records, edges, warnings := ReadRecordRelations(r.Context(), dir)
	scope, contextName := q.Get("scope"), q.Get("context")
	if scope == "" {
		scope = "project"
	}
	if contextName == "" {
		contextName = "project"
	}
	for _, record := range records {
		if record.Kind == kind && record.ID == id && (kind != "memory" || record.Scope == scope) && (kind != "knowledge" || record.Context == contextName) {
			writeJSON(w, projectReferences(records, edges, record, warnings))
			return
		}
	}
	status := http.StatusNotFound
	if len(warnings) > 0 {
		status = http.StatusServiceUnavailable
	}
	writeTaskError(w, status, fmt.Sprintf("Reference source unavailable: %s %s", kind, id))
}
