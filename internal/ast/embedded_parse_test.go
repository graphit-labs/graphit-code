package ast

import (
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	sitter "github.com/tree-sitter/go-tree-sitter"
)

// A single-file component mixes languages in one file, and tree-sitter-vue and
// tree-sitter-svelte hand the body of <script> and <style> over as one opaque
// raw_text. Until the sub-parse existed, a .vue contributed no IMPORTS edge, no
// entity from the script, and no export flag — the body was searchable as text
// (file_fts indexes the whole source) but had no structure at all.
//
// The part that fails silently is the LINE OFFSET: the inner parse sees only the
// block's own text, so every line it reports is relative to the block. Every
// fixture here therefore puts the script AFTER the template — a fixture with the
// script at the top passes with a broken offset of zero.

// stageEmbedded stages the outer language's query file plus the query files of
// every language its blocks embed. The inner languages have to be resolvable from
// the PROJECT directory: the runtime install is not guaranteed to be present, and
// a test that silently falls back to it is testing the machine, not the code.
func stageEmbedded(t *testing.T, langName, grammar, ext, queryFile string, inner ...string) string {
	t.Helper()
	projectDir := stageGrammar(t, langName, grammar, ext, queryFile)
	qdir := filepath.Join(projectDir, brand.DotDir(), "ast", "queries")
	for _, f := range inner {
		body, err := os.ReadFile(filepath.Join("queries", f))
		if err != nil {
			t.Skipf("no %s: %v", f, err)
		}
		if err := os.WriteFile(filepath.Join(qdir, f), body, 0o644); err != nil {
			t.Fatal(err)
		}
	}
	return projectDir
}

// entityAt finds an entity by label and name, returning its line span.
func entityAt(pf *ParsedFile, label, name string) (Entity, bool) {
	for _, ents := range pf.Entities {
		for _, e := range ents {
			if e.GraphLabel == label && e.Name == name {
				return e, true
			}
		}
	}
	return Entity{}, false
}

func wantEntityLine(t *testing.T, pf *ParsedFile, label, name string, line int) {
	t.Helper()
	e, ok := entityAt(pf, label, name)
	if !ok {
		var have []string
		for _, ents := range pf.Entities {
			for _, x := range ents {
				have = append(have, x.GraphLabel+":"+x.Name)
			}
		}
		t.Errorf("no %s node named %q; have %v", label, name, have)
		return
	}
	if e.Line != line {
		t.Errorf("%s %q is at line %d, want %d", label, name, e.Line, line)
	}
}

// The <script> is at the END of the file and the import is on line 8, so a
// line offset of zero reports line 2 and the assertion catches it.
const vueSFCFixture = `<template>
  <div class="wrap">
    <h1>{{ title }}</h1>
  </div>
</template>

<script setup>
import { ref } from 'vue'

const title = ref('Pedidos')

function reload() {
  title.value = 'x'
}
</script>
`

func TestVueScriptBodyContributesImportsAndEntitiesAtAbsoluteLines(t *testing.T) {
	projectDir := stageEmbedded(t, "vue", "tree-sitter-vue", ".vue", "vue.yaml", "javascript.yaml")
	pf := parseFixture(t, projectDir, "App.vue", vueSFCFixture)

	// The IMPORTS edge is built from an entity under the "imports" data key, and
	// the statement's own line is what makes it navigable.
	imp, ok := entityAt(pf, "Import", "vue")
	if !ok {
		t.Fatalf("no Import entity for 'vue'; entities: %v", entityLabelsOf(pf))
	}
	if imp.Line != 8 {
		t.Errorf("import of 'vue' is at line %d, want 8 (line offset not applied)", imp.Line)
	}

	// A function declared in the script is a Function at its absolute line.
	wantEntityLine(t, pf, "Function", "reload", 12)

	// The markup entity is untouched: the Element and its attributes survive.
	if _, ok := entityAt(pf, "Element", "script"); !ok {
		t.Error("the script Element disappeared; the outer parse must be unchanged")
	}
	if _, ok := entityAt(pf, "Attribute", "setup"); !ok {
		t.Error("the <script setup> attribute disappeared")
	}
	if _, ok := entityAt(pf, "Element", "h1"); !ok {
		t.Error("the template markup disappeared")
	}
}

// A <script lang="ts"> selects a different grammar for the same block.
func TestVueScriptLangAttributeSelectsTypeScript(t *testing.T) {
	projectDir := stageEmbedded(t, "vue", "tree-sitter-vue", ".vue", "vue.yaml",
		"javascript.yaml", "typescript.yaml")
	pf := parseFixture(t, projectDir, "Typed.vue", `<template>
  <p>{{ n }}</p>
</template>

<script setup lang="ts">
import type { Ref } from 'vue'

interface Props {
  n: number
}
</script>
`)
	if _, ok := entityAt(pf, "Import", "vue"); !ok {
		t.Errorf("no Import for 'vue' from a lang=\"ts\" block; entities: %v", entityLabelsOf(pf))
	}
	// `interface` is TypeScript-only: seeing it proves the ts grammar ran, not js.
	wantEntityLine(t, pf, "Interface", "Props", 8)
}

// A <style> with no lang attribute is CSS.
func TestVueStyleBodyIsParsedAsCSS(t *testing.T) {
	projectDir := stageEmbedded(t, "vue", "tree-sitter-vue", ".vue", "vue.yaml",
		"javascript.yaml", "css.yaml")
	pf := parseFixture(t, projectDir, "Styled.vue", `<template>
  <div class="card">x</div>
</template>

<style scoped>
.card {
  color: red;
}
</style>
`)
	if _, ok := entityAt(pf, "CssClass", "card"); !ok {
		t.Errorf("no CssClass for '.card' from the <style> body; entities: %v", entityLabelsOf(pf))
	}
	wantEntityLine(t, pf, "CssProperty", "color", 7)
}

// A language with no registered grammar is skipped, silently, and the rest of the
// file still indexes. This is the second acceptance scenario.
func TestVueStyleWithUnknownLangIsSkippedWithoutError(t *testing.T) {
	projectDir := stageEmbedded(t, "vue", "tree-sitter-vue", ".vue", "vue.yaml",
		"javascript.yaml", "css.yaml")
	pf := parseFixture(t, projectDir, "Scss.vue", `<template>
  <div class="card">x</div>
</template>

<script setup>
import { ref } from 'vue'
</script>

<style scoped lang="scss">
$brand: red;
.card { color: $brand; }
</style>
`)
	// Nothing from the scss body.
	if _, ok := entityAt(pf, "CssClass", "card"); ok {
		t.Error("the scss body produced a CssClass; it has no grammar and must be skipped")
	}
	// The rest of the file is indexed normally.
	if _, ok := entityAt(pf, "Import", "vue"); !ok {
		t.Error("the script block was lost because the style block was skipped")
	}
	if _, ok := entityAt(pf, "Element", "style"); !ok {
		t.Error("the style Element disappeared")
	}
}

func TestSvelteScriptBodyContributesImportsAtAbsoluteLines(t *testing.T) {
	projectDir := stageEmbedded(t, "svelte", "tree-sitter-svelte", ".svelte", "svelte.yaml",
		"javascript.yaml", "css.yaml")
	pf := parseFixture(t, projectDir, "Card.svelte", `<div class="card">
  <h2>{ title }</h2>
</div>

<script>
import { onMount } from 'svelte'

export let title = 'Card'

function ping() {
  return title
}
</script>

<style>
.card { color: red; }
</style>
`)
	imp, ok := entityAt(pf, "Import", "svelte")
	if !ok {
		t.Fatalf("no Import entity for 'svelte'; entities: %v", entityLabelsOf(pf))
	}
	if imp.Line != 6 {
		t.Errorf("import of 'svelte' is at line %d, want 6", imp.Line)
	}
	wantEntityLine(t, pf, "Function", "ping", 10)
	if _, ok := entityAt(pf, "CssClass", "card"); !ok {
		t.Errorf("no CssClass from the <style> body; entities: %v", entityLabelsOf(pf))
	}
}

// HTML was left out of the first design draft on the grounds that html.yaml is
// stable and the gain is smaller. It is in because enabling it needed no change to
// a single existing html assertion — the condition the scope set. The discriminator
// there is `type`, not `lang`: HTML has no `lang` on a script, and only the values
// that ARE JavaScript are mapped, so a JSON payload or an import map is skipped
// instead of being parsed as code.
func TestHTMLInlineScriptAndStyleAreParsed(t *testing.T) {
	projectDir := stageEmbedded(t, "html", "tree-sitter-html", ".html", "html.yaml",
		"javascript.yaml", "css.yaml")
	pf := parseFixture(t, projectDir, "page.html", `<!DOCTYPE html>
<html>
<head>
  <style>
    .card { color: red; }
  </style>
</head>
<body>
  <script type="application/json">
    {"not": "code"}
  </script>
  <script type="module">
    import { boot } from './boot.js'
    function start() { boot() }
  </script>
</body>
</html>
`)
	imp, ok := entityAt(pf, "Import", "./boot.js")
	if !ok {
		t.Fatalf("no Import from the inline module; entities: %v", entityLabelsOf(pf))
	}
	if imp.Line != 13 {
		t.Errorf("the inline import is at line %d, want 13", imp.Line)
	}
	wantEntityLine(t, pf, "Function", "start", 14)
	if _, ok := entityAt(pf, "CssClass", "card"); !ok {
		t.Error("no CssClass from the inline <style>")
	}
	// The JSON payload is not code, and `type` is what says so.
	if _, ok := entityAt(pf, "Variable", "not"); ok {
		t.Error("the application/json body was parsed as JavaScript")
	}
}

func entityLabelsOf(pf *ParsedFile) []string {
	var out []string
	for _, ents := range pf.Entities {
		for _, e := range ents {
			out = append(out, e.GraphLabel+":"+e.Name+"@"+strconv.Itoa(e.Line))
		}
	}
	return out
}

// The offset and the merge, as units

// shiftParsedLines is the one place the offset is applied, so it is the one place
// worth testing directly: every entity, every call site and every reference moves,
// and nothing else does.
func TestShiftParsedLinesMovesEveryLineBearingRecord(t *testing.T) {
	pf := &ParsedFile{
		Entities: map[string][]Entity{
			"functions": {{Name: "go", Line: 2, EndLine: 4, GraphLabel: "Function"}},
			"imports":   {{Name: "vue", Line: 1, EndLine: 1, GraphLabel: "Import"}},
		},
		CallSites:  []CallInfo{{Name: "ref", Line: 3}},
		References: []ReferenceInfo{{TargetName: "title", Line: 2, RelType: "REFERENCES"}},
	}
	shiftParsedLines(pf, 6)

	if got := pf.Entities["functions"][0]; got.Line != 8 || got.EndLine != 10 {
		t.Errorf("function moved to %d..%d, want 8..10", got.Line, got.EndLine)
	}
	if got := pf.Entities["imports"][0]; got.Line != 7 || got.EndLine != 7 {
		t.Errorf("import moved to %d..%d, want 7..7", got.Line, got.EndLine)
	}
	if got := pf.CallSites[0].Line; got != 9 {
		t.Errorf("call site moved to %d, want 9", got)
	}
	if got := pf.References[0].Line; got != 8 {
		t.Errorf("reference moved to %d, want 8", got)
	}
}

// A zero offset is the block that starts on line 1, and it must be a no-op rather
// than an off-by-one.
func TestShiftParsedLinesZeroIsANoOp(t *testing.T) {
	pf := &ParsedFile{
		Entities: map[string][]Entity{"f": {{Name: "a", Line: 3, EndLine: 5}}},
	}
	shiftParsedLines(pf, 0)
	if got := pf.Entities["f"][0]; got.Line != 3 || got.EndLine != 5 {
		t.Errorf("zero shift moved %d..%d", got.Line, got.EndLine)
	}
}

// mergeParsedInto folds the inner parse into the outer one. An entity the outer
// parse already recorded must be completed, not duplicated — the same rule
// AddOrMergeEntity exists for.
func TestMergeParsedIntoFoldsEntitiesCallsAndReferences(t *testing.T) {
	outer := &ParsedFile{
		Entities: map[string][]Entity{
			"elements": {{Name: "script", Line: 7, EndLine: 15, GraphLabel: "Element"}},
		},
	}
	inner := &ParsedFile{
		Entities: map[string][]Entity{
			"imports":   {{Name: "vue", Line: 8, EndLine: 8, GraphLabel: "Import"}},
			"functions": {{Name: "reload", Line: 12, EndLine: 14, GraphLabel: "Function"}},
			// Same identity as the outer entity: label, name, context and line.
			"elements": {{Name: "script", Line: 7, EndLine: 15, GraphLabel: "Element"}},
		},
		CallSites:  []CallInfo{{Name: "ref", Line: 10}},
		References: []ReferenceInfo{{TargetName: "title", Line: 13}},
	}
	mergeParsedInto(outer, inner)

	if n := len(outer.Entities["elements"]); n != 1 {
		t.Errorf("the script Element was duplicated: %d rows", n)
	}
	if n := len(outer.Entities["imports"]); n != 1 {
		t.Errorf("imports merged as %d rows, want 1", n)
	}
	if _, ok := entityAt(outer, "Function", "reload"); !ok {
		t.Error("the inner function did not reach the outer parse")
	}
	if len(outer.CallSites) != 1 || outer.CallSites[0].Name != "ref" {
		t.Errorf("call sites merged as %v", outer.CallSites)
	}
	if len(outer.References) != 1 || outer.References[0].TargetName != "title" {
		t.Errorf("references merged as %v", outer.References)
	}
}

// The config, which fails OPEN

// A node kind that does not exist in the grammar matches nothing, in silence —
// the same failure mode a broken query pattern has. So a malformed `embedded`
// entry is dropped at load time with a warning, rather than kept as a block that
// can never fire.
func TestEmbeddedConfigDropsMalformedEntries(t *testing.T) {
	cases := []struct {
		name string
		yaml string
		keep int
	}{
		{"complete", "pattern: '(script_element (raw_text) @body)'\n    text_capture: body\n    default: javascript", 1},
		{"missing pattern", "text_capture: body\n    default: javascript", 0},
		{"missing text_capture", "pattern: '(script_element (raw_text) @body)'\n    default: javascript", 0},
		{"no default and no languages", "pattern: '(x) @body'\n    text_capture: body", 0},
		{"languages only", "pattern: '(x) @body'\n    text_capture: body\n    lang_capture: lang\n    languages:\n      ts: typescript", 1},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			doc := "language: x\nembedded:\n  - " + tc.yaml + "\n"
			qf, ok := parseQueryFile([]byte(doc), "test.yaml")
			if !ok && tc.keep > 0 {
				t.Fatalf("whole file rejected, want %d block(s)", tc.keep)
			}
			if len(qf.Embedded) != tc.keep {
				t.Errorf("kept %d embedded block(s), want %d", len(qf.Embedded), tc.keep)
			}
		})
	}
}

// A `languages` map with no `lang_capture` can never be consulted, so keeping it
// would advertise a mapping the engine cannot reach.
func TestEmbeddedConfigDropsUnreachableLanguagesMap(t *testing.T) {
	qf, ok := parseQueryFile([]byte(`language: x
embedded:
  - pattern: '(script_element (raw_text) @body)'
    text_capture: body
    default: javascript
    languages:
      ts: typescript
`), "test.yaml")
	if !ok {
		t.Fatal("file rejected")
	}
	if len(qf.Embedded) != 1 {
		t.Fatalf("kept %d blocks, want 1", len(qf.Embedded))
	}
	if len(qf.Embedded[0].Languages) != 0 {
		t.Errorf("kept an unreachable languages map: %v", qf.Embedded[0].Languages)
	}
}

// Language keys are matched case-insensitively, so they are normalised once at
// load time rather than at every lookup.
func TestEmbeddedConfigLowercasesLanguageKeys(t *testing.T) {
	qf, _ := parseQueryFile([]byte(`language: x
embedded:
  - pattern: '(script_element (raw_text) @body)'
    text_capture: body
    lang_capture: lang
    languages:
      TS: typescript
`), "test.yaml")
	if len(qf.Embedded) != 1 {
		t.Fatalf("kept %d blocks, want 1", len(qf.Embedded))
	}
	if qf.Embedded[0].Languages["ts"] != "typescript" {
		t.Errorf("languages = %v, want key 'ts'", qf.Embedded[0].Languages)
	}
}

// The same guard TestEveryShippedQueryPatternCompiles gives patterns: a node kind
// that the grammar does not have is a block that never fires, and nothing else
// reports it.
func TestEveryShippedEmbeddedBlockNamesRealGrammarNodes(t *testing.T) {
	files, err := loadQueriesFromDir("queries")
	if err != nil {
		t.Fatalf("load queries: %v", err)
	}
	// Resolved against the SHIPPED set, not against the installed runtime: a
	// machine with no runtime extraction would otherwise fail this for a reason
	// that has nothing to do with the query files under test.
	shipped := make(map[string]bool, len(files))
	for _, qf := range files {
		if qf.Parser != "antlr4" {
			shipped[strings.ToLower(qf.Language)] = true
		}
	}

	checked := 0
	for _, qf := range files {
		if len(qf.Embedded) == 0 {
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
		for _, blk := range qf.Embedded {
			checked++
			q, qErr := compileEmbeddedPattern(lang, blk.Pattern)
			if qErr != nil {
				t.Errorf("%s: embedded pattern does not compile against %s: %v",
					qf.Language, grammar, qErr)
				continue
			}
			if captureIndex(q, blk.TextCapture) < 0 {
				t.Errorf("%s: embedded text_capture %q is not in the pattern",
					qf.Language, blk.TextCapture)
			}
			if blk.LangCapture != "" && captureIndex(q, blk.LangCapture) < 0 {
				t.Errorf("%s: embedded lang_capture %q is not in the pattern",
					qf.Language, blk.LangCapture)
			}
			// The languages a block names must be languages that exist, or the
			// mapping is a promise the engine cannot keep.
			for attr, target := range blk.Languages {
				if !shipped[strings.ToLower(target)] {
					t.Errorf("%s: embedded lang=%q maps to unknown language %q",
						qf.Language, attr, target)
				}
			}
			if blk.Default != "" && !shipped[strings.ToLower(blk.Default)] {
				t.Errorf("%s: embedded default language %q is unknown",
					qf.Language, blk.Default)
			}
		}
	}
	if checked == 0 {
		t.Fatal("no embedded blocks found in the shipped query files")
	}
}

// The three HTML-shaped grammars agree on how an attribute is written, and the
// shipped patterns depend on that agreement. This pins it: for each of them, the
// `lang`/`type` pattern must select the body when the attribute is present, and the
// generic pattern must select it when it is not. A grammar bump that breaks the
// shape becomes a failing test rather than a language silently resolved from
// `default`.
func TestShippedPatternsSelectWithAndWithoutTheAttribute(t *testing.T) {
	cases := []struct {
		lang, grammar, attr, withAttr, without string
	}{
		{"vue", "tree-sitter-vue", "lang",
			"<script setup lang=\"ts\">\nx\n</script>\n", "<script setup>\nx\n</script>\n"},
		{"svelte", "tree-sitter-svelte", "lang",
			"<script lang=\"ts\">\nx\n</script>\n", "<script>\nx\n</script>\n"},
		{"html", "tree-sitter-html", "type",
			"<script type=\"module\">\nx\n</script>\n", "<script>\nx\n</script>\n"},
	}
	for _, tc := range cases {
		lang, err := resolveTreeSitterLang(tc.lang, tc.grammar)
		if err != nil || lang == nil {
			t.Skipf("%s grammar unavailable", tc.lang)
		}
		specific := "(script_element (start_tag (attribute (attribute_name) @_a " +
			"(quoted_attribute_value (attribute_value) @lang))) (raw_text) @body (#eq? @_a \"" + tc.attr + "\"))"
		generic := "(script_element (raw_text) @body)"

		if n := countPatternMatches(t, lang, specific, tc.withAttr); n != 1 {
			t.Errorf("%s: specific pattern matched %d times with the attribute, want 1", tc.lang, n)
		}
		if n := countPatternMatches(t, lang, specific, tc.without); n != 0 {
			t.Errorf("%s: specific pattern matched %d times without the attribute, want 0", tc.lang, n)
		}
		if n := countPatternMatches(t, lang, generic, tc.without); n != 1 {
			t.Errorf("%s: generic pattern matched %d times, want 1", tc.lang, n)
		}
	}
}

func countPatternMatches(t *testing.T, lang *sitter.Language, pattern, source string) int {
	t.Helper()
	q, qErr := sitter.NewQuery(lang, pattern)
	if qErr != nil {
		t.Fatalf("compile %q: %v", pattern, qErr)
	}
	p := sitter.NewParser()
	if err := p.SetLanguage(lang); err != nil {
		t.Fatal(err)
	}
	src := []byte(source)
	tree := p.Parse(src, nil)
	if tree == nil {
		t.Fatal("nil tree")
	}
	defer tree.Close()
	qc := sitter.NewQueryCursor()
	m := qc.Matches(q, tree.RootNode(), src)
	n := 0
	for m.Next() != nil {
		n++
	}
	return n
}

// Mapping a NEW language onto an embedded block has to be YAML alone — not one line
// of Go. This test is what pins that promise: it adds ONE line to the
// `languages:` do bloco de script do html.yaml e espera entidades da linguagem
// new language, at its absolute line.
//
// The only condition is that the language exists, meaning it has a query file: that
// is where the extension registration comes from, and `tsLangConfigByName` resolves by
// name out of the same tables. A language with no query file is skipped silently, as
// `lang="scss"`.
func TestMappingANewInnerLanguageIsYAMLOnly(t *testing.T) {
	projectDir := stageEmbedded(t, "html", "tree-sitter-html", ".html", "html.yaml",
		"javascript.yaml", "css.yaml", "json.yaml")

	qpath := filepath.Join(projectDir, brand.DotDir(), "ast", "queries", "html.yaml")
	body, err := os.ReadFile(qpath)
	if err != nil {
		t.Fatal(err)
	}
	doc := strings.Replace(string(body),
		"      module: javascript",
		"      module: javascript\n      application/json: json", 1)
	if doc == string(body) {
		t.Fatal("html.yaml no longer maps `module: javascript` on the script block")
	}
	if err := os.WriteFile(qpath, []byte(doc), 0o644); err != nil {
		t.Fatal(err)
	}

	pf := parseFixture(t, projectDir, "page.html", `<html>
<body>
  <script type="application/json">
    { "feature": "flags" }
  </script>
</body>
</html>
`)
	// json.yaml's own labels, from a body html.yaml previously skipped.
	wantEntityLine(t, pf, "Pair", "feature", 4)
	wantEntityLine(t, pf, "Value", "flags", 4)
	// And the markup is untouched.
	if _, ok := entityAt(pf, "AttributeValue", "application/json"); !ok {
		t.Error("the type attribute disappeared")
	}
}

// A body that is one statement used to be the only case that produced anything at
// all — as a Text node, because dataText rejects an interior newline. That Text
// node is still written, so the element's text stays searchable alongside the
// structure now extracted from it.
func TestSingleStatementScriptKeepsItsTextNodeAndGainsStructure(t *testing.T) {
	projectDir := stageEmbedded(t, "vue", "tree-sitter-vue", ".vue", "vue.yaml", "javascript.yaml")
	pf := parseFixture(t, projectDir, "One.vue", `<template>
  <p>x</p>
</template>

<script>const a = 1;</script>
`)
	if _, ok := entityAt(pf, "Text", "const a = 1;"); !ok {
		t.Errorf("the element's Text node was lost; entities: %v", entityLabelsOf(pf))
	}
	wantEntityLine(t, pf, "Variable", "a", 5)
}

// Sanity: the block's own language must not be re-entered, or a language that
// embeds itself parses forever.
func TestEmbeddedParseDoesNotRecurseIntoItself(t *testing.T) {
	projectDir := stageGrammar(t, "vue", "tree-sitter-vue", ".vue", "vue.yaml")
	qdir := filepath.Join(projectDir, brand.DotDir(), "ast", "queries")
	body, err := os.ReadFile(filepath.Join("queries", "vue.yaml"))
	if err != nil {
		t.Skip("no vue.yaml")
	}
	doc := strings.Replace(string(body), "default: javascript", "default: vue", 1)
	if doc == string(body) {
		t.Skip("vue.yaml no longer declares a javascript default")
	}
	if err := os.WriteFile(filepath.Join(qdir, "vue.yaml"), []byte(doc), 0o644); err != nil {
		t.Fatal(err)
	}
	// Completing at all is the assertion.
	parseFixture(t, projectDir, "Loop.vue", `<template>
  <p>x</p>
</template>

<script>
<template><b>y</b></template>
</script>
`)
}
