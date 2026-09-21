package uiserver

import (
	"github.com/graphit-labs/graphit-code/internal/relations"
	"testing"
)

func TestReferenceNavigationUsesStoredEdgesAndAuthorizedIdentity(t *testing.T) {
	source := referenceTarget{Kind: "task", ID: "t", Scope: "project", ScopeID: "p", Title: "Task", Href: "/task/explorer/t"}
	target := referenceTarget{Kind: "memory", ID: "m", Scope: "project", ScopeID: "p", Title: "Memory", Href: "/memory/explorer/project/m"}
	unrelated := referenceTarget{Kind: "memory", ID: "m", Scope: "user", ScopeID: "u", Title: "Personal memory", Href: "/memory/explorer/user/m"}
	edge := relations.Edge{Source: referenceIdentity(source), Target: referenceIdentity(target), Relation: "supports"}
	records := []referenceTarget{source, target, unrelated}
	view := projectReferences(records, []relations.Edge{edge}, source, nil)
	if len(view.Outgoing) != 1 || view.Outgoing[0].Target.Href != target.Href {
		t.Fatalf("%+v", view)
	}
	if back := projectReferences(records, []relations.Edge{edge}, target, nil); len(back.Incoming) != 1 {
		t.Fatal("missing backlink")
	}
	if back := projectReferences(records, []relations.Edge{edge}, unrelated, nil); len(back.Incoming) != 0 {
		t.Fatal("personal scope leaked")
	}
	edge.Target.ScopeID = "unavailable-project"
	view = projectReferences(records, []relations.Edge{edge}, source, nil)
	if len(view.Outgoing) != 1 || view.Outgoing[0].Target.Href != "" {
		t.Fatal("unresolved identity acquired unauthorized local link")
	}
	if empty := projectReferences(records, nil, source, nil); len(empty.Outgoing) != 0 {
		t.Fatal("inferred an edge without persistence")
	}
}
