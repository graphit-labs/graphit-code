package ast

import (
	"fmt"
	"path/filepath"
	"strings"
)

func ConvertToCache(pf *ParsedFile, rootPath string, indexSource bool, cluster string) *parseCacheEntry {
	abs, _ := filepath.Abs(pf.Path)
	relPath := computeRelPath(rootPath, abs)
	if relPath == "" {
		return nil
	}

	entry := &parseCacheEntry{
		RelPath:  relPath,
		Language: pf.Language,
		IsDepend: pf.IsDepend,
	}

	src := ""
	if indexSource {
		src = pf.Source
		if src == "" {
			if raw, err := ReadFileBytes(abs); err == nil {
				src = string(raw)
			}
		}
	}
	entry.Source = src

	entry.FileRow = []string{
		relPath, filepath.Base(abs), relPath,
		fmt.Sprint(pf.IsDepend), pf.Language, src,
	}
	if cluster != "" {
		entry.FileRow = append(entry.FileRow, cluster)
	}

	nameToUID := make(map[string]string)

	dirSet := make(map[string]bool)
	dir := filepath.Dir(relPath)
	for dir != "." && dir != "" {
		if !dirSet[dir] {
			dirSet[dir] = true
			entry.DirPaths = append(entry.DirPaths, dir)
		}
		dir = filepath.Dir(dir)
	}

	for dataKey, entities := range pf.Entities {
		for _, e := range entities {
			label := e.GraphLabel
			if label == "" {
				if len(dataKey) > 0 {
					label = strings.ToUpper(dataKey[:1]) + dataKey[1:]
				}
			}
			if label == "" {
				continue
			}

			if dataKey == "imports" {
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
				continue
			}

			if (dataKey == "parameters" || dataKey == "fields") && e.Context == "" {
				continue
			}

			uid := entityUID(relPath, e.Name, e.Context)

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

			entry.Entities = append(entry.Entities, cachedEntity{
				Label:       label,
				UID:         uid,
				Name:        e.Name,
				Path:        relPath,
				Line:        e.Line,
				EndLine:     e.EndLine,
				Docstring:   e.Docstring,
				Lang:        pf.Language,
				Complexity:  e.Complexity,
				Context:     e.Context,
				ContextType: e.ContextType,
				IsDep:       pf.IsDepend,
				IsExported:  isExported,
				Decorators:  decorators,
				Args:        e.Args,
				Value:       getProperty(e.Properties, "value"),
			})

			nameToUID[e.Name] = uid

			if e.Context != "" {
				parentUID := nameToUID[e.Context]
				if parentUID == "" {
					parentUID = entityUID(relPath, e.Context, "")
				}
				parentLabel := contextTypeToLabel(e.ContextType)
				entry.ContainsEdges = append(entry.ContainsEdges, cachedContainsEdge{
					ParentUID:   parentUID,
					ChildUID:    uid,
					ParentLabel: parentLabel,
					ChildLabel:  label,
				})
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
		callerUID := ""
		if call.SourceName != "" {
			callerUID = nameToUID[call.SourceName]
			if callerUID == "" {
				callerUID = entityUID(relPath, call.SourceName, "")
			}
		}
		calleeUID := call.FullName
		if calleeUID == "" {
			calleeUID = call.Name
		}
		sourceType := call.SourceType
		if sourceType == "" {
			sourceType = "Function"
		}

		entry.Calls = append(entry.Calls, cachedCall{
			CallerUID:    callerUID,
			CalleeUID:    calleeUID,
			SourceType:   sourceType,
			Line:         call.Line,
			Path:         relPath,
			ReceiverType: call.ReceiverType,
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
		sourceUID := ""
		if ref.SourceName != "" {
			sourceUID = nameToUID[ref.SourceName]
			if sourceUID == "" {
				sourceUID = entityUID(relPath, ref.SourceName, "")
			}
		}
		entry.References = append(entry.References, cachedReference{
			SourceUID: sourceUID,
			TargetUID: ref.TargetName,
			RelType:   ref.RelType,
			Path:      relPath,
			Line:      ref.Line,
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
