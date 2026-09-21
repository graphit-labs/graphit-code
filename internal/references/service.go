// Package references composes authorized domain stores for relationship analysis.
// Producers depend only on the lower-level relations package.
package references

import (
	"context"
	"fmt"
	"sort"

	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/relations"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/task"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

type Record struct {
	Entity relations.Entity `json:"entity"`
	Title  string           `json:"title"`
}
type Snapshot struct {
	Records  []Record         `json:"records"`
	Edges    []relations.Edge `json:"edges"`
	Warnings []string         `json:"warnings"`
	Complete bool             `json:"complete"`
}

func Read(ctx context.Context, dir string, includeUser bool) Snapshot {
	out := Snapshot{Records: []Record{}, Edges: []relations.Edge{}, Warnings: []string{}, Complete: true}
	projectID := store.ProjectID(dir)
	warn := func(label string, err error) {
		out.Complete = false
		out.Warnings = append(out.Warnings, label+": "+err.Error())
	}
	collect := func(label string, edges []relations.Edge, complete bool, err error) {
		if err != nil {
			warn(label, err)
			return
		}
		if !complete {
			warn(label, fmt.Errorf("relations require reconciliation"))
		}
		out.Edges = append(out.Edges, edges...)
	}
	add := func(kind, id, title, scope, scopeID, contextName string) {
		out.Records = append(out.Records, Record{relations.Entity{Type: kind, ID: id, Scope: scope, ScopeID: scopeID, Context: contextName}, title})
	}
	svc, err := task.Open(dir)
	if err != nil {
		warn("Task", err)
	} else {
		tasks, sessions, e := svc.ReferenceRecords(ctx)
		if e != nil {
			warn("Task", e)
		} else {
			for _, v := range tasks {
				add("task", v.ID, v.Title, "project", v.ProjectID, "")
			}
			for _, v := range sessions {
				add("session", v.ID, v.Title, "project", v.ProjectID, "")
			}
		}
		edges, complete, e := svc.ReferenceEdges(ctx)
		collect("Task", edges, complete, e)
	}
	memStore, e := memory.NewMemoryStore()
	if e != nil {
		warn("Memory", e)
	} else {
		scopes := []memory.MemoryScope{memory.MemoryScopeProject}
		if includeUser {
			scopes = append(scopes, memory.MemoryScopeUser)
		}
		for _, scope := range scopes {
			scopeID := projectID
			if scope == memory.MemoryScopeUser {
				scopeID, e = memory.UserScopeIDForContext(ctx)
				if e != nil {
					warn("User memory", e)
					continue
				}
			}
			ms := memory.NewMemoryService(scope, scopeID, memStore).WithContext(ctx)
			rows, e := ms.LiveMemories(ctx)
			if e != nil {
				warn(string(scope)+" memory", e)
				continue
			}
			for _, v := range rows {
				add("memory", v.ID, v.Title, string(scope), v.ScopeID, "")
			}
			edges, complete, e := ms.ReferenceEdges(ctx)
			collect(string(scope)+" memory", edges, complete, e)
		}
	}
	contexts := append([]string{""}, knowledge.InstalledContextsIn(dir)...)
	for _, contextName := range contexts {
		path := knowledge.ReadDirIn(dir, contextName)
		if path == "" {
			continue
		}
		db, e := wiki.OpenWikiDB(ctx, path)
		if e != nil {
			warn("Knowledge "+contextName, e)
			continue
		}
		titles, e := db.PageTitles(ctx)
		edges, complete, re := db.ReferenceEdges(ctx)
		db.Close()
		if e != nil {
			warn("Knowledge "+contextName, e)
			continue
		}
		for id, title := range titles {
			add("knowledge", id, title, "project", projectID, contextName)
		}
		for i := range edges {
			edges[i].Source.Scope = "project"
			edges[i].Source.ScopeID = projectID
			edges[i].Source.Context = contextName
			edges[i].Target = relations.Qualify(edges[i].Source, []relations.Ref{{Target: edges[i].Target}})[0].Target
		}
		collect("Knowledge "+contextName, edges, complete, re)
	}
	sort.Slice(out.Records, func(i, j int) bool { return out.Records[i].Entity.Key() < out.Records[j].Entity.Key() })
	sort.Slice(out.Edges, func(i, j int) bool { return edgeKey(out.Edges[i]) < edgeKey(out.Edges[j]) })
	return out
}
func edgeKey(e relations.Edge) string { return e.Source.Key() + e.Target.Key() + e.Relation + e.Field }

type Filter struct {
	SourceType, SourceID, TargetType, TargetID, Relation, Scope, ScopeID, TargetScope, TargetScopeID string
	Context, TargetContext                                                                           *string
}

func Select(edges []relations.Edge, f Filter) []relations.Edge {
	out := []relations.Edge{}
	for _, e := range edges {
		if (f.SourceType != "" && e.Source.Type != f.SourceType) || (f.SourceID != "" && e.Source.ID != f.SourceID) || (f.TargetType != "" && e.Target.Type != f.TargetType) || (f.TargetID != "" && e.Target.ID != f.TargetID) || (f.Relation != "" && e.Relation != f.Relation) || (f.Scope != "" && e.Source.Scope != f.Scope) || (f.ScopeID != "" && e.Source.ScopeID != f.ScopeID) || (f.Context != nil && e.Source.Context != *f.Context) || (f.TargetScope != "" && e.Target.Scope != f.TargetScope) || (f.TargetScopeID != "" && e.Target.ScopeID != f.TargetScopeID) || (f.TargetContext != nil && e.Target.Context != *f.TargetContext) {
			continue
		}
		out = append(out, e)
	}
	return out
}

// Reconcile is an explicit maintenance operation. It replays known structured
// metadata and structural links without parsing prose or changing record bodies.
func Reconcile(ctx context.Context, dir string, includeUser bool) []string {
	failures := []string{}
	run := func(label string, err error) {
		if err != nil {
			failures = append(failures, label+": "+err.Error())
		}
	}
	svc, err := task.Open(dir)
	if err != nil {
		run("Task", err)
	} else {
		run("Task", svc.ReconcileReferences(ctx))
	}
	ms, err := memory.NewMemoryStore()
	if err != nil {
		run("Memory", err)
	} else {
		projectID := store.ProjectID(dir)
		run("Project memory", memory.NewMemoryService(memory.MemoryScopeProject, projectID, ms).WithContext(ctx).ReconcileReferences(ctx))
		if includeUser {
			userID, e := memory.UserScopeIDForContext(ctx)
			if e != nil {
				run("User memory", e)
			} else {
				run("User memory", memory.NewMemoryService(memory.MemoryScopeUser, userID, ms).WithContext(ctx).ReconcileReferences(ctx))
			}
		}
	}
	// Imported contexts may be immutable published artifacts. Never rewrite them
	// as part of a project's maintenance operation.
	if dir := knowledge.ReadDirIn(dir, ""); dir != "" {
		run("Project knowledge", wiki.EnsureReferences(ctx, dir))
	}
	return failures
}
