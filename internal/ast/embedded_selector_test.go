package ast

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// A block used to be selected by node KIND, and that is static in a way the
// problem is not: in tree-sitter-xml `<execute>` is an `element` like every other
// one — its name lives in `STag/Name` — so `node: element` matches the whole
// document. Enumerating names (`names: [execute, statement]`) would answer one
// example and nothing beyond it: not "any tag matching ^sql", not "only inside
// <mapper>", not "whichever tag carries a type attribute".
//
// The engine already has a dynamic, declarative, compiled node-selection language:
// the tree-sitter pattern. These tests pin that a block can be selected by one.
//
// The inner language here is JavaScript, not SQL, and that is not a matter of
// taste. Measured 2026-08-05: the tree-sitter `sql` grammar assigns NO field names
// to `create_table`'s children, while every pattern in `sql.yaml` requires a `name:`
// field — so all three of them match zero times, and a `.sql` file only produces
// entities because CompositeParser silently falls back to ANTLR. Writing these
// tests against `sql` would have asserted that bug rather than this selector.
// See docs/specs/embedded_language_parsing.md.

// stageEmbeddedYAML stages a language plus its inner languages, then appends an
// `embedded:` section to the staged outer YAML.
func stageEmbeddedYAML(t *testing.T, langName, grammar, ext, queryFile, embedded string, inner ...string) string {
	t.Helper()
	projectDir := stageEmbedded(t, langName, grammar, ext, queryFile, inner...)
	qpath := filepath.Join(projectDir, brand.DotDir(), "ast", "queries", queryFile)
	body, err := os.ReadFile(qpath)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(qpath, append(body, []byte("\n"+embedded+"\n")...), 0o644); err != nil {
		t.Fatal(err)
	}
	return projectDir
}

const xmlExecuteBlock = `
embedded:
  - pattern: '(element (STag (Name) @_tag) (content (CharData) @body) (#eq? @_tag "execute"))'
    text_capture: body
    default: javascript
`

// The requested scenario, with a language whose queries actually work: in an XML,
// only <execute>'s body is handed to the inner grammar, not every element's.
func TestEmbeddedPatternSelectsElementByName(t *testing.T) {
	projectDir := stageEmbeddedYAML(t, "xml", "tree-sitter-xml", ".xml", "xml.yaml",
		xmlExecuteBlock, "javascript.yaml")

	pf := parseFixture(t, projectDir, "mapper.xml", `<mapper>
  <changeset id="1">
    <note>const nao_deve_existir = 1</note>
    <execute>const cliente = 1</execute>
  </changeset>
</mapper>
`)
	tbl, ok := entityAt(pf, "Variable", "cliente")
	if !ok {
		t.Fatalf("no Variable from the <execute> body; entities: %v", entityLabelsOf(pf))
	}
	if tbl.Line != 4 {
		t.Errorf("the variable is at line %d, want 4 (absolute)", tbl.Line)
	}
	// <note> is an element too, and must NOT have been handed to the javascript grammar.
	if _, ok := entityAt(pf, "Variable", "nao_deve_existir"); ok {
		t.Error("<note> was parsed as JavaScript; the pattern must select only <execute>")
	}
	// The XML markup is untouched.
	if _, ok := entityAt(pf, "Element", "execute"); !ok {
		t.Error("the execute Element disappeared")
	}
}

// `#match?` is regex, which is the point of reusing the query language: several
// names without enumerating them, and rules nobody listed in advance.
func TestEmbeddedPatternSelectsByRegex(t *testing.T) {
	projectDir := stageEmbeddedYAML(t, "xml", "tree-sitter-xml", ".xml", "xml.yaml", `
embedded:
  - pattern: '(element (STag (Name) @_tag) (content (CharData) @body) (#match? @_tag "^sql"))'
    text_capture: body
    default: javascript
`, "javascript.yaml")

	pf := parseFixture(t, projectDir, "q.xml", `<root>
  <sqlSetup>const cliente = 1</sqlSetup>
  <other>const pedido = 1</other>
</root>
`)
	if _, ok := entityAt(pf, "Variable", "cliente"); !ok {
		t.Errorf("sqlSetup was not matched by the regex; entities: %v", entityLabelsOf(pf))
	}
	if _, ok := entityAt(pf, "Variable", "pedido"); ok {
		t.Error("<other> was parsed as JavaScript; ^sql must not match it")
	}
}

// The language can come from a capture — and the PATTERN locates it, so no
// grammar-specific attribute node kind has to become configuration. XML writes an
// attribute as Attribute → Name / AttValue, which is exactly what the engine
// constants attribute_name / attribute_value do NOT match.
func TestEmbeddedPatternReadsLanguageFromACapture(t *testing.T) {
	projectDir := stageEmbeddedYAML(t, "xml", "tree-sitter-xml", ".xml", "xml.yaml", `
embedded:
  - pattern: '(element (STag (Name) @_tag (Attribute (Name) @_a (AttValue) @lang)) (content (CharData) @body) (#eq? @_a "type"))'
    text_capture: body
    lang_capture: lang
    languages:
      js: javascript
`, "javascript.yaml")

	pf := parseFixture(t, projectDir, "typed.xml", `<root>
  <run type="js">const cliente = 1</run>
  <run type="ruby">const nao_deve_existir = 1</run>
</root>
`)
	if _, ok := entityAt(pf, "Variable", "cliente"); !ok {
		t.Errorf("type=\"js\" did not select the javascript grammar; entities: %v", entityLabelsOf(pf))
	}
	// `ruby` is not in the allowlist, so that block is skipped in silence.
	if _, ok := entityAt(pf, "Variable", "nao_deve_existir"); ok {
		t.Error("type=\"ruby\" was parsed; it is not in the languages allowlist")
	}
}

// The offset still applies, and the fixture puts the block at the END so a zero
// offset cannot pass.
func TestEmbeddedPatternKeepsAbsoluteLines(t *testing.T) {
	projectDir := stageEmbeddedYAML(t, "xml", "tree-sitter-xml", ".xml", "xml.yaml",
		xmlExecuteBlock, "javascript.yaml")

	pf := parseFixture(t, projectDir, "late.xml", `<mapper>
  <doc>
    filler
  </doc>
  <doc>
    filler
  </doc>
  <execute>const cliente = 1</execute>
</mapper>
`)
	tbl, ok := entityAt(pf, "Variable", "cliente")
	if !ok {
		t.Fatalf("no Variable; entities: %v", entityLabelsOf(pf))
	}
	if tbl.Line != 8 {
		t.Errorf("the variable is at line %d, want 8", tbl.Line)
	}
}

// The simple form must keep working exactly as before, unchanged, or every shipped
// SFC grammar regresses.
func TestSimpleFormStillWorksAlongsidePatternForm(t *testing.T) {
	projectDir := stageEmbedded(t, "vue", "tree-sitter-vue", ".vue", "vue.yaml", "javascript.yaml")
	pf := parseFixture(t, projectDir, "Still.vue", `<template>
  <p>x</p>
</template>

<script setup>
import { ref } from 'vue'
</script>
`)
	imp, ok := entityAt(pf, "Import", "vue")
	if !ok {
		t.Fatalf("the simple form regressed; entities: %v", entityLabelsOf(pf))
	}
	if imp.Line != 6 {
		t.Errorf("import at line %d, want 6", imp.Line)
	}
}

// The config, which still fails OPEN

func TestEmbeddedPatternConfigValidation(t *testing.T) {
	cases := []struct {
		name string
		yaml string
		keep int
	}{
		{
			"pattern with text_capture",
			"pattern: '(x) @body'\n    text_capture: body\n    default: javascript",
			1,
		},
		{
			"pattern without text_capture",
			"pattern: '(x) @body'\n    default: javascript",
			0,
		},
		{
			"no pattern at all",
			"text_capture: body\n    default: javascript",
			0,
		},
		{
			"lang_capture with no languages and no default",
			"pattern: '(x) @body'\n    text_capture: body\n    lang_capture: lang",
			0,
		},
		{
			"lang_capture with languages",
			"pattern: '(x) @body'\n    text_capture: body\n    lang_capture: lang\n    languages:\n      sql: sql",
			1,
		},
		{
			// An explicit empty map is a declaration, not an omission: "claim these
			// bodies and map none of them". It is how a <style lang="..."> block
			// refuses every preprocessor without letting the generic block behind it
			// parse SCSS as CSS.
			"lang_capture with an explicitly empty languages map",
			"pattern: '(x) @body'\n    text_capture: body\n    lang_capture: lang\n    languages: {}",
			1,
		},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			doc := "language: x\nembedded:\n  - " + tc.yaml + "\n"
			qf, _ := parseQueryFile([]byte(doc), "test.yaml")
			if len(qf.Embedded) != tc.keep {
				t.Errorf("kept %d embedded block(s), want %d: %+v", len(qf.Embedded), tc.keep, qf.Embedded)
			}
		})
	}
}

// Order is the whole mechanism for an optional attribute, so it gets its own test:
// the specific block claims the body it matched, and the generic block behind it
// must not take it again. Without the claim, a `<style lang="scss">` would be
// skipped by the specific block and then parsed as CSS by the generic one — the
// exact wrong answer the allowlist exists to prevent.
func TestFirstMatchingBlockClaimsTheBodyAndTheRestSkipIt(t *testing.T) {
	projectDir := stageEmbedded(t, "vue", "tree-sitter-vue", ".vue", "vue.yaml",
		"javascript.yaml", "typescript.yaml", "css.yaml")

	pf := parseFixture(t, projectDir, "Claim.vue", `<template>
  <div class="card">x</div>
</template>

<script setup lang="ts">
interface Props { n: number }
</script>

<style scoped lang="scss">
$brand: red;
.card { color: $brand; }
</style>
`)
	// The specific script block won: TypeScript ran, not the generic block's
	// javascript default. `interface` is TypeScript-only, so this is proof.
	if _, ok := entityAt(pf, "Interface", "Props"); !ok {
		t.Errorf("the lang=\"ts\" block did not win; entities: %v", entityLabelsOf(pf))
	}
	// The specific style block matched, mapped nothing for scss, and CLAIMED the
	// body — so the generic style block must not have parsed it as CSS.
	if _, ok := entityAt(pf, "CssClass", "card"); ok {
		t.Error("the scss body fell through to the generic block and was parsed as CSS")
	}
	if _, ok := entityAt(pf, "CssProperty", "color"); ok {
		t.Error("the scss body fell through to the generic block and was parsed as CSS")
	}
}

// And the generic block still fires when the specific one does not match.
func TestGenericBlockFiresWhenTheAttributeIsAbsent(t *testing.T) {
	projectDir := stageEmbedded(t, "vue", "tree-sitter-vue", ".vue", "vue.yaml",
		"javascript.yaml", "css.yaml")

	pf := parseFixture(t, projectDir, "Plain.vue", `<template>
  <div class="card">x</div>
</template>

<script setup>
const total = 1
</script>

<style scoped>
.card { color: red; }
</style>
`)
	if _, ok := entityAt(pf, "Variable", "total"); !ok {
		t.Errorf("the generic script block did not fire; entities: %v", entityLabelsOf(pf))
	}
	if _, ok := entityAt(pf, "CssClass", "card"); !ok {
		t.Error("the generic style block did not fire")
	}
}

// A pattern that does not compile is a block that never fires, in silence — the
// same failure mode a broken query has. The shipped files must not carry one.
func TestEveryShippedEmbeddedPatternCompiles(t *testing.T) {
	files, err := loadQueriesFromDir("queries")
	if err != nil {
		t.Fatalf("load queries: %v", err)
	}
	for _, qf := range files {
		var patterned []EmbeddedBlock
		for _, blk := range qf.Embedded {
			if blk.Pattern != "" {
				patterned = append(patterned, blk)
			}
		}
		if len(patterned) == 0 {
			continue
		}
		grammar := qf.Grammar
		if grammar == "" {
			grammar = "tree-sitter-" + qf.Language
		}
		lang, err := resolveTreeSitterLang(qf.Language, grammar)
		if err != nil || lang == nil {
			t.Logf("skip %s: grammar unavailable", qf.Language)
			continue
		}
		for _, blk := range patterned {
			q, err := compileEmbeddedPattern(lang, blk.Pattern)
			if err != nil {
				t.Errorf("%s: embedded pattern does not compile: %v", qf.Language, err)
				continue
			}
			if captureIndex(q, blk.TextCapture) < 0 {
				t.Errorf("%s: embedded text_capture %q is not in the pattern",
					qf.Language, blk.TextCapture)
			}
		}
	}
}
