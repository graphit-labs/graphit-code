package ast

import (
	"fmt"
	"log/slog"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/ast/wasmts"
)

type tsLangConfig struct {
	Language   string
	Extensions []string
	TSLang     *wasmts.Language
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

// grammarNameMap maps the language name from YAML configs to the tree-sitter
// grammar function name convention. Most are identical, but some differ:
//   "go" -> "go" (tree_sitter_go)
//   "csharp" -> "c_sharp" (tree_sitter_c_sharp)
var grammarNameMap = map[string]string{
	"csharp": "c_sharp",
}

func grammarFuncName(lang string) string {
	if mapped, ok := grammarNameMap[lang]; ok {
		return mapped
	}
	return lang
}

func init() {
	tsExtMap = make(map[string]*tsLangConfig)

	// Load all individual .wasm grammar files from the resolution chain
	builtinGrammars := initBuiltinGrammars()
	if builtinGrammars == nil {
		slog.Debug("no WASM grammars loaded, tree-sitter unavailable")
		return
	}

	// Load YAML query files and match them with available grammars.
	// YAML uses language names like "csharp", but the grammar function
	// is "c_sharp" — grammarFuncName() handles the mapping.
	runtimeQ := loadRuntimeCached()
	for _, qf := range runtimeQ {
		funcName := grammarFuncName(qf.Language)
		grammar, ok := builtinGrammars[funcName]
		if !ok {
			continue
		}
		cfg := &tsLangConfig{
			Language:   qf.Language,
			Extensions: qf.Extensions,
			TSLang:     grammar,
		}
		for _, ext := range qf.Extensions {
			tsExtMap[ext] = cfg
		}
	}
}

type TreeSitterParser struct {
	projectDir string
}

func (t *TreeSitterParser) Parse(path string, isDepend bool, opts ParseOptions) (*ParsedFile, error) {
	ext := strings.ToLower(path[strings.LastIndex(path, "."):])
	cfg, ok := tsExtMap[ext]
	if !ok {
		// Try loading a plug-and-play grammar from the project directory
		langName := strings.TrimPrefix(ext, ".")
		lang, err := getLanguage(langName, t.projectDir)
		if err != nil {
			return nil, fmt.Errorf("no tree-sitter grammar for %s", ext)
		}
		cfg = &tsLangConfig{Language: langName, Extensions: []string{ext}, TSLang: lang}
	}

	src, err := ReadFileBytes(path)
	if err != nil {
		return nil, err
	}

	// Lock the WASM module for the entire parse operation. All tree-sitter
	// operations (parser, tree, query, node) go through the same Module.call()
	// which operates on shared WASM linear memory. Concurrent access from
	// multiple goroutines corrupts memory and causes fatal "split stack
	// overflow" panics in wazero's AOT compiler engine.
	// Different language grammars use separate modules, so they still run
	// in parallel — only same-language files are serialized.
	cfg.TSLang.LockModule()
	defer cfg.TSLang.UnlockModule()

	parser, err := cfg.TSLang.NewParser()
	if err != nil {
		return nil, fmt.Errorf("tree-sitter create parser %s: %w", path, err)
	}
	defer parser.Close()

	tree, err := parser.Parse(src)
	if err != nil {
		return nil, fmt.Errorf("tree-sitter parse %s: %w", path, err)
	}
	defer tree.Close()

	root, err := tree.RootNode()
	if err != nil {
		return nil, fmt.Errorf("tree-sitter root node %s: %w", path, err)
	}

	result := &ParsedFile{
		Path:     path,
		Language: cfg.Language,
		IsDepend: isDepend,
		Source:   string(src),
		Entities: make(map[string][]Entity),
	}


	var langConfig *ExternalQueryFile
	var queries []tsQueryDef
	if t.projectDir != "" {
		queries = mergedQueriesFor(t.projectDir, cfg.Language, ext, cfg.TSLang)
		langConfig = resolvedLangConfigFor(t.projectDir, cfg.Language, ext)
	}

	specificLabels := map[string]bool{
		"Struct": true, "Interface": true, "Class": true, "Trait": true, "Enum": true,
	}
	seenNames := map[string]bool{}

	for _, qdef := range queries {
		q, qErr := cfg.TSLang.NewQuery(qdef.Pattern)
		if qErr != nil {
			continue
		}

		qc, qcErr := cfg.TSLang.NewQueryCursor()
		if qcErr != nil {
			q.Close()
			continue
		}

		if err := qc.Exec(q, root); err != nil {
			q.Close()
			qc.Close()
			continue
		}

		for {
			match, ok, err := qc.NextMatch(src)
			if err != nil || !ok {
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

				startPt, spErr := capture.Node.StartPoint()
				endPt, epErr := capture.Node.EndPoint()
				if spErr != nil || epErr != nil {
					continue
				}
				startLine := int(startPt.Row) + 1
				endLine := int(endPt.Row) + 1

				parent, _ := capture.Node.Parent()
				entitySource := ""
				complexity := 1
				if parent != nil {
					entitySource = parent.Content()
					complexity = ComputeCyclomaticComplexity(entitySource)
				}

				contextName, contextType := resolveParentContextWASM(capture.Node, langConfig)

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

	extractDocstringsWASM(root, result, langConfig)


	relationTypes := buildRelationTypeMap(queries)

	attachDecorators(result, relationTypes)

	detectExportsWASM(root, result, cfg.Language, langConfig, relationTypes)


	processRelations(result, relationTypes)

	resolveReceiverTypes(result, src, cfg.Language, langConfig)

	return result, nil
}


func buildRelationTypeMap(queries []tsQueryDef) map[string]string {
	m := make(map[string]string)
	for _, q := range queries {
		if q.Type == "relation" && q.RelationType != "" {
			m[q.DataKey] = q.RelationType
		}
	}
	return m
}


func processRelations(result *ParsedFile, relationTypes map[string]string) {
	for dk, relType := range relationTypes {
		entities, ok := result.Entities[dk]
		if !ok {
			continue
		}

		switch relType {
		case "CALLS":
			for _, e := range entities {
				result.CallSites = append(result.CallSites, CallInfo{
					Name:       e.Name,
					Line:       e.Line,
					SourceName: e.Context,
					SourceType: e.ContextType,
				})
			}
		case "INSTANTIATES":
			for _, e := range entities {
				result.CallSites = append(result.CallSites, CallInfo{
					Name:       e.Name,
					Line:       e.Line,
					FullName:   "new:" + e.Name,
					SourceName: e.Context,
					SourceType: e.ContextType,
				})
			}
		case "DECORATOR", "EXPORT":
			continue
		default:
			for _, e := range entities {
				if strings.HasSuffix(relType, "_FIELD") && e.Context == "" {
					continue
				}
				result.References = append(result.References, ReferenceInfo{
					TargetName: e.Name,
					RelType:    relType,
					Line:       e.Line,
					SourceName: e.Context,
				})
			}
		}

		delete(result.Entities, dk)
	}
}

func resolveParentContextWASM(node *wasmts.Node, langConfig *ExternalQueryFile) (string, string) {
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

	current, _ := node.Parent()
	for current != nil {
		nodeType, err := current.Type()
		if err != nil {
			break
		}
		if label, ok := parentTypes[nodeType]; ok {

			nameNode, _ := current.ChildByFieldName("name")
			if nameNode != nil {
				return nameNode.Content(), label
			}
		}

		if anonTypes[nodeType] {
			grandparent, _ := current.Parent()
			if grandparent != nil {
				gpType, _ := grandparent.Type()
				if gpType == "variable_declarator" {
					nameNode, _ := grandparent.ChildByFieldName("name")
					if nameNode != nil {
						return nameNode.Content(), "Function"
					}
				}
			}
		}
		current, _ = current.Parent()
	}
	return "", ""
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



func extractDocstringsWASM(root *wasmts.Node, result *ParsedFile, langConfig *ExternalQueryFile) {
	if root == nil {
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
		childCount, err := node.ChildCount()
		if err != nil {
			return
		}
		for i := 0; i < int(childCount); i++ {
			child, err := node.Child(i)
			if err != nil || child == nil {
				continue
			}

			nodeType, err := child.Type()
			if err != nil {
				continue
			}
			if declTypes[nodeType] {

				if i > 0 {
					prev, err := node.Child(i - 1)
					if err == nil && prev != nil {
						prevType, _ := prev.Type()
						if comTypes[prevType] {
							commentText := cleanDocstring(prev.Content())
							if commentText != "" {
								sp, _ := child.StartPoint()
								declLine := int(sp.Row) + 1

								nameNode, _ := child.ChildByFieldName("name")
								if nameNode != nil {
									name := nameNode.Content()
									if e, ok := entityIdx[entityKey{declLine, name}]; ok {
										e.Docstring = commentText
									}
								}
							}
						}
					}
				}

				if nodeType == "function_definition" || nodeType == "class_definition" {
					body, _ := child.ChildByFieldName("body")
					if body != nil {
						bodyCC, _ := body.ChildCount()
						if bodyCC > 0 {
							firstStmt, _ := body.Child(0)
							if firstStmt != nil {
								fsType, _ := firstStmt.Type()
								if fsType == "expression_statement" {
									fsCC, _ := firstStmt.ChildCount()
									if fsCC > 0 {
										expr, _ := firstStmt.Child(0)
										if expr != nil {
											exprType, _ := expr.Type()
											if exprType == "string" {
												sp, _ := child.StartPoint()
												declLine := int(sp.Row) + 1
												nameNode, _ := child.ChildByFieldName("name")
												if nameNode != nil {
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

			walk(child)
		}
	}

	walk(root)
}



var defaultCommentTypes = map[string]bool{
	"comment":           true,
	"block_comment":     true,
	"line_comment":      true,
	"multiline_comment": true,
}

func cleanDocstring(raw string) string {
	lines := strings.Split(raw, "\n")
	var cleaned []string
	for _, line := range lines {
		line = strings.TrimSpace(line)

		for _, prefix := range []string{"///", "//!", "//", "/**", "*/", "*", "# ", "#", `"""`, "'''", "/*"} {
			if strings.HasPrefix(line, prefix) {
				line = strings.TrimPrefix(line, prefix)
				line = strings.TrimSpace(line)
				break
			}
		}
		if line != "" {
			cleaned = append(cleaned, line)
		}
	}
	return strings.Join(cleaned, "\n")
}

func attachDecorators(result *ParsedFile, relationTypes map[string]string) {
	decoratorEntities, ok := result.Entities["decorators"]
	if !ok || len(decoratorEntities) == 0 {
		return
	}

	type entityRef struct {
		dataKey string
		index   int
		line    int
	}
	var allEntities []entityRef
	for dk, entities := range result.Entities {
		if _, isRelation := relationTypes[dk]; isRelation {
			continue
		}
		for i, e := range entities {
			if e.GraphLabel != "" {
				allEntities = append(allEntities, entityRef{dk, i, e.Line})
			}
		}
	}

	for _, dec := range decoratorEntities {
		bestIdx := -1
		bestDist := int(^uint(0) >> 1)
		for j, ref := range allEntities {
			dist := ref.line - dec.Line
			if dist >= 0 && dist < bestDist {
				bestDist = dist
				bestIdx = j
			}
		}
		if bestIdx >= 0 {
			ref := allEntities[bestIdx]
			e := &result.Entities[ref.dataKey][ref.index]
			if e.Properties == nil {
				e.Properties = make(map[string]string)
			}
			if existing := e.Properties["decorators"]; existing != "" {
				e.Properties["decorators"] = existing + "," + dec.Name
			} else {
				e.Properties["decorators"] = dec.Name
			}
		}
	}

	delete(result.Entities, "decorators")
}

func resolveReceiverTypes(result *ParsedFile, src []byte, lang string, langConfig *ExternalQueryFile) {
	if len(result.CallSites) == 0 {
		return
	}

	lines := strings.Split(string(src), "\n")

	methodToClass := make(map[string]string)
	for _, dataKey := range []string{"functions", "methods"} {
		for _, e := range result.Entities[dataKey] {
			if e.Context != "" && (e.ContextType == "Class" || e.ContextType == "Struct") {
				methodToClass[e.Name] = e.Context
			}
		}
	}

	selfKeywords := selfKeywordsForLang(lang, langConfig)

	for i := range result.CallSites {
		call := &result.CallSites[i]

		if strings.HasPrefix(call.FullName, "new:") {
			call.ReceiverType = strings.TrimPrefix(call.FullName, "new:")
			continue
		}

		if call.SourceName != "" && len(selfKeywords) > 0 {
			className := methodToClass[call.SourceName]
			if className == "" {
				continue
			}

			lineIdx := call.Line - 1
			if lineIdx < 0 || lineIdx >= len(lines) {
				continue
			}
			lineText := lines[lineIdx]

			for _, kw := range selfKeywords {
				if strings.Contains(lineText, kw+call.Name) {
					call.ReceiverType = className
					break
				}
			}
		}
	}
}

func selfKeywordsForLang(lang string, langConfig *ExternalQueryFile) []string {
	_ = lang
	if langConfig != nil && langConfig.SelfKeywords != nil {
		return langConfig.SelfKeywords
	}
	return nil
}

func detectExportsWASM(root *wasmts.Node, result *ParsedFile, lang string, langConfig *ExternalQueryFile, relationTypes map[string]string) {

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


	if strategy == "export_statement" && root != nil {
		childCount, _ := root.ChildCount()
		for i := 0; i < int(childCount); i++ {
			child, err := root.Child(i)
			if err != nil || child == nil {
				continue
			}
			childType, _ := child.Type()
			if childType == "export_statement" {
				decl, _ := child.ChildByFieldName("declaration")
				if decl != nil {
					nameNode, _ := decl.ChildByFieldName("name")
					if nameNode != nil {
						exportedNames[nameNode.Content()] = true
					}
				}

				cc, _ := child.ChildCount()
				for j := 0; j < int(cc); j++ {
					spec, _ := child.Child(j)
					if spec == nil {
						continue
					}
					specType, _ := spec.Type()
					if specType == "export_clause" {
						specCC, _ := spec.ChildCount()
						for k := 0; k < int(specCC); k++ {
							es, _ := spec.Child(k)
							if es == nil {
								continue
							}
							esType, _ := es.Type()
							if esType == "export_specifier" {
								nameNode, _ := es.ChildByFieldName("name")
								if nameNode != nil {
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

	delete(result.Entities, "exports")
}


func isExported(strategy string, e *Entity, exportedNames map[string]bool, config map[string]string, configList map[string][]string) bool {
	switch strategy {
	case "capitalized_name":
		return len(e.Name) > 0 && e.Name[0] >= 'A' && e.Name[0] <= 'Z'

	case "no_prefix":
		prefix := config["prefix"]
		if prefix == "" {
			prefix = "_"
		}
		return len(e.Name) > 0 && !strings.HasPrefix(e.Name, prefix)

	case "export_statement":
		return exportedNames[e.Name]

	case "modifier":
		keyword := config["keyword"]
		if keyword == "" {
			return false
		}
		return e.Source != "" && containsModifier(e.Source, keyword)

	case "no_modifier":
		keywords := configList["keywords"]
		if len(keywords) == 0 {
			return true
		}
		if e.Source == "" {
			return true
		}
		for _, kw := range keywords {
			if containsModifier(e.Source, kw) {
				return false
			}
		}
		return true

	case "no_static":
		return e.Source != "" && !containsModifier(e.Source, "static")

	case "none":
		return false

	default:
		return false
	}
}

func containsModifier(source, modifier string) bool {

	check := source
	if len(check) > 200 {
		check = check[:200]
	}
	if idx := strings.Index(check, "\n"); idx > 0 {
		check = check[:idx]
	}
	return strings.Contains(check, modifier+" ") || strings.Contains(check, modifier+"\t") ||
		strings.HasPrefix(strings.TrimSpace(check), modifier)
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

func GetTreeSitterParser(ext string, projectDir ...string) *TreeSitterParser {
	if HasTreeSitterForExtension(ext) {
		pd := ""
		if len(projectDir) > 0 {
			pd = projectDir[0]
		}
		return &TreeSitterParser{projectDir: pd}
	}
	return nil
}

func TreeSitterSupportedExtensions() []string {
	var exts []string
	for ext := range tsExtMap {
		exts = append(exts, ext)
	}
	return exts
}
