package uiserver

import (
	"context"
	"fmt"
	"net/http"
	"sort"
	"time"

	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/lancequery"
	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/task"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

type nowItem struct {
	ID             string `json:"id"`
	Title          string `json:"title"`
	Href           string `json:"href"`
	Owner          string `json:"owner,omitempty"`
	UpdatedAt      string `json:"updated_at,omitempty"`
	Status         string `json:"status,omitempty"`
	LeaseExpiresAt string `json:"lease_expires_at,omitempty"`
	Progress       string `json:"progress,omitempty"`
	NextStep       string `json:"next_step,omitempty"`
}
type nowSection struct {
	Items   []nowItem `json:"items"`
	Total   int       `json:"total"`
	HasMore bool      `json:"has_more"`
	Error   string    `json:"error,omitempty"`
}
type nowView struct {
	ProjectID   string                `json:"project_id"`
	GeneratedAt string                `json:"generated_at"`
	Sections    map[string]nowSection `json:"sections"`
}

func activityDate(value string) time.Time {
	for _, layout := range []string{time.RFC3339Nano, "2006-01-02"} {
		if v, e := time.Parse(layout, value); e == nil {
			return v
		}
	}
	return time.Time{}
}
func finishNow(items []nowItem, err error) nowSection {
	if items == nil {
		items = []nowItem{}
	}
	sort.Slice(items, func(i, j int) bool {
		a, b := activityDate(items[i].UpdatedAt), activityDate(items[j].UpdatedAt)
		if a.Equal(b) {
			return items[i].ID < items[j].ID
		}
		return a.After(b)
	})
	s := nowSection{Items: items, Total: len(items), HasMore: len(items) > 20}
	if len(s.Items) > 20 {
		s.Items = s.Items[:20]
	}
	if err != nil {
		s.Error = err.Error()
	}
	return s
}

type metadataQuerier interface {
	QueryStore(context.Context, lancequery.Request) (lancequery.Result, error)
}

func activityMetadata(ctx context.Context, q metadataQuerier, req lancequery.Request, kind string) ([]nowItem, error) {
	items := []nowItem{}
	req.Limit = 1024
	for req.Offset = 0; ; req.Offset += req.Limit {
		result, err := q.QueryStore(ctx, req)
		if err != nil {
			return nil, err
		}
		for _, r := range result.Rows {
			str := func(k string) string { v, _ := r[k].(string); return v }
			id, updated := str("id"), str("updated_at")
			if kind == "knowledge" {
				id, updated = str("slug"), str("updated")
			}
			items = append(items, nowItem{ID: id, Title: str("title"), Href: referenceHref(kind, id, "project", ""), Owner: str("updated_by"), UpdatedAt: updated})
		}
		if len(result.Rows) < req.Limit {
			break
		}
	}
	return items, nil
}
func readWorkspaceNow(ctx context.Context, dir string, now time.Time) nowView {
	out := nowView{ProjectID: store.ProjectID(dir), GeneratedAt: now.UTC().Format(time.RFC3339Nano), Sections: map[string]nowSection{}}
	svc, err := task.Open(dir)
	var tasks, sessions []task.ActivityRecord
	if err == nil {
		tasks, sessions, err = svc.CurrentActivity(ctx, now)
	}
	for kind, rows := range map[string][]task.ActivityRecord{"tasks": tasks, "sessions": sessions} {
		items := []nowItem{}
		entity := "task"
		if kind == "sessions" {
			entity = "session"
		}
		for _, v := range rows {
			items = append(items, nowItem{ID: v.ID, Title: v.Title, Href: referenceHref(entity, v.ID, "project", ""), Owner: v.Owner, UpdatedAt: v.UpdatedAt, Status: v.Status, LeaseExpiresAt: v.LeaseExpiresAt, Progress: v.Progress, NextStep: v.NextStep})
		}
		out.Sections[kind] = finishNow(items, err)
	}
	ms, e := memory.NewMemoryStore()
	var memories []nowItem
	if e == nil {
		service := memory.NewMemoryService(memory.MemoryScopeProject, out.ProjectID, ms).WithContext(ctx)
		memories, e = activityMetadata(ctx, service, lancequery.Request{Table: "memories", Filter: "superseded = false", Columns: []string{"id", "title", "updated_at", "updated_by"}}, "memory")
	}
	out.Sections["memories"] = finishNow(memories, e)
	path := knowledge.ReadDirIn(dir, "")
	var documents []nowItem
	e = nil
	if path != "" {
		db, openErr := wiki.OpenWikiDB(ctx, path)
		e = openErr
		if e == nil {
			// A project with no index yet is empty, not a failed data source.
			var schema lancequery.Schema
			schema, e = db.DescribeStore(ctx, nil)
			if e == nil {
				for _, table := range schema.Tables {
					if table.Name == "chunks" {
						documents, e = activityMetadata(ctx, db, lancequery.Request{Table: "chunks", Columns: []string{"slug", "title", "updated"}}, "knowledge")
						break
					}
				}
			}
			db.Close()
		}
	}
	out.Sections["knowledge"] = finishNow(documents, e)
	return out
}
func (h *TaskHandler) handleWorkspaceNow(w http.ResponseWriter, r *http.Request) {
	dir := h.projectDir(r)
	if dir == "" {
		writeTaskError(w, 400, "project_dir is required")
		return
	}
	if store.ProjectID(dir) == "" {
		writeTaskError(w, 400, "project is not initialized")
		return
	}
	ctx, cancel := context.WithTimeout(r.Context(), 15*time.Second)
	defer cancel()
	result := readWorkspaceNow(ctx, dir, time.Now())
	if ctx.Err() != nil {
		writeTaskError(w, http.StatusServiceUnavailable, fmt.Sprintf("activity refresh: %v", ctx.Err()))
		return
	}
	writeJSON(w, result)
}
