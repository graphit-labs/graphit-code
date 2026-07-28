package ast

import (
	"fmt"
	"log/slog"
	"path/filepath"
	"sort"
	"strings"
	"sync"

	sitter "github.com/tree-sitter/go-tree-sitter"
)

type tsLangConfig struct {
	Language   string
	Grammar    string
	Extensions []string
}

// tsQueryDef mirrors ExternalQueryDef for direct struct cast.
type tsQueryDef struct {
	DataKey      string
	GraphLabel   string
	Pattern      string
	NameCapture  string
	Type         string
	RelationType string
}

var tsExtMap map[string]*tsLangConfig
var tsGrammarMap map[string]*tsLangConfig

var grammarLoader *DynGrammarLoader
var grammarLoaderOnce sync.Once

func initGrammarLoader() {
	grammarLoader = NewDynGrammarLoader()
}

// resolvedLang memoizes a grammar resolution outcome (a *sitter.Language or the
// terminal error) so the expensive lookup runs once per language, not per file.
type resolvedLang struct {
	lang *sitter.Language
	err  error
}

// langResolveCache maps a language name to its memoized resolution.
var langResolveCache sync.Map // map[string]resolvedLang

// resolveTreeSitterLang returns the *sitter.Language for a grammar, preferring a
// dynamically loaded shared library and falling back to a compiled-in native
// grammar. Resolution is O(languages), not O(files): the uncached path runs the
// loader's findLibrary (several failing os.Stat syscalls on the common install
// with no .so grammars) plus a ts.NewLanguage allocation, so the outcome is
// memoized per language name for the life of the process. Grammars are static
// per process, so a negative result is cached too (a .so dropped in mid-run is
// intentionally not picked up until restart).
func resolveTreeSitterLang(langName, grammarName string) (*sitter.Language, error) {
	if v, ok := langResolveCache.Load(langName); ok {
		r := v.(resolvedLang)
		return r.lang, r.err
	}

	grammarLoaderOnce.Do(initGrammarLoader)
	lang, loadErr := grammarLoader.Load(langName)
	if loadErr != nil {
		lang = NativeLanguage(langName)
		if lang == nil {
			r := resolvedLang{nil, fmt.Errorf("grammar load failed for %s: %w", grammarName, loadErr)}
			langResolveCache.Store(langName, r)
			return nil, r.err
		}
	}
	langResolveCache.Store(langName, resolvedLang{lang, nil})
	return lang, nil
}

// parserPool reuses sitter.Parser instances across parse calls.
// ts_parser_new() allocates ~50KB of C state; pooling amortizes this
// across thousands of files parsed per indexing run.
var parserPool = sync.Pool{
	New: func() any {
		return sitter.NewParser()
	},
}

// queryCursorPool reuses sitter.QueryCursor instances across query executions.
// Each cursor is a lightweight C allocation, but at scale (N files × M queries)
// the cumulative allocation cost is significant.
var queryCursorPool = sync.Pool{
	New: func() any {
		return sitter.NewQueryCursor()
	},
}

// tsConfigOf builds the extension config a query file describes, or nil when the
// file is not a tree-sitter language.
func tsConfigOf(qf ExternalQueryFile) *tsLangConfig {
	if qf.Parser == "antlr4" {
		return nil
	}
	grammar := qf.Grammar
	if grammar == "" {
		grammar = "tree-sitter-" + qf.Language
	}
	return &tsLangConfig{
		Language:   qf.Language,
		Grammar:    grammar,
		Extensions: qf.Extensions,
	}
}

// extTablesMu guards the four global extension tables.
//
// They used to be written once at package init and read forever, so no lock was
// needed. They are now rebuilt whenever the runtime or user query directory
// changes under a running process, which makes them shared mutable state. Reads
// are on the per-file hot path, hence RWMutex rather than a plain Mutex.
var extTablesMu sync.RWMutex

// rebuildExtTables recomputes the languages that exist for every project: the
// installed runtime, then the user's own global query directory on top.
//
// Project-scoped languages are not here — there is no single project — and are
// resolved per directory by tsLangConfigFor.
func rebuildExtTables() {
	runtimeQ := runtimeQueryState.cached()
	userQ := userQueryState.cached()

	ts := make(map[string]*tsLangConfig)
	tsGram := make(map[string]*tsLangConfig)
	antlrExt := make(map[string][]*antlrLangConfig)
	antlrGram := make(map[string]*antlrLangConfig)

	register := func(files []ExternalQueryFile) {
		for _, qf := range files {
			if qf.Parser == "antlr4" {
				cfg := antlrConfigOf(qf)
				for _, ext := range qf.Extensions {
					e := strings.ToLower(ext)
					antlrExt[e] = append(antlrExt[e], cfg)
				}
				antlrGram[cfg.Grammar] = cfg
				continue
			}
			cfg := tsConfigOf(qf)
			if cfg == nil {
				continue
			}
			for _, ext := range qf.Extensions {
				ts[strings.ToLower(ext)] = cfg
			}
			tsGram[cfg.Grammar] = cfg
		}
	}
	register(runtimeQ)
	// The user directory outranks the runtime, matching resolveQueriesForLang.
	register(userQ)

	extTablesMu.Lock()
	tsExtMap, tsGrammarMap = ts, tsGram
	antlrExtMap, antlrGrammarMap = antlrExt, antlrGram
	extTablesMu.Unlock()
}

func initTsExtMap() {
	loadRuntimeCached()
	loadUserCached()
	rebuildExtTables()
}

// projectTsExtCache memoizes the per-project extension table, keyed by project
// directory. loadProjectCached already caches the parsed files; this caches the
// small map derived from them so it is not rebuilt once per file indexed.
var projectTsExtCache sync.Map // map[string]map[string]*tsLangConfig

func projectTsExtMap(projectDir string) map[string]*tsLangConfig {
	if v, ok := projectTsExtCache.Load(projectDir); ok {
		return v.(map[string]*tsLangConfig)
	}
	m := make(map[string]*tsLangConfig)
	for _, qf := range loadProjectCached(projectDir) {
		cfg := tsConfigOf(qf)
		if cfg == nil {
			continue
		}
		for _, ext := range qf.Extensions {
			m[strings.ToLower(ext)] = cfg
		}
	}
	projectTsExtCache.Store(projectDir, m)
	return m
}

// tsLangConfigFor resolves an extension for one project: the project's own query
// files first, then the global table. This is the lookup that lets a project
// introduce a language, rather than only override one the runtime declares.
func tsLangConfigFor(projectDir, ext string) (*tsLangConfig, bool) {
	ext = strings.ToLower(ext)
	if projectDir != "" {
		if cfg, ok := projectTsExtMap(projectDir)[ext]; ok {
			return cfg, true
		}
	}
	extTablesMu.RLock()
	defer extTablesMu.RUnlock()
	cfg, ok := tsExtMap[ext]
	return cfg, ok
}

func init() {
	initTsExtMap()
}

type TreeSitterParser struct {
	projectDir string
}

func (t *TreeSitterParser) Parse(path string, isDepend bool, opts ParseOptions) (*ParsedFile, error) {
	ext := strings.ToLower(path[strings.LastIndex(path, "."):])
	cfg, ok := tsLangConfigFor(t.projectDir, ext)
	if !ok {
		return nil, fmt.Errorf("no grammar for %s", ext)
	}
	return t.parseWithConfig(path, ext, cfg, isDepend, opts)
}

func (t *TreeSitterParser) ParseWithGrammar(path, grammarName string, isDepend bool, opts ParseOptions) (*ParsedFile, error) {
	extTablesMu.RLock()
	cfg, ok := tsGrammarMap[grammarName]
	extTablesMu.RUnlock()
	if !ok {
		return nil, fmt.Errorf("unknown tree-sitter grammar: %s", grammarName)
	}
	ext := strings.ToLower(path[strings.LastIndex(path, "."):])
	return t.parseWithConfig(path, ext, cfg, isDepend, opts)
}

func (t *TreeSitterParser) parseWithConfig(path, ext string, cfg *tsLangConfig, isDepend bool, opts ParseOptions) (*ParsedFile, error) {
	src, err := ReadFileBytes(path)
	if err != nil {
		return nil, err
	}

	langName := strings.TrimPrefix(cfg.Grammar, "tree-sitter-")
	lang, langErr := resolveTreeSitterLang(langName, cfg.Grammar)
	if langErr != nil {
		return nil, langErr
	}

	p := parserPool.Get().(*sitter.Parser)
	if err := p.SetLanguage(lang); err != nil {
		parserPool.Put(p)
		return nil, fmt.Errorf("tree-sitter set language failed: %w", err)
	}

	tree := p.Parse(src, nil)
	parserPool.Put(p)
	if tree == nil {
		return nil, fmt.Errorf("tree-sitter parse returned nil tree")
	}
	defer tree.Close()

	root := tree.RootNode()
	if root == nil {
		return nil, fmt.Errorf("tree-sitter root node is nil")
	}

	result := &ParsedFile{
		Path:     path,
		Language: cfg.Language,
		IsDepend: isDepend,
		Entities: make(map[string][]Entity),
	}
	// Only materialise the whole-file source when it will actually be stored;
	// otherwise this is a full copy of every parsed file, discarded immediately.
	if opts.IndexSource {
		result.Source = string(src)
	}

	specificLabels := map[string]bool{
		"Struct": true, "Interface": true, "Class": true, "Trait": true, "Enum": true,
	}
	seenNames := map[string]bool{}

	var langConfig *ExternalQueryFile
	var compiledEntries []compiledQueryEntry
	if t.projectDir != "" {
		compiledEntries = compiledQueriesFor(t.projectDir, cfg.Language, ext, lang)
		langConfig = resolvedLangConfigFor(t.projectDir, cfg.Language, ext)
	}

	var rpcQueries []ExternalQueryDef
	for _, ce := range compiledEntries {
		rpcQueries = append(rpcQueries, ExternalQueryDef(ce.Def))
	}

	// Export strategy is fixed per file, so the modifier-based verdict can be
	// decided while each entity's body text is still in hand — no need to retain
	// that text on the Entity.
	exportStrategy, exportCfg, exportCfgList := exportStrategyOf(langConfig)

	// Declaration nodes for the entities found below, collected while their name
	// nodes are already in hand. See attachDocstringsTS.
	docM := newDocstringMatchers(langConfig, lang)
	var docSites []*sitter.Node

	for i, ce := range compiledEntries {
		qdef := rpcQueries[i]

		qc := queryCursorPool.Get().(*sitter.QueryCursor)
		matches := qc.Matches(ce.Query, root, src)

		for {
			match := matches.Next()
			if match == nil {
				break
			}

			for ci := range match.Captures {
				capture := &match.Captures[ci]
				name := capture.Node.Utf8Text(src)
				if name == "" {
					continue
				}

				if qdef.DataKey == "imports" {
					name = strings.Trim(name, "'\"")
				}

				if !specificLabels[qdef.GraphLabel] && seenNames[name] {
					continue
				}

				startPt := capture.Node.StartPosition()
				startLine := int(startPt.Row) + 1

				parent := capture.Node.Parent()
				endLine := startLine
				if parent != nil {
					parentEndPt := parent.EndPosition()
					endLine = int(parentEndPt.Row) + 1
				}
				entitySource := ""
				complexity := 1
				if parent != nil {
					entitySource = parent.Utf8Text(src)
					complexity = ComputeCyclomaticComplexity(entitySource)
				}

				contextName, contextType := resolveParentContextTS(&capture.Node, src, langConfig)

				result.AddEntity(qdef.DataKey, Entity{
					Name:           name,
					Line:           startLine,
					EndLine:        endLine,
					ModifierExport: ModifierExportVerdict(exportStrategy, entitySource, exportCfg, exportCfgList),
					GraphLabel:     qdef.GraphLabel,
					Complexity:     complexity,
					Context:        contextName,
					ContextType:    contextType,
				})

				if docM.on && qdef.GraphLabel != "" {
					if decl := declSiteFor(&capture.Node, docM); decl != nil {
						docSites = append(docSites, decl)
					}
				}

				if specificLabels[qdef.GraphLabel] {
					seenNames[name] = true
				}
			}
		}
		queryCursorPool.Put(qc)
	}

	attachDocstringsTS(docSites, src, result, docM)

	// Comments are entities in their own right, in every language: the text is
	// the name, so "what does the documentation say" is answerable by search.
	extractCommentsTS(root, src, result,
		commentQueryFor(cfg.Grammar, lang, langConfig), docM, filepath.Base(path))

	relationTypes := buildRelationTypeMap(rpcQueries)
	attachDecorators(result, relationTypes)

	detectExportsTS(root, src, result, cfg.Language, langConfig, relationTypes)

	processRelations(result, relationTypes)
	resolveReceiverTypes(result, src, cfg.Language, langConfig)

	return result, nil
}

var defaultContextTypes = map[string]string{
	"class_declaration":     "Class",
	"class_definition":      "Class",
	"interface_declaration": "Interface",
	"struct_declaration":    "Struct",
	"trait_declaration":     "Trait",
	"namespace_declaration": "Namespace",
	"enum_declaration":      "Enum",
	"function_declaration":  "Function",
	"function_definition":   "Function",
	"method_declaration":    "Method",
	"method_definition":     "Method",
}

var defaultAnonFuncTypes = map[string]bool{
	"arrow_function":      true,
	"function_expression": true,
	"function":            true,
}

var defaultCommentTypes = map[string]bool{
	"comment":           true,
	"block_comment":     true,
	"line_comment":      true,
	"multiline_comment": true,
}

func SafeIsNull(n *sitter.Node) bool {
	return n == nil
}

func SafeChild(n *sitter.Node, idx int) *sitter.Node {
	if n == nil {
		return nil
	}
	return n.Child(uint(idx))
}

func SafeParent(n *sitter.Node) *sitter.Node {
	if n == nil {
		return nil
	}
	return n.Parent()
}

func SafeType(n *sitter.Node) string {
	if n == nil {
		return ""
	}
	return n.Kind()
}

func SafeChildByFieldName(n *sitter.Node, name string) *sitter.Node {
	if n == nil {
		return nil
	}
	return n.ChildByFieldName(name)
}

func SafeChildCount(n *sitter.Node) int {
	if n == nil {
		return 0
	}
	return int(n.ChildCount())
}

func resolveParentContextTS(node *sitter.Node, src []byte, langConfig *ExternalQueryFile) (string, string) {
	parentTypes := defaultContextTypes
	anonTypes := defaultAnonFuncTypes

	if langConfig != nil {
		if len(langConfig.ContextTypes) > 0 {
			parentTypes = langConfig.ContextTypes
		}
		if len(langConfig.AnonFuncTypes) > 0 {
			anonTypes = make(map[string]bool, len(langConfig.AnonFuncTypes))
			for _, t := range langConfig.AnonFuncTypes {
				anonTypes[t] = true
			}
		}
	}

	current := SafeParent(node)
	for !SafeIsNull(current) {
		nodeType := SafeType(current)
		if label, ok := parentTypes[nodeType]; ok {
			nameNode := SafeChildByFieldName(current, "name")
			if !SafeIsNull(nameNode) {
				return nameNode.Utf8Text(src), label
			}
		}

		if anonTypes[nodeType] {
			grandparent := SafeParent(current)
			if !SafeIsNull(grandparent) {
				if SafeType(grandparent) == "variable_declarator" {
					nameNode := SafeChildByFieldName(grandparent, "name")
					if !SafeIsNull(nameNode) {
						return nameNode.Utf8Text(src), "Function"
					}
				}
			}
		}
		current = SafeParent(current)
	}
	return "", ""
}

// kindMatcher matches node kinds by numeric symbol id when the grammar knows
// the name, falling back to the kind string otherwise. Node.Kind() allocates a
// Go string per call (C.GoString), and the docstring walk calls it for every
// node in the file; KindId() is a plain uint16 read.
type kindMatcher struct {
	ids   map[uint16]bool
	names map[string]bool // only for names the grammar did not resolve
}

func newKindMatcher(lang *sitter.Language, names map[string]bool) kindMatcher {
	m := kindMatcher{ids: make(map[uint16]bool, len(names))}
	for n := range names {
		if lang != nil {
			if id := lang.IdForNodeKind(n, true); id != 0 {
				m.ids[id] = true
				continue
			}
		}
		// Unknown to this grammar: keep matching by string so behaviour is
		// identical to the previous implementation.
		if m.names == nil {
			m.names = make(map[string]bool)
		}
		m.names[n] = true
	}
	return m
}

func (m kindMatcher) match(n *sitter.Node) bool {
	if n == nil {
		return false
	}
	if m.ids[n.KindId()] {
		return true
	}
	if m.names == nil {
		return false
	}
	return m.names[n.Kind()]
}

// docstringMatchers holds the compiled declaration/comment matchers for a file,
// or reports that the language has no declaration types and docstrings are not
// extracted at all.
type docstringMatchers struct {
	decl kindMatcher
	com  kindMatcher
	on   bool
}

func newDocstringMatchers(langConfig *ExternalQueryFile, lang *sitter.Language) docstringMatchers {
	var declTypes, comTypes map[string]bool
	if langConfig != nil && len(langConfig.DeclarationTypes) > 0 {
		declTypes = make(map[string]bool, len(langConfig.DeclarationTypes))
		for _, dt := range langConfig.DeclarationTypes {
			declTypes[dt] = true
		}
	}
	if langConfig != nil && len(langConfig.CommentTypes) > 0 {
		comTypes = make(map[string]bool, len(langConfig.CommentTypes))
		for _, ct := range langConfig.CommentTypes {
			comTypes[ct] = true
		}
	}
	if declTypes == nil {
		return docstringMatchers{}
	}
	if comTypes == nil {
		comTypes = defaultCommentTypes
	}
	return docstringMatchers{
		decl: newKindMatcher(lang, declTypes),
		com:  newKindMatcher(lang, comTypes),
		on:   true,
	}
}

// commentQueryCache holds one synthesized comment query per grammar.
var commentQueryCache sync.Map // map[string]*sitter.Query

// commentQueryFor builds a query matching every comment node kind the grammar
// actually has.
//
// Comments are not reachable through the per-language query files: those describe
// declarations, and no language declares a pattern for its own comments. Scanning
// the tree for them would reintroduce the whole-file traversal that was just
// removed, so the kinds are turned into one query instead and run by the same
// engine, on the C side, as part of the existing pass.
//
// Kinds absent from a grammar are dropped rather than passed through, because a
// single unknown node kind makes the whole query fail to compile — and the set of
// comment kinds is a union across languages, so most of it is absent from any one
// of them.
func commentQueryFor(grammarName string, lang *sitter.Language, langConfig *ExternalQueryFile) *sitter.Query {
	if lang == nil {
		return nil
	}
	if v, ok := commentQueryCache.Load(grammarName); ok {
		q, _ := v.(*sitter.Query)
		return q
	}

	kinds := make([]string, 0, len(defaultCommentTypes)+4)
	seen := map[string]bool{}
	add := func(k string) {
		if k == "" || seen[k] {
			return
		}
		seen[k] = true
		if lang.IdForNodeKind(k, true) != 0 {
			kinds = append(kinds, k)
		}
	}
	if langConfig != nil {
		for _, c := range langConfig.CommentTypes {
			add(c)
		}
	}
	for k := range defaultCommentTypes {
		add(k)
	}
	sort.Strings(kinds)

	var q *sitter.Query
	if len(kinds) > 0 {
		var b strings.Builder
		b.WriteByte('[')
		for _, k := range kinds {
			b.WriteString("(" + k + ")")
		}
		b.WriteString("] @c")
		compiled, err := sitter.NewQuery(lang, b.String())
		if err != nil {
			slog.Warn("comment query failed to compile", "grammar", grammarName, "error", err)
		} else {
			q = compiled
		}
	}
	commentQueryCache.Store(grammarName, q)
	return q
}

// extractCommentsTS records every comment in the file as an entity whose name is
// the comment's own text, and attaches it to what it documents.
//
// A comment that sits immediately before a declaration documents that
// declaration and points at it. Every other comment — a note inside a function
// body, a licence header, a commented-out line — points at the file, so it is
// still reachable rather than being dropped for having no owner.
func extractCommentsTS(root *sitter.Node, src []byte, result *ParsedFile,
	q *sitter.Query, m docstringMatchers, fileName string) {
	if q == nil || SafeIsNull(root) {
		return
	}

	qc := queryCursorPool.Get().(*sitter.QueryCursor)
	defer queryCursorPool.Put(qc)

	seen := map[string]bool{}
	matches := qc.Matches(q, root, src)
	for {
		match := matches.Next()
		if match == nil {
			break
		}
		for ci := range match.Captures {
			node := &match.Captures[ci].Node
			text := cleanDocstring(node.Utf8Text(src))
			if text == "" || seen[text] {
				continue
			}
			seen[text] = true

			line := int(node.StartPosition().Row) + 1
			target := fileName
			if m.on {
				if next := node.NextSibling(); !SafeIsNull(next) && m.decl.match(next) {
					if nameNode := SafeChildByFieldName(next, "name"); !SafeIsNull(nameNode) {
						target = nameNode.Utf8Text(src)
					}
				}
			}

			result.AddEntity("comments", Entity{
				Name:       text,
				Line:       line,
				EndLine:    int(node.EndPosition().Row) + 1,
				GraphLabel: LabelComment,
			})
			result.References = append(result.References, ReferenceInfo{
				SourceName: text,
				TargetName: target,
				RelType:    "REFERENCES",
				Line:       line,
			})
		}
	}
}

// declSiteFor returns the innermost ancestor of a captured name node that the
// language calls a declaration, or nil.
//
// Queries capture the name, not the declaration around it, and how far apart the
// two sit depends on the grammar — one level for `function_declaration name:`,
// two for a Go `var_declaration > var_spec`. Walking up from the capture costs a
// handful of steps per entity; finding the same nodes by scanning the tree costs
// one visit per node in the file.
func declSiteFor(nameNode *sitter.Node, m docstringMatchers) *sitter.Node {
	for n := SafeParent(nameNode); !SafeIsNull(n); n = SafeParent(n) {
		if m.decl.match(n) {
			return n
		}
	}
	return nil
}

// attachDocstringsTS assigns each declaration's documentation to the entity that
// shares its line and name.
//
// This used to run as a second full traversal of the tree, after the query pass
// had already found every entity. The traversal visited every node to locate the
// few that are declarations, and each visit crosses into the C library several
// times (child, kind, null checks), so its cost tracked file size rather than
// entity count. The query pass already holds the nodes, so the sites are
// collected there and only they are examined here.
//
// The pairing rule is unchanged: a declaration documents the entity recorded at
// the declaration's own start line under the declaration's own name. Declarations
// whose name sits on a later line than the declaration keyword — a signature
// broken across lines — therefore still go undocumented, exactly as before.
func attachDocstringsTS(sites []*sitter.Node, src []byte, result *ParsedFile, m docstringMatchers) {
	if !m.on || len(sites) == 0 {
		return
	}

	type entityKey struct {
		line int
		name string
	}
	entityIdx := make(map[entityKey]*Entity)
	for dataKey := range result.Entities {
		for i := range result.Entities[dataKey] {
			e := &result.Entities[dataKey][i]
			if e.GraphLabel != "" {
				entityIdx[entityKey{e.Line, e.Name}] = e
			}
		}
	}
	if len(entityIdx) == 0 {
		return
	}

	for _, decl := range sites {
		if SafeIsNull(decl) {
			continue
		}
		nameNode := SafeChildByFieldName(decl, "name")
		if SafeIsNull(nameNode) {
			continue
		}
		sp := decl.StartPosition()
		e, ok := entityIdx[entityKey{int(sp.Row) + 1, nameNode.Utf8Text(src)}]
		if !ok {
			continue
		}

		// A comment immediately preceding the declaration.
		if prev := decl.PrevSibling(); !SafeIsNull(prev) && m.com.match(prev) {
			if commentText := cleanDocstring(prev.Utf8Text(src)); commentText != "" {
				e.Docstring = commentText
			}
		}

		// Python-style: a bare string as the first statement of the body.
		if e.Docstring != "" {
			continue
		}
		if kind := SafeType(decl); kind != "function_definition" && kind != "class_definition" {
			continue
		}
		body := SafeChildByFieldName(decl, "body")
		if SafeChildCount(body) == 0 {
			continue
		}
		firstStmt := SafeChild(body, 0)
		if SafeType(firstStmt) != "expression_statement" || SafeChildCount(firstStmt) == 0 {
			continue
		}
		if expr := SafeChild(firstStmt, 0); SafeType(expr) == "string" {
			e.Docstring = cleanDocstring(expr.Utf8Text(src))
		}
	}
}

func detectExportsTS(root *sitter.Node, src []byte, result *ParsedFile, lang string, langConfig *ExternalQueryFile, relationTypes map[string]string) {
	exportedNames := make(map[string]bool)

	var strategy string
	var stratConfig map[string]string
	var stratConfigList map[string][]string

	if langConfig != nil && langConfig.Exports != nil {
		strategy = langConfig.Exports.Strategy
		stratConfig = langConfig.Exports.Config
		stratConfigList = langConfig.Exports.ConfigList
	} else {
		strategy = "none"
	}

	if strategy == "export_statement" && !SafeIsNull(root) {
		childCount := SafeChildCount(root)
		for i := 0; i < childCount; i++ {
			child := SafeChild(root, i)
			if SafeIsNull(child) {
				continue
			}
			if SafeType(child) == "export_statement" {
				decl := SafeChildByFieldName(child, "declaration")
				if !SafeIsNull(decl) {
					nameNode := SafeChildByFieldName(decl, "name")
					if !SafeIsNull(nameNode) {
						exportedNames[nameNode.Utf8Text(src)] = true
					}
				}

				cc := SafeChildCount(child)
				for j := 0; j < cc; j++ {
					spec := SafeChild(child, j)
					if SafeIsNull(spec) {
						continue
					}
					if SafeType(spec) == "export_clause" {
						specCC := SafeChildCount(spec)
						for k := 0; k < specCC; k++ {
							es := SafeChild(spec, k)
							if SafeIsNull(es) {
								continue
							}
							if SafeType(es) == "export_specifier" {
								nameNode := SafeChildByFieldName(es, "name")
								if !SafeIsNull(nameNode) {
									exportedNames[nameNode.Utf8Text(src)] = true
								}
							}
						}
					}
				}
			}
		}
	}

	for dataKey := range result.Entities {
		if _, isRelation := relationTypes[dataKey]; isRelation {
			continue
		}
		for i := range result.Entities[dataKey] {
			e := &result.Entities[dataKey][i]
			if e.GraphLabel == "" || e.Name == "" {
				continue
			}

			exported := isExported(strategy, e, exportedNames, stratConfig, stratConfigList)

			if exported {
				if e.Properties == nil {
					e.Properties = make(map[string]string)
				}
				e.Properties["is_exported"] = "true"
			}
		}
	}
}

// HasTreeSitterForExtension answers for the languages every project has. Callers
// that know which project they are working in should use the …In variant, or a
// language declared only by that project's query files will be invisible to them.
func HasTreeSitterForExtension(ext string) bool {
	return HasTreeSitterForExtensionIn("", ext)
}

func HasTreeSitterForExtensionIn(projectDir, ext string) bool {
	_, ok := tsLangConfigFor(projectDir, ext)
	return ok
}

func TreeSitterLangForExtension(ext string) string {
	return TreeSitterLangForExtensionIn("", ext)
}

func TreeSitterLangForExtensionIn(projectDir, ext string) string {
	if cfg, ok := tsLangConfigFor(projectDir, ext); ok {
		return cfg.Language
	}
	return ""
}

func TreeSitterSupportedExtensions() []string {
	extTablesMu.RLock()
	defer extTablesMu.RUnlock()
	var exts []string
	for ext := range tsExtMap {
		exts = append(exts, ext)
	}
	return exts
}
