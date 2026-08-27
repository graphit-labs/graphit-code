package ast

import (
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"

	sitter "github.com/tree-sitter/go-tree-sitter"
)

// flatLanguages are the shipped grammars whose entities genuinely have no
// enclosing entity, so an empty context is the right answer rather than a
// forgotten one. Each needs a reason, because "I did not think about it" and
// "there is nothing to think about" look identical in a query file — and telling
// them apart is the whole point of this test.
var flatLanguages = map[string]string{
	"css": "selectors are declared at the top level of a stylesheet; the entities " +
		"are the selectors themselves, and a rule_set is named BY them",
	"clojure": "defn/def/defmacro forms are top-level; nesting them is not the idiom",
	"plpgsql": "the only entity is a DECLARE-section variable; a plpgsql source " +
		"has no function of its own to nest it under — that container is the " +
		"CREATE FUNCTION this grammar is spliced into from PostgreSQL's ANTLR " +
		"parser, not something this grammar's own tree can name",
}

// A grammar whose containers cannot be named leaves every entity attached to the
// File. That failure is silent: the index succeeds, the graph is merely wrong,
// and nothing says so. xml, json, yaml and svelte all shipped that way.
//
// A query file must therefore state, in one of the three ways the engine
// understands, how its entities find their parent:
//
//	context_types           the ancestor kinds that are containers
//	context_name_paths      how to name a container with no "name" field
//	parent_capture          containment declared in the pattern itself
//
// or be listed in flatLanguages with a reason.
func TestEveryShippedGrammarDeclaresItsContainment(t *testing.T) {
	files, err := filepath.Glob(filepath.Join("queries", "*.yaml"))
	if err != nil || len(files) == 0 {
		t.Fatalf("no query files: %v", err)
	}

	checked := 0
	for _, path := range files {
		body, err := os.ReadFile(path)
		if err != nil {
			t.Fatalf("read %s: %v", path, err)
		}
		qf, ok := parseQueryFile(body, path)
		if !ok {
			t.Errorf("%s: rejected by the loader", filepath.Base(path))
			continue
		}

		if reason := flatLanguages[qf.Language]; reason != "" {
			if len(qf.ContextTypes) > 0 {
				t.Errorf("%s: listed as flat (%s) but declares context_types — "+
					"remove it from flatLanguages", filepath.Base(path), reason)
			}
			continue
		}

		checked++
		if len(qf.ContextTypes) > 0 {
			continue
		}
		if anyEntityQueryDeclaresParent(qf) {
			continue
		}
		t.Errorf("%s: declares no containment. Its entities will all be attached "+
			"to the File. Declare context_types (plus context_name_paths if the "+
			"container has no \"name\" field), or parent_capture on the patterns, "+
			"or add %q to flatLanguages with a reason.",
			filepath.Base(path), qf.Language)
	}

	if checked == 0 {
		t.Fatal("no query file was checked — the glob or the loader is broken")
	}
	// An exemption for a grammar that no longer ships is an exemption nobody is
	// reading, and it would silently cover a future grammar that reuses the name.
	for lang := range flatLanguages {
		if _, err := os.Stat(filepath.Join("queries", lang+".yaml")); err != nil {
			t.Errorf("flatLanguages exempts %q, which ships no query file", lang)
		}
	}
	t.Logf("%d grammars checked, %d declared flat", checked, len(flatLanguages))
}

// callableContainerExemptions are rules that a query turns into a callable without
// being the node its parameters hang from, so declaring them as contexts would name
// the wrong owner rather than a missing one. Keyed "language:rule".
var callableContainerExemptions = map[string]string{
	"haskell:data_constructor": "a data constructor's fields are attributed to the " +
		"data_type around it, which is already a declared context",
	// These two were exempted on a reason that turned out to be FALSE — that the
	// parameters resolved through a nearer declared context. They did not: julia lost
	// them to an empty context and r attributed them to a Function named after the
	// `function` keyword. Both now state containment with parent_capture on the
	// Parameter query, which is why the exemption is about the CALLABLE query only.
	"julia:assignment": "a bare assignment is not a container; the short form's " +
		"parameters are placed by parent_capture, not by the ancestor walk",
	"r:binary_operator": "every r binary operator is one of these, so it cannot be a " +
		"container; parameters are placed by parent_capture instead",
}

// TestEveryCallableContainerIsDeclaredAsAContext is the narrow half of the check
// above, and the one that catches real damage.
//
// TestEveryShippedGrammarDeclaresItsContainment asks whether a grammar declared ANY
// containment. That passes as soon as one rule is listed, which is how plsql.yaml
// shipped with eleven contexts and still lost a third of its parameters: it declares
// procedures from procedure_body AND from procedure_spec — a package spec uses the
// latter — but only procedure_body was a context. resolveParentContextAntlr walked
// past every spec-declared subprogram to the package around it, so 9052 parameters
// of one Oracle export were owned by a Package instead of the Procedure or Function
// that declares them, colliding the uids of same-named parameters across the package.
//
// Callables are singled out because their case is not merely wrong but lossy:
// ConvertToCache DISCARDS a parameter whose context is empty, so a callable that is
// not a context can cost the parameter entirely rather than misfile it.
func TestEveryCallableContainerIsDeclaredAsAContext(t *testing.T) {
	callable := map[string]bool{
		"Function": true, "Method": true, "Procedure": true,
		"StoredProcedure": true, "Constructor": true,
	}

	files, err := filepath.Glob(filepath.Join("queries", "*.yaml"))
	if err != nil || len(files) == 0 {
		t.Fatalf("no query files: %v", err)
	}

	checked := 0
	for _, path := range files {
		body, err := os.ReadFile(path)
		if err != nil {
			t.Fatalf("read %s: %v", path, err)
		}
		qf, ok := parseQueryFile(body, path)
		if !ok || flatLanguages[qf.Language] != "" {
			continue
		}

		anon := make(map[string]bool, len(qf.AnonFuncTypes))
		for _, a := range qf.AnonFuncTypes {
			anon[a] = true
		}

		for _, q := range qf.Queries {
			if strings.EqualFold(q.Type, "relation") || !callable[q.GraphLabel] {
				continue
			}
			// Containment stated in the pattern needs no ancestor walk.
			if q.ParentCapture != "" {
				continue
			}
			rule := containerRuleOf(q.Pattern, qf.Parser)
			if rule == "" {
				continue
			}
			if _, declared := qf.ContextTypes[rule]; declared {
				checked++
				continue
			}
			// An anonymous function assigned to a name is resolved by anonHit
			// through the variable_declarator, not by context_types.
			if patternMentionsAnyKind(q.Pattern, anon) {
				continue
			}
			if reason := callableContainerExemptions[qf.Language+":"+rule]; reason != "" {
				continue
			}
			t.Errorf("%s: %s is declared from %q, which is not in context_types. "+
				"Parameters of these callables will be attributed to whatever "+
				"encloses them — or dropped, since a parameter with no owner is "+
				"discarded. Add %q to context_types, or to "+
				"callableContainerExemptions with a reason.",
				filepath.Base(path), q.GraphLabel, rule, rule)
		}
	}

	if checked == 0 {
		t.Fatal("no callable container was verified — the glob or the loader is broken")
	}
	t.Logf("%d callable declarations verified against context_types", checked)
}

// nonCallableContainerLabels are the labels that own columns, fields, attributes and
// nested declarations. Undeclared, they misfile that content instead of losing it —
// which is why they get their own, weaker check.
var nonCallableContainerLabels = map[string]bool{
	"Class": true, "Struct": true, "Interface": true, "Trait": true, "Package": true,
	"Type": true, "Trigger": true, "Table": true, "View": true, "Enum": true,
	"Namespace": true, "Object": true, "Record": true, "Protocol": true, "Mixin": true,
	"Extension": true, "MaterializedView": true, "Program": true, "Section": true,
	"Pair": true, "Mapping": true, "Schema": true, "Implementation": true,
	"DataType": true, "TypeClass": true, "Paragraph": true, "FileDescription": true,
	"ArrayTable": true, "Element": true,
}

// nonCallableExemptions are rules that produce a container entity without BEING the
// node its content hangs from. Keyed "language:rule". Every entry states why, because
// the whole value of this test is telling "considered and fine" apart from "forgotten".
var nonCallableExemptions = map[string]string{
	// Wrapper nodes: something nearer IS declared, so the walk stops there first.
	"c:type_definition":                 "struct_specifier/enum_specifier sit inside and are declared",
	"cpp:type_definition":               "struct_specifier/class_specifier sit inside and are declared",
	"go:type_declaration":               "type_spec sits inside and is declared",
	"plsql:type_definition":             "create_type encloses it and is declared",
	"scala:type_definition":             "a type alias has no nested entities",
	"rust:type_item":                    "a type alias has no nested entities",
	"typescript:type_alias_declaration": "a type alias has no nested entities",
	"tsx:type_alias_declaration":        "a type alias has no nested entities",

	// A package/namespace DECLARATION is a sibling of the declarations it labels, not
	// their parent node, so making it a context would attribute by file position rather
	// than by containment.
	"java:package_declaration": "sibling of the type declarations, not their parent",
	"kotlin:package_header":    "sibling of the declarations, not their parent",
	"scala:package_clause":     "sibling of the declarations, not their parent",
	"protobuf:package":         "sibling of the message declarations, not their parent",

	// A JS/TS pair is captured ONLY when its value is a literal, so a captured pair
	// never contains another captured pair — unlike json.yaml, which captures every
	// pair and therefore does declare `pair` as a context. What owns these is the
	// nearest named declaration, which is why `lexical_declaration` IS a context here:
	// `const X = { … }` owns its pairs, and a pair under an anonymous `export default`
	// belongs to the file, which is the honest answer for a config object.
	"javascript:pair": "only literal-valued pairs are captured, so one never nests in another",
	"typescript:pair": "only literal-valued pairs are captured, so one never nests in another",
	"tsx:pair":        "only literal-valued pairs are captured, so one never nests in another",

	// Self-naming tags: the enclosing `element` is the declared context, and resolve()
	// already skips a container that names the entity itself.
	"html:start_tag":          "element is the container; resolve() skips the self-name",
	"html:self_closing_tag":   "a self-closing tag contains nothing",
	"svelte:start_tag":        "element is the container; resolve() skips the self-name",
	"svelte:self_closing_tag": "a self-closing tag contains nothing",
	"vue:start_tag":           "element is the container; resolve() skips the self-name",
	"vue:self_closing_tag":    "a self-closing tag contains nothing",
	"xml:STag":                "element is the container; resolve() skips the self-name",
	"xml:EmptyElemTag":        "an empty-element tag contains nothing",

	// Nothing nests inside these.
	"postgresql:createextensionstmt": "CREATE EXTENSION declares no members",
	"toml:pair":                      "a toml pair holds a value, not nested pairs",
	"cobol85:entryStatement":         "a statement, not a container",
	"cobol85:programIdParagraph":     "programUnit is the container and is declared",
	"cobol85:procedureSectionHeader": "procedureSection is the container and is declared",
}

// TestEveryNonCallableContainerIsDeclaredAsAContext is the weaker half of the same
// audit: a Table, Type, Package or Element that is not a context does not lose its
// columns and fields the way a callable loses its parameters — it MISFILES them, onto
// whatever encloses it. Quieter, and still wrong.
//
// Two live bugs were found by running this audit by hand: toml declared `table` as a
// context with no context_name_paths, so every pair fell back to the File, and html had
// the same hole. Both are fixed and pinned by their own tests; this keeps the class from
// coming back.
func TestEveryNonCallableContainerIsDeclaredAsAContext(t *testing.T) {
	files, err := filepath.Glob(filepath.Join("queries", "*.yaml"))
	if err != nil || len(files) == 0 {
		t.Fatalf("no query files: %v", err)
	}

	checked := 0
	for _, path := range files {
		body, err := os.ReadFile(path)
		if err != nil {
			t.Fatalf("read %s: %v", path, err)
		}
		qf, ok := parseQueryFile(body, path)
		if !ok || flatLanguages[qf.Language] != "" {
			continue
		}

		for _, q := range qf.Queries {
			if strings.EqualFold(q.Type, "relation") || !nonCallableContainerLabels[q.GraphLabel] {
				continue
			}
			if q.ParentCapture != "" {
				continue
			}
			rule := containerRuleOf(q.Pattern, qf.Parser)
			if rule == "" {
				continue
			}
			if _, declared := qf.ContextTypes[rule]; declared {
				checked++
				continue
			}
			if reason := nonCallableExemptions[qf.Language+":"+rule]; reason != "" {
				continue
			}
			t.Errorf("%s: %s is declared from %q, which is not in context_types. "+
				"Whatever nests inside it — columns, fields, attributes, members — will "+
				"be attributed to what encloses %q instead. Add it to context_types "+
				"(plus context_name_paths if the node has no \"name\" field), or to "+
				"nonCallableExemptions with a reason.",
				filepath.Base(path), q.GraphLabel, rule, rule)
		}
	}

	if checked == 0 {
		t.Fatal("no non-callable container was verified — the glob or the loader is broken")
	}
	t.Logf("%d non-callable declarations verified, %d exemptions on record",
		checked, len(nonCallableExemptions))
}

// transparentContextGrammars records grammars whose declared contexts cannot be named,
// leaving every entity inside them attached to the File.
//
// It is EMPTY, and that is the point: elixir, graphql, hcl and protobuf were all
// here, each measured — an `rpc` inside a service came out with context "", attributes
// inside an hcl `resource` block likewise — and each now declares a context_name_paths
// entry. An addition here is a regression, not a design choice; the reason must say what
// is broken, not why it is acceptable.
var transparentContextGrammars = map[string]string{}

// A declared context whose name cannot be resolved is transparent, which is the same
// silent failure as not declaring it — and it is how toml and html both shipped. A
// context on a node with no "name" field MUST declare a context_name_paths entry.
//
// The grammar-wide check is sound: if a grammar defines no field called "name", then no
// node in it can have one, so SafeChildByFieldName(n, "name") returns nil for every
// context and nameNodeOf makes all of them transparent.
func TestNamelessContextsDeclareANamePath(t *testing.T) {
	known := 0
	files, _ := filepath.Glob(filepath.Join("queries", "*.yaml"))
	for _, path := range files {
		body, err := os.ReadFile(path)
		if err != nil {
			t.Fatalf("read %s: %v", path, err)
		}
		qf, ok := parseQueryFile(body, path)
		if !ok || qf.Parser == "antlr4" || len(qf.ContextTypes) == 0 {
			continue
		}
		grammar := qf.Grammar
		if grammar == "" {
			grammar = "tree-sitter-" + qf.Language
		}
		lang, err := resolveTreeSitterLang(qf.Language, grammar)
		if err != nil || lang == nil {
			continue
		}

		if lang.FieldIdForName("name") != 0 {
			continue
		}
		for kind := range qf.ContextTypes {
			if _, hasPath := qf.ContextNamePaths[kind]; hasPath {
				continue
			}
			if reason := transparentContextGrammars[qf.Language]; reason != "" {
				known++
				continue
			}
			t.Errorf("%s: context %q relies on a \"name\" field, but %s defines no such "+
				"field at all — every context here is transparent and its content falls "+
				"back to the File. Declare context_name_paths[%s], or add %q to "+
				"transparentContextGrammars with what is broken.",
				filepath.Base(path), kind, grammar, kind, qf.Language)
		}
	}

	for lang := range transparentContextGrammars {
		if _, err := os.Stat(filepath.Join("queries", lang+".yaml")); err != nil {
			t.Errorf("transparentContextGrammars records %q, which ships no query file", lang)
		}
	}
	t.Logf("%d contexts are transparent across %d recorded grammars — each one is a live "+
		"bug, not an exemption", known, len(transparentContextGrammars))
}

// containerRuleOf names the rule a pattern declares its entity from: the parent
// segment of an ANTLR XPath, the root node of a tree-sitter s-expression.
func containerRuleOf(pattern, parser string) string {
	pattern = strings.TrimSpace(pattern)
	if pattern == "" {
		return ""
	}
	if parser == "antlr4" {
		var segs []string
		for _, s := range strings.Split(pattern, "/") {
			if s = stripPredicate(s); s != "" {
				segs = append(segs, s)
			}
		}
		if len(segs) >= 2 {
			return segs[len(segs)-2]
		}
		if len(segs) == 1 {
			return segs[0]
		}
		return ""
	}
	m := treeSitterRootRe.FindStringSubmatch(pattern)
	if m == nil {
		return ""
	}
	return m[1]
}

var (
	treeSitterRootRe = regexp.MustCompile(`^\(\s*([A-Za-z_][A-Za-z0-9_]*)`)
	treeSitterKindRe = regexp.MustCompile(`\(\s*([A-Za-z_][A-Za-z0-9_]*)`)
)

func stripPredicate(s string) string {
	if i := strings.IndexByte(s, '['); i >= 0 {
		s = s[:i]
	}
	return strings.TrimSpace(s)
}

// patternMentionsAnyKind reports whether the pattern names one of kinds as a node.
func patternMentionsAnyKind(pattern string, kinds map[string]bool) bool {
	if len(kinds) == 0 {
		return false
	}
	for _, m := range treeSitterKindRe.FindAllStringSubmatch(pattern, -1) {
		if kinds[m[1]] {
			return true
		}
	}
	return false
}

// anyEntityQueryDeclaresParent reports whether the file expresses containment in
// its patterns rather than through the ancestor walk.
func anyEntityQueryDeclaresParent(qf ExternalQueryFile) bool {
	for _, q := range qf.Queries {
		if strings.EqualFold(q.Type, "relation") {
			continue
		}
		if q.ParentCapture != "" {
			return true
		}
	}
	return false
}

// A declared name path that does not resolve against the real grammar is the
// same silent failure as declaring nothing: nameNodeOf returns nil and the
// container is skipped. Compiling the path against the grammar catches a typo
// or a renamed node at test time.
func TestDeclaredContextNamePathsResolveAgainstTheGrammar(t *testing.T) {
	files, _ := filepath.Glob(filepath.Join("queries", "*.yaml"))
	for _, path := range files {
		body, err := os.ReadFile(path)
		if err != nil {
			t.Fatalf("read %s: %v", path, err)
		}
		qf, ok := parseQueryFile(body, path)
		if !ok || len(qf.ContextNamePaths) == 0 || qf.Parser == "antlr4" {
			continue
		}
		grammar := qf.Grammar
		if grammar == "" {
			grammar = "tree-sitter-" + qf.Language
		}
		lang, err := resolveTreeSitterLang(qf.Language, grammar)
		if err != nil || lang == nil {
			t.Logf("%s: grammar %s unavailable, skipped", filepath.Base(path), grammar)
			continue
		}

		for kind, p := range qf.ContextNamePaths {
			if _, declared := qf.ContextTypes[kind]; !declared {
				t.Errorf("%s: context_name_paths names %q, which is not in "+
					"context_types — the path will never be used",
					filepath.Base(path), kind)
			}
			if lang.IdForNodeKind(kind, true) == 0 {
				t.Errorf("%s: context_name_paths key %q is not a node kind in %s",
					filepath.Base(path), kind, grammar)
			}
			// A value is one or more ALTERNATIVE paths separated by "|", each a
			// "/"-separated list of segments, each segment optionally selecting an
			// occurrence as `kind[n]`. Parsed here exactly as parsePathSegment parses
			// it, so a typo in the syntax fails the test rather than silently making
			// the whole context transparent.
			for _, alt := range strings.Split(p, "|") {
				alt = strings.TrimSpace(alt)
				if alt == "" {
					t.Errorf("%s: context_name_paths[%s] has an empty alternative",
						filepath.Base(path), kind)
					continue
				}
				for _, raw := range strings.Split(alt, "/") {
					seg, idx := parsePathSegment(raw)
					// Segments may be field names, which are not node kinds, so a
					// segment only has to be one or the other.
					if lang.IdForNodeKind(seg, true) == 0 && !isFieldName(lang, seg) {
						t.Errorf("%s: context_name_paths[%s] segment %q is neither a node "+
							"kind nor a field of %s", filepath.Base(path), kind, seg, grammar)
						continue
					}
					// An index only selects among same-kind children, so it is
					// meaningless on a field: a field holds exactly one node, and
					// childBySegment skips the field branch when an index is present.
					if idx >= 0 && lang.IdForNodeKind(seg, true) == 0 {
						t.Errorf("%s: context_name_paths[%s] indexes %q, which is a field "+
							"and not a node kind — an index cannot select among fields",
							filepath.Base(path), kind, seg)
					}
				}
			}
		}
	}
}

func isFieldName(lang *sitter.Language, name string) bool {
	return lang.FieldIdForName(name) != 0
}
