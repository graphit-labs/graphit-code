package ast

import (
	"os"
	"testing"
)

func jsParse(t *testing.T, src string) *ParsedFile {
	t.Helper()
	proj := stageGrammar(t, "javascript", "tree-sitter-javascript", ".js", "javascript.yaml")
	cfg, ok := tsLangConfigByName(proj, "javascript")
	if !ok {
		t.Skip("javascript grammar unavailable")
	}
	p := &TreeSitterParser{projectDir: proj}
	pf, err := p.parseSource("cfg.js", ".js", cfg, []byte(src), 0, 0, false, ParseOptions{})
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	return pf
}

// A config file written in .js has to be as reachable as the same config in .json.
//
// It was not, and the only difference was the extension: tsconfig.json in this
// repository yields 20 Pair and 20 Value nodes, while tailwind.config.js beside it
// yielded zero entities — the indexer reported it under "Empty (0 entities)". Every
// pattern the JS grammar declared needed a NAME, and `export default { … }` declares
// none, so nothing matched at all.
func TestExportedConfigObjectIsIndexed(t *testing.T) {
	pf := jsParse(t, `export default {
  darkMode: 'class',
  maxRetries: 3,
  strict: true,
  'accordion-down': 'accordion-down 0.2s ease-out',
}
`)

	for _, key := range []string{"darkMode", "maxRetries", "strict", "accordion-down"} {
		if _, ok := entityAt(pf, "Pair", key); !ok {
			t.Errorf("no Pair for key %q; labels: %v", key, entityLabelsOf(pf))
		}
	}
	for _, val := range []string{"class", "3", "true"} {
		if _, ok := entityAt(pf, "Value", val); !ok {
			t.Errorf("no Value node for %q; the key alone is half the content", val)
		}
	}
}

// A lookup table is the other shape worth having, and it is nested one level down —
// which is exactly what an anchored pattern would have missed.
func TestDeclaredLookupTableIsIndexed(t *testing.T) {
	pf := jsParse(t, `const CONFIG = {
  langs: {
    pkb: 'sql',
    pks: 'sql',
    tsx: 'typescript',
  },
}
`)
	for _, key := range []string{"pkb", "pks", "tsx"} {
		if _, ok := entityAt(pf, "Pair", key); !ok {
			t.Errorf("no Pair for %q in a declared table; labels: %v", key, entityLabelsOf(pf))
		}
	}
}

// A key whose value is an object or an array is content too, and often the ONLY
// content there is.
//
// `plugins: { autoprefixer: {} }` names a plugin; the empty object means "no options",
// not "nothing here". An earlier version of these patterns captured only literal-valued
// pairs and left postcss.config.js — whose every value is an empty object — reporting
// zero entities, so the graph could not answer that autoprefixer is used at all.
//
// The VALUE is not captured for these: there is no literal to capture, and the nodes
// beneath speak for themselves. So it costs one node per key, not a tree.
func TestKeysWithStructuralValuesAreIndexedWithoutTheirTree(t *testing.T) {
	pf := jsParse(t, `export default {
  plugins: {
    tailwindcss: {},
    autoprefixer: {},
  },
}
`)
	for _, key := range []string{"plugins", "tailwindcss", "autoprefixer"} {
		if _, ok := entityAt(pf, "Pair", key); !ok {
			t.Errorf("no Pair for %q; a plugin named by an empty object is still a fact "+
				"the graph should hold", key)
		}
	}
	if _, ok := entityAt(pf, "Value", "{}"); ok {
		t.Error("an empty object was given a Value node of its own")
	}
}

// Array items are content the key alone does not carry — the glob in `content: [...]`,
// the preset in a plugin list. json.yaml already records them this way.
func TestArrayItemsAreIndexedUnderTheirKey(t *testing.T) {
	pf := jsParse(t, `export default {
  content: ['./index.html', './src/**/*.tsx'],
}
`)
	for _, item := range []string{"./index.html", "./src/**/*.tsx"} {
		if _, ok := entityAt(pf, "Value", item); !ok {
			t.Errorf("no Value for array item %q; only the key survived", item)
		}
	}
}

// `require('x')` IS a dependency. Without this it arrived as a call to a stub named
// `require`, with the module's own name unreachable — so "what does this project depend
// on" silently missed every CommonJS import.
func TestRequireIsADependencyNotACallToRequire(t *testing.T) {
	pf := jsParse(t, "const a = require('tailwindcss-animate')\n")

	found := false
	for _, e := range entitiesOfLabel(pf, "Import") {
		if e.Name == "'tailwindcss-animate'" || e.Name == "tailwindcss-animate" {
			found = true
		}
	}
	if !found {
		t.Errorf("require() did not become an Import; entities: %v", entityLabelsOf(pf))
	}
	if _, ok := entityAt(pf, "Import", "require"); ok {
		t.Error("the `require` identifier itself was indexed as a dependency")
	}
}

// THE TEST THAT SHOULD HAVE COME FIRST: the real files, not a fixture of my own
// shape.
//
// An earlier attempt anchored these patterns at the exported object, which kept the
// added nodes to 7.8% and dropped the call-site options — and passed a flat fixture
// while leaving tailwind.config.js exactly as empty as before. In a real config the
// literals are three or four levels down (`theme.extend.colors.border`), so the anchor
// matched nothing that mattered. A fixture written to the shape one imagines proves the
// pattern compiles, not that it answers the question.
func TestTheConfigFilesThatWereEmptyAreNotEmptyAnyMore(t *testing.T) {
	for _, c := range []struct {
		path      string
		wantKeys  []string
		wantValue string
	}{
		{
			path:      "../ui/tailwind.config.js",
			wantKeys:  []string{"border", "background", "accordion-down"},
			wantValue: "hsl(var(--border))",
		},
		{
			path:     "../ui/postcss.config.js",
			wantKeys: []string{"plugins", "tailwindcss", "autoprefixer"},
		},
	} {
		src, err := os.ReadFile(c.path)
		if err != nil {
			t.Skipf("%s unavailable: %v", c.path, err)
		}
		pf := jsParse(t, string(src))

		for _, k := range c.wantKeys {
			if _, ok := entityAt(pf, "Pair", k); !ok {
				t.Errorf("%s: no Pair for %q — a nested config key is still invisible",
					c.path, k)
			}
		}
		if c.wantValue != "" {
			if _, ok := entityAt(pf, "Value", c.wantValue); !ok {
				t.Errorf("%s: no Value node for %q — the content is what search needs",
					c.path, c.wantValue)
			}
		}
	}
}
