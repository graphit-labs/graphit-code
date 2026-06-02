package ast

import (
	"context"
	"fmt"
	"strings"

	sitter "github.com/smacker/go-tree-sitter"
	"github.com/smacker/go-tree-sitter/c"
	"github.com/smacker/go-tree-sitter/cpp"
	"github.com/smacker/go-tree-sitter/csharp"
	"github.com/smacker/go-tree-sitter/golang"
	"github.com/smacker/go-tree-sitter/java"
	"github.com/smacker/go-tree-sitter/javascript"
	"github.com/smacker/go-tree-sitter/kotlin"
	"github.com/smacker/go-tree-sitter/php"
	"github.com/smacker/go-tree-sitter/python"
	"github.com/smacker/go-tree-sitter/ruby"
	"github.com/smacker/go-tree-sitter/rust"
	"github.com/smacker/go-tree-sitter/sql"
	"github.com/smacker/go-tree-sitter/swift"
	"github.com/smacker/go-tree-sitter/typescript/tsx"
	"github.com/smacker/go-tree-sitter/typescript/typescript"

	dart "github.com/graphit-labs/graphit-code/internal/ast/lang_dart"
)

type tsLangConfig struct {
	Language   string
	Extensions []string
	TSLang     *sitter.Language
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


var treeSitterGrammars = map[string]*sitter.Language{
	"javascript": javascript.GetLanguage(),
	"typescript": typescript.GetLanguage(),
	"tsx":        tsx.GetLanguage(),
	"python":     python.GetLanguage(),
	"go":         golang.GetLanguage(),
	"java":       java.GetLanguage(),
	"kotlin":     kotlin.GetLanguage(),
	"csharp":     csharp.GetLanguage(),
	"ruby":       ruby.GetLanguage(),
	"php":        php.GetLanguage(),
	"rust":       rust.GetLanguage(),
	"swift":      swift.GetLanguage(),
	"dart":       dart.GetLanguage(),
	"c":          c.GetLanguage(),
	"cpp":        cpp.GetLanguage(),
	"sql":        sql.GetLanguage(),
}

var tsExtMap map[string]*tsLangConfig

func init() {
	tsExtMap = make(map[string]*tsLangConfig)
	runtimeQ := loadRuntimeCached()
	for _, qf := range runtimeQ {
		grammar, ok := treeSitterGrammars[qf.Language]
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
		return nil, fmt.Errorf("no tree-sitter grammar for %s", ext)
	}

	src, err := ReadFileBytes(path)
	if err != nil {
		return nil, err
	}

	parser := sitter.NewParser()
	parser.SetLanguage(cfg.TSLang)

	tree, err := parser.ParseCtx(context.Background(), nil, src)
	if err != nil {
		return nil, fmt.Errorf("tree-sitter parse %s: %w", path, err)
	}
	defer tree.Close()

	root := tree.RootNode()

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
		q, qErr := sitter.NewQuery([]byte(qdef.Pattern), cfg.TSLang)
		if qErr != nil {
			continue
		}

		qc := sitter.NewQueryCursor()
		qc.Exec(q, root)

		for {
			match, ok := qc.NextMatch()
			if !ok {
				break
			}

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

				startLine := int(capture.Node.StartPoint().Row) + 1
				endLine := int(capture.Node.EndPoint().Row) + 1

				parent := capture.Node.Parent()
				entitySource := ""
				complexity := 1
				if parent != nil {
					entitySource = parent.Content(src)
					complexity = ComputeCyclomaticComplexity(entitySource)
				}

				contextName, contextType := resolveParentContext(capture.Node, src, langConfig)

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
	}

	extractDocstrings(root, src, result, langConfig)


	relationTypes := buildRelationTypeMap(queries)

	attachDecorators(result, relationTypes)

	detectExports(root, src, result, cfg.Language, langConfig, relationTypes)


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

func resolveParentContext(node *sitter.Node, src []byte, langConfig *ExternalQueryFile) (string, string) {
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

	current := node.Parent()
	for current != nil {
		nodeType := current.Type()
		if label, ok := parentTypes[nodeType]; ok {

			nameNode := current.ChildByFieldName("name")
			if nameNode != nil {
				return nameNode.Content(src), label
			}
		}

		if anonTypes[nodeType] {
			grandparent := current.Parent()
			if grandparent != nil && grandparent.Type() == "variable_declarator" {
				nameNode := grandparent.ChildByFieldName("name")
				if nameNode != nil {
					return nameNode.Content(src), "Function"
				}
			}
		}
		current = current.Parent()
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



func extractDocstrings(root *sitter.Node, src []byte, result *ParsedFile, langConfig *ExternalQueryFile) {
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

	var walk func(node *sitter.Node)
	walk = func(node *sitter.Node) {
		for i := 0; i < int(node.ChildCount()); i++ {
			child := node.Child(i)
			if child == nil {
				continue
			}

			nodeType := child.Type()
			if declTypes[nodeType] {

				if i > 0 {
					prev := node.Child(i - 1)
					if prev != nil && comTypes[prev.Type()] {
						commentText := cleanDocstring(prev.Content(src))
						if commentText != "" {
							declLine := int(child.StartPoint().Row) + 1

							nameNode := child.ChildByFieldName("name")
							if nameNode != nil {
								name := nameNode.Content(src)
								if e, ok := entityIdx[entityKey{declLine, name}]; ok {
									e.Docstring = commentText
								}
							}
						}
					}
				}

				if nodeType == "function_definition" || nodeType == "class_definition" {
					body := child.ChildByFieldName("body")
					if body != nil && body.ChildCount() > 0 {
						firstStmt := body.Child(0)
						if firstStmt != nil && firstStmt.Type() == "expression_statement" {
							if firstStmt.ChildCount() > 0 {
								expr := firstStmt.Child(0)
								if expr != nil && expr.Type() == "string" {
									declLine := int(child.StartPoint().Row) + 1
									nameNode := child.ChildByFieldName("name")
									if nameNode != nil {
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

		for _, prefix := range []string{"///", "//!", "//", "/**", "*/", "*", "# ", "#", "\"\"\"", "'''", "/*"} {
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

func detectExports(root *sitter.Node, src []byte, result *ParsedFile, lang string, langConfig *ExternalQueryFile, relationTypes map[string]string) {

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
		for i := 0; i < int(root.ChildCount()); i++ {
			child := root.Child(i)
			if child == nil {
				continue
			}
			if child.Type() == "export_statement" {
				decl := child.ChildByFieldName("declaration")
				if decl != nil {
					nameNode := decl.ChildByFieldName("name")
					if nameNode != nil {
						exportedNames[nameNode.Content(src)] = true
					}
				}

				for j := 0; j < int(child.ChildCount()); j++ {
					spec := child.Child(j)
					if spec != nil && spec.Type() == "export_clause" {
						for k := 0; k < int(spec.ChildCount()); k++ {
							es := spec.Child(k)
							if es != nil && es.Type() == "export_specifier" {
								nameNode := es.ChildByFieldName("name")
								if nameNode != nil {
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
