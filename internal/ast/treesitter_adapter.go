package ast

import (
	"fmt"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/ast/wasmts"
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

// ParseWithGrammar parses using a specific tree-sitter grammar name (e.g. "tree-sitter-sql"),
// bypassing the extension-based lookup. Used by CompositeParser for --grammar overrides.
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

	lang, cleanup, err := getTSLanguage(cfg.Grammar, t.projectDir)
	if err != nil {
		return nil, err
	}
	defer cleanup()

	p, err := lang.NewParser()
	if err != nil {
		return nil, err
	}
	defer p.Close()

	tree, err := p.Parse(src)
	if err != nil {
		return nil, err
	}
	defer tree.Close()

	root, err := tree.RootNode()
	if err != nil {
		return nil, err
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
	var queries []tsQueryDef
	if t.projectDir != "" {
		queries = mergedQueriesFor(t.projectDir, cfg.Language, ext, nil)
		langConfig = resolvedLangConfigFor(t.projectDir, cfg.Language, ext)
	}

	var rpcQueries []ExternalQueryDef
	for _, q := range queries {
		rpcQueries = append(rpcQueries, ExternalQueryDef(q))
	}

	for _, qdef := range rpcQueries {
		q, qErr := lang.NewQuery(qdef.Pattern)
		if qErr != nil {
			continue
		}

		qc, qcErr := lang.NewQueryCursor()
		if qcErr != nil {
			q.Close()
			continue
		}

		if err := qc.Exec(q, root); err != nil {
			qc.Close()
			q.Close()
			continue
		}

		for {
			match, ok, matchErr := qc.NextMatch(src)
			if matchErr != nil || !ok {
				break
			}

			for _, capture := range match.Captures {
				name := capture.Node.Content()
				if name == "" {
					continue
				}

				if qdef.DataKey == "imports" {
					name = strings.Trim(name, "'\"")
				}

				if !specificLabels[qdef.GraphLabel] && seenNames[name] {
					continue
				}

				startPt, startPtErr := capture.Node.StartPoint()
				if startPtErr != nil {
					continue
				}
				startLine := int(startPt.Row) + 1

				parent, parentErr := capture.Node.Parent()
				if parentErr != nil {
					continue
				}

				endLine := startLine
				if !SafeIsNull(parent) {
					parentEndPt, parentEndPtErr := parent.EndPoint()
					if parentEndPtErr == nil {
						endLine = int(parentEndPt.Row) + 1
					}
				}
				entitySource := ""
				complexity := 1
				if !SafeIsNull(parent) {
					entitySource = parent.Content()
					complexity = ComputeCyclomaticComplexity(entitySource)
				}

				contextName, contextType := resolveParentContextTS(capture.Node, langConfig)

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
		q.Close()
		qc.Close()
	}

	extractDocstringsTS(root, result, langConfig)

	relationTypes := buildRelationTypeMap(rpcQueries)
	attachDecorators(result, relationTypes)

	detectExportsTS(root, result, cfg.Language, langConfig, relationTypes)

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

func SafeIsNull(n *wasmts.Node) bool {
	if n == nil {
		return true
	}
	isNull, err := n.IsNull()
	if err != nil {
		return true
	}
	return isNull
}

func SafeChild(n *wasmts.Node, idx int) *wasmts.Node {
	if n == nil {
		return nil
	}
	child, err := n.Child(idx)
	if err != nil {
		return nil
	}
	return child
}

func SafeParent(n *wasmts.Node) *wasmts.Node {
	if n == nil {
		return nil
	}
	parent, err := n.Parent()
	if err != nil {
		return nil
	}
	return parent
}

func SafeType(n *wasmts.Node) string {
	if n == nil {
		return ""
	}
	t, err := n.Type()
	if err != nil {
		return ""
	}
	return t
}

func SafeChildByFieldName(n *wasmts.Node, name string) *wasmts.Node {
	if n == nil {
		return nil
	}
	child, err := n.ChildByFieldName(name)
	if err != nil {
		return nil
	}
	return child
}

func SafeChildCount(n *wasmts.Node) int {
	if n == nil {
		return 0
	}
	cnt, err := n.ChildCount()
	if err != nil {
		return 0
	}
	return int(cnt)
}

func resolveParentContextTS(node *wasmts.Node, langConfig *ExternalQueryFile) (string, string) {
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
				return nameNode.Content(), label
			}
		}

		if anonTypes[nodeType] {
			grandparent := SafeParent(current)
			if !SafeIsNull(grandparent) {
				if SafeType(grandparent) == "variable_declarator" {
					nameNode := SafeChildByFieldName(grandparent, "name")
					if !SafeIsNull(nameNode) {
						return nameNode.Content(), "Function"
					}
				}
			}
		}
		current = SafeParent(current)
	}
	return "", ""
}

func extractDocstringsTS(root *wasmts.Node, result *ParsedFile, langConfig *ExternalQueryFile) {
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

	var walk func(node *wasmts.Node)
	walk = func(node *wasmts.Node) {
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
							commentText := cleanDocstring(prev.Content())
							if commentText != "" {
								sp, errSp := child.StartPoint()
								if errSp == nil {
									declLine := int(sp.Row) + 1
									nameNode := SafeChildByFieldName(child, "name")
									if !SafeIsNull(nameNode) {
										name := nameNode.Content()
										if e, ok := entityIdx[entityKey{declLine, name}]; ok {
											e.Docstring = commentText
										}
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
												sp, errSp := child.StartPoint()
												if errSp == nil {
													declLine := int(sp.Row) + 1
													nameNode := SafeChildByFieldName(child, "name")
													if !SafeIsNull(nameNode) {
														name := nameNode.Content()
														if e, ok := entityIdx[entityKey{declLine, name}]; ok && e.Docstring == "" {
															e.Docstring = cleanDocstring(expr.Content())
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
			}
			walk(child)
		}
	}
	walk(root)
}

func detectExportsTS(root *wasmts.Node, result *ParsedFile, lang string, langConfig *ExternalQueryFile, relationTypes map[string]string) {
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
						exportedNames[nameNode.Content()] = true
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
									exportedNames[nameNode.Content()] = true
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

// TSConfigForGrammar returns the config for a named tree-sitter grammar, or nil.
func TSConfigForGrammar(name string) *tsLangConfig {
	return tsGrammarMap[name]
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
