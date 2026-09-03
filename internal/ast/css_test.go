package ast

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func filepathGlobQueries() ([]string, error) {
	return filepath.Glob(filepath.Join("queries", "*.yaml"))
}

func osReadFile(path string) ([]byte, error) { return os.ReadFile(path) }

// CSS had a grammar registered in treesitter_native.go and no query file at all,
// so every .css file in every indexed project resolved a parser and produced
// nothing: no selector, no property, no value, no @import edge.
func TestCSSIsExtracted(t *testing.T) {
	projectDir := stageGrammar(t, "css", "tree-sitter-css", ".css", "css.yaml")
	pf := parseFixture(t, projectDir, "site.css", `@import url("reset.css");

:root {
  --brand-color: #ff6600;
}

.card, #main {
  color: var(--brand-color);
  display: none;
}

a:hover::after {
  content: "arrow";
}

nav {
  padding: 4px;
}

@media (max-width: 600px) {
  .sidebar { display: none; }
}

@keyframes fade { from { opacity: 0; } }

@font-face {
  font-family: "Acme";
  src: url("acme.woff2");
}

[data-state="open"] { color: red; }
`)
	got := nodesOf(pf)

	for _, want := range [][2]string{
		{"CssClass", "card"},
		{"CssClass", "sidebar"},
		{"CssId", "main"},
		{"CssElement", "a"},
		{"CssElement", "nav"},
		{"CssPseudoClass", "hover"},
		{"CssPseudoClass", "root"},
		{"CssPseudoElement", "after"},
		{"CssProperty", "color"},
		{"CssProperty", "display"},
		{"CssVariable", "--brand-color"},
		{"Keyframes", "fade"},
		{"MediaFeature", "max-width"},
		{"AtRule", "@font-face"},
		{"Attribute", "data-state"},
	} {
		if !got[want].present {
			t.Errorf("no %s node named %q", want[0], want[1])
		}
	}

	if got[[2]string{"CssClass", "root"}].present {
		t.Error("the pseudo-class :root was indexed as a CssClass")
	}
	if got[[2]string{"CssClass", "hover"}].present {
		t.Error("the pseudo-class :hover was indexed as a CssClass")
	}

	wantNode(t, got, "Value", "none", "display", "CssProperty")
	wantNode(t, got, "Value", "#ff6600", "--brand-color", "CssVariable")
	wantNode(t, got, "Value", "arrow", "content", "CssProperty")
	wantNode(t, got, "Value", "600px", "max-width", "MediaFeature")
	wantNode(t, got, "AttributeValue", "open", "data-state", "Attribute")

	var sawImport bool
	for _, e := range pf.Entities["imports"] {
		if e.Name == "reset.css" {
			sawImport = true
		}
	}
	if !sawImport {
		t.Errorf("@import url(\"reset.css\") produced no import; got %v", pf.Entities["imports"])
	}
	entry := ConvertToCache(pf, projectDir, false, "")
	if entry == nil {
		t.Fatal("no cache entry")
	}
	var importedModule bool
	for _, imp := range entry.Imports {
		if imp.ModuleName == "reset.css" {
			importedModule = true
		}
	}
	if !importedModule {
		t.Errorf("no IMPORTS edge to reset.css; got %+v", entry.Imports)
	}

	refs := map[string]bool{}
	for _, r := range pf.References {
		refs[r.TargetName] = true
	}
	for _, want := range []string{"--brand-color", "acme.woff2"} {
		if !refs[want] {
			t.Errorf("no REFERENCES edge to %q; got %v", want, keysOfBool(refs))
		}
	}
}

func TestCSSPaddedFunctionReferencesAreNormalised(t *testing.T) {
	projectDir := stageGrammar(t, "css", "tree-sitter-css", ".css", "css.yaml")
	pf := parseFixture(t, projectDir, "padded.css", `:root {
  --brand-color: #ff6600;
}

.card {
  color: var( --brand-color );
}

@font-face {
  font-family: "Acme";
  src: url( " acme.woff2 " );
}
`)

	refs := map[string]bool{}
	for _, r := range pf.References {
		refs[r.TargetName] = true
	}
	for _, want := range []string{"--brand-color", "acme.woff2"} {
		if !refs[want] {
			t.Errorf("no REFERENCES edge to %q; got %v", want, keysOfBool(refs))
		}
	}
	for bad := range refs {
		if bad != strings.TrimSpace(bad) {
			t.Errorf("reference target %q carries whitespace, so it resolves to nothing", bad)
		}
	}
}

func keysOfBool(m map[string]bool) []string {
	out := make([]string, 0, len(m))
	for k := range m {
		out = append(out, k)
	}
	return out
}

// HTML was already extracted, but not to the same depth as the rest: the doctype
// and the body of an inline <script> or <style> were parsed and dropped, and an
// attribute on a script or style tag had to be proven rather than assumed —
// tree-sitter-html puts those under script_element/style_element, not element.
func TestHTMLDetailIsExtracted(t *testing.T) {
	projectDir := stageGrammar(t, "html", "tree-sitter-html", ".html", "html.yaml")
	pf := parseFixture(t, projectDir, "page.html", `<!DOCTYPE html>
<html lang="pt-BR">
<head>
  <title>Pedidos</title>
  <style>.card{color:red}</style>
</head>
<body>
  <input disabled type=text>
  <script src="/app.js"></script>
  <script>window.APP_ENV="prod";</script>
</body>
</html>
`)
	got := nodesOf(pf)

	if !got[[2]string{"Doctype", "<!DOCTYPE html>"}].present {
		var have []string
		for k := range got {
			if k[0] == "Doctype" {
				have = append(have, k[1])
			}
		}
		t.Errorf("no Doctype node; got %v", have)
	}

	wantNode(t, got, "Attribute", "src", "script", "Element")
	wantNode(t, got, "AttributeValue", "/app.js", "src", "Attribute")

	wantNode(t, got, "Attribute", "disabled", "input", "Element")
	wantNode(t, got, "Attribute", "type", "input", "Element")
	wantNode(t, got, "AttributeValue", "text", "type", "Attribute")

	wantNode(t, got, "Text", `window.APP_ENV="prod";`, "script", "Element")
	wantNode(t, got, "Text", ".card{color:red}", "style", "Element")

	wantNode(t, got, "Text", "Pedidos", "title", "Element")
	wantNode(t, got, "AttributeValue", "pt-BR", "lang", "Attribute")
}

// Svelte had the same defect CSS had: a vendored grammar registered in
// treesitter_native.go with no query file, so no extension was registered for it
// and .svelte files were never even discovered.
func TestSvelteIsExtracted(t *testing.T) {
	projectDir := stageGrammar(t, "svelte", "tree-sitter-svelte", ".svelte", "svelte.yaml")
	pf := parseFixture(t, projectDir, "Card.svelte", `<script>
  export let title = "Pedidos";
</script>

<div class="card" on:click={handleClick}>
  {title}
  {#if count > 0}<span>ok</span>{/if}
</div>
`)
	got := nodesOf(pf)

	wantNode(t, got, "Element", "div", "", "")
	wantNode(t, got, "Attribute", "class", "div", "Element")
	wantNode(t, got, "AttributeValue", "card", "class", "Attribute")
	wantNode(t, got, "Attribute", "on:click", "div", "Element")
	wantNode(t, got, "Value", "handleClick", "on:click", "Attribute")
	wantNode(t, got, "Condition", "count > 0", "div", "Element")

	var sawBinding bool
	for _, r := range pf.References {
		if r.TargetName == "title" {
			sawBinding = true
		}
	}
	if !sawBinding {
		t.Errorf("the {title} binding produced no reference; got %+v", pf.References)
	}
}

func TestSvelteBindingIsNormalised(t *testing.T) {
	projectDir := stageGrammar(t, "svelte", "tree-sitter-svelte", ".svelte", "svelte.yaml")
	pf := parseFixture(t, projectDir, "Padded.svelte", `<script>
  export let title = "Pedidos";
  export let subtitle = "Hoje";
</script>

<div class="card">
  { title }
  {subtitle}
</div>
`)

	targets := map[string]bool{}
	sources := map[string]string{}
	for _, r := range pf.References {
		targets[r.TargetName] = true
		sources[r.TargetName] = r.SourceName
	}
	for _, want := range []string{"title", "subtitle"} {
		if !targets[want] {
			t.Errorf("no REFERENCES edge to %q; got %v", want, keysOfBool(targets))
		}
		if got := sources[want]; got != "div" {
			t.Errorf("the {%s} binding is attributed to %q, want the div that renders it", want, got)
		}
	}
	for bad := range targets {
		if bad != strings.TrimSpace(bad) {
			t.Errorf("reference target %q carries whitespace, so it resolves to nothing", bad)
		}
	}
}

// A binding holding a string literal must not be reported as a reference to an
// identifier. The distinction is the whole point: `{ "foo" }` renders a constant and
// depends on nothing, while a reference to `foo` is an edge to a declaration that may
// well exist elsewhere for unrelated reasons — a false dependency, not a missing one.
//
// This is what declaring parent_capture on these patterns cost. It was added believing
// it was needed for two things, and needed for neither: the element already came from
// context_types, and trimming is unconditional in the adapter. What it did do was opt
// the captured name into dataText, which strips matched quote pairs.
func TestQuotedBindingIsNotAnIdentifierReference(t *testing.T) {
	t.Run("svelte", func(t *testing.T) {
		projectDir := stageGrammar(t, "svelte", "tree-sitter-svelte", ".svelte", "svelte.yaml")
		pf := parseFixture(t, projectDir, "Lit.svelte", "<div>\n  { \"foo\" }\n  { 'bar' }\n  { title }\n</div>\n")
		assertLiteralsAreNotIdentifiers(t, pf, `"foo"`, `'bar'`)
	})
	t.Run("vue", func(t *testing.T) {
		projectDir := stageGrammar(t, "vue", "tree-sitter-vue", ".vue", "vue.yaml")
		pf := parseFixture(t, projectDir, "Lit.vue", "<template>\n  <div>\n    {{ \"foo\" }}\n    {{ title }}\n  </div>\n</template>\n")
		assertLiteralsAreNotIdentifiers(t, pf, `"foo"`)
	})
}

func assertLiteralsAreNotIdentifiers(t *testing.T, pf *ParsedFile, wantLiterals ...string) {
	t.Helper()
	targets := map[string]bool{}
	for _, r := range pf.References {
		targets[r.TargetName] = true
	}
	if !targets["title"] {
		t.Errorf("the { title } binding produced no reference; got %v", keysOfBool(targets))
	}
	for _, lit := range wantLiterals {
		if !targets[lit] {
			t.Errorf("no reference target %s; got %v", lit, keysOfBool(targets))
		}
		bare := strings.Trim(lit, `"'`)
		if targets[bare] {
			t.Errorf("the literal %s was indexed as a reference to the identifier %q", lit, bare)
		}
	}
}

// Vue's own contribution to an otherwise HTML-shaped tree is directive_attribute,
// where the shorthand sigil is an ANONYMOUS token: `:label`, `@click` and `#footer`
// differ by nothing else. The sigil is also the separator of the long form, so
// `v-on:click` matches an unanchored `":" . (directive_value)` and an event handler
// gets reported as a prop — which is why the shorthand patterns anchor the sigil to
// the first child and the long forms go through a predicate on directive_name.
func TestVueIsExtracted(t *testing.T) {
	projectDir := stageGrammar(t, "vue", "tree-sitter-vue", ".vue", "vue.yaml")
	pf := parseFixture(t, projectDir, "TodoList.vue", `<template>
  <div class="list" @click="focus">
    <h1>{{ title }}</h1>
    <TodoItem
      v-for="item in items"
      :label="item.text"
      v-bind:done="item.done"
      @remove="onRemove(item.id)"
      v-on:edit="onEdit"
      v-if="visible"
    />
    <input v-model="draft" disabled>
    <template v-slot:footer><span #actions>ok</span></template>
  </div>
</template>

<script setup>
const title = "Pedidos";
</script>

<style scoped>.list{color:red}</style>
`)
	got := nodesOf(pf)

	wantNode(t, got, "Element", "div", "template", "Element")
	wantNode(t, got, "Element", "TodoItem", "div", "Element")
	wantNode(t, got, "Attribute", "class", "div", "Element")
	wantNode(t, got, "AttributeValue", "list", "class", "Attribute")
	wantNode(t, got, "Attribute", "setup", "script", "Element")

	wantNode(t, got, "Prop", "label", "TodoItem", "Element")
	wantNode(t, got, "Value", "item.text", "label", "Prop")
	wantNode(t, got, "EventHandler", "click", "div", "Element")
	wantNode(t, got, "Value", "focus", "click", "EventHandler")
	wantNode(t, got, "EventHandler", "remove", "TodoItem", "Element")
	wantNode(t, got, "Slot", "actions", "span", "Element")

	wantNode(t, got, "Prop", "done", "TodoItem", "Element")
	wantNode(t, got, "EventHandler", "edit", "TodoItem", "Element")
	wantNode(t, got, "Slot", "footer", "template", "Element")

	if got[[2]string{"Prop", "edit"}].present {
		t.Error("v-on:edit was indexed as a Prop")
	}
	if got[[2]string{"Prop", "footer"}].present {
		t.Error("v-slot:footer was indexed as a Prop")
	}

	wantNode(t, got, "Condition", "visible", "TodoItem", "Element")
	wantNode(t, got, "Loop", "item in items", "TodoItem", "Element")
	wantNode(t, got, "Directive", "v-model", "input", "Element")
	wantNode(t, got, "Value", "draft", "v-model", "Directive")

	wantNode(t, got, "Text", `const title = "Pedidos";`, "script", "Element")
	wantNode(t, got, "Text", ".list{color:red}", "style", "Element")

	refs := map[string]bool{}
	for _, r := range pf.References {
		refs[r.TargetName] = true
	}
	if !refs["title"] {
		t.Errorf("the {{ title }} interpolation produced no reference; got %v", keysOfBool(refs))
	}
}

func TestPaddedAttributeReferenceIsNormalised(t *testing.T) {
	projectDir := stageGrammar(t, "html", "tree-sitter-html", ".html", "html.yaml")
	pf := parseFixture(t, projectDir, "padded.html", `<div id=" main " class=" card ">
  <a href=" /pedidos ">ok</a>
</div>
`)

	targets := map[string]bool{}
	for _, r := range pf.References {
		targets[r.TargetName] = true
	}
	for _, want := range []string{"main", "card", "/pedidos"} {
		if !targets[want] {
			t.Errorf("no REFERENCES edge to %q; got %v", want, keysOfBool(targets))
		}
	}
	for bad := range targets {
		if bad != strings.TrimSpace(bad) {
			t.Errorf("reference target %q carries whitespace, so it resolves to nothing", bad)
		}
	}
}

var grammarsWithoutDefaultQueries = map[string]string{
	"markdown": "documents belong to the knowledge wiki, which chunks, links and ranks " +
		"prose; in the code graph they were a File node per page and a Heading node per " +
		"section. The grammar stays registered so a project that wants markdown structure " +
		"can add its own markdown.yaml, and so anything else in the framework can parse " +
		"markdown without reinstating the extension.",
}

// A grammar registered in treesitter_native.go with no query file is dead weight:
// no extension is registered for it, so files in that language are not discovered,
// and nothing reports the omission. CSS and Svelte both sat like that.
//
// c_sharp and proto are named differently in the two places on purpose —
// their query files declare `language: csharp` / `protobuf` with the
// real grammar in the `grammar` field — so they are matched through it.
func TestEveryNativeGrammarHasQueries(t *testing.T) {
	declared := map[string]bool{}
	files, err := filepathGlobQueries()
	if err != nil {
		t.Fatalf("glob: %v", err)
	}
	for _, path := range files {
		body, err := osReadFile(path)
		if err != nil {
			t.Fatalf("read %s: %v", path, err)
		}
		qf, ok := parseQueryFile(body, path)
		if !ok {
			continue
		}
		declared[qf.Language] = true
		if qf.Grammar != "" {
			declared[strings.TrimPrefix(qf.Grammar, "tree-sitter-")] = true
		}
	}

	var orphans []string
	for name := range nativeGrammars {
		if _, exempt := grammarsWithoutDefaultQueries[name]; exempt {
			continue
		}
		if !declared[name] && !declared[strings.ReplaceAll(name, "-", "_")] {
			orphans = append(orphans, name)
		}
	}
	if len(orphans) > 0 {
		t.Errorf("native grammars with no query file, so no extension is registered "+
			"and their files are never indexed: %v", orphans)
	}

	for name := range grammarsWithoutDefaultQueries {
		if _, ok := nativeGrammars[name]; !ok {
			t.Errorf("%s is exempted from needing a query file but is no longer a "+
				"registered grammar — drop the exemption, or restore the registration if "+
				"an override is still meant to be able to resolve it", name)
		}
	}
}

// No shipped query file may claim a markdown extension. Documents belong to the
// knowledge wiki, which chunks, links and ranks prose in ways a code graph cannot;
// in the graph they were a File node per page and a Heading node per section, and
// the ones outside the docs tree — README.md, CONTRIBUTING.md, AGENTS.md, every
// task log — were not covered by the ast.index_docs exclusion at all.
//
// The assertion is against the query files in the source tree, so it does not
// depend on what the launcher last unpacked into the developer's home.
//
// What this test deliberately does NOT assert is the absence of the grammar. It is
// registered, compiled in, and resolvable — see grammarsWithoutDefaultQueries.
// Extensions are what a query file grants, so removing the file is the whole of
// "not parsed for the AST", and it leaves the grammar available to a project that
// adds its own markdown.yaml and to anything else that needs to parse markdown.
func TestNoQueryFileClaimsMarkdown(t *testing.T) {
	files, err := filepathGlobQueries()
	if err != nil {
		t.Fatalf("glob: %v", err)
	}
	if len(files) == 0 {
		t.Fatal("no query files found — the glob is broken, not the language set")
	}

	for _, path := range files {
		body, err := osReadFile(path)
		if err != nil {
			t.Fatalf("read %s: %v", path, err)
		}
		qf, ok := parseQueryFile(body, path)
		if !ok {
			continue
		}
		for _, ext := range qf.Extensions {
			switch strings.ToLower(ext) {
			case ".md", ".markdown", ".mdx":
				t.Errorf("%s registers %s for language %q — that puts every document in "+
					"the code graph as a Heading node per section, including the ones "+
					"outside knowledge.docs_dir", filepath.Base(path), ext, qf.Language)
			}
		}
	}

	if _, ok := nativeGrammars["markdown"]; !ok {
		t.Error("the markdown grammar is no longer registered — the query file is what " +
			"grants extensions, so unregistering the grammar takes away the opt-in and " +
			"any other use of it without making the default any safer")
	}
}
