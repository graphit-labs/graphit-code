package ast

import (
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"sync"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/version"
	sitter "github.com/tree-sitter/go-tree-sitter"
	"gopkg.in/yaml.v3"
)

// ExternalQueryFile represents a YAML file with extraction queries and
// language configuration. Supports both tree-sitter and ANTLR v4 parsers.
// Files are loaded from the project's queries directory (ast.queries_dir,
// by default .graphit/ast/queries/), the user's own, and the runtime's.
type ExternalQueryFile struct {
	Language   string             `yaml:"language"`
	Extensions []string           `yaml:"extensions,omitempty"`
	Parser     string             `yaml:"parser,omitempty"`     // "tree-sitter" (default) or "antlr4"
	Grammar    string             `yaml:"grammar,omitempty"`    // Binary name for ANTLR (e.g. "antlr-plsql")
	StartRule  string             `yaml:"start_rule,omitempty"` // ANTLR start rule (required when parser=antlr4)
	Queries    []ExternalQueryDef `yaml:"queries"`

	// Merge says what this file does to the same language declared at a lower
	// level — the user's directory over the runtime's, the project's over both.
	//
	// Absent, it REPLACES it, which is what every level has always done: the file
	// is the whole language, and anything the level below said about it is gone.
	// `merge: true` merges into it instead.
	//
	// Opt-in and stated in the affirmative, so the file says what it does rather
	// than which behaviour it is switching off — and so a plain bool is enough:
	// there is no third state to encode.
	//
	// Merging is what makes a partial file possible. Replacement means a project
	// that wants one more query, or one different pattern, has to copy the whole
	// shipped file and then own every future fix to it; the copy also silently
	// takes over `extensions`, `grammar` and every language-level setting, so
	// omitting one of those in the copy breaks the language rather than leaving
	// it alone. See mergeQueryFile for what merging does field by field.
	Merge bool `yaml:"merge,omitempty"`

	Exclusive bool `yaml:"exclusive,omitempty"`

	// Language-level configuration (all optional — engine uses sensible defaults)
	Exports      *ExportConfig     `yaml:"exports,omitempty"`
	SelfKeywords []string          `yaml:"self_keywords,omitempty"`
	ContextTypes map[string]string `yaml:"context_types,omitempty"`
	// ContextNamePaths says how to read a context node's name when it is not in
	// a "name" field: a `/`-separated path of field names or child kinds, from
	// the context node down to the node holding the text. Data-format grammars
	// need this — xml's `element` keeps its name at STag/Name, json's `pair` at
	// key/string_content — and without it every entity fell back to the File.
	ContextNamePaths map[string]string `yaml:"context_name_paths,omitempty"`
	AnonFuncTypes    []string          `yaml:"anon_func_types,omitempty"`
	DeclarationTypes []string          `yaml:"declaration_types,omitempty"`
	CommentTypes     []string          `yaml:"comment_types,omitempty"`

	// EmbedLabels names the graph labels of THIS language that get a vector, and
	// so are reachable by semantic search rather than by keyword alone.
	//
	// It is declared per grammar because only the grammar knows what it produces.
	// A hardcoded list in Go answers for languages the binary has never seen — a
	// grammar installed from the Hub, or written into `ast.queries_dir` — and
	// answers wrongly by construction: its labels are absent from the list, so
	// nothing it indexes is ever embedded, silently. Every other language-shaped
	// decision here already lives in the YAML (comment_types, context_types,
	// declaration_types, target_rules); this is the same rule applied to embedding.
	//
	// ORDER IS MEANINGFUL. One (path, uid) can carry two labels — a TypeScript
	// `class Foo` beside `interface Foo`, a Table beside a View of the same name —
	// and the embedding cache is keyed without the label, so the two collide on one
	// entry. The label listed EARLIER wins, which makes this list the grammar's own
	// statement of which reading of a name is the primary one.
	//
	// A label naming CONTENT rather than an identifier belongs here as readily as a
	// declaration: Comment's name is the comment's prose, which is exactly what
	// semantic search is for. Omit the label and its entities stay keyword-only —
	// they are always in entity_fts, because that indexes every entity by name.
	//
	// Empty means this language embeds nothing. That is a real answer for a
	// grammar with no prose and no bodies, but it is rarely the intended one, so
	// TestEveryShippedGrammarDeclaresEmbedLabels fails when a shipped grammar is
	// simply silent about it.
	EmbedLabels []string `yaml:"embed_labels,omitempty"`

	// TargetRules declares, per relation type, how this language resolves a target
	// captured by name. See ExternalQueryDef.TargetLabels for what is being decided.
	//
	// Stating it once per relation beats repeating it on every query: the SQL family
	// declares forty-one DML queries whose targets are all schema objects.
	//
	// The rule is per RELATION TYPE, not per query, because that is all a cached
	// reference carries — so the labels declared here and on every query of that
	// relation are unioned. A wider set only ever rejects more, never resolves
	// wrongly: resolution demands a single match inside the set.
	//
	//	target_rules:
	//	  SELECTS: { labels: [Table, View], fallback: Table }
	TargetRules map[string]TargetRuleDecl `yaml:"target_rules,omitempty"`

	// Embedded declares the regions of a file written in another language — the
	// body of a single-file component's <script> and <style>, which the outer
	// grammar hands over as one opaque text node. See EmbeddedBlock.
	Embedded []EmbeddedBlock `yaml:"embedded,omitempty"`

	// TextNormalizers are this language's named ways of turning escaped text back
	// into what it represents, for an embedded block to name. See TextNormalizer.
	TextNormalizers map[string]TextNormalizer `yaml:"text_normalizers,omitempty"`

	// Complexity declares which parsed node kinds count as a branch when scoring
	// cyclomatic complexity by walking the entity's own syntax subtree. Absent
	// (nil), the entity's complexity is the base 1 — this language has no
	// complexity signal yet, not a guessed one. See ComplexityConfig.
	Complexity *ComplexityConfig `yaml:"complexity,omitempty"`
}

// ComplexityConfig lists the real syntax-tree shapes that count as a decision
// point for this language, so complexity is scored by walking the parsed
// entity subtree instead of scanning its text for keywords.
//
// NodeTypes are NAMED node kinds — if_statement, for_statement, a switch's
// case clause, and so on — each occurrence anywhere in the entity's subtree
// (except inside a nested declaration, which is scored on its own) adds one.
// A chained "else if" does not need its own entry: every grammar checked here
// re-emits it as another if node nested in the else branch, so counting the
// if kind already counts each link in the chain.
//
// Operators are ANONYMOUS token kinds — the literal text of a short-circuit
// operator, e.g. "&&" and "||" — matched wherever that token appears as a
// leaf, however deep. Only list an operator here when the grammar has no
// named node of its own for the boolean combination (Go, C, Java, JavaScript,
// TypeScript, Rust, Ruby, PHP all spell && / || as bare tokens). A grammar
// that already wraps && / || in a named node — Kotlin's conjunction_expression
// and disjunction_expression, Swift's, Dart's logical_and_expression and
// logical_or_expression — belongs in NodeTypes instead: listing it in both
// would count the same operator twice.
// HeadCalls covers grammars where every control form — if, when, cond, case —
// parses as the SAME node kind, distinguished only by the text of its own
// first named child: Clojure's `(if ...)`/`(when ...)`/`(cond ...)` are all a
// bare list_lit whose first named child is a sym_lit reading "if"/"when"/
// "cond", and Elixir's `if`/`case`/`cond`/`for` are all a call whose first
// named child is an identifier reading the macro's name. NodeTypes cannot
// express this — it counts a kind on sight, with no way to ask what its
// child says — so this is a second, narrower check: NodeType names the
// wrapping kind, and a match on the child's own text is what actually counts
// as a branch.
type HeadCallConfig struct {
	NodeType string   `yaml:"node_type"`
	Names    []string `yaml:"names"`

	// PairNames and SubjectPairNames count once per CLAUSE instead of once
	// per form, for head names whose clauses are plain alternating children
	// rather than a node of their own — Clojure's `(cond t1 r1 t2 r2 ...)`
	// and `(case x t1 r1 t2 r2 ... default)`. Every other language checked
	// here has a real per-clause node (switch_case, case_when_part_statement,
	// ...) and belongs in NodeTypes instead; this exists because Clojure's
	// grammar does not have one.
	//
	// PairNames counts floor(n/2) — cond has no subject, so every child
	// after the head is part of a test/result pair.
	//
	// SubjectPairNames counts floor((n-1)/2) — case's first child after the
	// head is the value being matched, not a clause, and integer division
	// already drops a trailing default with no test of its own; there is
	// nothing left to subtract for it.
	PairNames        []string `yaml:"pair_names,omitempty"`
	SubjectPairNames []string `yaml:"subject_pair_names,omitempty"`
}

type ComplexityConfig struct {
	NodeTypes []string        `yaml:"node_types,omitempty"`
	Operators []string        `yaml:"operators,omitempty"`
	HeadCalls *HeadCallConfig `yaml:"head_calls,omitempty"`
}

// EmbeddedBlock declares a region of a file written in a different language.
//
// A single-file component is several languages in one file, and the outer grammar
// does not look inside: tree-sitter-vue, tree-sitter-svelte and tree-sitter-html all
// hand the body of <script> and <style> over as a single `raw_text` node. The
// configuration is the LANGUAGE's, not the engine's — the same reasoning as
// context_types and exports.
//
// The selector is a TREE-SITTER PATTERN, the same language every `queries[].pattern`
// is written in, and it is the only selector: the `<script>` of a Vue component and
// the `<execute>` of some project's XML are the same question — "which node is this
// block" — and only a query answers it generally. A node kind cannot say "this
// element and not its siblings", which is what an XML needs, and it brings `#eq?`,
// `#match?` with regex, sibling anchors and nesting for free.
//
// Because the pattern also LOCATES the language value, no attribute node kind has to
// be hardcoded or configured: tree-sitter-xml writes an attribute as
// `Attribute` → `Name` / `AttValue` and the HTML-shaped grammars as
// `attribute` → `attribute_name` / `attribute_value`, and a pattern expresses either.
//
// An OPTIONAL selector attribute — `<script lang="ts">` and `<script>` are both
// valid — is expressed as two blocks, the specific one first. The first block whose
// pattern matches a given body node claims it, so the generic one acts as the
// fallback. The claim is taken at the match, before the language resolves, which is
// what makes `lang="scss"` skip instead of falling through to the generic block.
type EmbeddedBlock struct {
	// Pattern is the tree-sitter query that selects the blocks.
	Pattern string `yaml:"pattern"`
	// TextCapture names the capture in Pattern whose node's text IS the body.
	TextCapture string `yaml:"text_capture"`
	// LangCapture names the capture holding the value that selects the language.
	// Absent means the block is always Default.
	LangCapture string `yaml:"lang_capture,omitempty"`
	// Default is the language of a matched body when LangCapture is not declared or
	// resolves empty.
	Default string `yaml:"default,omitempty"`
	// Languages maps a LangCapture value to a Graphit language name. It is an
	// ALLOWLIST: a value that is not a key here is skipped in silence — `lang="scss"`
	// has no grammar, and a warning per block would be a log line per file in a
	// project full of them. Keys are lowercased at load time.
	Languages map[string]string `yaml:"languages,omitempty"`

	// HostLabels names the graph labels that may be the SOURCE of what this block
	// contains — the unit this block belongs to, in the host format's own terms.
	//
	// Absent, the host is the innermost entity that strictly contains the block (see
	// hostEntityAt), which is right when the block is the CONTENT of something: the
	// element carrying a value is a wrapper, and the unit is an ancestor of it.
	//
	// It is wrong when the block is an ATTRIBUTE of the very element that names the
	// unit — an XML-exported screen's `<Trigger Name="POST-QUERY" TriggerText="…"/>`. There the unit
	// and the block occupy the same line, so "strictly contains" excludes exactly the
	// entity that should answer, and every enclosing entity is coarser than it. Naming
	// the labels says which entities are units, so the choice stops depending on
	// whether a span happens to be wider.
	//
	// Declared, containment is still required but no longer has to be strict, and only
	// these labels are considered. Innermost still wins among them, so a nested unit
	// beats the one around it.
	HostLabels []string `yaml:"host_labels,omitempty"`

	WrapPrefix string `yaml:"wrap_prefix,omitempty"`
	WrapSuffix string `yaml:"wrap_suffix,omitempty"`

	// Normalize names one of the host language's `text_normalizers` to run on the
	// body before the sub-parse.
	//
	// A block embedded in XML is almost never plain text: `<` and `&` are markup, so
	// `WHERE qt > 0` reaches the file as `qt &gt; 0` and the host grammar splits the
	// content into CharData / EntityRef / CharData. Capturing the whole `content`
	// keeps the body intact; the normalizer makes it parseable again.
	//
	// It names a normalizer rather than declaring one, and the ENGINE knows no
	// escaping scheme at all: how a language escapes its text is a fact about that
	// language, so it lives in that language's YAML — the same reasoning as
	// context_types and embedded itself.
	//
	// Opt-in per block, because escaping is a property of the position, not of the
	// language alone: an XML element's content is escaped, but an HTML `<script>`'s
	// raw_text is not, even though both hosts have entities.
	Normalize string `yaml:"normalize,omitempty"`
}

// TextNormalizer is a declared, named way to turn a language's escaped text back
// into the text it represents.
//
// The engine applies it and knows nothing else: there is no built-in entity table,
// no "xml mode". A grammar that escapes differently — or a future one nobody has
// met — declares its own and an embedded block names it.
//
// THE INVARIANT the engine does enforce: a normalizer may not change the NUMBER OF
// NEWLINES. Every line the sub-parse reports is shifted by the block's start row in
// the host file, so a replacement that produced a line break would move every entity
// after it inside the block — trading a visible syntax error for a wrong line
// number, which is the failure mode this whole module exists to avoid. A pair whose
// replacement contains a line break is dropped at load time; a numeric reference
// that would decode to one is left as written.
type TextNormalizer struct {
	// Replace maps literal text to its replacement, applied left to right.
	Replace map[string]string `yaml:"replace,omitempty"`
	// NumericCharRefs decodes `&#62;` and `&#x3E;` — the open-ended half of the same
	// scheme, which a fixed table cannot express.
	NumericCharRefs bool `yaml:"numeric_char_refs,omitempty"`
}

// ExportConfig defines how the engine determines export/visibility for a language.
type ExportConfig struct {
	// Strategy is one of: capitalized_name, no_prefix, modifier, export_statement,
	// no_modifier, no_static, none.
	Strategy   string              `yaml:"strategy"`
	Config     map[string]string   `yaml:"config,omitempty"`
	ConfigList map[string][]string `yaml:"config_list,omitempty"`
}

type ExternalQueryDef struct {
	DataKey      string `yaml:"data_key"`
	GraphLabel   string `yaml:"graph_label"`
	Pattern      string `yaml:"pattern"`
	NameCapture  string `yaml:"name_capture,omitempty"`
	Type         string `yaml:"type,omitempty"`          // "entity" (default) or "relation"
	RelationType string `yaml:"relation_type,omitempty"` // e.g. CALLS, INHERITS, READS_FIELD

	// ValueCapture names the capture holding the value that belongs to the
	// entity named by NameCapture, and ValueLabel the node table that value
	// lands in.
	//
	// Data formats are key/value languages, and a key on its own is half the
	// content: an XML attribute node named "env" says nothing about "prod".
	// Storing the value only as a property does not help either — the search
	// index reads name and docstring, never value — so the value becomes a node
	// in its own right, named after itself, CONTAINed by the key. It is also
	// written to the key's `value` property so `RETURN n.name, n.value` works
	// without a traversal.
	ValueCapture string `yaml:"value_capture,omitempty"`
	ValueLabel   string `yaml:"value_label,omitempty"`

	NameReject string `yaml:"name_reject,omitempty"`

	// SpanCapture names the capture whose node DELIMITS the entity, when the
	// entity is wider than the declaration its name sits in.
	//
	// Absent, an entity spans from the name node to the end of that node's PARENT,
	// which is right for a language where the name is written inside the thing it
	// names: a Go `func` declaration, a class, a procedure body. It is wrong for a
	// data format, where the name is inside the START TAG — `(STag (Name) @name)`
	// makes every XML Element span one line, ending before its own content begins.
	//
	// What that costs is not cosmetic. An embedded block is attributed to the
	// innermost entity CONTAINING it (see hostEntityAt), so a grammar whose
	// entities end at their start tag has no host to offer: the SQL inside a
	// configuration element could only ever be attributed to the file. And the
	// unit worth attributing to is frequently named by a CHILD that appears AFTER
	// the block — a flow's `<name>` written past the `<config>` that holds the
	// statement — so neither end of the unit is derivable from the name's position.
	// Naming the capture states the extent the pattern already matched.
	//
	// It decides the LINE RANGE and nothing else. The export verdict reads the
	// declaration's own text and complexity is scored over the declaration's own
	// subtree; both stay on the name's parent, because widening them is a separate
	// question that no grammar has asked.
	SpanCapture string `yaml:"span_capture,omitempty"`

	// NameIsData says this entity's NAME is a data value rather than an identifier, so
	// it is normalised the way a value is: matched surrounding quotes come off, and
	// blank, multi-line or over-long text is dropped instead of indexed as a name.
	//
	// A data format needs it because an attribute value IS quoted at the source, and an
	// entity whose name keeps its delimiters answers no query anyone writes —
	// `WHERE n.name = 'POST-QUERY'` does not match `"POST-QUERY"` — and resolves against
	// no reference captured from an identifier.
	//
	// It has to be DECLARED rather than inferred from the captured node, because a
	// quoted literal deliberately does not collapse into the identifier of the same
	// spelling: `:prop="foo"` binds the variable `foo` and `:prop='"foo"'` passes the
	// string, and unquoting the second would index it as a reference to the first.
	// A query that declares `value_capture` or `parent_capture` is already describing
	// data and needs nothing extra.
	NameIsData bool `yaml:"name_is_data,omitempty"`

	// ParentCapture names the capture holding the enclosing entity's name, and
	// ParentLabel the node table that entity lives in. Both are needed because
	// context_types cannot express these grammars: it walks up the tree and
	// reads the ancestor's `name` field, and tree-sitter-xml `element`,
	// tree-sitter-json `pair` and tree-sitter-html `start_tag` have no field by
	// that name. Naming the capture states the containment the pattern already
	// matched instead of re-deriving it.
	ParentCapture string `yaml:"parent_capture,omitempty"`
	ParentLabel   string `yaml:"parent_label,omitempty"`

	// TargetLabels names the graph labels the target of this relation may resolve
	// to. A relation query captures its target by NAME — the callee of a call, the
	// table of a SELECT — and the name alone does not say what kind of thing it is.
	//
	// Absent, the target may resolve to any label THIS grammar declares, which is
	// the default that makes a new grammar work with nothing but its own yaml.
	// Narrow it when the language distinguishes: a Go call reaches a Function or a
	// Method, never a Parameter that happens to share the name.
	//
	// Ambiguity is judged inside this set, so narrowing also resolves names that a
	// wider set would reject: with `[Function, Method]`, a call to `Rule` still
	// resolves when the only other `Rule` is a Struct.
	//
	// Unioned with the language-level target_rules and with every other query of the
	// same relation type — see ExternalQueryFile.TargetRules for why the rule cannot
	// be per query. Use target_rules for the language's default and this for a
	// grammar that declares nothing at the language level.
	TargetLabels []string `yaml:"target_labels,omitempty"`

	// QualifierCapture names what a captured target belongs to, so the target is
	// resolved as `QUALIFIER.NAME` instead of by the bare name.
	//
	// It exists because a bare name is not an identity when the language allows the
	// same one in many scopes. A column is the case that forces it: `ORDER_ID` lives
	// in dozens of tables, so `UPDATE PEDIDO SET ORDER_ID = …` captured as `ORDER_ID`
	// either resolves to nothing (the engine refuses to pick one of many candidates,
	// correctly) or, worse, lands on a single shared node that then answers "who
	// writes this column" with every table's writers at once.
	//
	// The qualifier is almost never a descendant of the captured node — the table of
	// an UPDATE is a SIBLING subtree of its SET clause — so the path is anchored at an
	// ANCESTOR: the first segment names the enclosing rule to climb to, and the rest
	// walks down from there. `update_statement/general_table_ref` reads as "from the
	// update statement around me, take the table".
	//
	// Qualifying also makes the unresolved case safe: a stub named `PEDIDO.ORDER_ID`
	// records one missing column of one table, where a stub named `ORDER_ID` silently
	// merges every table's.
	QualifierCapture string `yaml:"qualifier_capture,omitempty"`

	// TargetFallback says what the target becomes when the name resolves to nothing
	// here — a call into a dependency, a table whose DDL is not in the corpus.
	//
	//	stub          keep a placeholder node named after the target (the default).
	//	              It records "something outside this corpus is used here", which
	//	              is always true and never invents a relationship.
	//	file          point at the file that holds the reference. For a relation
	//	              whose target is only meaningful as a declaration — a comment
	//	              documenting something, a url() — the file is the honest answer.
	//	<Label>       a stub carrying that label, for a relation whose target is a
	//	              known kind of thing even when undeclared: `SELECT ... FROM
	//	              PEDIDO` depends on a table whether or not its DDL is indexed.
	//
	// This is why the engine no longer hardcodes `Table`: it was the fallback for
	// EVERY unresolved reference, which turned Go methods and .go filenames into
	// 1637 "tables" in a repository with no database at all.
	TargetFallback string `yaml:"target_fallback,omitempty"`
}

// TargetRuleDecl is a language-level resolution rule for one relation type.
type TargetRuleDecl struct {
	Labels   []string `yaml:"labels,omitempty"`
	Fallback string   `yaml:"fallback,omitempty"`
}

func userASTDir() string {
	d := brand.GlobalDir()
	if d == "" {
		return ""
	}
	return filepath.Join(d, "ast")
}

func runtimeASTDir() string {
	d := brand.RuntimeDir(version.Version)
	if d == "" {
		return ""
	}
	return filepath.Join(d, "ast")
}

func userQueriesDir() string {
	d := userASTDir()
	if d == "" {
		return ""
	}
	return filepath.Join(d, "queries")
}

func runtimeQueriesDir() string {
	d := runtimeASTDir()
	if d == "" {
		return ""
	}
	return filepath.Join(d, "queries")
}

func projectQueriesDir(projectDir string) string {
	if projectDir == "" {
		return ""
	}
	rel := config.ResolveASTQueriesDir(nil, config.LoadProjectConfig(projectDir))
	return filepath.Join(projectDir, rel)
}

// ProjectQueriesDir is projectQueriesDir for callers outside this package —
// the Hub, which packages a project's grammar files into a language artifact
// and has to look where that project actually keeps them.
func ProjectQueriesDir(projectDir string) string {
	return projectQueriesDir(projectDir)
}

func loadQueriesFromDir(dir string) ([]ExternalQueryFile, error) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, fmt.Errorf("read queries dir: %w", err)
	}

	var result []ExternalQueryFile
	for _, entry := range entries {
		if entry.IsDir() {
			continue
		}
		name := entry.Name()
		ext := strings.ToLower(filepath.Ext(name))
		if ext != ".yaml" && ext != ".yml" {
			continue
		}

		path := filepath.Join(dir, name)
		data, err := os.ReadFile(path)
		if err != nil {
			slog.Warn("skip external query file: read error", "path", path, "error", err)
			continue
		}

		qf, ok := parseQueryFile(data, path)
		if ok {
			result = append(result, qf)
		}
	}

	return result, nil
}

func parseQueryFile(data []byte, sourcePath string) (ExternalQueryFile, bool) {
	var qf ExternalQueryFile
	if err := yaml.Unmarshal(data, &qf); err != nil {
		slog.Warn("skip external query file: YAML parse error", "path", sourcePath, "error", err)
		return qf, false
	}

	if qf.Language == "" {
		slog.Warn("skip external query file: missing 'language' field", "path", sourcePath)
		return qf, false
	}

	var valid []ExternalQueryDef
	for i, q := range qf.Queries {
		if q.DataKey == "" {
			slog.Warn("skip query: missing 'data_key'", "path", sourcePath, "index", i)
			continue
		}
		if q.Pattern == "" {
			slog.Warn("skip query: missing 'pattern'", "path", sourcePath, "index", i, "data_key", q.DataKey)
			continue
		}
		if q.NameCapture == "" {
			q.NameCapture = "name"
		}
		if q.ValueCapture != "" && q.ValueLabel == "" {
			slog.Warn("ignore value_capture: missing 'value_label'",
				"path", sourcePath, "index", i, "data_key", q.DataKey)
			q.ValueCapture = ""
		}
		if q.ParentCapture != "" && q.ParentLabel == "" {
			slog.Warn("ignore parent_capture: missing 'parent_label'",
				"path", sourcePath, "index", i, "data_key", q.DataKey)
			q.ParentCapture = ""
		}
		if q.NameReject != "" {
			if _, err := regexp.Compile(q.NameReject); err != nil {
				slog.Warn("ignore name_reject: pattern does not compile",
					"path", sourcePath, "index", i, "data_key", q.DataKey,
					"name_reject", q.NameReject, "error", err)
				q.NameReject = ""
			}
		}
		if q.ValueCapture != "" && q.Type == "relation" {
			slog.Warn("ignore value_capture: not supported on relation queries",
				"path", sourcePath, "index", i, "data_key", q.DataKey)
			q.ValueCapture = ""
		}
		valid = append(valid, q)
	}
	qf.Queries = valid
	normalizers := validTextNormalizers(qf.TextNormalizers, sourcePath)
	if len(normalizers) == 0 {
		qf.TextNormalizers = nil
	} else {
		clean := make(map[string]TextNormalizer, len(normalizers))
		for name, n := range normalizers {
			clean[name] = *n
		}
		qf.TextNormalizers = clean
	}
	qf.Embedded = validEmbeddedBlocks(qf.Embedded, normalizers, sourcePath)

	if len(qf.Queries) == 0 && !hasLangConfig(&qf) {
		return qf, false
	}

	return qf, true
}

func validTextNormalizers(decl map[string]TextNormalizer, sourcePath string) map[string]*TextNormalizer {
	if len(decl) == 0 {
		return nil
	}
	out := make(map[string]*TextNormalizer, len(decl))
	for name, n := range decl {
		name = strings.TrimSpace(name)
		if name == "" {
			slog.Warn("skip text_normalizer: blank name", "path", sourcePath)
			continue
		}
		clean := TextNormalizer{NumericCharRefs: n.NumericCharRefs}
		for from, to := range n.Replace {
			if from == "" {
				slog.Warn("ignore text_normalizer pair: blank key",
					"path", sourcePath, "normalizer", name)
				continue
			}
			if strings.ContainsAny(to, "\n\r") {
				slog.Warn("ignore text_normalizer pair: replacement contains a line break, which would shift every line after it",
					"path", sourcePath, "normalizer", name, "from", from)
				continue
			}
			if clean.Replace == nil {
				clean.Replace = make(map[string]string, len(n.Replace))
			}
			clean.Replace[from] = to
		}
		if len(clean.Replace) == 0 && !clean.NumericCharRefs {
			slog.Warn("skip text_normalizer: nothing to do",
				"path", sourcePath, "normalizer", name)
			continue
		}
		c := clean
		out[name] = &c
	}
	if len(out) == 0 {
		return nil
	}
	return out
}

func validEmbeddedBlocks(blocks []EmbeddedBlock, normalizers map[string]*TextNormalizer, sourcePath string) []EmbeddedBlock {
	if len(blocks) == 0 {
		return nil
	}
	var valid []EmbeddedBlock
	for i, blk := range blocks {
		blk.Pattern = strings.TrimSpace(blk.Pattern)
		blk.TextCapture = strings.TrimSpace(strings.TrimPrefix(blk.TextCapture, "@"))
		blk.LangCapture = strings.TrimSpace(strings.TrimPrefix(blk.LangCapture, "@"))
		blk.Default = strings.TrimSpace(blk.Default)

		if blk.Pattern == "" {
			slog.Warn("skip embedded block: missing 'pattern'", "path", sourcePath, "index", i)
			continue
		}
		if blk.TextCapture == "" {
			slog.Warn("skip embedded block: missing 'text_capture', so no body can be found",
				"path", sourcePath, "index", i)
			continue
		}
		if blk.LangCapture == "" && blk.Languages != nil {
			slog.Warn("ignore embedded languages: missing 'lang_capture'",
				"path", sourcePath, "index", i)
			blk.Languages = nil
		}
		if blk.Default == "" && blk.Languages == nil {
			slog.Warn("skip embedded block: no 'default' and no 'languages', so no language can ever resolve",
				"path", sourcePath, "index", i, "pattern", blk.Pattern)
			continue
		}
		if strings.ContainsAny(blk.WrapPrefix, "\n\r") ||
			strings.ContainsAny(blk.WrapSuffix, "\n\r") {
			slog.Warn("ignore embedded wrap: prefix or suffix contains a line break, "+
				"which would shift every line the sub-parse reports",
				"path", sourcePath, "index", i, "pattern", blk.Pattern)
			blk.WrapPrefix, blk.WrapSuffix = "", ""
		}
		blk.Normalize = strings.TrimSpace(blk.Normalize)
		if blk.Normalize != "" && normalizers[blk.Normalize] == nil {
			slog.Warn("ignore embedded normalize: no such text_normalizer in this language",
				"path", sourcePath, "index", i, "normalize", blk.Normalize)
			blk.Normalize = ""
		}
		if len(blk.Languages) > 0 {
			lc := make(map[string]string, len(blk.Languages))
			for k, v := range blk.Languages {
				k, v = strings.ToLower(strings.TrimSpace(k)), strings.TrimSpace(v)
				if k == "" || v == "" {
					slog.Warn("ignore embedded language mapping: blank key or value",
						"path", sourcePath, "index", i)
					continue
				}
				lc[k] = v
			}
			blk.Languages = lc
		}
		valid = append(valid, blk)
	}
	return valid
}

// LoadExternalQueries scans the project's queries directory — ast.queries_dir,
// by default .graphit/ast/queries — and returns all valid external query files.
// This is the project-level loader. For the full resolution chain, use
// resolveQueriesForLang.
func LoadExternalQueries(projectDir string) ([]ExternalQueryFile, error) {
	return loadQueriesFromDir(projectQueriesDir(projectDir))
}

// LoadUserQueries loads query files from the user-editable global directory:
// ~/.graphit/ast/queries/
func LoadUserQueries() ([]ExternalQueryFile, error) {
	dir := userQueriesDir()
	if dir == "" {
		return nil, nil
	}
	return loadQueriesFromDir(dir)
}

// LoadRuntimeQueries loads query files from the version-scoped runtime directory:
// ~/.graphit/runtime/<version>/ast/queries/
func LoadRuntimeQueries() ([]ExternalQueryFile, error) {
	dir := runtimeQueriesDir()
	if dir == "" {
		return nil, nil
	}
	return loadQueriesFromDir(dir)
}

var mergedQueryCache sync.Map

var compiledQueryCache sync.Map

type compiledQueryEntry struct {
	Def   tsQueryDef
	Query *sitter.Query
	// Capture indices for Def's name, value and parent captures, resolved once
	// at compile time; -1 when the pattern has no such capture. The executor
	// compares these against QueryCapture.Index, which is a uint32 read —
	// resolving the names per match would be a string compare per capture on
	// the hot path.
	NameIdx   int
	ValueIdx  int
	ParentIdx int
	// SpanIdx is the capture whose node delimits the entity — see
	// ExternalQueryDef.SpanCapture.
	SpanIdx int
	// QualifierIdx is the capture whose text qualifies the target — see
	// ExternalQueryDef.QualifierCapture. A tree-sitter pattern is structural and
	// matches the whole shape at once, so the qualifier is simply another capture
	// beside the name; the ANTLR backend has to climb to an ancestor for the same
	// thing because its patterns match one node.
	QualifierIdx int
}

// nameRejectCache memoises the compiled `name_reject` expressions.
//
// Keyed by the pattern text and shared by both backends: a query is matched once per
// file, and compiling the same expression per file would cost a regexp compile per
// query per file. Compiled ONCE here rather than stored on the query, because
// ExternalQueryDef is converted to tsQueryDef by a positional cast — a field the two
// do not share breaks that conversion at compile time.
var nameRejectCache sync.Map

// nameRejectMatcher returns the compiled expression for a query's `name_reject`, or nil
// when the query declares none or the expression does not compile.
//
// nil for a broken expression is safe because the load-time validation already dropped
// it with a warning; this is the second reader of the same field, and it must not turn a
// bad pattern into a panic on the parse path.
func nameRejectMatcher(pattern string) *regexp.Regexp {
	if pattern == "" {
		return nil
	}
	if re, ok := nameRejectCache.Load(pattern); ok {
		if re == nil {
			return nil
		}
		return re.(*regexp.Regexp)
	}
	re, err := regexp.Compile(pattern)
	if err != nil {
		nameRejectCache.Store(pattern, (*regexp.Regexp)(nil))
		return nil
	}
	nameRejectCache.Store(pattern, re)
	return re
}

func captureIndex(q *sitter.Query, name string) int {
	if name == "" {
		return -1
	}
	if idx, ok := q.CaptureIndexForName(name); ok {
		return int(idx)
	}
	return -1
}

const queryStaleCheckInterval = 2 * time.Second

type queryDirState struct {
	mu        sync.Mutex
	loaded    bool
	files     []ExternalQueryFile
	signature string
	lastCheck time.Time
}

func (s *queryDirState) get(dirOf func() string, load func() ([]ExternalQueryFile, error)) ([]ExternalQueryFile, bool) {
	s.mu.Lock()
	defer s.mu.Unlock()

	now := time.Now()
	if s.loaded && now.Sub(s.lastCheck) < queryStaleCheckInterval {
		return s.files, false
	}
	s.lastCheck = now

	dir := dirOf()
	sig := dir + "\x00" + queryDirSignature(dir)
	if s.loaded && sig == s.signature {
		return s.files, false
	}

	files, err := load()
	if err != nil {
		slog.Warn("query load error", "dir", dir, "error", err)
		files = nil
	}
	if s.loaded {
		slog.Info("query files changed on disk, reloading", "dir", dir, "files", len(files))
	}
	s.files, s.signature, s.loaded = files, sig, true
	return s.files, true
}

func (s *queryDirState) cached() []ExternalQueryFile {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.files
}

func queryDirSignature(dir string) string {
	if dir == "" {
		return ""
	}
	entries, err := os.ReadDir(dir)
	if err != nil {
		return "missing"
	}
	names := make([]string, 0, len(entries))
	for _, e := range entries {
		if e.IsDir() {
			continue
		}
		n := e.Name()
		if !strings.HasSuffix(n, ".yaml") && !strings.HasSuffix(n, ".yml") {
			continue
		}
		info, err := e.Info()
		if err != nil {
			continue
		}
		names = append(names, fmt.Sprintf("%s:%d:%d", n, info.Size(), info.ModTime().UnixNano()))
	}
	sort.Strings(names)
	return strings.Join(names, "|")
}

var runtimeQueryState queryDirState
var userQueryState queryDirState

func loadRuntimeCached() []ExternalQueryFile {
	files, changed := runtimeQueryState.get(runtimeQueriesDir, LoadRuntimeQueries)
	if changed {
		invalidateDerivedQueryCaches()
	}
	return files
}

func loadUserCached() []ExternalQueryFile {
	files, changed := userQueryState.get(userQueriesDir, LoadUserQueries)
	if changed {
		invalidateDerivedQueryCaches()
	}
	return files
}

var projectQueryStates sync.Map

func loadProjectCached(projectDir string) []ExternalQueryFile {
	v, _ := projectQueryStates.LoadOrStore(projectDir, &queryDirState{})
	st := v.(*queryDirState)
	files, changed := st.get(
		func() string { return projectQueriesDir(projectDir) },
		func() ([]ExternalQueryFile, error) { return LoadExternalQueries(projectDir) },
	)
	if changed {
		invalidateDerivedQueryCaches()
	}
	return files
}

func (qf *ExternalQueryFile) mergesOnto() bool {
	return qf.Merge
}

// mergeOnto folds one level of query files onto the level below it, matching by
// language name, and returns the upper level as it effectively stands.
//
// The result is the UPPER level, not the union of the two: which level answers
// for a language is still resolveQueriesForLang's decision, and a language the
// upper level says nothing about must stay unanswered here so that decision can
// fall through to the level below. What merging changes is what the upper
// level's own files contain.
//
// A file that does not ask to merge is returned untouched, and a level in which
// no file asks to merge is returned as the same slice — the common case costs
// one pass over a handful of structs and no allocation.
func mergeOnto(base, over []ExternalQueryFile) []ExternalQueryFile {
	if len(base) == 0 || len(over) == 0 {
		return over
	}
	merges := false
	for i := range over {
		if over[i].mergesOnto() {
			merges = true
			break
		}
	}
	if !merges {
		return over
	}

	byLang := make(map[string]*ExternalQueryFile, len(base))
	for i := range base {
		key := strings.ToLower(base[i].Language)
		if _, seen := byLang[key]; !seen {
			byLang[key] = &base[i]
		}
	}

	out := make([]ExternalQueryFile, len(over))
	for i := range over {
		out[i] = over[i]
		if !over[i].mergesOnto() {
			continue
		}
		if b := byLang[strings.ToLower(over[i].Language)]; b != nil {
			out[i] = mergeQueryFile(*b, over[i])
		}
	}
	return out
}

func mergeQueryFile(base, over ExternalQueryFile) ExternalQueryFile {
	merged := over

	if len(merged.Extensions) == 0 {
		merged.Extensions = base.Extensions
	}
	if merged.Parser == "" {
		merged.Parser = base.Parser
	}
	if merged.Grammar == "" {
		merged.Grammar = base.Grammar
	}
	if merged.StartRule == "" {
		merged.StartRule = base.StartRule
	}
	if !over.Exclusive {
		merged.Exclusive = base.Exclusive
	}

	merged.Queries = mergeQueryDefs(base.Queries, over.Queries)

	if merged.Exports == nil {
		merged.Exports = base.Exports
	}
	if len(merged.SelfKeywords) == 0 {
		merged.SelfKeywords = base.SelfKeywords
	}
	if len(merged.AnonFuncTypes) == 0 {
		merged.AnonFuncTypes = base.AnonFuncTypes
	}
	if len(merged.DeclarationTypes) == 0 {
		merged.DeclarationTypes = base.DeclarationTypes
	}
	if len(merged.CommentTypes) == 0 {
		merged.CommentTypes = base.CommentTypes
	}
	if len(merged.EmbedLabels) == 0 {
		merged.EmbedLabels = base.EmbedLabels
	}

	merged.TargetRules = mergeStringKeyed(base.TargetRules, over.TargetRules)
	merged.ContextTypes = mergeStringKeyed(base.ContextTypes, over.ContextTypes)
	merged.ContextNamePaths = mergeStringKeyed(base.ContextNamePaths, over.ContextNamePaths)
	merged.TextNormalizers = mergeStringKeyed(base.TextNormalizers, over.TextNormalizers)
	merged.Embedded = mergeEmbedded(base.Embedded, over.Embedded)
	merged.Complexity = mergeComplexity(base.Complexity, over.Complexity)

	return merged
}

func mergeQueryDefs(base, over []ExternalQueryDef) []ExternalQueryDef {
	if len(base) == 0 {
		return over
	}
	if len(over) == 0 {
		return base
	}
	redeclared := make(map[string]bool, len(over))
	for _, q := range over {
		redeclared[q.DataKey] = true
	}
	merged := make([]ExternalQueryDef, 0, len(base)+len(over))
	for _, q := range base {
		if !redeclared[q.DataKey] {
			merged = append(merged, q)
		}
	}
	return append(merged, over...)
}

// mergeEmbedded puts the upper level's blocks in front of the lower level's.
//
// Order is meaning here — the first block whose pattern matches a body claims it
// — so the level that overrides has to come first, or its `<script lang="ts">`
// would never be reached past the generic `<script>` it was written to precede.
// Nothing is dropped: a project adding one block keeps every block the language
// shipped with, behind its own.
func mergeEmbedded(base, over []EmbeddedBlock) []EmbeddedBlock {
	if len(base) == 0 {
		return over
	}
	if len(over) == 0 {
		return base
	}
	merged := make([]EmbeddedBlock, 0, len(base)+len(over))
	merged = append(merged, over...)
	return append(merged, base...)
}

func mergeComplexity(base, over *ComplexityConfig) *ComplexityConfig {
	if over == nil {
		return base
	}
	if base == nil {
		return over
	}
	merged := *over
	if len(merged.NodeTypes) == 0 {
		merged.NodeTypes = base.NodeTypes
	}
	if len(merged.Operators) == 0 {
		merged.Operators = base.Operators
	}
	if merged.HeadCalls == nil {
		merged.HeadCalls = base.HeadCalls
	}
	return &merged
}

func mergeStringKeyed[V any](base, over map[string]V) map[string]V {
	if len(over) == 0 {
		return base
	}
	if len(base) == 0 {
		return over
	}
	merged := make(map[string]V, len(base)+len(over))
	for k, v := range base {
		merged[k] = v
	}
	for k, v := range over {
		merged[k] = v
	}
	return merged
}

func overlayByLang(base, over []ExternalQueryFile) []ExternalQueryFile {
	if len(base) == 0 {
		return over
	}
	if len(over) == 0 {
		return base
	}
	declared := make(map[string]bool, len(over))
	for i := range over {
		declared[strings.ToLower(over[i].Language)] = true
	}
	all := make([]ExternalQueryFile, 0, len(base)+len(over))
	all = append(all, over...)
	for i := range base {
		if !declared[strings.ToLower(base[i].Language)] {
			all = append(all, base[i])
		}
	}
	return all
}

const belowProjectCacheKey = ""

var belowProjectCache sync.Map

var projectEffectiveCache sync.Map

func userLevelQueryFiles() []ExternalQueryFile {
	return mergeOnto(loadRuntimeCached(), loadUserCached())
}

func belowProjectQueryFiles() []ExternalQueryFile {
	runtimeQ := loadRuntimeCached()
	userLevel := userLevelQueryFiles()
	if v, ok := belowProjectCache.Load(belowProjectCacheKey); ok {
		return v.([]ExternalQueryFile)
	}
	inherited := overlayByLang(runtimeQ, userLevel)
	belowProjectCache.Store(belowProjectCacheKey, inherited)
	return inherited
}

func allEffectiveQueryFiles(projectDir string) []ExternalQueryFile {
	return overlayByLang(belowProjectQueryFiles(), loadProjectCached(projectDir))
}

func effectiveProjectQueryFiles(projectDir string) []ExternalQueryFile {
	if projectDir == "" {
		return nil
	}
	projectQ := loadProjectCached(projectDir)
	inherited := belowProjectQueryFiles()
	if v, ok := projectEffectiveCache.Load(projectDir); ok {
		return v.([]ExternalQueryFile)
	}
	merged := mergeOnto(inherited, projectQ)
	projectEffectiveCache.Store(projectDir, merged)
	return merged
}

// invalidateDerivedQueryCaches drops everything computed from query files.
//
// Compiled *sitter.Query objects are dropped, not closed. Nothing in this package
// has ever closed them — they live as long as the process — so a parse already
// holding a slice of them keeps working on valid pointers. Closing here would be
// a use-after-free for that parse; leaking the handful of queries a reload
// orphans is the cheaper mistake, and reloads happen when someone installs a
// grammar, not in a loop.
func invalidateDerivedQueryCaches() {
	clearSyncMap(&mergedQueryCache)
	clearSyncMap(&compiledQueryCache)
	clearSyncMap(&embedLabelCache)
	clearSyncMap(&projectTsExtCache)
	clearSyncMap(&projectTsLangCache)
	clearSyncMap(&belowProjectCache)
	clearSyncMap(&projectEffectiveCache)
	rebuildExtTables()
}

func clearSyncMap(m *sync.Map) {
	m.Range(func(k, _ any) bool {
		m.Delete(k)
		return true
	})
}

// InvalidateQueryCaches forces the next lookup to re-read every query directory.
// Call it after installing or removing a grammar so the change is visible without
// waiting for the staleness check.
func InvalidateQueryCaches() {
	runtimeQueryState.mu.Lock()
	runtimeQueryState.loaded = false
	runtimeQueryState.mu.Unlock()

	userQueryState.mu.Lock()
	userQueryState.loaded = false
	userQueryState.mu.Unlock()

	projectQueryStates.Range(func(_, v any) bool {
		st := v.(*queryDirState)
		st.mu.Lock()
		st.loaded = false
		st.mu.Unlock()
		return true
	})

	invalidateGrammarFilters()
	invalidateGrammarOverrides()

	invalidateDerivedQueryCaches()
}

func resolveQueriesForLang(projectDir, lang, ext string) []ExternalQueryFile {
	filter := grammarFilterFor(projectDir)

	if projectMatch := filter.keepFiles(filterByLangExt(effectiveProjectQueryFiles(projectDir), lang, ext)); len(projectMatch) > 0 {
		return projectMatch
	}

	if userMatch := filter.keepFiles(filterByLangExt(userLevelQueryFiles(), lang, ext)); len(userMatch) > 0 {
		return userMatch
	}

	return filter.keepFiles(filterByLangExt(loadRuntimeCached(), lang, ext))
}

func filterByLangExt(files []ExternalQueryFile, lang, ext string) []ExternalQueryFile {
	var result []ExternalQueryFile
	for _, f := range files {
		if !strings.EqualFold(f.Language, lang) {
			continue
		}
		if len(f.Extensions) > 0 {
			found := false
			for _, e := range f.Extensions {
				if strings.EqualFold(e, ext) {
					found = true
					break
				}
			}
			if !found {
				continue
			}
		}
		result = append(result, f)
	}
	return result
}

func mergedQueriesFor(projectDir, lang, ext string, tsLang *sitter.Language) []tsQueryDef {
	cacheKey := projectDir + "|" + lang + "|" + ext
	if cached, ok := mergedQueryCache.Load(cacheKey); ok {
		return cached.([]tsQueryDef)
	}

	resolved := resolveQueriesForLang(projectDir, lang, ext)
	if len(resolved) == 0 {
		return nil
	}

	var result []tsQueryDef
	var compiled []compiledQueryEntry
	for _, ef := range resolved {
		for _, eq := range ef.Queries {
			qd := tsQueryDef(eq)
			if tsLang != nil {
				q, qErr := sitter.NewQuery(tsLang, qd.Pattern)
				if qErr != nil {
					slog.Warn("skip resolved query: invalid pattern",
						"language", lang, "data_key", qd.DataKey, "error", qErr)
					continue
				}
				compiled = append(compiled, compiledQueryEntry{
					Def:          qd,
					Query:        q,
					NameIdx:      captureIndex(q, qd.NameCapture),
					ValueIdx:     captureIndex(q, qd.ValueCapture),
					ParentIdx:    captureIndex(q, qd.ParentCapture),
					SpanIdx:      captureIndex(q, qd.SpanCapture),
					QualifierIdx: captureIndex(q, qd.QualifierCapture),
				})
			}
			result = append(result, qd)
		}
	}

	if len(result) > 0 {
		mergedQueryCache.Store(cacheKey, result)
	}
	if len(compiled) > 0 {
		compiledQueryCache.Store(cacheKey, compiled)
	}
	return result
}

func compiledQueriesFor(projectDir, lang, ext string, tsLang *sitter.Language) []compiledQueryEntry {
	cacheKey := projectDir + "|" + lang + "|" + ext
	if cached, ok := compiledQueryCache.Load(cacheKey); ok {
		return cached.([]compiledQueryEntry)
	}
	mergedQueriesFor(projectDir, lang, ext, tsLang)
	if cached, ok := compiledQueryCache.Load(cacheKey); ok {
		return cached.([]compiledQueryEntry)
	}
	return nil
}

func hasLangConfig(qf *ExternalQueryFile) bool {
	return qf.Exports != nil ||
		len(qf.SelfKeywords) > 0 ||
		len(qf.ContextTypes) > 0 ||
		len(qf.AnonFuncTypes) > 0 ||
		len(qf.DeclarationTypes) > 0 ||
		len(qf.CommentTypes) > 0 ||
		len(qf.EmbedLabels) > 0 ||
		len(qf.Embedded) > 0 ||
		len(qf.TextNormalizers) > 0 ||
		qf.Complexity != nil
}

func resolvedLangConfigFor(projectDir, lang, ext string) *ExternalQueryFile {
	resolved := resolveQueriesForLang(projectDir, lang, ext)
	for i := range resolved {
		if hasLangConfig(&resolved[i]) {
			return &resolved[i]
		}
	}
	return nil
}

var embedLabelCache sync.Map

// EmbedLabelsForLang answers which graph labels of one language get a vector, in
// the order that language declared them. See ExternalQueryFile.EmbedLabels.
//
// It resolves by language ALONE, with no extension, because its caller is the
// embedder — which reads entities out of the parse cache, where an entity carries
// the language that produced it and never the extension of the file it came from.
// (An entity from an embedded block carries the INNER language, so the <style> of
// a .vue file is answered for by css.yaml, which is the correct answer and one an
// extension-keyed lookup would get wrong.)
//
// Level precedence is the same as everywhere else — project, then user, then
// runtime — and the first level that declares the language answers for it.
func EmbedLabelsForLang(projectDir, lang string) []string {
	if lang == "" {
		return nil
	}
	cacheKey := projectDir + "|" + strings.ToLower(lang)
	if cached, ok := embedLabelCache.Load(cacheKey); ok {
		return cached.([]string)
	}

	labels := firstDeclaredEmbedLabels(
		effectiveProjectQueryFiles(projectDir),
		userLevelQueryFiles(),
		loadRuntimeCached(),
	)(lang)

	embedLabelCache.Store(cacheKey, labels)
	return labels
}

func firstDeclaredEmbedLabels(levels ...[]ExternalQueryFile) func(string) []string {
	return func(lang string) []string {
		for _, files := range levels {
			for i := range files {
				if !strings.EqualFold(files[i].Language, lang) {
					continue
				}
				if len(files[i].EmbedLabels) > 0 {
					return append([]string(nil), files[i].EmbedLabels...)
				}
			}
		}
		return nil
	}
}
