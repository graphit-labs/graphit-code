package ast

import (
	"path/filepath"
	"sort"
	"strconv"
	"strings"
)

func ConvertToCache(pf *ParsedFile, rootPath string, _ bool, cluster string) *parseCacheEntry {
	abs, _ := filepath.Abs(pf.Path)
	relPath := computeRelPath(rootPath, abs)
	if relPath == "" {
		return nil
	}

	entry := &parseCacheEntry{
		RelPath:  relPath,
		Language: pf.Language,
		IsDepend: pf.IsDepend,
		Cluster:  cluster,
	}

	nameToUID := make(map[string]string)

	dataKeys := make([]string, 0, len(pf.Entities))
	for dataKey := range pf.Entities {
		dataKeys = append(dataKeys, dataKey)
	}
	sort.Strings(dataKeys)

	for _, dataKey := range dataKeys {
		for _, e := range pf.Entities[dataKey] {
			// A content-named entity's `name` is never a legitimate context target —
			// nothing nests inside a Value/Text/Comment node — so it never needs an
			// entry here, which also skips using its (possibly huge) content as a map
			// key.
			if e.Name != "" && !contentNamedLabels[entityLabelOf(dataKey, e)] {
				nameToUID[e.Name] = entityUID(relPath, e.Name, e.Context)
			}
		}
	}

	entityRows := make(map[[2]string]int)

	containsEdges := make(map[cachedContainsEdge]bool)

	dirSet := make(map[string]bool)
	dir := filepath.Dir(relPath)
	for dir != "." && dir != "" {
		if !dirSet[dir] {
			dirSet[dir] = true
			entry.DirPaths = append(entry.DirPaths, dir)
		}
		dir = filepath.Dir(dir)
	}

	for _, dataKey := range dataKeys {
		for i, e := range pf.Entities[dataKey] {
			label := entityLabelOf(dataKey, e)
			if label == "" {
				continue
			}

			if dataKey == "imports" {
				label = importEntityLabel(e.GraphLabel)
				canonName := canonicalModuleName(e.Name, relPath)
				entry.Imports = append(entry.Imports, cachedImport{
					FileUID:      relPath,
					ModuleUID:    canonName,
					ModuleName:   canonName,
					RawImport:    e.Name,
					Alias:        getProperty(e.Properties, "alias"),
					ImportedName: getProperty(e.Properties, "imported_name"),
					Line:         e.Line,
					Lang:         pf.Language,
					SourceFile:   relPath,
				})
			}

			if (dataKey == "parameters" || dataKey == "fields") && e.Context == "" {
				continue
			}

			uid := entityUID(relPath, e.Name, e.Context)
			switch {
			case label == LabelComment:
				// Must match commentUIDName's use in extractCommentsTS's
				// ReferenceInfo.SourceName exactly — see commentUIDName.
				uid = entityUID(relPath, commentUIDName(e.Line), "")
			case contentNamedLabels[label]:
				uid = contentNamedUID(relPath, dataKey, i)
			}

			var decorators []string
			isExported := false
			if e.Properties != nil {
				if dec := e.Properties["decorators"]; dec != "" {
					for _, d := range strings.Split(dec, ",") {
						d = strings.TrimSpace(d)
						if d != "" {
							decorators = append(decorators, d)
						}
					}
				}
				if e.Properties["is_exported"] == "true" {
					isExported = true
				}
			}

			rowKey := [2]string{uid, label}
			if idx, seen := entityRows[rowKey]; seen {
				row := &entry.Entities[idx]
				if row.Value == "" {
					row.Value = getProperty(e.Properties, "value")
				}
				if row.Docstring == "" {
					row.Docstring = e.Docstring
				}
				if len(row.Args) == 0 {
					row.Args = e.Args
				}
				if len(row.Decorators) == 0 {
					row.Decorators = decorators
				}
			} else {
				entityRows[rowKey] = len(entry.Entities)
				entry.Entities = append(entry.Entities, cachedEntity{
					Label:       label,
					UID:         uid,
					Name:        e.Name,
					Path:        relPath,
					Line:        e.Line,
					EndLine:     e.EndLine,
					Docstring:   e.Docstring,
					Lang:        langOr(e.Lang, pf.Language),
					Complexity:  e.Complexity,
					Context:     e.Context,
					ContextType: e.ContextType,
					IsDep:       pf.IsDepend,
					IsExported:  isExported,
					Decorators:  decorators,
					Args:        e.Args,
					Value:       getProperty(e.Properties, "value"),
				})
			}

			var parentUID string
			if e.Context != "" {
				parentUID = nameToUID[e.Context]
				if parentUID == "" {
					parentUID = entityUID(relPath, e.Context, "")
				}
			}

			if !contentNamedLabels[label] {
				nameToUID[e.Name] = uid
			}

			// Nothing contains itself. The lookup order above is what makes this
			// rare, but same-name nesting is not the only way to arrive here --
			// any future context rule that names an entity after itself would --
			// and a self CONTAINS edge is never a legitimate answer.
			if e.Context != "" && parentUID != uid {
				parentLabel := contextTypeToLabel(e.ContextType)
				edge := cachedContainsEdge{
					ParentUID:   parentUID,
					ChildUID:    uid,
					ParentLabel: parentLabel,
					ChildLabel:  label,
				}
				if !containsEdges[edge] {
					containsEdges[edge] = true
					entry.ContainsEdges = append(entry.ContainsEdges, edge)
				}
			}

			if dataKey != "parameters" && dataKey != "fields" {
				for _, arg := range e.Args {
					if arg == "" {
						continue
					}
					paramUID := uid + "::" + arg
					entry.Parameters = append(entry.Parameters, cachedParameter{
						UID:     paramUID,
						Name:    arg,
						FuncUID: uid,
						Path:    relPath,
						Line:    e.Line,
						Lang:    pf.Language,
					})
				}
			}

			if dataKey == "fields" && e.Context != "" {
				parentUID := nameToUID[e.Context]
				if parentUID == "" {
					parentUID = entityUID(relPath, e.Context, "")
				}
				parentType := "Class"
				if strings.ToLower(e.ContextType) == "struct" {
					parentType = "Struct"
				}
				entry.Fields = append(entry.Fields, cachedField{
					UID:        uid,
					Name:       e.Name,
					ParentUID:  parentUID,
					ParentType: parentType,
					Path:       relPath,
					Line:       e.Line,
					Lang:       pf.Language,
				})
			}
		}
	}

	for _, call := range pf.CallSites {
		callerUID := relPath
		callerType := LabelFile
		if call.SourceName != "" {
			callerUID = nameToUID[call.SourceName]
			if callerUID == "" {
				callerUID = entityUID(relPath, call.SourceName, "")
			}
			callerType = ""
		}
		calleeUID := call.FullName
		if calleeUID == "" {
			calleeUID = call.Name
		}
		sourceType := call.SourceType
		if callerType == LabelFile {
			sourceType = LabelFile
		} else if sourceType == "" {
			sourceType = "Function"
		}

		entry.Calls = append(entry.Calls, cachedCall{
			CallerUID:    callerUID,
			CalleeUID:    calleeUID,
			SourceType:   sourceType,
			Line:         call.Line,
			Path:         relPath,
			ReceiverType: call.ReceiverType,
			Lang:         langOr(call.Lang, pf.Language),
		})
	}

	for _, ent := range entry.Entities {
		if ent.Label != "Class" && ent.Label != "Struct" && ent.Label != "Interface" {
			continue
		}

		for _, entities := range pf.Entities {
			for _, e := range entities {
				if entityUID(relPath, e.Name, e.Context) != ent.UID {
					continue
				}
				if e.Properties == nil {
					continue
				}
				bases := e.Properties["bases"]
				if bases == "" {
					continue
				}
				for _, base := range strings.Split(bases, ",") {
					base = strings.TrimSpace(base)
					if base == "" {
						continue
					}
					relType := "INHERITS"
					if ent.Label == "Interface" || strings.Contains(getProperty(e.Properties, "implements"), base) {
						relType = "IMPLEMENTS"
					}
					entry.Inheritance = append(entry.Inheritance, cachedInheritance{
						ChildUID:  ent.UID,
						ParentUID: base,
						RelType:   relType,
						Path:      relPath,
						Line:      e.Line,
					})
				}
			}
		}
	}

	for _, ref := range pf.References {
		sourceUID := relPath
		if ref.SourceName != "" {
			sourceUID = nameToUID[ref.SourceName]
			if sourceUID == "" {
				sourceUID = entityUID(relPath, ref.SourceName, "")
			}
		}
		if ref.RelType == RelReadsField || ref.RelType == RelWritesField {
			entry.FieldAccess = append(entry.FieldAccess, cachedFieldAccess{
				SourceUID: sourceUID,
				FieldUID:  ref.TargetName,
				IsWrite:   ref.RelType == RelWritesField,
				Path:      relPath,
				Line:      ref.Line,
			})
			continue
		}

		entry.References = append(entry.References, cachedReference{
			SourceUID: sourceUID,
			TargetUID: ref.TargetName,
			RelType:   ref.RelType,
			Path:      relPath,
			Line:      ref.Line,
			Lang:      langOr(ref.Lang, pf.Language),
		})
	}

	return entry
}

func computeRelPath(rootPath, absPath string) string {
	rootAbs, _ := filepath.Abs(rootPath)
	rel, err := filepath.Rel(rootAbs, absPath)
	if err != nil || strings.HasPrefix(rel, "..") {
		return ""
	}
	return rel
}

func getProperty(props map[string]string, key string) string {
	if props == nil {
		return ""
	}
	return props[key]
}

func importEntityLabel(declared string) string {
	switch declared {
	case LabelInclude, LabelExport:
		return declared
	default:
		return LabelImport
	}
}

func entityLabelOf(dataKey string, e Entity) string {
	if e.GraphLabel != "" {
		return e.GraphLabel
	}
	if dataKey == "" {
		return ""
	}
	return strings.ToUpper(dataKey[:1]) + dataKey[1:]
}

var contentNamedLabels = map[string]bool{
	"Value": true, "AttributeValue": true, "Text": true, LabelComment: true,
}

func contentNamedUID(relPath, dataKey string, index int) string {
	return entityUID(relPath, dataKey+"#"+strconv.Itoa(index), "")
}

func commentUIDName(line int) string {
	return "comment@L" + strconv.Itoa(line)
}
