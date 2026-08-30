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

// tsQueryDef mirrors ExternalQueryDef for direct struct cast. The cast is
// positional: keep the fields in the same order as ExternalQueryDef.
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

// tsLangNameMap resolves a language NAME to its config, lowercased.
//
// tsExtMap answers "what parses .vue" and tsGrammarMap "what is tree-sitter-vue".
// Neither answers "what is the language called typescript", which is the question
// an embedded block asks: `lang="ts"` names a language, not an extension and not a
// grammar.
var tsLangNameMap map[string]*tsLangConfig

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
			// The language name is not always the grammar name. yaml_lang.yaml
			// declares `language: yaml_lang` — the identifier is qualified
			// because "yaml" is also the query files' own format — and
			// `grammar: tree-sitter-yaml`. Looking up only the language name
			// meant .yaml and .yml resolved no grammar at all and were never
			// parsed. The grammar field already names the grammar; use it.
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
		Exclusive:  qf.Exclusive,
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
	// The user directory outranks the runtime, matching resolveQueriesForLang —
	// and folded onto it first, so a user file declaring `merge: true` registers
	// the extensions and grammar it did not restate instead of unregistering
	// them.
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

// projectTsExtCache memoizes the per-project extension table, keyed by project
// directory. loadProjectCached already caches the parsed files; this caches the
// small map derived from them so it is not rebuilt once per file indexed.
var projectTsExtCache sync.Map // map[string]map[string]*tsLangConfig

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

// projectTsLangCache memoizes the per-project language-name table, keyed by
// project directory. Same reasoning as projectTsExtCache: derived from the parsed
// query files, so it is not rebuilt once per file indexed.
var projectTsLangCache sync.Map // map[string]map[string]*tsLangConfig

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

// tsLangConfigByName resolves a language by name for one project, project files
// first and then the global table — the same precedence as tsLangConfigFor.
//
// The second return value is a representative extension for that language, which
// the query resolution needs: queries are keyed by (language, extension), and a
// language's own first extension is the one its query file declares.
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

// primaryExtOf is the extension a language's queries are filed under. Empty when
// the language declares none, which filterByLangExt reads as "any".
func primaryExtOf(cfg *tsLangConfig) string {
	if cfg == nil || len(cfg.Extensions) == 0 {
		return ""
	}
	return strings.ToLower(cfg.Extensions[0])
}

// tsLangConfigFor resolves an extension for one project: the project's own query
// files first, then the global table. This is the lookup that lets a project
// introduce a language, rather than only override one the runtime declares.
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
	// A --grammar override does not revive a disabled grammar: discovery would
	// have dropped its files anyway, so honouring it here would only move the
	// failure. See docs/specs/ast_module.md.
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

// parseSource parses src as cfg's language and returns what it found.
//
// It was split out of parseWithConfig, which read the file and derived everything
// from that one buffer — the context resolver, the docstring matchers and the
// position of every entity. A single-file component needs a REGION of a file
// parsed with another grammar, and there was no way to hand this function a
// sub-buffer.
//
// lineOffset is added to the line of every record produced, once, at the end: the
// inner parse sees only the block's text, so every line it reports is relative to
// the block. embedDepth bounds the sub-parse — a language whose embedded block
// names its own language would otherwise recurse until the stack ends.
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

	// Cancellation is cooperative and lives in Go, not in the library's hooks.
	// tree-sitter 0.25 offers both a parser and a query-cursor progress callback,
	// and NEITHER is usable here:
	//   - QueryCursor.MatchesWithOptions passes a Go-allocated TSQueryCursorOptions
	//     to C and nothing keeps it alive; ts_query_cursor_exec_with_options
	//     returns immediately and the iteration happens later, so the GC collects
	//     the payload and the next match jumps through a dangling callback. It
	//     segfaults inside cgo, which kills the process (query.go:786).
	//   - Parser.ParseWithOptions is safe but leaks: it pairs Save/Unref for the
	//     input payload and only Saves the options payload (parser.go:351), so a
	//     handle is retained per parsed file — unbounded in a daemon.
	//
	// Checking between matches instead costs nothing and is nearly as prompt: the
	// cursor's time is spread over millions of Next calls (~6.6 µs each measured),
	// not spent in one long one. What stays uninterruptible is a single
	// ts_parser_parse, bounded by file size — 7.7 s for 47 MB, milliseconds for
	// anything normal.
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

	complexM := newComplexityMatcher(langConfig, lang)

	// One resolver per file: it memoises the ancestor walk, so entities that
	// share a container pay for that walk once instead of once each.
	ctxResolver := newContextResolver(lang, langConfig, src)

	for i, ce := range compiledEntries {
		qdef := rpcQueries[i]

		qc := queryCursorPool.Get().(*sitter.QueryCursor)
		matches := qc.Matches(ce.Query, root, src)

		for {
			// Between matches, not inside one: this is the granularity the
			// library safely allows, and it is fine because no single match
			// takes long.
			if opts.Cancelled != nil && opts.Cancelled() {
				// Dropped rather than pooled: this cursor is mid-execution, and
				// handing that state to the next caller is not worth saving one
				// C allocation.
				return nil, context.Canceled
			}
			match := matches.Next()
			if match == nil {
				break
			}

			// A pattern's helper captures — the `@_attr`, `@_type`, `@_def`
			// convention used throughout the query files — exist to be tested by
			// a predicate, not to be indexed. Every capture used to become an
			// entity carrying the query's graph_label, so HCL's
			// `(block (identifier) @_type (string_lit) @_rtype (string_lit) @name)`
			// turned one `resource "aws_instance" "web"` block into three
			// Resource nodes named `resource`, `"aws_instance"` and `"web"`, and
			// html.yaml's `(#eq? @_attr "id")` queries emitted a REFERENCES edge
			// to the literal name `id` alongside the one to the id's value.
			// name_capture already says which capture is the entity — the ANTLR
			// adapter has always honoured it — so honour it here too.
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

			// The text that qualifies this match's target, read once per match
			// because it is the same for every capture in it.
			qualifier := ""
			if ce.QualifierIdx >= 0 {
				qualifier = dataText(captureTextAt(match, ce.QualifierIdx, src))
			}

			// A query that names a value or a parent is describing data, so its
			// key is data too and gets the same normalisation: TOML and YAML
			// array items are quoted scalars, and `"alpha"` is not a name.
			//
			// `name_is_data` is the same statement made on its own, for a query whose
			// NAME is data and that declares neither of those — a unit named after an
			// XML attribute. It cannot be inferred from the captured node: a quoted
			// literal deliberately does NOT collapse into the identifier of the same
			// spelling (see TestQuotedBindingIsNotAnIdentifierReference), so only the
			// grammar can say which of the two this is.
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

				// What the grammar says can never be a name here. Before qualification,
				// so the expression sees the bare name it was written against, and
				// before the entity is built, because the point is that this match
				// records NOTHING. See ExternalQueryDef.NameReject.
				if re := nameRejectMatcher(qdef.NameReject); re != nil && re.MatchString(name) {
					continue
				}

				// A query that asks to qualify its target and cannot emits
				// NOTHING — the unqualified edge is the harmful one, collapsing
				// every owner's same-named member onto one node. See
				// ExternalQueryDef.QualifierCapture.
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
				// A declared span replaces both ends, because the entity is the
				// construct that capture delimits and not the declaration its name
				// sits in. Only the line range: entitySource and complexity below
				// stay on the name's parent. See ExternalQueryDef.SpanCapture.
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

				// The value is a node of its own, contained by the key, so both
				// halves of the pair are searchable and the pair survives as an
				// edge: Attribute "env" CONTAINS AttributeValue "prod".
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

		// An abandoned cursor simply stops yielding matches: no error, just
		// fewer entities. Returning that as a successful parse is worse than
		// returning nothing, because the caller stores it in the parse cache and
		// every later run trusts the truncated result.
		if opts.Cancelled != nil && opts.Cancelled() {
			return nil, context.Canceled
		}
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

	// The offset is applied here, once, rather than at each of the dozen places a
	// line is computed above. Every pass has finished, so every record that carries
	// a line is in `result` and nothing later reads a stale one.
	shiftParsedLines(result, lineOffset)

	// The blocks written in another language, parsed with that language's grammar
	// and folded in. Done last so the sub-parse merges into a finished result, and
	// after the shift so its own offsets are absolute already.
	if langConfig != nil && len(langConfig.Embedded) > 0 {
		t.parseEmbedded(path, root, src, lang, langConfig, result,
			lineOffset, embedDepth, isDepend, opts)
	}

	return result, nil
}

// maxDataValueLen caps the text a value capture may contribute.
//
// A value node's name *is* the value, and that name becomes a UID, an FTS row
// and a bag of trigrams. A YAML block scalar holding a shell script, or a JSON
// value that is itself a document, is none of the things a name is for, and
// indexing it costs more than it can ever return. Values past this length are
// dropped rather than truncated: a truncated value that still looks like a
// value is worse than an absent one.
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

// captureNodeAt returns the node captured under a capture index, or nil when the
// match did not bind it — an optional node in the pattern, or a quantified one
// that matched zero times.
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

// score walks root — an entity's own declaration node — and returns 1 plus
// one for every branch, operator and head-call match found in its subtree,
// skipping past any nested declaration boundary.
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
			// Matched by TEXT, not Kind(): most grammars spell && / || as a
			// leaf whose own kind IS that text (Go, C, Java, JS, ...), but
			// Julia and Scala give every operator the same generic kind
			// ("operator", "operator_identifier") and only the leaf's text
			// says which one it is. Checking text works for both.
			score++
		} else if m.headCallKind != "" && n.Kind() == m.headCallKind {
			// Clojure's list_lit and Elixir's call are the same node whether
			// they are "if" or an ordinary function invocation — only the
			// first named child's own text (the head symbol / the callee
			// identifier) says which. See HeadCallConfig.
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
