package ast

import (
	"context"
	"fmt"
	"os"
	"strings"
	"sync"

	sitter "github.com/smacker/go-tree-sitter"
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

// grammarLoader is the global dynamic grammar loader.
// Initialized once via initGrammarLoader().
var grammarLoader *DynGrammarLoader
var grammarLoaderOnce sync.Once

func initGrammarLoader() {
	var opts []DynGrammarLoaderOption
	// GRAPHIT_GRAMMAR_DIR allows dev/CI to point to the build output directory.
	if dir := os.Getenv("GRAPHIT_GRAMMAR_DIR"); dir != "" {
		opts = append(opts, WithExtraPaths(dir))
	}
	grammarLoader = NewDynGrammarLoader(opts...)
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


func initTsExtMap() {
	tsExtMap = make(map[string]*tsLangConfig)
	tsGrammarMap = make(map[string]*tsLangConfig)

	runtimeQ := loadRuntimeCached()
	for _, qf := range runtimeQ {
		if qf.Parser == "antlr4" {
			continue
		}
		grammar := qf.Grammar
		if grammar == "" {
			grammar = "tree-sitter-" + qf.Language
		}
		cfg := &tsLangConfig{
			Language:   qf.Language,
			Grammar:    grammar,
			Extensions: qf.Extensions,
		}
		for _, ext := range qf.Extensions {
			tsExtMap[ext] = cfg
		}
		tsGrammarMap[grammar] = cfg
	}
}

func init() {
	initTsExtMap()
}

type TreeSitterParser struct {
	projectDir    string
}

func (t *TreeSitterParser) Parse(path string, isDepend bool, opts ParseOptions) (*ParsedFile, error) {
	ext := strings.ToLower(path[strings.LastIndex(path, "."):])
	cfg, ok := tsExtMap[ext]
	if !ok {
		return nil, fmt.Errorf("no grammar for %s", ext)
	}
	return t.parseWithConfig(path, ext, cfg, isDepend, opts)
}

func (t *TreeSitterParser) ParseWithGrammar(path, grammarName string, isDepend bool, opts ParseOptions) (*ParsedFile, error) {
	cfg, ok := tsGrammarMap[grammarName]
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

	grammarLoaderOnce.Do(initGrammarLoader)
	// Strip "tree-sitter-" prefix to get the language name for dynamic loading.
	langName := strings.TrimPrefix(cfg.Grammar, "tree-sitter-")
	lang, err := grammarLoader.Load(langName)
	if err != nil {
		return nil, fmt.Errorf("grammar load failed for %s: %w", cfg.Grammar, err)
	}

	p := parserPool.Get().(*sitter.Parser)
	p.SetLanguage(lang)

	tree, err := p.ParseCtx(context.Background(), nil, src)
	parserPool.Put(p) // return parser to pool immediately after parse
	if err != nil {
		return nil, fmt.Errorf("tree-sitter parse failed: %w", err)
	}
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
		Source:   string(src),
		Entities: make(map[string][]Entity),
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

	for i, ce := range compiledEntries {
		qdef := rpcQueries[i]

		qc := queryCursorPool.Get().(*sitter.QueryCursor)
		qc.Exec(ce.Query, root)

		for {
			match, ok := qc.NextMatch()
			if !ok {
				break
			}
			match = qc.FilterPredicates(match, src)

			for _, capture := range match.Captures {
				name := capture.Node.Content(src)
				if name == "" {
					continue
				}

				if qdef.DataKey == "imports" {
					name = strings.Trim(name, "'\"")
				}

				if !specificLabels[qdef.GraphLabel] && seenNames[name] {
					continue
				}

				startPt := capture.Node.StartPoint()
				startLine := int(startPt.Row) + 1

				parent := capture.Node.Parent()
				endLine := startLine
				if parent != nil {
					parentEndPt := parent.EndPoint()
					endLine = int(parentEndPt.Row) + 1
				}
				entitySource := ""
				complexity := 1
				if parent != nil {
					entitySource = parent.Content(src)
					complexity = ComputeCyclomaticComplexity(entitySource)
				}

				contextName, contextType := resolveParentContextTS(capture.Node, src, langConfig)

				result.AddEntity(qdef.DataKey, Entity{
					Name:        name,
					Line:        startLine,
					EndLine:     endLine,
					Source:      entitySource,
					GraphLabel:  qdef.GraphLabel,
					Complexity:  complexity,
					Context:     contextName,
					ContextType: contextType,
				})

				if specificLabels[qdef.GraphLabel] {
					seenNames[name] = true
				}
			}
		}
		queryCursorPool.Put(qc)
	}

	extractDocstringsTS(root, src, result, langConfig)

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
	return n.Child(idx)
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
	return n.Type()
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
				return nameNode.Content(src), label
			}
		}

		if anonTypes[nodeType] {
			grandparent := SafeParent(current)
			if !SafeIsNull(grandparent) {
				if SafeType(grandparent) == "variable_declarator" {
					nameNode := SafeChildByFieldName(grandparent, "name")
					if !SafeIsNull(nameNode) {
						return nameNode.Content(src), "Function"
					}
				}
			}
		}
		current = SafeParent(current)
	}
	return "", ""
}

func extractDocstringsTS(root *sitter.Node, src []byte, result *ParsedFile, langConfig *ExternalQueryFile) {
	if SafeIsNull(root) {
		return
	}

	var declTypes map[string]bool
	var comTypes map[string]bool
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
		return
	}
	if comTypes == nil {
		comTypes = defaultCommentTypes
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

	var walk func(node *sitter.Node)
	walk = func(node *sitter.Node) {
		childCount := SafeChildCount(node)
		for i := 0; i < childCount; i++ {
			child := SafeChild(node, i)
			if SafeIsNull(child) {
				continue
			}

			nodeType := SafeType(child)
			if declTypes[nodeType] {
				if i > 0 {
					prev := SafeChild(node, i-1)
					if !SafeIsNull(prev) {
						if comTypes[SafeType(prev)] {
							commentText := cleanDocstring(prev.Content(src))
							if commentText != "" {
								sp := child.StartPoint()
								declLine := int(sp.Row) + 1
								nameNode := SafeChildByFieldName(child, "name")
								if !SafeIsNull(nameNode) {
									name := nameNode.Content(src)
									if e, ok := entityIdx[entityKey{declLine, name}]; ok {
										e.Docstring = commentText
									}
								}
							}
						}
					}
				}

				if nodeType == "function_definition" || nodeType == "class_definition" {
					body := SafeChildByFieldName(child, "body")
					if !SafeIsNull(body) {
						if SafeChildCount(body) > 0 {
							firstStmt := SafeChild(body, 0)
							if !SafeIsNull(firstStmt) {
								if SafeType(firstStmt) == "expression_statement" {
									if SafeChildCount(firstStmt) > 0 {
										expr := SafeChild(firstStmt, 0)
										if !SafeIsNull(expr) {
											if SafeType(expr) == "string" {
												sp := child.StartPoint()
												declLine := int(sp.Row) + 1
												nameNode := SafeChildByFieldName(child, "name")
												if !SafeIsNull(nameNode) {
													name := nameNode.Content(src)
													if e, ok := entityIdx[entityKey{declLine, name}]; ok && e.Docstring == "" {
														e.Docstring = cleanDocstring(expr.Content(src))
													}
												}
											}
										}
									}
								}
							}
						}
					}
				}
			}
			walk(child)
		}
	}
	walk(root)
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
						exportedNames[nameNode.Content(src)] = true
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
									exportedNames[nameNode.Content(src)] = true
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

func HasTreeSitterForExtension(ext string) bool {
	_, ok := tsExtMap[strings.ToLower(ext)]
	return ok
}

func TreeSitterLangForExtension(ext string) string {
	if cfg, ok := tsExtMap[strings.ToLower(ext)]; ok {
		return cfg.Language
	}
	return ""
}

func TreeSitterSupportedExtensions() []string {
	var exts []string
	for ext := range tsExtMap {
		exts = append(exts, ext)
	}
	return exts
}
