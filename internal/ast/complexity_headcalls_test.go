package ast

import (
	"testing"

	sitter "github.com/tree-sitter/go-tree-sitter"
)

// TestComplexityHeadCallsClojureAndElixir checks the head_calls mechanism
// against real parsed Clojure and Elixir, both of which spell every control
// form (if, when, cond, case, ...) as the same node kind as an ordinary call
// — the branch is only visible in the first named child's own text. See
// HeadCallConfig and complexityMatcher.score.
func TestComplexityHeadCallsClojureAndElixir(t *testing.T) {
	cases := []struct {
		lang      string
		src       string
		headCalls *HeadCallConfig
		operators []string
		want      int
	}{
		{
			lang: "clojure",
			src:  "(defn f [x] (if (> x 0) x (- x)))",
			headCalls: &HeadCallConfig{
				NodeType: "list_lit",
				Names:    []string{"if", "when", "cond", "and", "or"},
			},
			want: 2, // base 1 + if. The comparison (> x 0) is an ordinary call, not a branch.
		},
		{
			lang: "clojure",
			src:  "(defn f [x] (when a b) (cond (> x 0) 1 :else 0) (and a b))",
			headCalls: &HeadCallConfig{
				NodeType: "list_lit",
				Names:    []string{"if", "when", "cond", "and", "or"},
			},
			want: 4, // base 1 + when + cond (once, not per clause) + and
		},
		{
			lang: "elixir",
			src:  "def f(x) do\n  if x > 0 do\n    x\n  else\n    -x\n  end\nend",
			headCalls: &HeadCallConfig{
				NodeType: "call",
				Names:    []string{"if", "unless", "case", "cond", "for", "with"},
			},
			want: 2, // base 1 + if. def itself is a call too, but not in Names.
		},
		{
			lang: "elixir",
			src:  "def f(x) do\n  case x do\n    1 -> 1\n    _ -> 0\n  end\n  a && b\nend",
			headCalls: &HeadCallConfig{
				NodeType: "call",
				Names:    []string{"if", "unless", "case", "cond", "for", "with"},
			},
			operators: []string{"&&", "||", "and", "or"},
			want:      3, // base 1 + case (once, not per clause) + &&
		},
		{
			// cond has no subject: every child after the head is part of a
			// test/result pair. 3 pairs here, including the :else fallback,
			// which is an ordinary pair syntactically — nothing marks it as
			// a default the way other languages' switch/case do.
			lang: "clojure",
			src:  "(defn f [x] (cond (> x 0) 1 (< x 0) -1 :else 0))",
			headCalls: &HeadCallConfig{
				NodeType:  "list_lit",
				PairNames: []string{"cond"},
			},
			want: 4, // base 1 + 3 pairs
		},
		{
			// case's first child after the head is the subject (x), not a
			// clause — subject_pair_names accounts for it. A trailing
			// default with no test of its own is naturally dropped by the
			// integer division, not counted as an extra clause.
			lang: "clojure",
			src:  "(defn f [x] (case x 1 :one 2 :two :other))",
			headCalls: &HeadCallConfig{
				NodeType:         "list_lit",
				SubjectPairNames: []string{"case"},
			},
			want: 3, // base 1 + 2 pairs (the trailing :other is not a third pair)
		},
	}

	for _, tt := range cases {
		t.Run(tt.lang, func(t *testing.T) {
			loader := NewDynGrammarLoader(WithProjectDir("."))
			t.Cleanup(loader.Close)
			lang, err := loader.Load(tt.lang)
			if err != nil {
				t.Skipf("%s grammar not available: %v", tt.lang, err)
			}
			p := sitter.NewParser()
			t.Cleanup(p.Close)
			if err := p.SetLanguage(lang); err != nil {
				t.Fatalf("SetLanguage: %v", err)
			}
			src := []byte(tt.src)
			tree := p.Parse(src, nil)
			t.Cleanup(tree.Close)

			langConfig := &ExternalQueryFile{
				Complexity: &ComplexityConfig{HeadCalls: tt.headCalls, Operators: tt.operators},
			}
			m := newComplexityMatcher(langConfig, lang)
			if !m.on {
				t.Fatal("matcher did not activate")
			}
			if got := m.score(tree.RootNode(), src); got != tt.want {
				t.Errorf("score() = %d, want %d\nsource: %s", got, tt.want, tt.src)
			}
		})
	}
}
