package references

import (
	"github.com/graphit-labs/graphit-code/internal/relations"
	"testing"
)

func TestSelectIncomingTypedReferencesDoesNotConfuseEqualIDs(t *testing.T) {
	edges := []relations.Edge{
		{Source: relations.Entity{Type: "task", ID: "t", Scope: "project", ScopeID: "p"}, Target: relations.Entity{Type: "memory", ID: "same"}, Relation: "supports"},
		{Source: relations.Entity{Type: "task", ID: "t", Scope: "project", ScopeID: "p"}, Target: relations.Entity{Type: "knowledge", ID: "same"}, Relation: "supports"},
	}
	got := Select(edges, Filter{TargetType: "memory", TargetID: "same", ScopeID: "p"})
	if len(got) != 1 || got[0].Target.Type != "memory" {
		t.Fatalf("%+v", got)
	}
	if got := Select(edges, Filter{ScopeID: "other"}); len(got) != 0 {
		t.Fatal("scope filter ignored")
	}
}
