package wiki

import "testing"

func TestExplicitReferenceFrontmatter(t *testing.T) {
	for _, tc := range []struct {
		body  string
		count int
		bad   bool
	}{
		{"# Page\nmentions tsk-text", -1, false},
		{"---\nreferences: []\n---\n# Page", 0, false},
		{"---\nreferences:\n  - target: {type: memory, id: mem-id, scope: project, scope_id: p-id}\n    relation: supports\n---\n# Page", 1, false},
		{"---\nreferences: tsk-text\n---", 0, true},
		{"---\nreferences: [{target: {type: memory}}]\n---", 0, true},
	} {
		refs, err := FrontmatterReferences(tc.body)
		if (err != nil) != tc.bad {
			t.Fatalf("%s: %v", tc.body, err)
		}
		if tc.bad {
			continue
		}
		if tc.count < 0 {
			if refs != nil {
				t.Fatal("inferred prose reference")
			}
			continue
		}
		if refs == nil || len(*refs) != tc.count {
			t.Fatalf("%+v", refs)
		}
		if tc.count > 0 && (*refs)[0].Target.ScopeID != "p-id" {
			t.Fatal("scope_id lost")
		}
	}
}
