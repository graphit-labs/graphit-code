package ast

import (
	"context"
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
	Exclusive  bool
}

type tsQueryDef struct {
	DataKey      string
	GraphLabel   string
	Pattern      string
	NameCapture  string
	Type         string
	RelationType string
	ValueCapture string
	ValueLabel   string
	// NameReject is a regexp the captured name must not match — see
	// ExternalQueryDef.NameReject.
	NameReject string
	// SpanCapture names the capture whose node delimits the entity, for a grammar
	// whose name sits in something narrower than the entity — see
	// ExternalQueryDef.SpanCapture.
	SpanCapture string
	// NameIsData says the entity's name is a data value, not an identifier — see
	// ExternalQueryDef.NameIsData.
	NameIsData    bool
	ParentCapture string
	ParentLabel   string
	// Mirrors ExternalQueryDef because the two types are cast into each other
	// directly — field ORDER is part of the contract, not just the names.
	TargetLabels []string
	// QualifierCapture here names a CAPTURE of the pattern itself, not a rule path: a
	// tree-sitter pattern is structural and matches the whole shape at once, so an
	// UPDATE's table is capturable beside its column. On ANTLR, whose pattern matches
	// ONE node, the same field is a path anchored at an ancestor.
	QualifierCapture string
	TargetFallback   string
}

var tsExtMap map[string]*tsLangConfig
var tsGrammarMap map[string]*tsLangConfig

var tsLangNameMap map[string]*tsLangConfig

var grammarLoader *DynGrammarLoader
var grammarLoaderOnce sync.Once

func initGrammarLoader() {
	grammarLoader = NewDynGrammarLoader()
}

type resolvedLang struct {
	lang *sitter.Language
	err  error
}

var langResolveCache sync.Map

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
			lang = NativeLanguage(strings.TrimPrefix(grammarName, "tree-sitter-"))
		}
		if lang == nil {
			r := resolvedLang{nil, fmt.Errorf("grammar load failed for %s: %w", grammarName, loadErr)}
			langResolveCache.Store(langName, r)
			return nil, r.err
		}
	}
	langResolveCache.Store(langName, resolvedLang{lang, nil})
	return lang, nil
}

var parserPool = sync.Pool{
	New: func() any {
		return sitter.NewParser()
	},
}

var queryCursorPool = sync.Pool{
	New: func() any {
		return sitter.NewQueryCursor()
	},
}

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
		Exclusive:  qf.Exclusive,
	}
}

var extTablesMu sync.RWMutex

func rebuildExtTables() {
	runtimeQ := runtimeQueryState.cached()
	userQ := userQueryState.cached()

	ts := make(map[string]*tsLangConfig)
	tsGram := make(map[string]*tsLangConfig)
	tsName := make(map[string]*tsLangConfig)
	antlrExt := make(map[string][]*antlrLangConfig)
	antlrGram := make(map[string]*antlrLangConfig)

	register := func(files []ExternalQueryFile) {
		for _, qf := range files {
			if qf.Parser == "antlr4" {
				cfg := antlrConfigOf(qf)
				if !cfg.Exclusive {
					for _, ext := range qf.Extensions {
						e := strings.ToLower(ext)
						antlrExt[e] = append(antlrExt[e], cfg)
					}
				}
				antlrGram[cfg.Grammar] = cfg
				continue
			}
			cfg := tsConfigOf(qf)
			if cfg == nil {
				continue
			}
			if !cfg.Exclusive {
				for _, ext := range qf.Extensions {
					ts[strings.ToLower(ext)] = cfg
				}
			}
			tsGram[cfg.Grammar] = cfg
			tsName[strings.ToLower(cfg.Language)] = cfg
		}
	}
	register(runtimeQ)
	register(mergeOnto(runtimeQ, userQ))

	extTablesMu.Lock()
	tsExtMap, tsGrammarMap, tsLangNameMap = ts, tsGram, tsName
	antlrExtMap, antlrGrammarMap = antlrExt, antlrGram
	extTablesMu.Unlock()
}

func initTsExtMap() {
	loadRuntimeCached()
	loadUserCached()
	rebuildExtTables()
}

var projectTsExtCache sync.Map

func projectTsExtMap(projectDir string) map[string]*tsLangConfig {
	if v, ok := projectTsExtCache.Load(projectDir); ok {
		return v.(map[string]*tsLangConfig)
	}
	m := make(map[string]*tsLangConfig)
	for _, qf := range effectiveProjectQueryFiles(projectDir) {
		cfg := tsConfigOf(qf)
		if cfg == nil || cfg.Exclusive {
			continue
		}
		for _, ext := range qf.Extensions {
			m[strings.ToLower(ext)] = cfg
		}
	}
	projectTsExtCache.Store(projectDir, m)
	return m
}

var projectTsLangCache sync.Map

func projectTsLangMap(projectDir string) map[string]*tsLangConfig {
	if v, ok := projectTsLangCache.Load(projectDir); ok {
		return v.(map[string]*tsLangConfig)
	}
	m := make(map[string]*tsLangConfig)
	for _, qf := range effectiveProjectQueryFiles(projectDir) {
		cfg := tsConfigOf(qf)
		if cfg == nil {
			continue
		}
		m[strings.ToLower(cfg.Language)] = cfg
	}
	projectTsLangCache.Store(projectDir, m)
	return m
}

func tsLangConfigByName(projectDir, name string) (*tsLangConfig, bool) {
	name = strings.ToLower(strings.TrimSpace(name))
	if name == "" {
		return nil, false
	}
	if projectDir != "" {
		if cfg, ok := projectTsLangMap(projectDir)[name]; ok {
			return withGrammarEnabled(projectDir, cfg)
		}
	}
	extTablesMu.RLock()
	cfg, ok := tsLangNameMap[name]
	extTablesMu.RUnlock()
	if !ok {
		return nil, false
	}
	return withGrammarEnabled(projectDir, cfg)
}

// withGrammarEnabled drops a resolved language whose grammar the project disabled.
//
// Consulted only after the table answered, so an extension nothing claims — most
// of them, on a full walk — never resolves the configuration at all.
func withGrammarEnabled(projectDir string, cfg *tsLangConfig) (*tsLangConfig, bool) {
	if cfg == nil || !grammarEnabledIn(projectDir, cfg.Language, cfg.Grammar) {
		return nil, false
	}
	return cfg, true
}

func primaryExtOf(cfg *tsLangConfig) string {
	if cfg == nil || len(cfg.Extensions) == 0 {
		return ""
	}
	return strings.ToLower(cfg.Extensions[0])
}

func tsLangConfigFor(projectDir, ext string) (*tsLangConfig, bool) {
	ext = strings.ToLower(ext)
	if projectDir != "" {
		if cfg, ok := projectTsExtMap(projectDir)[ext]; ok {
			return withGrammarEnabled(projectDir, cfg)
		}
	}
	extTablesMu.RLock()
	cfg, ok := tsExtMap[ext]
	extTablesMu.RUnlock()
	if !ok {
		return nil, false
	}
	return withGrammarEnabled(projectDir, cfg)
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
	if !grammarEnabledIn(t.projectDir, cfg.Language, cfg.Grammar) {
		return nil, fmt.Errorf("grammar disabled by configuration: %s", grammarName)
	}
	ext := strings.ToLower(path[strings.LastIndex(path, "."):])
	return t.parseWithConfig(path, ext, cfg, isDepend, opts)
}

func (t *TreeSitterParser) parseWithConfig(path, ext string, cfg *tsLangConfig, isDepend bool, opts ParseOptions) (*ParsedFile, error) {
	src, err := ReadFileBytes(path)
	if err != nil {
		return nil, err
	}
	return t.parseSource(path, ext, cfg, src, 0, 0, isDepend, opts)
}

func (t *TreeSitterParser) parseSource(path, ext string, cfg *tsLangConfig, src []byte,
	lineOffset, embedDepth int, isDepend bool, opts ParseOptions) (*ParsedFile, error) {

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

	exportStrategy, exportCfg, exportCfgList := exportStrategyOf(langConfig)

	docM := newDocstringMatchers(langConfig, lang)
	var docSites []*sitter.Node

	complexM := newComplexityMatcher(langConfig, lang)

	ctxResolver := newContextResolver(lang, langConfig, src)

	for i, ce := range compiledEntries {
		qdef := rpcQueries[i]

		qc := queryCursorPool.Get().(*sitter.QueryCursor)
		matches := qc.Matches(ce.Query, root, src)

		for {
			if opts.Cancelled != nil && opts.Cancelled() {
				return nil, context.Canceled
			}
			match := matches.Next()
			if match == nil {
				break
			}

			parentName := ""
			if ce.ParentIdx >= 0 {
				parentName = dataText(captureTextAt(match, ce.ParentIdx, src))
			}
			valueText := ""
			var valueNode *sitter.Node
			if ce.ValueIdx >= 0 {
				if n := captureNodeAt(match, ce.ValueIdx); n != nil {
					valueText = dataText(n.Utf8Text(src))
					valueNode = n
				}
			}

			qualifier := ""
			if ce.QualifierIdx >= 0 {
				qualifier = dataText(captureTextAt(match, ce.QualifierIdx, src))
			}

			isData := ce.ValueIdx >= 0 || ce.ParentIdx >= 0 || qdef.NameIsData

			for ci := range match.Captures {
				capture := &match.Captures[ci]
				if ce.NameIdx >= 0 && int(capture.Index) != ce.NameIdx {
					continue
				}
				name := capture.Node.Utf8Text(src)
				if isData {
					name = dataText(name)
				}
				// dataText trims, but it only runs on a query that declares a
				// value or a parent. A name is an identifier, a selector or a
				// reference target — never the whitespace around it — and a
				// padded one is not merely ugly: `{ title }` in svelte captured
				// `title ` and wrote a REFERENCES edge to a name no declaration
				// has, with nothing reporting it. Trimming is a no-op on the 350+
				// patterns that capture an identifier node.
				name = strings.TrimSpace(name)
				if name == "" {
					continue
				}

				if qdef.DataKey == "imports" {
					name = strings.Trim(name, "'\"")
				}

				if re := nameRejectMatcher(qdef.NameReject); re != nil && re.MatchString(name) {
					continue
				}

				if qdef.QualifierCapture != "" {
					if qualifier == "" {
						continue
					}
					name = qualifier + "." + name
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
				if ce.SpanIdx >= 0 {
					if n := captureNodeAt(match, ce.SpanIdx); n != nil {
						startLine = int(n.StartPosition().Row) + 1
						endLine = int(n.EndPosition().Row) + 1
					}
				}
				entitySource := ""
				complexity := 1
				if parent != nil {
					entitySource = parent.Utf8Text(src)
					complexity = complexM.score(parent, src)
				}

				contextName, contextType := "", ""
				if parentName != "" {
					contextName, contextType = parentName, qdef.ParentLabel
				} else {
					contextName, contextType = ctxResolver.resolve(&capture.Node)
				}

				var props map[string]string
				if valueText != "" {
					props = map[string]string{"value": valueText}
				}

				result.AddOrMergeEntity(qdef.DataKey, Entity{
					Name:           name,
					Line:           startLine,
					EndLine:        endLine,
					ModifierExport: ModifierExportVerdict(exportStrategy, entitySource, exportCfg, exportCfgList),
					GraphLabel:     qdef.GraphLabel,
					Complexity:     complexity,
					Context:        contextName,
					ContextType:    contextType,
					Properties:     props,
				})

				if valueText != "" && qdef.ValueLabel != "" {
					vLine := startLine
					vEnd := endLine
					if valueNode != nil {
						vLine = int(valueNode.StartPosition().Row) + 1
						vEnd = int(valueNode.EndPosition().Row) + 1
					}
					result.AddOrMergeEntity(qdef.DataKey, Entity{
						Name:        valueText,
						Line:        vLine,
						EndLine:     vEnd,
						GraphLabel:  qdef.ValueLabel,
						Complexity:  1,
						Context:     name,
						ContextType: qdef.GraphLabel,
					})
				}

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

		if opts.Cancelled != nil && opts.Cancelled() {
			return nil, context.Canceled
		}
	}

	attachDocstringsTS(docSites, src, result, docM)

	extractCommentsTS(root, src, result,
		commentQueryFor(cfg.Grammar, lang, langConfig), docM, filepath.Base(path))

	relationTypes := buildRelationTypeMap(rpcQueries)
	attachDecorators(result, relationTypes)

	detectExportsTS(root, src, result, cfg.Language, langConfig, relationTypes)

	processRelations(result, relationTypes)
	resolveReceiverTypes(result, src, cfg.Language, langConfig)

	shiftParsedLines(result, lineOffset)

	if langConfig != nil && len(langConfig.Embedded) > 0 {
		t.parseEmbedded(path, root, src, lang, langConfig, result,
			lineOffset, embedDepth, isDepend, opts)
	}

	return result, nil
}

const maxDataValueLen = 256

// dataText normalises the text of a value or parent capture.
//
// Grammars disagree about whether a value's delimiters belong to the value node:
// tree-sitter-xml's AttValue, tree-sitter-toml's string and tree-sitter-hcl's
// string_lit all span their quotes, while tree-sitter-json's string_content and
// tree-sitter-html's attribute_value do not. A node named `"prod"` is not what
// anyone searches for, so the quotes come off here rather than in nine query
// files. Returns "" for anything unusable as a name — blank, multi-line, or
// past maxDataValueLen — which the caller treats as "no value".
// unquoteMatchedPair removes matched surrounding quote pairs, one pair at a time.
//
// Python's `"""abc"""` reduces to `abc` while `"say 'hi'"` keeps its inner quote: a
// cutset trim (strings.Trim) is per character and would eat that one.
func unquoteMatchedPair(s string) string {
	for len(s) >= 2 {
		q := s[0]
		if (q != '"' && q != '\'' && q != '`') || s[len(s)-1] != q {
			break
		}
		s = strings.TrimSpace(s[1 : len(s)-1])
	}
	return s
}

func dataText(s string) string {
	s = unquoteMatchedPair(strings.TrimSpace(s))
	if s == "" || len(s) > maxDataValueLen || strings.ContainsAny(s, "\n\r") {
		return ""
	}
	return s
}

func captureNodeAt(match *sitter.QueryMatch, idx int) *sitter.Node {
	for ci := range match.Captures {
		if int(match.Captures[ci].Index) == idx {
			return &match.Captures[ci].Node
		}
	}
	return nil
}

func captureTextAt(match *sitter.QueryMatch, idx int, src []byte) string {
	if n := captureNodeAt(match, idx); n != nil {
		return n.Utf8Text(src)
	}
	return ""
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

type kindMatcher struct {
	ids   map[uint16]bool
	names map[string]bool
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

// complexityMatcher scores cyclomatic complexity by walking an entity's real
// syntax subtree — never its text. branches matches named decision-point node
// kinds (if_statement, a switch's case clause, ...); operators matches leaf
// tokens by their own text (the literal "&&", "||", ...), which is what most
// grammars need, and also covers the two that give every operator the same
// generic kind and only the leaf's text says which one it is (Julia, Scala —
// see score). boundary holds the node kinds that start a nested declaration —
// the walk stops there, because that entity is scored on its own, and folding
// its branches into the container would double-count them.
//
// on is false when the language YAML has no `complexity:` block. score then
// returns the base 1: no branch is invented by scanning text for keywords,
// which is imprecise in ways a real syntax check is not (a tab where a
// keyword scan expects a space breaks the match; a keyword inside a comment
// or a string literal does not break it, and should). A language with no
// `complexity:` block yet has no complexity signal, not a wrong one.
type complexityMatcher struct {
	branches                 kindMatcher
	operators                map[string]bool
	boundary                 map[string]bool
	headCallKind             string
	headCallNames            map[string]bool
	headCallPairNames        map[string]bool
	headCallSubjectPairNames map[string]bool
	on                       bool
}

func newComplexityMatcher(langConfig *ExternalQueryFile, lang *sitter.Language) complexityMatcher {
	if langConfig == nil || langConfig.Complexity == nil {
		return complexityMatcher{}
	}
	cfg := langConfig.Complexity
	if len(cfg.NodeTypes) == 0 && len(cfg.Operators) == 0 && cfg.HeadCalls == nil {
		return complexityMatcher{}
	}
	nodeTypes := make(map[string]bool, len(cfg.NodeTypes))
	for _, t := range cfg.NodeTypes {
		nodeTypes[t] = true
	}
	var operators map[string]bool
	if len(cfg.Operators) > 0 {
		operators = make(map[string]bool, len(cfg.Operators))
		for _, o := range cfg.Operators {
			operators[o] = true
		}
	}
	var boundary map[string]bool
	if len(langConfig.ContextTypes) > 0 {
		boundary = make(map[string]bool, len(langConfig.ContextTypes))
		for k := range langConfig.ContextTypes {
			boundary[k] = true
		}
	}
	m := complexityMatcher{
		branches:  newKindMatcher(lang, nodeTypes),
		operators: operators,
		boundary:  boundary,
		on:        true,
	}
	if cfg.HeadCalls != nil {
		m.headCallKind = cfg.HeadCalls.NodeType
		m.headCallNames = make(map[string]bool, len(cfg.HeadCalls.Names))
		for _, n := range cfg.HeadCalls.Names {
			m.headCallNames[n] = true
		}
		if len(cfg.HeadCalls.PairNames) > 0 {
			m.headCallPairNames = make(map[string]bool, len(cfg.HeadCalls.PairNames))
			for _, n := range cfg.HeadCalls.PairNames {
				m.headCallPairNames[n] = true
			}
		}
		if len(cfg.HeadCalls.SubjectPairNames) > 0 {
			m.headCallSubjectPairNames = make(map[string]bool, len(cfg.HeadCalls.SubjectPairNames))
			for _, n := range cfg.HeadCalls.SubjectPairNames {
				m.headCallSubjectPairNames[n] = true
			}
		}
	}
	return m
}

func (m complexityMatcher) score(root *sitter.Node, src []byte) int {
	if !m.on || root == nil {
		return 1
	}
	score := 1
	var walk func(n *sitter.Node, depth int)
	walk = func(n *sitter.Node, depth int) {
		if n == nil {
			return
		}
		if depth > 0 && m.boundary[n.Kind()] {
			return
		}
		if m.branches.match(n) {
			score++
		} else if len(m.operators) > 0 && n.ChildCount() == 0 && m.operators[n.Utf8Text(src)] {
			score++
		} else if m.headCallKind != "" && n.Kind() == m.headCallKind {
			if head := n.NamedChild(0); head != nil {
				headText := head.Utf8Text(src)
				afterHead := int(n.NamedChildCount()) - 1
				switch {
				case m.headCallNames[headText]:
					score++
				case m.headCallPairNames[headText] && afterHead > 0:
					score += afterHead / 2
				case m.headCallSubjectPairNames[headText] && afterHead > 1:
					score += (afterHead - 1) / 2
				}
			}
		}
		for i := 0; i < int(n.ChildCount()); i++ {
			walk(n.Child(uint(i)), depth+1)
		}
	}
	walk(root, 0)
	return score
}

var commentQueryCache sync.Map

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

func extractCommentsTS(root *sitter.Node, src []byte, result *ParsedFile,
	q *sitter.Query, m docstringMatchers, fileName string) {
	if q == nil || SafeIsNull(root) {
		return
	}

	qc := queryCursorPool.Get().(*sitter.QueryCursor)
	defer queryCursorPool.Put(qc)

	seen := map[uint]bool{}
	matches := qc.Matches(q, root, src)
	for {
		match := matches.Next()
		if match == nil {
			break
		}
		for ci := range match.Captures {
			node := &match.Captures[ci].Node
			start := node.StartByte()
			if seen[start] {
				continue
			}
			seen[start] = true
			text := cleanDocstring(node.Utf8Text(src))
			if text == "" {
				continue
			}

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
				SourceName: commentUIDName(line),
				TargetName: target,
				RelType:    "REFERENCES",
				Line:       line,
			})
		}
	}
}

func declSiteFor(nameNode *sitter.Node, m docstringMatchers) *sitter.Node {
	for n := SafeParent(nameNode); !SafeIsNull(n); n = SafeParent(n) {
		if m.decl.match(n) {
			return n
		}
	}
	return nil
}

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

		if prev := decl.PrevSibling(); !SafeIsNull(prev) && m.com.match(prev) {
			if commentText := cleanDocstring(prev.Utf8Text(src)); commentText != "" {
				e.Docstring = commentText
			}
		}

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
