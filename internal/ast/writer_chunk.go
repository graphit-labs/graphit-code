package ast

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

func (w *GraphWriter) WriteChunkIncremental(ctx context.Context, chunk []*ParsedFile, repoPath string) error {
	if len(chunk) == 0 {
		return nil
	}

	var cmds []BatchQuery

	var deletePaths []string
	for _, pf := range chunk {
		abs, _ := filepath.Abs(pf.Path)
		deletePaths = append(deletePaths, w.rel(abs))
	}

	cmds = append(cmds, BatchQuery{
		Cypher: `UNWIND $files AS file MATCH (f:File {path: file})-[r:CONTAINS]->() DELETE r`,
		Params: map[string]any{"files": deletePaths},
	})
	cmds = append(cmds, BatchQuery{
		Cypher: `UNWIND $files AS file MATCH (f:File {path: file}) DETACH DELETE f`,
		Params: map[string]any{"files": deletePaths},
	})

	var fileData []map[string]any
	for _, pf := range chunk {
		abs, _ := filepath.Abs(pf.Path)
		rel := w.rel(abs)
		if rel == "" {
			continue
		}

		src := ""
		if w.indexSource {
			src = pf.Source
			if src == "" {
				if raw, err := os.ReadFile(abs); err == nil {
					src = string(raw)
				}
			}
		}

		fileData = append(fileData, map[string]any{
			"path": rel, "name": filepath.Base(abs), "rel": rel, "dep": pf.IsDepend, "lang": pf.Language, "src": src,
		})
	}

	clusterSet := ""
	if w.cluster != "" {
		clusterSet = ", f.cluster = '" + w.cluster + "'"
	}
	cmds = append(cmds, BatchQuery{
		Cypher: `UNWIND $data AS row MERGE (f:File {path: row.path}) SET f.name = row.name, f.relative_path = row.rel, f.is_dependency = row.dep, f.lang = row.lang, f.source = row.src` + clusterSet,
		Params: map[string]any{"data": fileData},
	})

	dirSet := make(map[string]string)
	dirRels := make(map[string]string)
	fileRels := make(map[string]string)

	for _, pf := range chunk {
		abs, _ := filepath.Abs(pf.Path)
		relPath := w.rel(abs)

		dirPart := filepath.Dir(relPath)
		if dirPart == "." {
			continue
		}

		parts := strings.Split(dirPart, string(filepath.Separator))
		parentPath := ""

		for _, part := range parts {
			if part == "." || part == "" {
				continue
			}
			curPath := part
			if parentPath != "" {
				curPath = filepath.Join(parentPath, part)
			}

			dirSet[curPath] = part
			if parentPath != "" {
				dirRels[curPath] = parentPath
			}
			parentPath = curPath
		}

		if parentPath != "" {
			fileRels[relPath] = parentPath
		}
	}

	var dirsData []map[string]any
	for path, name := range dirSet {
		dirsData = append(dirsData, map[string]any{"path": path, "name": name})
	}
	if len(dirsData) > 0 {
		dirClusterSet := ""
		if w.cluster != "" {
			dirClusterSet = ", d.cluster = '" + w.cluster + "'"
		}
		cmds = append(cmds, BatchQuery{
			Cypher: `UNWIND $data AS row MERGE (d:Directory {path: row.path}) SET d.name = row.name` + dirClusterSet,
			Params: map[string]any{"data": dirsData},
		})
	}

	var d2dData []map[string]any
	for child, parent := range dirRels {
		d2dData = append(d2dData, map[string]any{"child": child, "parent": parent})
	}
	if len(d2dData) > 0 {
		cmds = append(cmds, BatchQuery{
			Cypher: `UNWIND $data AS row MATCH (p:Directory {path: row.parent}), (c:Directory {path: row.child}) CREATE (p)-[:CONTAINS]->(c)`,
			Params: map[string]any{"data": d2dData},
		})
	}

	var d2fData []map[string]any
	for file, dir := range fileRels {
		d2fData = append(d2fData, map[string]any{"file": file, "dir": dir})
	}
	if len(d2fData) > 0 {
		cmds = append(cmds, BatchQuery{
			Cypher: `UNWIND $data AS row MATCH (p:Directory {path: row.dir}), (f:File {path: row.file}) CREATE (p)-[:CONTAINS]->(f)`,
			Params: map[string]any{"data": d2fData},
		})
	}

	entityDataByLabel := make(map[string][]map[string]any)
	importData := []map[string]any{}
	var childEdges []map[string]any
	var decoratorUpdates []BatchQuery

	for _, pf := range chunk {
		abs, _ := filepath.Abs(pf.Path)
		relPath := w.rel(abs)

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
					importData = append(importData, map[string]any{
						"name": canonName, "raw_import": e.Name,
						"path": relPath, "lang": pf.Language, "line": e.Line,
					})
					continue
				}

				if (dataKey == "parameters" || dataKey == "fields") && e.Context == "" {
					continue
				}

				uid := entityUID(relPath, e.Name, e.Context)
				parentUID := ""
				if e.Context != "" {
					parentUID = entityUID(relPath, e.Context, "")
				}

				var decorators string
				isExported := false
				if e.Properties != nil {
					decorators = e.Properties["decorators"]
					if e.Properties["is_exported"] == "true" {
						isExported = true
					}
				}

				entityDataByLabel[label] = append(entityDataByLabel[label], map[string]any{
					"uid": uid, "name": e.Name, "path": relPath, "line": e.Line, "el": e.EndLine,
					"lang": pf.Language, "doc": e.Docstring,
					"dep": pf.IsDepend, "cc": e.Complexity,
					"ctx": e.Context, "ctx_type": e.ContextType,
					"is_exported": isExported,
				})

				if decorators != "" {
					decParts := make([]string, 0)
					for _, d := range strings.Split(decorators, ",") {
						d = strings.TrimSpace(d)
						if d != "" {
							decParts = append(decParts, d)
						}
					}
					if len(decParts) > 0 {
						decoratorUpdates = append(decoratorUpdates, BatchQuery{
							Cypher: fmt.Sprintf(`MATCH (n:%s {uid: $uid}) SET n.decorators = $decs`, label),
							Params: map[string]any{"uid": uid, "decs": decParts},
						})
					}
				}

				if e.Context != "" {
					childEdges = append(childEdges, map[string]any{
						"parent_uid":   parentUID,
						"child_uid":    uid,
						"child_label":  label,
						"parent_label": e.ContextType,
					})
				}
			}
		}
	}

	for label, data := range entityDataByLabel {
		if len(data) == 0 {
			continue
		}
		entClusterSet := ""
		if w.cluster != "" {
			entClusterSet = ", n.cluster = '" + w.cluster + "'"
		}
		cmds = append(cmds, BatchQuery{
			Cypher: fmt.Sprintf(`UNWIND $data AS row MERGE (n:%s {uid: row.uid}) 
				SET n.name=row.name, n.path=row.path, n.line_number=row.line, n.end_line=row.el,
				n.lang=row.lang, n.docstring=row.doc, n.is_dependency=row.dep, 
				n.cyclomatic_complexity=row.cc, n.is_stub=false,
				n.context=row.ctx, n.context_type=row.ctx_type,
				n.is_exported=row.is_exported`, label) + entClusterSet,
			Params: map[string]any{"data": data},
		})
		cmds = append(cmds, BatchQuery{
			Cypher: fmt.Sprintf(`UNWIND $data AS row MATCH (f:File {path: row.path}), (n:%s {uid: row.uid}) CREATE (f)-[:CONTAINS]->(n)`, label),
			Params: map[string]any{"data": data},
		})
	}

	cmds = append(cmds, decoratorUpdates...)

	type edgeKey struct{ parent, child string }
	edgeGroups := make(map[edgeKey][]map[string]any)
	for _, ce := range childEdges {
		k := edgeKey{ce["parent_label"].(string), ce["child_label"].(string)}
		edgeGroups[k] = append(edgeGroups[k], ce)
	}
	for k, data := range edgeGroups {
		parentLabel := contextTypeToLabel(k.parent)
		childLabel := k.child
		if parentLabel == "" || childLabel == "" {
			continue
		}
		cmds = append(cmds, BatchQuery{
			Cypher: fmt.Sprintf(`UNWIND $data AS row MATCH (p:%s {uid: row.parent_uid}), (c:%s {uid: row.child_uid}) CREATE (p)-[:CONTAINS]->(c)`, parentLabel, childLabel),
			Params: map[string]any{"data": data},
		})
	}

	if len(importData) > 0 {
		modClusterSet := ""
		if w.cluster != "" {
			modClusterSet = ", m.cluster = '" + w.cluster + "'"
		}
		cmds = append(cmds, BatchQuery{
			Cypher: `UNWIND $data AS row MERGE (m:Module {uid: row.name}) SET m.name=row.name, m.lang = row.lang, m.full_import_name = row.raw_import, m.is_stub=false` + modClusterSet,
			Params: map[string]any{"data": importData},
		})
		cmds = append(cmds, BatchQuery{
			Cypher: `UNWIND $data AS row MATCH (f:File {path: row.path}), (m:Module {uid: row.name}) CREATE (f)-[r:IMPORTS {source_file: row.path}]->(m) SET r.line_number=row.line`,
			Params: map[string]any{"data": importData},
		})
	}

	var parentCallsData []map[string]any // calls with known enclosing entity
	var fileCallsData []map[string]any   // calls without parent context (file-level fallback)
	refsDataByType := make(map[string][]map[string]any)

	parentRefsDataByType := make(map[string][]map[string]any)
	var inheritsData []map[string]any
	var fieldAccessData []map[string]any

	for _, pf := range chunk {
		abs, _ := filepath.Abs(pf.Path)
		relPath := w.rel(abs)

		for _, call := range pf.CallSites {
			targetUID := call.Name
			if call.FullName != "" {
				targetUID = call.FullName
			}
			if call.SourceName != "" {

				parentCallsData = append(parentCallsData, map[string]any{
					"path":          relPath,
					"uid":           targetUID,
					"line":          call.Line,
					"full":          call.FullName,
					"source_uid":    entityUID(relPath, call.SourceName, ""),
					"source_type":   call.SourceType,
					"receiver_type": call.ReceiverType,
				})
			} else {

				fileCallsData = append(fileCallsData, map[string]any{
					"path":          relPath,
					"uid":           targetUID,
					"line":          call.Line,
					"full":          call.FullName,
					"source_uid":    "",
					"source_type":   "",
					"receiver_type": call.ReceiverType,
				})
			}
		}

		for _, ref := range pf.References {

			if ref.RelType == "INHERITS" || ref.RelType == "IMPLEMENTS" {
				if ref.SourceName != "" {
					inheritsData = append(inheritsData, map[string]any{
						"child": entityUID(relPath, ref.SourceName, ""), "parent": ref.TargetName,
						"child_name": ref.SourceName, "parent_name": ref.TargetName,
						"line": ref.Line, "path": relPath, "rel_type": ref.RelType,
					})
				}
				continue
			}

			if ref.RelType == "READS_FIELD" || ref.RelType == "WRITES_FIELD" {
				if ref.SourceName != "" {
					fieldAccessData = append(fieldAccessData, map[string]any{
						"source":   entityUID(relPath, ref.SourceName, ""),
						"field":    ref.TargetName,
						"line":     ref.Line,
						"path":     relPath,
						"rel_type": ref.RelType,
					})
				}
				continue
			}
			targetLabel := "Table"
			if ref.RelType == "SELECTS" || ref.RelType == "CREATES" {
				targetLabel = "Table"
			}
			if ref.SourceName != "" {

				parentRefsDataByType[ref.RelType] = append(parentRefsDataByType[ref.RelType], map[string]any{
					"path":       relPath,
					"uid":        ref.TargetName,
					"line":       ref.Line,
					"label":      targetLabel,
					"source_uid": entityUID(relPath, ref.SourceName, ""),
				})
			} else {

				refsDataByType[ref.RelType] = append(refsDataByType[ref.RelType], map[string]any{
					"path": relPath, "uid": ref.TargetName, "line": ref.Line, "label": targetLabel,
				})
			}
		}
	}

	allCallsData := append(parentCallsData, fileCallsData...)
	if len(allCallsData) > 0 {
		cmds = append(cmds, BatchQuery{
			Cypher: `UNWIND $data AS row MERGE (t:Function {uid: row.uid}) ON CREATE SET t.name=row.uid, t.is_stub=true`,
			Params: map[string]any{"data": allCallsData},
		})
	}

	if len(parentCallsData) > 0 {

		byType := make(map[string][]map[string]any)
		for _, cd := range parentCallsData {
			st := cd["source_type"].(string)
			if st == "" {
				st = "Function"
			}
			byType[st] = append(byType[st], cd)
		}
		for srcType, data := range byType {
			srcLabel := contextTypeToLabel(strings.ToLower(srcType))
			if srcLabel == "" {
				srcLabel = "Function"
			}
			cmds = append(cmds, BatchQuery{
				Cypher: fmt.Sprintf(`UNWIND $data AS row MATCH (s:%s {uid: row.source_uid}), (t:Function {uid: row.uid}) CREATE (s)-[r:CALLS {source_file: row.path}]->(t) SET r.line_number=row.line, r.full_call_name=row.full, r.receiver_type=row.receiver_type`, srcLabel),
				Params: map[string]any{"data": data},
			})
		}
	}

	if len(fileCallsData) > 0 {
		cmds = append(cmds, BatchQuery{
			Cypher: `UNWIND $data AS row MATCH (f:File {path: row.path}), (t:Function {uid: row.uid}) CREATE (f)-[r:CALLS {source_file: row.path}]->(t) SET r.line_number=row.line, r.full_call_name=row.full, r.receiver_type=row.receiver_type`,
			Params: map[string]any{"data": fileCallsData},
		})
	}

	if len(inheritsData) > 0 {
		var inheritsOnly, implementsOnly []map[string]any
		for _, d := range inheritsData {
			if d["rel_type"] == "IMPLEMENTS" {
				implementsOnly = append(implementsOnly, d)
			} else {
				inheritsOnly = append(inheritsOnly, d)
			}
		}

		if len(inheritsOnly) > 0 {
			cmds = append(cmds, BatchQuery{
				Cypher: `UNWIND $data AS row MERGE (p:Class {uid: row.parent}) ON CREATE SET p.name = row.parent, p.is_stub = true`,
				Params: map[string]any{"data": inheritsOnly},
			})
			cmds = append(cmds, BatchQuery{
				Cypher: `UNWIND $data AS row MATCH (c:Class {uid: row.child}), (p:Class {uid: row.parent}) CREATE (c)-[r:INHERITS]->(p) SET r.line_number = row.line`,
				Params: map[string]any{"data": inheritsOnly},
			})
		}

		if len(implementsOnly) > 0 {
			cmds = append(cmds, BatchQuery{
				Cypher: `UNWIND $data AS row MERGE (p:Interface {uid: row.parent}) ON CREATE SET p.name = row.parent, p.is_stub = true`,
				Params: map[string]any{"data": implementsOnly},
			})
			cmds = append(cmds, BatchQuery{
				Cypher: `UNWIND $data AS row MATCH (c:Class {uid: row.child}), (p:Interface {uid: row.parent}) CREATE (c)-[r:IMPLEMENTS]->(p) SET r.line_number = row.line`,
				Params: map[string]any{"data": implementsOnly},
			})
		}
	}

	if len(fieldAccessData) > 0 {

		readData := []map[string]any{}
		writeData := []map[string]any{}
		for _, fa := range fieldAccessData {
			if fa["rel_type"] == "WRITES_FIELD" {
				writeData = append(writeData, fa)
			} else {
				readData = append(readData, fa)
			}
		}
		if len(readData) > 0 {

			cmds = append(cmds, BatchQuery{
				Cypher: `UNWIND $data AS row MERGE (fld:Field {uid: row.field}) ON CREATE SET fld.name = row.field, fld.is_stub = true`,
				Params: map[string]any{"data": readData},
			})
			cmds = append(cmds, BatchQuery{
				Cypher: `UNWIND $data AS row MATCH (fn:Function {uid: row.source}), (fld:Field {uid: row.field}) CREATE (fn)-[r:READS_FIELD]->(fld) SET r.line_number = row.line, r.source_file = row.path`,
				Params: map[string]any{"data": readData},
			})
		}
		if len(writeData) > 0 {
			cmds = append(cmds, BatchQuery{
				Cypher: `UNWIND $data AS row MERGE (fld:Field {uid: row.field}) ON CREATE SET fld.name = row.field, fld.is_stub = true`,
				Params: map[string]any{"data": writeData},
			})
			cmds = append(cmds, BatchQuery{
				Cypher: `UNWIND $data AS row MATCH (fn:Function {uid: row.source}), (fld:Field {uid: row.field}) CREATE (fn)-[r:WRITES_FIELD]->(fld) SET r.line_number = row.line, r.source_file = row.path`,
				Params: map[string]any{"data": writeData},
			})
		}
	}

	for relType, data := range parentRefsDataByType {
		if len(data) == 0 {
			continue
		}

		cmds = append(cmds, BatchQuery{
			Cypher: `UNWIND $data AS row MERGE (t:Table {uid: row.uid}) ON CREATE SET t.name=row.uid, t.is_stub=true`,
			Params: map[string]any{"data": data},
		})

		cmds = append(cmds, BatchQuery{
			Cypher: fmt.Sprintf(`UNWIND $data AS row MATCH (s:Function {uid: row.source_uid}), (t:Table {uid: row.uid}) CREATE (s)-[r:%s {source_file: row.path}]->(t) SET r.line_number=row.line`, relType),
			Params: map[string]any{"data": data},
		})
		cmds = append(cmds, BatchQuery{
			Cypher: fmt.Sprintf(`UNWIND $data AS row MATCH (s:Procedure {uid: row.source_uid}), (t:Table {uid: row.uid}) CREATE (s)-[r:%s {source_file: row.path}]->(t) SET r.line_number=row.line`, relType),
			Params: map[string]any{"data": data},
		})
		cmds = append(cmds, BatchQuery{
			Cypher: fmt.Sprintf(`UNWIND $data AS row MATCH (s:Trigger {uid: row.source_uid}), (t:Table {uid: row.uid}) CREATE (s)-[r:%s {source_file: row.path}]->(t) SET r.line_number=row.line`, relType),
			Params: map[string]any{"data": data},
		})
	}

	for relType, data := range refsDataByType {
		if len(data) == 0 {
			continue
		}

		cmds = append(cmds, BatchQuery{
			Cypher: `UNWIND $data AS row MERGE (t:Table {uid: row.uid}) ON CREATE SET t.name=row.uid, t.is_stub=true`,
			Params: map[string]any{"data": data},
		})
		cmds = append(cmds, BatchQuery{
			Cypher: fmt.Sprintf(`UNWIND $data AS row MATCH (f:File {path: row.path}), (t:Table {uid: row.uid}) CREATE (f)-[r:%s {source_file: row.path}]->(t) SET r.line_number=row.line`, relType),
			Params: map[string]any{"data": data},
		})
	}

	var paramEdges []map[string]any
	for _, pf := range chunk {
		abs, _ := filepath.Abs(pf.Path)
		relPath := w.rel(abs)

		if params, ok := pf.Entities["parameters"]; ok {
			for _, p := range params {
				parentName := p.Context
				if parentName == "" {
					continue
				}
				funcUID := entityUID(relPath, parentName, "")
				paramEdges = append(paramEdges, map[string]any{
					"func_uid":   funcUID,
					"param_uid":  funcUID + "." + p.Name,
					"param_name": p.Name,
					"param_line": p.Line,
					"path":       relPath,
					"lang":       pf.Language,
				})
			}
		}
	}

	if len(paramEdges) > 0 {
		paramClusterSet := ""
		if w.cluster != "" {
			paramClusterSet = ", p.cluster = '" + w.cluster + "'"
		}
		cmds = append(cmds, BatchQuery{
			Cypher: `UNWIND $data AS row MERGE (p:Parameter {uid: row.param_uid}) SET p.name = row.param_name, p.path = row.path, p.line_number = row.param_line, p.lang = row.lang` + paramClusterSet,
			Params: map[string]any{"data": paramEdges},
		})
		cmds = append(cmds, BatchQuery{
			Cypher: `UNWIND $data AS row MATCH (f:Function {uid: row.func_uid}), (p:Parameter {uid: row.param_uid}) CREATE (f)-[:HAS_PARAMETER]->(p)`,
			Params: map[string]any{"data": paramEdges},
		})
	}

	var fieldEdges []map[string]any
	for _, pf := range chunk {
		abs, _ := filepath.Abs(pf.Path)
		relPath := w.rel(abs)

		if fields, ok := pf.Entities["fields"]; ok {
			for _, f := range fields {
				parentName := f.Context
				parentType := f.ContextType
				if parentName == "" {
					continue
				}
				parentUID := entityUID(relPath, parentName, "")
				fieldUID := parentUID + "." + f.Name
				fieldEdges = append(fieldEdges, map[string]any{
					"parent_uid":  parentUID,
					"parent_type": parentType,
					"field_uid":   fieldUID,
					"field_name":  f.Name,
					"field_line":  f.Line,
					"path":        relPath,
					"lang":        pf.Language,
				})
			}
		}
	}

	if len(fieldEdges) > 0 {
		fldClusterSet := ""
		if w.cluster != "" {
			fldClusterSet = ", fld.cluster = '" + w.cluster + "'"
		}
		cmds = append(cmds, BatchQuery{
			Cypher: `UNWIND $data AS row MERGE (fld:Field {uid: row.field_uid}) SET fld.name = row.field_name, fld.path = row.path, fld.line_number = row.field_line, fld.lang = row.lang, fld.context = row.parent_uid, fld.context_type = row.parent_type` + fldClusterSet,
			Params: map[string]any{"data": fieldEdges},
		})

		cmds = append(cmds, BatchQuery{
			Cypher: `UNWIND $data AS row MATCH (c:Class {uid: row.parent_uid}), (fld:Field {uid: row.field_uid}) CREATE (c)-[:HAS_FIELD]->(fld)`,
			Params: map[string]any{"data": fieldEdges},
		})
		cmds = append(cmds, BatchQuery{
			Cypher: `UNWIND $data AS row MATCH (s:Struct {uid: row.parent_uid}), (fld:Field {uid: row.field_uid}) CREATE (s)-[:HAS_FIELD]->(fld)`,
			Params: map[string]any{"data": fieldEdges},
		})
	}

	return w.db.ExecuteBatch(ctx, cmds)
}
