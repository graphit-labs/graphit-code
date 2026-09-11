package ast

import "testing"

func legacyIsExportedByModifier(strategy, source string, config map[string]string, configList map[string][]string) bool {
	switch strategy {
	case "modifier":
		keyword := config["keyword"]
		if keyword == "" {
			return false
		}
		return source != "" && containsModifier(source, keyword)
	case "no_modifier":
		keywords := configList["keywords"]
		if len(keywords) == 0 {
			return true
		}
		if source == "" {
			return true
		}
		for _, kw := range keywords {
			if containsModifier(source, kw) {
				return false
			}
		}
		return true
	case "no_static":
		return source != "" && !containsModifier(source, "static")
	}
	return false
}

func TestModifierExportVerdictMatchesLegacy(t *testing.T) {
	sources := []string{
		"",
		"public void run() {}",
		"private static int x;",
		"protected final String s;",
		"static void helper() {}",
		"func Exported() {}",
		"publicish notAModifier",
		"  public   void spaced()",
		"nonpublic",
		"PUBLIC UPPER",
		"export default function f(){}",
	}
	strategies := []struct {
		name string
		cfg  map[string]string
		list map[string][]string
	}{
		{"modifier", map[string]string{"keyword": "public"}, nil},
		{"modifier", map[string]string{}, nil},
		{"no_modifier", nil, map[string][]string{"keywords": {"private", "protected"}}},
		{"no_modifier", nil, map[string][]string{}},
		{"no_static", nil, nil},
		{"none", nil, nil},
		{"unknown_strategy", nil, nil},
	}

	for _, st := range strategies {
		for _, src := range sources {
			got := ModifierExportVerdict(st.name, src, st.cfg, st.list)
			want := legacyIsExportedByModifier(st.name, src, st.cfg, st.list)
			if got != want {
				t.Errorf("strategy=%q src=%q: got %v, want %v", st.name, src, got, want)
			}
		}
	}
}

// TestIsExportedUsesPrecomputedVerdict guards the wiring: isExported must honour
// the verdict decided at construction time for the modifier strategies.
func TestIsExportedUsesPrecomputedVerdict(t *testing.T) {
	for _, strategy := range []string{"modifier", "no_modifier", "no_static"} {
		for _, verdict := range []bool{true, false} {
			e := &Entity{Name: "X", ModifierExport: verdict}
			if got := isExported(strategy, e, nil, map[string]string{"keyword": "public"}, nil); got != verdict {
				t.Errorf("strategy=%s verdict=%v: isExported returned %v", strategy, verdict, got)
			}
		}
	}
}

func TestNonModifierStrategiesUnaffected(t *testing.T) {
	e := &Entity{Name: "Foo"}
	if !isExported("capitalized_name", e, nil, nil, nil) {
		t.Error("capitalized_name: expected Foo to be exported")
	}
	lower := &Entity{Name: "foo"}
	if isExported("capitalized_name", lower, nil, nil, nil) {
		t.Error("capitalized_name: expected foo not to be exported")
	}
	if !isExported("export_statement", e, map[string]bool{"Foo": true}, nil, nil) {
		t.Error("export_statement: expected Foo to be exported")
	}
	if isExported("none", e, nil, nil, nil) {
		t.Error("none: expected not exported")
	}
}
