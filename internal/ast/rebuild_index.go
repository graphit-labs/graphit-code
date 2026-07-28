package ast

import (
	"path/filepath"
	"sort"
	"strings"
)

type rebuildIndex struct {
	entries map[string]*parseCacheEntry

	labels               []string
	labelSet             map[string]bool
	containsPairs        [][2]string
	callerLabels         []string
	dmlTypes             []string
	dmlTargetLabels      []string
	dmlSourceLabels      []string
	paramOwnerLabels     []string
	fieldOwnerLabels     []string
	inheritLabels        []string
	decoratorOwnerLabels []string
	annotationKinds      []string
	hasParams            bool
	hasFields            bool
	hasInherits          bool
	hasDecorators        bool
	hasImports           bool

	entityUIDs  map[string]string
	fieldUIDs   map[string]bool
	dirPathSet  map[string]bool
	fileEntries []fileEntry
	emittedUIDs map[string]map[string]bool
	objects     map[string]objectRef
}

type fileEntry struct {
	relPath string
	entry   *parseCacheEntry
}

// objectRef is a schema-level object a reference from another file can resolve to.
type objectRef struct {
	uid     string
	label   string
	relPath string
}

// before orders two candidates for the same name so the winner does not depend on
// map iteration order.
func (o objectRef) before(other objectRef) bool {
	if o.relPath != other.relPath {
		return o.relPath < other.relPath
	}
	return o.label < other.label
}

// schemaObjectLabels are the labels whose name identifies one object across the
// whole schema, so a reference written in another file — `SELECT ... FROM PEDIDO`
// in a package body — can be resolved to the object itself instead of inventing a
// second node for it.
//
// Locals are deliberately absent: ID_PESSOA names a column in hundreds of tables,
// and resolving a reference to "one of them" would fabricate a dependency.
var schemaObjectLabels = map[string]bool{
	LabelTable: true, LabelView: true, LabelMaterializedView: true,
	LabelSequence: true, LabelSynonym: true, LabelPackage: true,
	LabelProcedure: true, LabelFunction: true, LabelTrigger: true,
	LabelType: true, LabelIndex: true,
	LabelDatabaseLink: true, "DBLink": true,
}

func newRebuildIndex(entries map[string]*parseCacheEntry) *rebuildIndex {
	ri := &rebuildIndex{
		entries:     entries,
		labelSet:    make(map[string]bool),
		entityUIDs:  make(map[string]string),
		fieldUIDs:   make(map[string]bool),
		dirPathSet:  make(map[string]bool),
		emittedUIDs: make(map[string]map[string]bool),
	}
	ri.scan()
	return ri
}

func (ri *rebuildIndex) emitUID(uid, table string) bool {
	if ri.emittedUIDs[uid] == nil {
		ri.emittedUIDs[uid] = make(map[string]bool)
	}
	if ri.emittedUIDs[uid][table] {
		return false
	}
	ri.emittedUIDs[uid][table] = true
	return true
}

func (ri *rebuildIndex) emittedIn(uid, table string) bool {
	return ri.emittedUIDs[uid] != nil && ri.emittedUIDs[uid][table]
}

func (ri *rebuildIndex) emittedAny(uid string) bool {
	return len(ri.emittedUIDs[uid]) > 0
}

func (ri *rebuildIndex) scan() {
	labelSet := make(map[string]bool)
	containsSet := make(map[[2]string]bool)
	callerSet := make(map[string]bool)
	dmlTypeSet := make(map[string]bool)
	dmlSrcSet := make(map[string]bool)
	paramOwnerSet := make(map[string]bool)
	fieldOwnerSet := make(map[string]bool)
	inheritLabelSet := make(map[string]bool)
	decOwnerSet := make(map[string]bool)
	annKindSet := make(map[string]bool)
	objects := make(map[string]objectRef)

	for relPath, entry := range ri.entries {
		ri.fileEntries = append(ri.fileEntries, fileEntry{relPath, entry})

		for _, dp := range entry.DirPaths {
			ri.dirPathSet[dp] = true
		}

		for _, ent := range entry.Entities {
			labelSet[ent.Label] = true
			ri.entityUIDs[ent.UID] = ent.Label
			if schemaObjectLabels[ent.Label] && ent.Name != "" {
				cand := objectRef{uid: ent.UID, label: ent.Label, relPath: relPath}
				if cur, seen := objects[ent.Name]; !seen || cand.before(cur) {
					objects[ent.Name] = cand
				}
			}
			if len(ent.Decorators) > 0 {
				ri.hasDecorators = true
				decOwnerSet[ent.Label] = true
				for _, dec := range ent.Decorators {
					if dec != "" {
						annKindSet[annotationKind(ent.Lang)] = true
					}
				}
			}
		}

		// Who owns a parameter decides which HAS_PARAMETER pairs the schema needs.
		// Deriving them from the CALL sources instead — as this used to — held only
		// by luck: a package whose procedure takes a parameter and calls nothing
		// produced no caller labels, and the group fell back to a hardcoded
		// `FROM Function`, a node table that corpus never creates.
		for _, p := range entry.Parameters {
			ri.hasParams = true
			if owner := ri.entityUIDs[p.FuncUID]; owner != "" {
				paramOwnerSet[owner] = true
			}
		}

		for _, ce := range entry.ContainsEdges {
			if ce.ChildLabel == LabelParameter && ce.ParentLabel != "" {
				ri.hasParams = true
				paramOwnerSet[ce.ParentLabel] = true
			}
		}

		for _, ent := range entry.Entities {
			if ent.Label == "Parameter" {
				ri.hasParams = true
				break
			}
		}

		for _, f := range entry.Fields {
			ri.fieldUIDs[f.UID] = true
			if f.ParentType != "" {
				fieldOwnerSet[f.ParentType] = true
			}

			if actual := ri.entityUIDs[f.ParentUID]; actual != "" {
				fieldOwnerSet[actual] = true
			}
			ri.hasFields = true
		}

		if len(entry.Imports) > 0 {
			ri.hasImports = true
		}

		for _, call := range entry.Calls {
			if call.CallerUID != "" {
				validTypes := map[string]bool{"Function": true, "Method": true, "Procedure": true, "Trigger": true, "Package": true}
				if validTypes[call.SourceType] {
					callerSet[call.SourceType] = true
				}
			}
		}

		for _, inh := range entry.Inheritance {
			ri.hasInherits = true
			inheritLabelSet[ri.entityUIDs[inh.ChildUID]] = true
			inheritLabelSet[ri.entityUIDs[inh.ParentUID]] = true
		}

		for range entry.FieldAccess {
			ri.hasFields = true
		}

		for _, ref := range entry.References {
			if ref.SourceUID != "" {
				dmlTypeSet[ref.RelType] = true
				if lbl := ri.entityUIDs[ref.SourceUID]; lbl != "" {
					dmlSrcSet[lbl] = true
				}
			}
		}

		for _, ce := range entry.ContainsEdges {
			containsSet[[2]string{ce.ParentLabel, ce.ChildLabel}] = true
		}
	}

	if len(callerSet) > 0 {
		labelSet["Function"] = true
	}
	if ri.hasImports {
		labelSet["Module"] = true
	}

	// Every object is known now, so a reference can be resolved to the node that
	// defines it. What a reference resolves to also decides which rel table groups
	// the schema needs: hardcoding Table as the only DML target both dropped edges
	// to views and sequences and made the DML graph vanish entirely on a corpus
	// with no CREATE TABLE files.
	ri.objects = objects
	dmlTargetSet := make(map[string]bool)
	for _, fe := range ri.fileEntries {
		for _, ref := range fe.entry.References {
			if ref.SourceUID == "" {
				continue
			}
			_, label := ri.resolveRefTarget(ref.TargetUID)
			dmlTargetSet[label] = true
		}
	}
	// Unresolved targets become stub Table nodes, so that node table has to exist
	// even when the corpus never declares a table of its own.
	if dmlTargetSet[LabelTable] {
		labelSet[LabelTable] = true
	}

	for l := range labelSet {
		ri.labels = append(ri.labels, l)
	}
	ri.labelSet = labelSet
	for l := range dmlTargetSet {
		if labelSet[l] {
			ri.dmlTargetLabels = append(ri.dmlTargetLabels, l)
		}
	}
	sort.Strings(ri.dmlTargetLabels)
	// A CONTAINS pair is only writable when both ends have a node table. The
	// parent side comes from an entity's declared context type, which is not
	// guaranteed to name a label any entity carries: 75121 Table->Column edges
	// against zero Table entities made LadybugDB reject the whole rel table group
	// ("Table Table does not exist") and abort the rebuild, losing the entire
	// graph. The writer already drops these edges — see containsEntityJSON — so
	// filtering here costs nothing and keeps the DDL buildable.
	for p := range containsSet {
		if labelSet[p[0]] && labelSet[p[1]] {
			ri.containsPairs = append(ri.containsPairs, p)
		}
	}
	for c := range callerSet {
		ri.callerLabels = append(ri.callerLabels, c)
	}
	for d := range dmlTypeSet {
		ri.dmlTypes = append(ri.dmlTypes, d)
	}
	for d := range dmlSrcSet {
		ri.dmlSourceLabels = append(ri.dmlSourceLabels, d)
	}
	for p := range paramOwnerSet {
		if labelSet[p] {
			ri.paramOwnerLabels = append(ri.paramOwnerLabels, p)
		}
	}
	sort.Strings(ri.paramOwnerLabels)
	for f := range fieldOwnerSet {
		ri.fieldOwnerLabels = append(ri.fieldOwnerLabels, f)
	}
	for l := range inheritLabelSet {
		if l != "" {
			ri.inheritLabels = append(ri.inheritLabels, l)
		}
	}
	for d := range decOwnerSet {
		ri.decoratorOwnerLabels = append(ri.decoratorOwnerLabels, d)
	}
	for k := range annKindSet {
		ri.annotationKinds = append(ri.annotationKinds, k)
	}
}

func (ri *rebuildIndex) schemaInfo() SchemaInfo {
	return SchemaInfo{
		Labels:               ri.labels,
		ContainsPairs:        ri.containsPairs,
		CallerLabels:         ri.callerLabels,
		DMLTypes:             ri.dmlTypes,
		DMLTargetLabels:      ri.dmlTargetLabels,
		DMLSourceLabels:      ri.dmlSourceLabels,
		ParamOwnerLabels:     ri.paramOwnerLabels,
		FieldOwnerLabels:     ri.fieldOwnerLabels,
		InheritLabels:        ri.inheritLabels,
		DecoratorOwnerLabels: ri.decoratorOwnerLabels,
		AnnotationKinds:      ri.annotationKinds,
		HasFields:            ri.hasFields,
		HasParams:            ri.hasParams,
		HasInherits:          ri.hasInherits,
		HasDecorators:        ri.hasDecorators,
	}
}

func (ri *rebuildIndex) fileNodeJSON(cluster string) []map[string]any {
	rows := make([]map[string]any, 0, len(ri.fileEntries))
	for _, fe := range ri.fileEntries {
		ri.emitUID(fe.relPath, "File")
		rows = append(rows, map[string]any{
			"path": fe.relPath, "name": filepath.Base(fe.relPath),
			"relative_path": fe.relPath, "is_dependency": fe.entry.IsDepend,
			"lang": fe.entry.Language, "cluster": cluster, "source": fe.entry.Source,
		})
	}
	return rows
}

func (ri *rebuildIndex) dirNodeJSON(cluster string) []map[string]any {
	rows := make([]map[string]any, 0, len(ri.dirPathSet))
	for dp := range ri.dirPathSet {
		ri.emitUID(dp, "Directory")
		rows = append(rows, map[string]any{
			"path": dp, "name": filepath.Base(dp), "cluster": cluster,
		})
	}
	return rows
}

func entityToJSON(ent cachedEntity, isStub bool) map[string]any {
	return map[string]any{
		"uid": ent.UID, "name": ent.Name, "path": ent.Path,
		"line_number": ent.Line, "end_line": ent.EndLine,
		"docstring": ent.Docstring, "lang": ent.Lang,
		"cyclomatic_complexity": ent.Complexity, "context": ent.Context,
		"context_type": ent.ContextType, "is_dependency": ent.IsDep,
		"is_exported": ent.IsExported, "value": ent.Value,
		"is_stub": isStub, "entry_point_score": 0,
	}
}

func stubJSON(uid string) map[string]any {
	return map[string]any{
		"uid": uid, "name": uid, "path": "",
		"line_number": 0, "end_line": 0, "docstring": "", "lang": "",
		"cyclomatic_complexity": 0, "context": "", "context_type": "",
		"is_dependency": false, "is_exported": false, "value": "",
		"is_stub": true, "entry_point_score": 0,
	}
}

func (ri *rebuildIndex) entityJSON(label string) []map[string]any {
	var rows []map[string]any
	for _, fe := range ri.fileEntries {
		for _, ent := range fe.entry.Entities {
			if ent.Label == label && ri.emitUID(ent.UID, label) {
				rows = append(rows, entityToJSON(ent, false))
			}
		}
	}
	return rows
}

func (ri *rebuildIndex) moduleJSON() []map[string]any {
	seen := make(map[string]bool)
	var rows []map[string]any
	for _, fe := range ri.fileEntries {
		for _, imp := range fe.entry.Imports {
			if !seen[imp.ModuleUID] {
				seen[imp.ModuleUID] = true
				ri.emitUID(imp.ModuleUID, "Module")
				rows = append(rows, map[string]any{
					"uid": imp.ModuleUID, "name": imp.ModuleName,
					"lang": imp.Lang, "full_import_name": imp.RawImport, "is_stub": false,
				})
			}
		}
	}
	return rows
}

func (ri *rebuildIndex) stubFunctionJSON() []map[string]any {
	var rows []map[string]any
	for _, fe := range ri.fileEntries {
		for _, call := range fe.entry.Calls {
			if call.CalleeUID != "" && !ri.emittedAny(call.CalleeUID) {
				ri.emitUID(call.CalleeUID, "Function")
				rows = append(rows, stubJSON(call.CalleeUID))
			}
		}
	}
	return rows
}

func (ri *rebuildIndex) stubClassJSON() []map[string]any {
	var rows []map[string]any
	for _, fe := range ri.fileEntries {
		for _, inh := range fe.entry.Inheritance {
			if inh.RelType == "INHERITS" && !ri.emittedAny(inh.ParentUID) {
				ri.emitUID(inh.ParentUID, "Class")
				rows = append(rows, stubJSON(inh.ParentUID))
			}
		}
	}
	return rows
}

func (ri *rebuildIndex) stubInterfaceJSON() []map[string]any {
	var rows []map[string]any
	for _, fe := range ri.fileEntries {
		for _, inh := range fe.entry.Inheritance {
			if inh.RelType == "IMPLEMENTS" && !ri.emittedAny(inh.ParentUID) {
				ri.emitUID(inh.ParentUID, "Interface")
				rows = append(rows, stubJSON(inh.ParentUID))
			}
		}
	}
	return rows
}

func (ri *rebuildIndex) stubFieldJSON() []map[string]any {
	var rows []map[string]any
	for _, fe := range ri.fileEntries {
		for _, fa := range fe.entry.FieldAccess {
			if !ri.emittedAny(fa.FieldUID) {
				ri.emitUID(fa.FieldUID, "Field")
				rows = append(rows, stubJSON(fa.FieldUID))
			}
		}
	}
	return rows
}

// resolveRefTarget maps a reference target to the node it means.
//
// A reference is cached as the bare object name — see ConvertToCache — while an
// entity's uid is scoped by the file that declares it, so the two never matched and
// EVERY DML target became a stub, duplicating each table: one node with its columns
// and lines, another with the inbound SELECTS. Resolving the name against the
// schema-level objects joins the two halves.
//
// An unresolved name keeps itself as the uid of a stub Table, which is what
// stubTableJSON creates for it — a table referenced by DML whose DDL is not in the
// corpus is still a dependency worth recording.
func (ri *rebuildIndex) resolveRefTarget(name string) (uid, label string) {
	if o, ok := ri.objects[name]; ok {
		return o.uid, o.label
	}
	return name, LabelTable
}

func (ri *rebuildIndex) stubTableJSON() []map[string]any {
	var rows []map[string]any
	for _, fe := range ri.fileEntries {
		for _, ref := range fe.entry.References {
			if ref.SourceUID == "" {
				continue
			}
			uid, label := ri.resolveRefTarget(ref.TargetUID)
			if label != LabelTable || ri.emittedAny(uid) {
				continue
			}
			ri.emitUID(uid, LabelTable)
			rows = append(rows, stubJSON(uid))
		}
	}
	return rows
}

func (ri *rebuildIndex) annotationNodeJSON(kind string) []map[string]any {
	seen := make(map[string]bool)
	var rows []map[string]any
	for _, fe := range ri.fileEntries {
		for _, ent := range fe.entry.Entities {
			if annotationKind(ent.Lang) != kind {
				continue
			}
			for _, dec := range ent.Decorators {
				if dec == "" {
					continue
				}
				uid := dec + ":" + ent.Lang
				if !seen[uid] {
					seen[uid] = true
					ri.emitUID(uid, kind)
					rows = append(rows, map[string]any{
						"uid": uid, "name": dec, "lang": ent.Lang, "is_stub": false,
					})
				}
			}
		}
	}
	return rows
}

func (ri *rebuildIndex) paramEdgeJSON(callerLabel string) []map[string]any {
	var rows []map[string]any
	seen := make(map[string]bool)
	for _, fe := range ri.fileEntries {

		for _, ce := range fe.entry.ContainsEdges {
			if ce.ChildLabel != "Parameter" {
				continue
			}
			key := ce.ParentUID + "→" + ce.ChildUID
			if seen[key] {
				continue
			}
			if ri.emittedIn(ce.ParentUID, callerLabel) && ri.emittedIn(ce.ChildUID, "Parameter") {
				seen[key] = true
				rows = append(rows, map[string]any{
					"func_uid": ce.ParentUID, "uid": ce.ChildUID,
					"source_file": fe.relPath, "line_number": 0,
				})
			}
		}

		for _, p := range fe.entry.Parameters {
			key := p.FuncUID + "→" + p.UID
			if !seen[key] && ri.emittedIn(p.FuncUID, callerLabel) && ri.emittedIn(p.UID, "Parameter") {
				seen[key] = true
				rows = append(rows, map[string]any{
					"func_uid": p.FuncUID, "uid": p.UID,
					"source_file": fe.relPath, "line_number": p.Line,
				})
			}
		}
	}
	return rows
}

func (ri *rebuildIndex) fieldEdgeJSON(parentType string) []map[string]any {
	var rows []map[string]any
	for _, fe := range ri.fileEntries {
		for _, f := range fe.entry.Fields {

			if ri.emittedIn(f.ParentUID, parentType) && ri.emittedIn(f.UID, "Field") {
				rows = append(rows, map[string]any{
					"parent_uid": f.ParentUID, "uid": f.UID,
					"source_file": fe.relPath, "line_number": f.Line,
				})
			}
		}
	}
	return rows
}

func (ri *rebuildIndex) importEdgeJSON() []map[string]any {
	var rows []map[string]any
	for _, fe := range ri.fileEntries {
		for _, imp := range fe.entry.Imports {
			if ri.emittedAny(imp.FileUID) && ri.emittedAny(imp.ModuleUID) {
				rows = append(rows, map[string]any{
					"file_uid": imp.FileUID, "module_uid": imp.ModuleUID,
					"alias": imp.Alias, "full_import_name": imp.RawImport,
					"imported_name": imp.ImportedName, "line_number": imp.Line,
					"source_file": imp.SourceFile,
				})
			}
		}
	}
	return rows
}

func (ri *rebuildIndex) callEdgeJSON(callerLabel string) []map[string]any {
	var rows []map[string]any
	for _, fe := range ri.fileEntries {
		for _, call := range fe.entry.Calls {
			if call.CallerUID != "" && call.SourceType == callerLabel &&
				ri.emittedIn(call.CallerUID, callerLabel) && ri.emittedIn(call.CalleeUID, "Function") {
				rows = append(rows, map[string]any{
					"caller_uid": call.CallerUID, "callee_uid": call.CalleeUID,
					"source_file": call.Path, "line_number": call.Line,
					"full_call_name": "", "receiver_type": call.ReceiverType,
				})
			}
		}
	}
	return rows
}

func (ri *rebuildIndex) inheritEdgeJSON(relType, fromLabel, toLabel string) []map[string]any {
	var rows []map[string]any
	for _, fe := range ri.fileEntries {
		for _, inh := range fe.entry.Inheritance {
			if inh.RelType != relType {
				continue
			}
			if ri.entityUIDs[inh.ChildUID] == fromLabel && ri.entityUIDs[inh.ParentUID] == toLabel &&
				ri.emittedAny(inh.ChildUID) && ri.emittedAny(inh.ParentUID) {
				rows = append(rows, map[string]any{
					"child_uid": inh.ChildUID, "parent_uid": inh.ParentUID,
					"source_file": inh.Path, "line_number": inh.Line,
				})
			}
		}
	}
	return rows
}

func (ri *rebuildIndex) fieldAccessEdgeJSON(write bool) []map[string]any {
	var rows []map[string]any
	for _, fe := range ri.fileEntries {
		for _, fa := range fe.entry.FieldAccess {
			if fa.IsWrite == write && ri.emittedAny(fa.SourceUID) && ri.emittedAny(fa.FieldUID) {
				rows = append(rows, map[string]any{
					"source_uid": fa.SourceUID, "field_uid": fa.FieldUID,
					"source_file": fa.Path, "line_number": fa.Line,
				})
			}
		}
	}
	return rows
}

func (ri *rebuildIndex) dmlEdgeJSON(relType, srcLabel, tgtLabel string) []map[string]any {
	var rows []map[string]any
	for _, fe := range ri.fileEntries {
		for _, ref := range fe.entry.References {
			if ref.RelType != relType || ref.SourceUID == "" ||
				ri.entityUIDs[ref.SourceUID] != srcLabel || !ri.emittedAny(ref.SourceUID) {
				continue
			}
			targetUID, targetLabel := ri.resolveRefTarget(ref.TargetUID)
			// emittedIn, not emittedAny: the COPY declares which node table the
			// target lives in, so a target resolved to a View must not be written
			// into the group that ends at Table.
			if targetLabel != tgtLabel || !ri.emittedIn(targetUID, tgtLabel) {
				continue
			}
			rows = append(rows, map[string]any{
				"source_uid": ref.SourceUID, "target_uid": targetUID,
				"source_file": ref.Path, "line_number": ref.Line,
			})
		}
	}
	return rows
}

func (ri *rebuildIndex) annotationEdgeJSON(kind, ownerLabel string) []map[string]any {
	var rows []map[string]any
	for _, fe := range ri.fileEntries {
		for _, ent := range fe.entry.Entities {
			if ent.Label != ownerLabel || annotationKind(ent.Lang) != kind {
				continue
			}
			for _, dec := range ent.Decorators {
				annUID := dec + ":" + ent.Lang
				if dec != "" && ri.emittedAny(ent.UID) && ri.emittedAny(annUID) {
					rows = append(rows, map[string]any{
						"entity_uid": ent.UID, "annotation_uid": annUID,
						"source_file": ent.Path, "line_number": ent.Line,
					})
				}
			}
		}
	}
	return rows
}

func (ri *rebuildIndex) containsFileEntityJSON(label string) []map[string]any {
	var rows []map[string]any
	for _, fe := range ri.fileEntries {
		for _, ent := range fe.entry.Entities {
			if ent.Label == label && ri.emittedIn(ent.Path, "File") && ri.emittedIn(ent.UID, label) {
				rows = append(rows, map[string]any{"path": ent.Path, "uid": ent.UID})
			}
		}
	}
	return rows
}

func (ri *rebuildIndex) containsDirDirJSON() []map[string]any {
	var rows []map[string]any
	for dp := range ri.dirPathSet {
		parent := filepath.Dir(dp)
		if strings.Contains(dp, "/") && ri.emittedAny(parent) && ri.emittedAny(dp) {
			rows = append(rows, map[string]any{
				"parent_dir": parent, "child_dir": dp,
			})
		}
	}
	return rows
}

func (ri *rebuildIndex) containsDirFileJSON() []map[string]any {
	var rows []map[string]any
	for _, fe := range ri.fileEntries {
		parent := filepath.Dir(fe.relPath)
		if strings.Contains(fe.relPath, "/") && ri.emittedAny(parent) && ri.emittedAny(fe.relPath) {
			rows = append(rows, map[string]any{
				"parent_dir": parent, "file_path": fe.relPath,
			})
		}
	}
	return rows
}

func (ri *rebuildIndex) containsEntityJSON(parentLabel, childLabel string) []map[string]any {
	var rows []map[string]any
	for _, fe := range ri.fileEntries {
		for _, ce := range fe.entry.ContainsEdges {
			if ce.ParentLabel == parentLabel && ce.ChildLabel == childLabel &&
				ri.emittedIn(ce.ParentUID, parentLabel) && ri.emittedIn(ce.ChildUID, childLabel) {
				rows = append(rows, map[string]any{
					"parent_uid": ce.ParentUID, "child_uid": ce.ChildUID,
				})
			}
		}
	}
	return rows
}

func annotationKind(lang string) string {
	switch strings.ToLower(lang) {
	case "python", "typescript", "javascript":
		return "Decorator"
	case "csharp", "php", "rust", "swift":
		return "Attribute"
	default:
		return "Annotation"
	}
}
