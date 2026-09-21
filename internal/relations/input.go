package relations

import (
	"context"
	"fmt"
	"strings"
)

type inputKey struct{}

// Inputs are partial-write semantics: nil preserves, a nonnil empty list clears.
func WithInputs(ctx context.Context, refs *[]Ref) context.Context {
	return context.WithValue(ctx, inputKey{}, refs)
}
func Inputs(ctx context.Context) *[]Ref { refs, _ := ctx.Value(inputKey{}).(*[]Ref); return refs }
func Validate(refs *[]Ref) error {
	if refs == nil {
		return nil
	}
	for i, r := range *refs {
		if r.Target.Type != "task" && r.Target.Type != "session" && r.Target.Type != "memory" && r.Target.Type != "knowledge" {
			return fmt.Errorf("references[%d].target.type must be task, session, memory or knowledge", i)
		}
		if strings.TrimSpace(r.Target.ID) == "" {
			return fmt.Errorf("references[%d].target.id is required", i)
		}
		if strings.ContainsAny(r.Target.ID, "\\\n\r") {
			return fmt.Errorf("references[%d].target.id must be a record identifier", i)
		}
		if r.Target.Scope != "" && r.Target.Scope != "project" && r.Target.Scope != "user" {
			return fmt.Errorf("references[%d].target.scope must be project or user", i)
		}
	}
	return nil
}

// Qualify fills only the source's own namespace. Cross-scope references must
// explicitly supply scope and scope_id; no filesystem paths are accepted.
func Qualify(source Entity, refs []Ref) []Ref {
	out := append([]Ref{}, refs...)
	for i := range out {
		target := &out[i].Target
		if target.Scope == "" {
			target.Scope = source.Scope
		}
		if target.ScopeID == "" && target.Scope == source.Scope {
			target.ScopeID = source.ScopeID
		}
		if target.Type == "knowledge" && target.Context == "project" {
			target.Context = ""
		} else if target.Type == "knowledge" && target.Context == "" && target.Scope == source.Scope && target.ScopeID == source.ScopeID {
			target.Context = source.Context
		}
		if out[i].Relation == "" {
			out[i].Relation = "relates_to"
		}
	}
	return out
}
