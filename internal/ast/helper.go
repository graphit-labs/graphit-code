package ast

import (
	"bytes"
	"strings"
)

func cleanDocstring(raw string) string {
	lines := strings.Split(raw, "\n")
	var cleaned []string
	for _, line := range lines {
		line = strings.TrimSpace(line)

		for _, prefix := range []string{"///", "//!", "//", "/**", "*/", "*", "# ", "#", `"""`, "'''", "/*", "--", "<!--"} {
			if strings.HasPrefix(line, prefix) {
				line = strings.TrimPrefix(line, prefix)
				line = strings.TrimSpace(line)
				break
			}
		}

		// Closing markers were never removed, only opening ones, so a one-line
		// Python docstring came out as `Alpha docstring."""` and a one-line block
		// comment kept its `*/`. The name of a Comment entity is the text itself,
		// which makes the leftovers visible to anyone searching.
		for _, suffix := range []string{`"""`, "'''", "*/", "-->"} {
			if strings.HasSuffix(line, suffix) {
				line = strings.TrimSpace(strings.TrimSuffix(line, suffix))
				break
			}
		}
		if line != "" {
			cleaned = append(cleaned, line)
		}
	}
	return strings.Join(cleaned, "\n")
}

func buildRelationTypeMap(queries []ExternalQueryDef) map[string]string {
	m := make(map[string]string)
	for _, q := range queries {
		if q.Type == "relation" && q.RelationType != "" {
			m[q.DataKey] = q.RelationType
		}
	}
	return m
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

	case "modifier", "no_modifier", "no_static":
		return e.ModifierExport

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

func resolveReceiverTypes(result *ParsedFile, src []byte, lang string, langConfig *ExternalQueryFile) {
	if len(result.CallSites) == 0 {
		return
	}

	methodToClass := make(map[string]string)
	for _, dataKey := range []string{"functions", "methods"} {
		for _, e := range result.Entities[dataKey] {
			if e.Context != "" && (e.ContextType == "Class" || e.ContextType == "Struct") {
				methodToClass[e.Name] = e.Context
			}
		}
	}

	selfKeywords := selfKeywordsForLang(lang, langConfig)

	var lineStarts []int
	lineAt := func(idx int) ([]byte, bool) {
		if lineStarts == nil {
			lineStarts = make([]int, 1, bytes.Count(src, []byte{'\n'})+1)
			for off := 0; off < len(src); {
				j := bytes.IndexByte(src[off:], '\n')
				if j < 0 {
					break
				}
				off += j + 1
				lineStarts = append(lineStarts, off)
			}
		}
		if idx < 0 || idx >= len(lineStarts) {
			return nil, false
		}
		start := lineStarts[idx]
		end := len(src)
		if idx+1 < len(lineStarts) {
			end = lineStarts[idx+1] - 1
		}
		return src[start:end], true
	}

	var needle []byte
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

			lineText, ok := lineAt(call.Line - 1)
			if !ok {
				continue
			}

			for _, kw := range selfKeywords {
				needle = append(needle[:0], kw...)
				needle = append(needle, call.Name...)
				if bytes.Contains(lineText, needle) {
					call.ReceiverType = className
					break
				}
			}
		}
	}
}

func exportStrategyOf(langConfig *ExternalQueryFile) (string, map[string]string, map[string][]string) {
	if langConfig == nil || langConfig.Exports == nil {
		return "", nil, nil
	}
	return langConfig.Exports.Strategy, langConfig.Exports.Config, langConfig.Exports.ConfigList
}

// ModifierExportVerdict evaluates the modifier-based export strategies against
// an entity's body text. It is called while building the entity, so the text
// does not have to be retained on the Entity afterwards. Behaviour is identical
// to the previous in-isExported logic.
func ModifierExportVerdict(strategy, source string, config map[string]string, configList map[string][]string) bool {
	switch strategy {
	case "modifier":
		keyword := config["keyword"]
		if keyword == "" {
			return false
		}
		return source != "" && containsModifier(source, keyword)

	case "no_modifier":
		keywords := configList["keywords"]
		if len(keywords) == 0 {
			return true
		}
		if source == "" {
			return true
		}
		for _, kw := range keywords {
			if containsModifier(source, kw) {
				return false
			}
		}
		return true

	case "no_static":
		return source != "" && !containsModifier(source, "static")
	}
	return false
}

func selfKeywordsForLang(lang string, langConfig *ExternalQueryFile) []string {
	_ = lang
	if langConfig != nil && langConfig.SelfKeywords != nil {
		return langConfig.SelfKeywords
	}
	return nil
}
