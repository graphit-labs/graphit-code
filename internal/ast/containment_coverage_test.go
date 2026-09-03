package ast

import (
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"

	sitter "github.com/tree-sitter/go-tree-sitter"
)

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
	for lang := range flatLanguages {
		if _, err := os.Stat(filepath.Join("queries", lang+".yaml")); err != nil {
			t.Errorf("flatLanguages exempts %q, which ships no query file", lang)
		}
	}
	t.Logf("%d grammars checked, %d declared flat", checked, len(flatLanguages))
}

var callableContainerExemptions = map[string]string{
	"haskell:data_constructor": "a data constructor's fields are attributed to the " +
		"data_type around it, which is already a declared context",
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

var nonCallableContainerLabels = map[string]bool{
	"Class": true, "Struct": true, "Interface": true, "Trait": true, "Package": true,
	"Type": true, "Trigger": true, "Table": true, "View": true, "Enum": true,
	"Namespace": true, "Object": true, "Record": true, "Protocol": true, "Mixin": true,
	"Extension": true, "MaterializedView": true, "Program": true, "Section": true,
	"Pair": true, "Mapping": true, "Schema": true, "Implementation": true,
	"DataType": true, "TypeClass": true, "Paragraph": true, "FileDescription": true,
	"ArrayTable": true, "Element": true,
}

var nonCallableExemptions = map[string]string{
	"c:type_definition":                 "struct_specifier/enum_specifier sit inside and are declared",
	"cpp:type_definition":               "struct_specifier/class_specifier sit inside and are declared",
	"go:type_declaration":               "type_spec sits inside and is declared",
	"plsql:type_definition":             "create_type encloses it and is declared",
	"scala:type_definition":             "a type alias has no nested entities",
	"rust:type_item":                    "a type alias has no nested entities",
	"typescript:type_alias_declaration": "a type alias has no nested entities",
	"tsx:type_alias_declaration":        "a type alias has no nested entities",

	"java:package_declaration": "sibling of the type declarations, not their parent",
	"kotlin:package_header":    "sibling of the declarations, not their parent",
	"scala:package_clause":     "sibling of the declarations, not their parent",
	"protobuf:package":         "sibling of the message declarations, not their parent",

	"javascript:pair": "only literal-valued pairs are captured, so one never nests in another",
	"typescript:pair": "only literal-valued pairs are captured, so one never nests in another",
	"tsx:pair":        "only literal-valued pairs are captured, so one never nests in another",

	"html:start_tag":          "element is the container; resolve() skips the self-name",
	"html:self_closing_tag":   "a self-closing tag contains nothing",
	"svelte:start_tag":        "element is the container; resolve() skips the self-name",
	"svelte:self_closing_tag": "a self-closing tag contains nothing",
	"vue:start_tag":           "element is the container; resolve() skips the self-name",
	"vue:self_closing_tag":    "a self-closing tag contains nothing",
	"xml:STag":                "element is the container; resolve() skips the self-name",
	"xml:EmptyElemTag":        "an empty-element tag contains nothing",

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
			for _, alt := range strings.Split(p, "|") {
				alt = strings.TrimSpace(alt)
				if alt == "" {
					t.Errorf("%s: context_name_paths[%s] has an empty alternative",
						filepath.Base(path), kind)
					continue
				}
				for _, raw := range strings.Split(alt, "/") {
					seg, idx := parsePathSegment(raw)
					if lang.IdForNodeKind(seg, true) == 0 && !isFieldName(lang, seg) {
						t.Errorf("%s: context_name_paths[%s] segment %q is neither a node "+
							"kind nor a field of %s", filepath.Base(path), kind, seg, grammar)
						continue
					}
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
