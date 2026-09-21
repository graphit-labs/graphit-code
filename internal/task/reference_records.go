package task

import (
	"context"
	"fmt"
	"github.com/graphit-labs/graphit-code/internal/relations"
)

// ReferenceRecords reads current records without exporting their audit histories.
// The caller receives no fencing tokens and must retain the project boundary.
func (s *Service) ReferenceRecords(ctx context.Context) (tasks []Task, sessions []Session, err error) {
	err = s.withTables(ctx, func(t *tables) error {
		var e error
		tasks, e = t.allTasks(ctx)
		if e != nil {
			return e
		}
		sessions, e = t.allSessions(ctx)
		return e
	})
	for i := range tasks {
		tasks[i].ClaimToken = ""
	}
	for i := range sessions {
		sessions[i].ClaimToken = ""
	}
	return
}

func (s *Service) ReferenceEdges(ctx context.Context) (edges []relations.Edge, initialized bool, err error) {
	err = s.withTables(ctx, func(t *tables) error {
		var e error
		tasks, e := t.allTasks(ctx)
		if e != nil {
			return e
		}
		sessions, e := t.allSessions(ctx)
		if e != nil {
			return e
		}
		heads := map[string]string{}
		for _, v := range tasks {
			heads[(relations.Entity{Type: "task", ID: v.ID, Scope: "project", ScopeID: v.ProjectID}).Key()] = fmt.Sprint(v.Revision)
		}
		for _, v := range sessions {
			heads[(relations.Entity{Type: "session", ID: v.ID, Scope: "project", ScopeID: v.ProjectID}).Key()] = fmt.Sprint(v.Revision)
		}
		edges, initialized, e = relations.ReadMatching(ctx, t.store, heads)
		return e
	})
	return
}
func (s *Service) ReconcileReferences(ctx context.Context) error {
	return s.withTables(ctx, func(t *tables) error {
		tasks, e := t.allTasks(ctx)
		if e != nil {
			return e
		}
		for _, v := range tasks {
			if e = s.projectTask(ctx, t, v, v.Owner); e != nil {
				return e
			}
		}
		sessions, e := t.allSessions(ctx)
		if e != nil {
			return e
		}
		for _, v := range sessions {
			if e = s.projectSession(ctx, t, v); e != nil {
				return e
			}
		}
		return nil
	})
}
