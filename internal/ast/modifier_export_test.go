package ast

import "testing"

func TestModifierExportVerdictUsesWholeCurrentModifiers(t *testing.T) {
	tests := []struct {
		name     string
		strategy string
		source   string
		config   map[string]string
		list     map[string][]string
		want     bool
	}{
		{"required modifier", "modifier", "public void run() {}", map[string]string{"keyword": "public"}, nil, true},
		{"required modifier missing", "modifier", "private void run() {}", map[string]string{"keyword": "public"}, nil, false},
		{"required modifier is a whole word", "modifier", "publicish void run() {}", map[string]string{"keyword": "public"}, nil, false},
		{"required modifier needs configuration", "modifier", "public void run() {}", nil, nil, false},
		{"forbidden modifier", "no_modifier", "protected final String s;", nil, map[string][]string{"keywords": {"private", "protected"}}, false},
		{"forbidden modifier absent", "no_modifier", "public final String s;", nil, map[string][]string{"keywords": {"private", "protected"}}, true},
		{"empty source has no forbidden modifier", "no_modifier", "", nil, map[string][]string{"keywords": {"private"}}, true},
		{"no forbidden modifiers configured", "no_modifier", "private int x;", nil, nil, true},
		{"instance member", "no_static", "public void run() {}", nil, nil, true},
		{"static member", "no_static", "private static int x;", nil, nil, false},
		{"no static needs source", "no_static", "", nil, nil, false},
		{"unknown strategy", "unknown", "public void run() {}", nil, nil, false},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			if got := ModifierExportVerdict(tc.strategy, tc.source, tc.config, tc.list); got != tc.want {
				t.Fatalf("ModifierExportVerdict(%q, %q) = %v, want %v", tc.strategy, tc.source, got, tc.want)
			}
		})
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
