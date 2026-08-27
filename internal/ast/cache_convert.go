package ast

import (
	"fmt"
	"path/filepath"
	"sort"
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
		Cluster:  cluster,
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

	// Data keys are visited in sorted order, not map order. Two queries can
	// legitimately produce the same entity — xml.yaml matches an element both
	// for its own sake and again to reach its text content — and the row that
	// wins decides what the node looks like. Under map iteration that verdict
	// changed between runs on identical input.
	dataKeys := make([]string, 0, len(pf.Entities))
	for dataKey := range pf.Entities {
		dataKeys = append(dataKeys, dataKey)
	}
	sort.Strings(dataKeys)

	// Pre-populate nameToUID in a first pass so that context lookups
	// (e.g., fields looking up their parent struct) are order-independent.
	// Go map iteration is non-deterministic, and without this pass, fields
	// processed before their parent struct would get a fallback UID that
	// doesn't match the actual struct node.
	for _, dataKey := range dataKeys {
		for _, e := range pf.Entities[dataKey] {
			if e.Name != "" {
				nameToUID[e.Name] = entityUID(relPath, e.Name, e.Context)
			}
		}
	}

	// entityRows indexes the rows already appended by (uid, label) so a repeated
	// entity is completed rather than duplicated. The graph writer keeps only
	// the first row per uid and label, so without this the second match's value
	// or docstring was simply discarded.
	entityRows := make(map[[2]string]int)

	// containsEdges deduplicates the CONTAINS edges written for this file.
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
		for _, e := range pf.Entities[dataKey] {
			label := entityLabelOf(dataKey, e)
			if label == "" {
				continue
			}

			// An import is recorded twice, on purpose, because it answers two
			// different questions. The File-[:IMPORTS]->Module edge says what this
			// file depends on, canonicalised so every file importing the same module
			// points at one node. The Import entity says where the statement is, in
			// this file, at this line — which the canonical module node cannot say,
			// and which is why `MATCH (n:Import)` used to be an error instead of an
			// answer: this branch ended in `continue`, so the entity was built and
			// thrown away.
			//
			// The label comes from importEntityLabel, not from this query file
			// directly — see there for why the declaration is only trusted for part
			// of the import family.
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
				// Same node, matched twice. Fill in what the earlier row lacked
				// instead of appending a row the writer would drop.
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
					Label:     label,
					UID:       uid,
					Name:      e.Name,
					Path:      relPath,
					Line:      e.Line,
					EndLine:   e.EndLine,
					Docstring: e.Docstring,
					// Not pf.Language: an entity from an embedded block belongs to the
					// grammar that parsed it, and the whole graph resolves names within
					// a language. See mergeParsedInto.
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

			// Resolved BEFORE this entity registers its own name. An element
			// nested in an element of the same name -- <frame> inside <frame>,
			// which Oracle Reports XML is full of -- would otherwise look itself
			// up: the assignment below overwrites the outer one, and the child
			// becomes its own parent. Entities arrive in document order, so the
			// name still in the map is the enclosing one.
			var parentUID string
			if e.Context != "" {
				parentUID = nameToUID[e.Context]
				if parentUID == "" {
					parentUID = entityUID(relPath, e.Context, "")
				}
			}

			nameToUID[e.Name] = uid

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
				// The graph writer emits a row per edge, so a repeat is a second
				// identical CONTAINS in the database, not a no-op.
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
		// Symmetric to references: a call with no entity around it is made by the
		// FILE. It holds for any language with top-level calls — an `init()` at the end
		// of a module, a bare script statement — not only for the embedded SQL that
		// exposed the case. Without this the CallerUID stayed empty and the edge was
		// built here and discarded by the writer.
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
			// The caller is the file, so the source end's label is File — any
			// SourceType the language declared would describe an entity that does
			// not exist here.
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
		// A reference with no entity around it belongs to the FILE, not to nobody.
		//
		// A statement at the top of a script — `insert into auditoria …` — has no
		// procedure around it, so SourceName is empty; with the UID empty alongside it,
		// the edge was built here and discarded by the writer, exactly as the Import
		// entity was built and thrown away with a `continue`. In a schema `.sql`, and
		// in the SQL of an embedded block, that is most of the DML.
		//
		// The file as source is the shape IMPORTS already uses
		// (File -[:IMPORTS]-> Module): "what touches this table" is a question about
		// the file when there is nothing smaller that can be named.
		sourceUID := relPath
		if ref.SourceName != "" {
			sourceUID = nameToUID[ref.SourceName]
			if sourceUID == "" {
				sourceUID = entityUID(relPath, ref.SourceName, "")
			}
		}
		// Field access has its own edge pair and its own target (Field), so it leaves
		// the reference list here.
		//
		// Staying in References was a dead end: the DML copy loop skips every type the
		// engine routes through a path of its own, and READS_FIELD/WRITES_FIELD are
		// among them; the dedicated path, in turn, reads entry.FieldAccess, which
		// nothing filled. The Go and TypeScript grammars did extract field access, and
		// it was thrown away between the parse and the graph — both relation tables
		// existed with zero edges.
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

// importEntityLabel names the entity an imports query produced.
//
// The declared label is honoured for the three forms the grammars actually capture,
// because they are not the same statement: an Import is `import x`, an Include is a C
// preprocessor `#include`, and an Export is a JavaScript `export ... from './x'`. All
// three pull a module in, which is why all three also produce the IMPORTS edge — the
// edge answers "what does this file depend on", the entity answers "what is written
// here, and where".
//
// Anything else becomes Import, and there are two ways to arrive here. A query declared
// `type: relation` carries no label at all, by this repository's own validator. And 22
// grammars used to declare Module on this data_key, which would be actively wrong: the
// entity uid is per file, so honouring it would fabricate a second Module node beside
// the canonical one every file already points at.
func importEntityLabel(declared string) string {
	switch declared {
	case LabelInclude, LabelExport:
		return declared
	default:
		return LabelImport
	}
}

// entityLabelOf is the graph label an entity will be written under.
//
// The query file's `graph_label` when it declares one, and otherwise the data key
// capitalised — `procedures` becomes `Procedure`… well, `Procedures`, which is why a
// query that means a specific label declares it. Shared so that anything reasoning
// about labels BEFORE the cache is built — picking an embedded block's host entity,
// say — classifies exactly as the writer will.
func entityLabelOf(dataKey string, e Entity) string {
	if e.GraphLabel != "" {
		return e.GraphLabel
	}
	if dataKey == "" {
		return ""
	}
	return strings.ToUpper(dataKey[:1]) + dataKey[1:]
}

// contentNamedLabels are the labels whose `name` IS their content rather than an
// identifier someone chose: a string literal, an attribute value, character data, a
// comment. They are produced by the grammars' `value_label` (Value, AttributeValue,
// Text across 37 shipped grammars) and by the engine's comment pass.
//
// They are excluded wherever the question is "which declaration is this" — picking a
// host entity, for one. A block of SQL sits INSIDE the character data that carries it,
// so the innermost entity spanning it is always that Text node, and attributing the
// statement to the text of the statement says nothing.
var contentNamedLabels = map[string]bool{
	"Value": true, "AttributeValue": true, "Text": true, LabelComment: true,
}
