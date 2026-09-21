package relations

import "testing"

func TestKnowledgeContextDoesNotCrossNamespace(t *testing.T) {
	source := Entity{Type: "knowledge", ID: "a", Scope: "project", ScopeID: "p-a", Context: "imported-a"}
	refs := Qualify(source, []Ref{{Target: Entity{Type: "knowledge", ID: "b", Scope: "project", ScopeID: "p-b"}}})
	if refs[0].Target.Context != "" {
		t.Fatal("foreign project inherited source context")
	}
}
