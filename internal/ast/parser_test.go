package ast

import (
	"testing"
)

func TestParsedFile_AddEntity(t *testing.T) {
	pf := &ParsedFile{}
	pf.AddEntity("functions", Entity{Name: "Foo", Line: 1})
	pf.AddEntity("functions", Entity{Name: "Bar", Line: 10})
	pf.AddEntity("classes", Entity{Name: "MyClass", Line: 5})

	funcs := pf.GetEntities("functions")
	if len(funcs) != 2 {
		t.Fatalf("expected 2 functions, got %d", len(funcs))
	}
	if funcs[0].Name != "Foo" || funcs[1].Name != "Bar" {
		t.Errorf("unexpected function names: %v", funcs)
	}

	classes := pf.GetEntities("classes")
	if len(classes) != 1 {
		t.Fatalf("expected 1 class, got %d", len(classes))
	}
}

func TestParsedFile_GetEntities_NilMap(t *testing.T) {
	pf := &ParsedFile{}
	result := pf.GetEntities("anything")
	if result != nil {
		t.Errorf("expected nil for uninitialized entities, got %v", result)
	}
}

func TestParsedFile_GetEntities_MissingKey(t *testing.T) {
	pf := &ParsedFile{Entities: map[string][]Entity{"functions": {{Name: "A"}}}}
	result := pf.GetEntities("classes")
	if result != nil {
		t.Errorf("expected nil for missing key, got %v", result)
	}
}

func TestParsedFile_AllEntities(t *testing.T) {
	pf := &ParsedFile{}
	pf.AddEntity("functions", Entity{Name: "F1"})
	pf.AddEntity("classes", Entity{Name: "C1"})
	pf.AddEntity("functions", Entity{Name: "F2"})

	all := pf.AllEntities()
	if len(all) != 3 {
		t.Fatalf("expected 3 entities, got %d", len(all))
	}

	names := make(map[string]bool)
	for _, e := range all {
		names[e.Name] = true
	}
	for _, expected := range []string{"F1", "F2", "C1"} {
		if !names[expected] {
			t.Errorf("expected entity %q in AllEntities()", expected)
		}
	}
}

func TestParsedFile_AllEntities_NilMap(t *testing.T) {
	pf := &ParsedFile{}
	all := pf.AllEntities()
	if all != nil {
		t.Errorf("expected nil for uninitialized entities, got %v", all)
	}
}

func TestParsedFile_EntityCount(t *testing.T) {
	tests := []struct {
		name     string
		entities map[string][]Entity
		want     int
	}{
		{
			name:     "nil_map",
			entities: nil,
			want:     0,
		},
		{
			name:     "empty_map",
			entities: map[string][]Entity{},
			want:     0,
		},
		{
			name: "single_type",
			entities: map[string][]Entity{
				"functions": {{Name: "A"}, {Name: "B"}},
			},
			want: 2,
		},
		{
			name: "multiple_types",
			entities: map[string][]Entity{
				"functions": {{Name: "A"}},
				"classes":   {{Name: "B"}, {Name: "C"}},
				"imports":   {{Name: "D"}},
			},
			want: 4,
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			pf := &ParsedFile{Entities: tt.entities}
			got := pf.EntityCount()
			if got != tt.want {
				t.Errorf("EntityCount() = %d, want %d", got, tt.want)
			}
		})
	}
}
