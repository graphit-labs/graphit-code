package main

import (
	"fmt"
	"strings"

	"github.com/graphit-labs/graphit-code/cmd/graphit-parser-plugin/treesitter"
	"github.com/graphit-labs/graphit-code/internal/ast"
)

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

func parseTreeSitter(req ast.ParseRequest) (*ast.ParsedFile, error) {
	lang, err := treesitter.GetLanguage(req.Grammar)
	if err != nil {
		return nil, err
	}

	p := treesitter.NewParser()
	defer p.Close()

	if !p.SetLanguage(lang) {
		return nil, fmt.Errorf("failed to set language for %s", req.Grammar)
	}

	tree, err := p.Parse(req.Content)
	if err != nil {
		return nil, err
	}
	defer tree.Close()

	root := tree.RootNode()

	result := &ast.ParsedFile{
		Path:     req.Path,
		Language: req.Language,
		IsDepend: req.IsDepend,
		Source:   string(req.Content),
		Entities: make(map[string][]ast.Entity),
	}

	specificLabels := map[string]bool{
		"Struct": true, "Interface": true, "Class": true, "Trait": true, "Enum": true,
	}
	seenNames := map[string]bool{}

	for _, qdef := range req.Queries {
		q, qErr := treesitter.NewQuery(lang, qdef.Pattern)
		if qErr != nil {
			continue
		}

		qc := treesitter.NewQueryCursor()
		qc.Exec(q, root)

		for {
			match, ok := qc.NextMatch(q, req.Content)
			if !ok {
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

				startPt := capture.Node.StartPoint()
				startLine := int(startPt.Row) + 1

				parent := capture.Node.Parent()

				endLine := startLine
				if !parent.IsNull() {
					parentEndPt := parent.EndPoint()
					endLine = int(parentEndPt.Row) + 1
				}
				entitySource := ""
				complexity := 1
				if !parent.IsNull() {
					entitySource = parent.Content()
					complexity = ast.ComputeCyclomaticComplexity(entitySource)
				}

				contextName, contextType := resolveParentContextTS(capture.Node, req.LangConfig)

				result.AddEntity(qdef.DataKey, ast.Entity{
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

	extractDocstringsTS(root, result, req.LangConfig)

	relationTypes := buildRelationTypeMap(req.Queries)

	attachDecorators(result, relationTypes)

	detectExportsTS(root, result, req.Language, req.LangConfig, relationTypes)

	processRelations(result, relationTypes)

	resolveReceiverTypes(result, req.Content, req.Language, req.LangConfig)

	return result, nil
}

func resolveParentContextTS(node treesitter.Node, langConfig *ast.ExternalQueryFile) (string, string) {
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
	for !current.IsNull() {
		nodeType := current.Type()
		if label, ok := parentTypes[nodeType]; ok {
			nameNode := current.ChildByFieldName("name")
			if !nameNode.IsNull() {
				return nameNode.Content(), label
			}
		}

		if anonTypes[nodeType] {
			grandparent := current.Parent()
			if !grandparent.IsNull() {
				gpType := grandparent.Type()
				if gpType == "variable_declarator" {
					nameNode := grandparent.ChildByFieldName("name")
					if !nameNode.IsNull() {
						return nameNode.Content(), "Function"
					}
				}
			}
		}
		current = current.Parent()
	}
	return "", ""
}

func extractDocstringsTS(root treesitter.Node, result *ast.ParsedFile, langConfig *ast.ExternalQueryFile) {
	if root.IsNull() {
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
	entityIdx := make(map[entityKey]*ast.Entity)
	for dataKey := range result.Entities {
		for i := range result.Entities[dataKey] {
			e := &result.Entities[dataKey][i]
			if e.GraphLabel != "" {
				entityIdx[entityKey{e.Line, e.Name}] = e
			}
		}
	}

	var walk func(node treesitter.Node)
	walk = func(node treesitter.Node) {
		childCount := node.ChildCount()
		for i := 0; i < int(childCount); i++ {
			child := node.Child(i)
			if child.IsNull() {
				continue
			}

			nodeType := child.Type()
			if declTypes[nodeType] {
				if i > 0 {
					prev := node.Child(i - 1)
					if !prev.IsNull() {
						prevType := prev.Type()
						if comTypes[prevType] {
							commentText := cleanDocstring(prev.Content())
							if commentText != "" {
								sp := child.StartPoint()
								declLine := int(sp.Row) + 1

								nameNode := child.ChildByFieldName("name")
								if !nameNode.IsNull() {
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
					body := child.ChildByFieldName("body")
					if !body.IsNull() {
						bodyCC := body.ChildCount()
						if bodyCC > 0 {
							firstStmt := body.Child(0)
							if !firstStmt.IsNull() {
								fsType := firstStmt.Type()
								if fsType == "expression_statement" {
									fsCC := firstStmt.ChildCount()
									if fsCC > 0 {
										expr := firstStmt.Child(0)
										if !expr.IsNull() {
											exprType := expr.Type()
											if exprType == "string" {
												sp := child.StartPoint()
												declLine := int(sp.Row) + 1
												nameNode := child.ChildByFieldName("name")
												if !nameNode.IsNull() {
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

func buildRelationTypeMap(queries []ast.ExternalQueryDef) map[string]string {
	m := make(map[string]string)
	for _, q := range queries {
		if q.Type == "relation" && q.RelationType != "" {
			m[q.DataKey] = q.RelationType
		}
	}
	return m
}

func attachDecorators(result *ast.ParsedFile, relationTypes map[string]string) {
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

func detectExportsTS(root treesitter.Node, result *ast.ParsedFile, lang string, langConfig *ast.ExternalQueryFile, relationTypes map[string]string) {
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

	if strategy == "export_statement" && !root.IsNull() {
		childCount := root.ChildCount()
		for i := 0; i < int(childCount); i++ {
			child := root.Child(i)
			if child.IsNull() {
				continue
			}
			childType := child.Type()
			if childType == "export_statement" {
				decl := child.ChildByFieldName("declaration")
				if !decl.IsNull() {
					nameNode := decl.ChildByFieldName("name")
					if !nameNode.IsNull() {
						exportedNames[nameNode.Content()] = true
					}
				}

				cc := child.ChildCount()
				for j := 0; j < int(cc); j++ {
					spec := child.Child(j)
					if spec.IsNull() {
						continue
					}
					specType := spec.Type()
					if specType == "export_clause" {
						specCC := spec.ChildCount()
						for k := 0; k < int(specCC); k++ {
							es := spec.Child(k)
							if es.IsNull() {
								continue
							}
							esType := es.Type()
							if esType == "export_specifier" {
								nameNode := es.ChildByFieldName("name")
								if !nameNode.IsNull() {
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

func isExported(strategy string, e *ast.Entity, exportedNames map[string]bool, config map[string]string, configList map[string][]string) bool {
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

func processRelations(result *ast.ParsedFile, relationTypes map[string]string) {
	for dk, relType := range relationTypes {
		entities, ok := result.Entities[dk]
		if !ok {
			continue
		}

		switch relType {
		case "CALLS":
			for _, e := range entities {
				result.CallSites = append(result.CallSites, ast.CallInfo{
					Name:       e.Name,
					Line:       e.Line,
					SourceName: e.Context,
					SourceType: e.ContextType,
				})
			}
		case "INSTANTIATES":
			for _, e := range entities {
				result.CallSites = append(result.CallSites, ast.CallInfo{
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
				result.References = append(result.References, ast.ReferenceInfo{
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

func resolveReceiverTypes(result *ast.ParsedFile, src []byte, lang string, langConfig *ast.ExternalQueryFile) {
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

func selfKeywordsForLang(lang string, langConfig *ast.ExternalQueryFile) []string {
	_ = lang
	if langConfig != nil && langConfig.SelfKeywords != nil {
		return langConfig.SelfKeywords
	}
	return nil
}
