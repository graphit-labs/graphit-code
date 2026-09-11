package ast

import (
	"path/filepath"
	"sort"
	"strings"
)

type rebuildIndex struct {
	entries map[string]*parseCacheEntry

	labels                  []string
	labelSet                map[string]bool
	containsPairs           [][2]string
	callerLabels            []string
	calleeLabels            []string
	dmlTypes                []string
	dmlTargetLabels         []string
	dmlSourceLabels         []string
	filePaths               map[string]bool
	hasCallTargets          bool
	paramOwnerLabels        []string
	fieldOwnerLabels        []string
	fieldAccessSourceLabels []string
	inheritLabels           []string
	decoratorOwnerLabels    []string
	annotationKinds         []string
	hasParams               bool
	hasFields               bool
	hasInherits             bool
	hasDecorators           bool
	hasImports              bool

	entityUIDs   map[string]string
	fieldUIDs    map[string]bool
	dirPathSet   map[string]bool
	fileEntries  []fileEntry
	emittedTable map[string]string
	emittedExtra map[uidTable]bool
	decls        map[string][]declRef
	rules        *TargetRules
}

type declRef struct {
	uid   string
	label string
	lang  string
}

type fileEntry struct {
	relPath string
	entry   *parseCacheEntry
}

func newRebuildIndex(entries map[string]*parseCacheEntry, rules *TargetRules) *rebuildIndex {
	ri := &rebuildIndex{
		entries:      entries,
		rules:        rules,
		labelSet:     make(map[string]bool),
		entityUIDs:   make(map[string]string),
		fieldUIDs:    make(map[string]bool),
		dirPathSet:   make(map[string]bool),
		emittedTable: make(map[string]string),
	}
	ri.scan()
	return ri
}

type uidTable struct{ uid, table string }

func (ri *rebuildIndex) emitUID(uid, table string) bool {
	first, seen := ri.emittedTable[uid]
	if !seen {
		ri.emittedTable[uid] = table
		return true
	}
	if first == table {
		return false
	}
	key := uidTable{uid: uid, table: table}
	if ri.emittedExtra[key] {
		return false
	}
	if ri.emittedExtra == nil {
		ri.emittedExtra = make(map[uidTable]bool)
	}
	ri.emittedExtra[key] = true
	return true
}

func (ri *rebuildIndex) emittedIn(uid, table string) bool {
	if table == "" {
		return false
	}
	if first, seen := ri.emittedTable[uid]; seen && first == table {
		return true
	}
	return ri.emittedExtra[uidTable{uid: uid, table: table}]
}

func (ri *rebuildIndex) emittedAny(uid string) bool {
	_, seen := ri.emittedTable[uid]
	return seen
}

func (ri *rebuildIndex) detachEmitState() (restore func()) {
	table, extra := ri.emittedTable, ri.emittedExtra
	ri.emittedTable, ri.emittedExtra = make(map[string]string), nil
	return func() { ri.emittedTable, ri.emittedExtra = table, extra }
}

func (ri *rebuildIndex) refSourceLabel(uid string) string {
	if lbl := ri.entityUIDs[uid]; lbl != "" {
		return lbl
	}
	if ri.filePaths[uid] {
		return LabelFile
	}
	return ""
}

func (ri *rebuildIndex) scan() {
	ri.filePaths = make(map[string]bool, len(ri.entries))
	for relPath := range ri.entries {
		ri.filePaths[relPath] = true
	}
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
	decls := make(map[string][]declRef)

	for relPath, entry := range ri.entries {
		ri.fileEntries = append(ri.fileEntries, fileEntry{relPath, entry})

		for _, dp := range entry.DirPaths {
			ri.dirPathSet[dp] = true
		}

		for _, ent := range entry.Entities {
			labelSet[ent.Label] = true
			ri.entityUIDs[ent.UID] = ent.Label
			if ent.Name != "" {
				declLang := ent.Lang
				if declLang == "" {
					declLang = entry.Language
				}
				ref := declRef{uid: ent.UID, label: ent.Label, lang: declLang}
				decls[ent.Name] = append(decls[ent.Name], ref)

				if ent.Context != "" {
					qualified := ent.Context + "." + ent.Name
					decls[qualified] = append(decls[qualified], ref)
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
			if call.CalleeUID != "" {
				ri.hasCallTargets = true
			}
			if call.CallerUID != "" && call.SourceType != "" {
				callerSet[call.SourceType] = true
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

	if len(callerSet) > 0 || ri.hasCallTargets {
		labelSet["Function"] = true
	}
	if ri.hasImports {
		labelSet["Module"] = true
	}

	ri.decls = decls

	calleeSet := map[string]bool{LabelFunction: true}
	for _, fe := range ri.fileEntries {
		for _, call := range fe.entry.Calls {
			if call.CalleeUID == "" {
				continue
			}
			if _, label := ri.resolveCallee(call.CalleeUID, langOr(call.Lang, fe.entry.Language)); labelSet[label] {
				calleeSet[label] = true
			}
		}
	}
	for l := range calleeSet {
		ri.calleeLabels = append(ri.calleeLabels, l)
	}
	sort.Strings(ri.calleeLabels)

	accessSrcSet := make(map[string]bool)
	for _, fe := range ri.fileEntries {
		for _, fa := range fe.entry.FieldAccess {
			if lbl := ri.entityUIDs[fa.SourceUID]; lbl != "" && labelSet[lbl] {
				accessSrcSet[lbl] = true
			}
		}
	}
	for l := range accessSrcSet {
		ri.fieldAccessSourceLabels = append(ri.fieldAccessSourceLabels, l)
	}
	sort.Strings(ri.fieldAccessSourceLabels)

	dmlTargetSet := make(map[string]bool)
	for _, fe := range ri.fileEntries {
		for _, ref := range fe.entry.References {
			if ref.SourceUID == "" {
				continue
			}
			_, label := ri.resolveRefTarget(ref, fe.entry.Language)
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
		// File is deliberately absent from labelSet — see refSourceLabel — but it is
		// a legitimate target whenever a REFERENCES with no resolved declaration
		// points at the file. Without this exception the schema never declares
		// `TO File` and every unresolved comment edge is silently dropped by the
		// writer.
		if labelSet[l] || l == LabelFile {
			ri.dmlTargetLabels = append(ri.dmlTargetLabels, l)
		}
	}
	sort.Strings(ri.dmlTargetLabels)
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

// collectRows materializes a streamed row producer. The streaming form is the real one:
// the export never holds a label's rows as a slice, and these collectors exist so callers
// that genuinely want the whole table — tests, and the delta probe — share one
// implementation with it rather than a second copy of the same loop.
func collectRows(stream func(emit func(map[string]any))) []map[string]any {
	var rows []map[string]any
	stream(func(r map[string]any) { rows = append(rows, r) })
	return rows
}

func (ri *rebuildIndex) fileNodeJSON() []map[string]any {
	return collectRows(ri.streamFileNodes)
}

func (ri *rebuildIndex) streamFileNodes(emit func(map[string]any)) {
	for _, fe := range ri.fileEntries {
		ri.emitUID(fe.relPath, "File")
		emit(map[string]any{
			"path": fe.relPath, "name": filepath.Base(fe.relPath),
			"relative_path": fe.relPath, "is_dependency": fe.entry.IsDepend,
			"lang": fe.entry.Language, "cluster": fe.entry.Cluster,
		})
	}
}

func (ri *rebuildIndex) streamDirNodes(clusterPathMap map[string]string, defaultCluster string, emit func(map[string]any)) {
	for dp := range ri.dirPathSet {
		ri.emitUID(dp, "Directory")
		cluster := resolveClusterForPath(dp, "", clusterPathMap, defaultCluster)
		emit(map[string]any{
			"path": dp, "name": filepath.Base(dp), "cluster": cluster,
		})
	}
}

func entityToJSON(ent cachedEntity, isStub bool, cluster string) map[string]any {
	m := map[string]any{
		"uid": ent.UID, "name": ent.Name, "path": ent.Path,
		"line_number": ent.Line, "end_line": ent.EndLine,
		"docstring": ent.Docstring, "lang": ent.Lang,
		"cyclomatic_complexity": ent.Complexity, "context": ent.Context,
		"context_type": ent.ContextType, "is_dependency": ent.IsDep,
		"is_exported": ent.IsExported, "value": ent.Value,
		"is_stub": isStub,
	}
	if cluster != "" {
		m["cluster"] = cluster
	}
	return m
}

func stubJSON(uid, lang, cluster string) map[string]any {
	m := map[string]any{
		"uid": uid, "name": uid, "path": "",
		"line_number": 0, "end_line": 0, "docstring": "", "lang": lang,
		"cyclomatic_complexity": 0, "context": "", "context_type": "",
		"is_dependency": false, "is_exported": false, "value": "",
		"is_stub": true,
	}
	if cluster != "" {
		m["cluster"] = cluster
	}
	return m
}

func (ri *rebuildIndex) entityJSON(label string) []map[string]any {
	return collectRows(func(emit func(map[string]any)) { ri.streamEntities(label, emit) })
}

func (ri *rebuildIndex) streamEntities(label string, emit func(map[string]any)) {
	for _, fe := range ri.fileEntries {
		for _, ent := range fe.entry.Entities {
			if ent.Label == label && ri.emitUID(ent.UID, label) {
				emit(entityToJSON(ent, false, fe.entry.Cluster))
			}
		}
	}
}

func (ri *rebuildIndex) streamModules(emit func(map[string]any)) {
	seen := make(map[string]bool)
	for _, fe := range ri.fileEntries {
		for _, imp := range fe.entry.Imports {
			if !seen[imp.ModuleUID] {
				seen[imp.ModuleUID] = true
				ri.emitUID(imp.ModuleUID, "Module")
				m := map[string]any{
					"uid": imp.ModuleUID, "name": imp.ModuleName,
					"lang": imp.Lang, "full_import_name": imp.RawImport, "is_stub": false,
				}
				if fe.entry.Cluster != "" {
					m["cluster"] = fe.entry.Cluster
				}
				emit(m)
			}
		}
	}
}

func (ri *rebuildIndex) resolveNamed(name, lang string, rule TargetRule) (declRef, bool) {
	var hit declRef
	found := 0
	for _, d := range ri.decls[name] {
		if d.lang != lang || !rule.allows(d.label) {
			continue
		}
		found++
		hit = d
	}
	return hit, found == 1
}

// resolveCallee says which node a call points at.
//
// The target is cached as a bare name — see ConvertToCache — while a declaration's uid
// is scoped by the file declaring it, so the two never matched and EVERY call got a
// second node: the declaration with its body and lines, and the stub with the inbound
// edges. No query could join the two halves, which is why `NOT ()-[:CALLS]->(f)` was
// true for every declaration in the graph.
//
// Resolving the name against the declarations merges the two halves. A stub remains
// only when there is nothing to merge with:
//
//   - nothing declares the name: `fmt.Println`, a library class. The stub records that
//     something outside this corpus is called here, and is worth keeping.
//   - the name has homonyms: pointing at "one of the repository's Handle" would invent
//     an edge nobody wrote — see declRef.
//   - the declaration is in another language: a canvas `fill()` in .tsx does not call
//     the Go function of the same name that happens to be the only one in the
//     repository. Without this guard the merge fabricated a tsx-to-Go edge, which is
//     worse than the stub it replaced.
func (ri *rebuildIndex) resolveCallee(calleeUID, callerLang string) (uid, label string) {
	rule := ri.rules.ForRelation(callerLang, RelCalls)
	if d, ok := ri.resolveNamed(calleeUID, callerLang, rule); ok {
		return d.uid, d.label
	}
	return calleeUID, LabelFunction
}

func (ri *rebuildIndex) stubFunctionJSON() []map[string]any {
	return collectRows(ri.streamStubFunctions)
}

func (ri *rebuildIndex) streamStubFunctions(emit func(map[string]any)) {
	for _, fe := range ri.fileEntries {
		for _, call := range fe.entry.Calls {
			if call.CalleeUID == "" {
				continue
			}
			uid, label := ri.resolveCallee(call.CalleeUID, langOr(call.Lang, fe.entry.Language))
			if label != LabelFunction || ri.emittedAny(uid) {
				continue
			}
			ri.emitUID(uid, LabelFunction)
			emit(stubJSON(uid, fe.entry.Language, fe.entry.Cluster))
		}
	}
}

func (ri *rebuildIndex) streamStubClasses(emit func(map[string]any)) {
	for _, fe := range ri.fileEntries {
		for _, inh := range fe.entry.Inheritance {
			if inh.RelType == "INHERITS" && !ri.emittedAny(inh.ParentUID) {
				ri.emitUID(inh.ParentUID, "Class")
				emit(stubJSON(inh.ParentUID, fe.entry.Language, fe.entry.Cluster))
			}
		}
	}
}

func (ri *rebuildIndex) streamStubInterfaces(emit func(map[string]any)) {
	for _, fe := range ri.fileEntries {
		for _, inh := range fe.entry.Inheritance {
			if inh.RelType == "IMPLEMENTS" && !ri.emittedAny(inh.ParentUID) {
				ri.emitUID(inh.ParentUID, "Interface")
				emit(stubJSON(inh.ParentUID, fe.entry.Language, fe.entry.Cluster))
			}
		}
	}
}

func (ri *rebuildIndex) resolveFieldTarget(name, lang string) string {
	// The access edge ends in the Field table by the writer's construction, so that
	// label is an engine invariant rather than language policy: the grammar's rule says
	// which names to look for, and this guarantees what was found is writable.
	rule := ri.rules.ForRelation(lang, RelReadsField)
	if d, ok := ri.resolveNamed(name, lang, rule); ok && d.label == LabelField {
		return d.uid
	}
	return name
}

func (ri *rebuildIndex) streamStubFields(emit func(map[string]any)) {
	for _, fe := range ri.fileEntries {
		for _, fa := range fe.entry.FieldAccess {
			uid := ri.resolveFieldTarget(fa.FieldUID, fe.entry.Language)
			if ri.emittedAny(uid) {
				continue
			}
			ri.emitUID(uid, LabelField)
			emit(stubJSON(uid, fe.entry.Language, fe.entry.Cluster))
		}
	}
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
//
// REFERENCES is the exception, and it is not a DML edge at all: it is what the
// comment extractor emits in EVERY language, pointing a comment at whatever it
// documents — the declaration below it, or the file when it documents nothing. So it
// resolves against the code declarations, and an unresolved one points at the File
// that holds the comment, because there is no third thing for it to mean.
//
// Falling back to Table here is what produced 1637 "tables" in a repository with no
// database at all, named after Go methods (MatchAt), struct fields (segments) and
// source files (convert.go) — every one of them a node nobody declared, polluting
// exactly the label a DML query trusts.
// The `lang` argument is the FILE's language, and it is only the fallback: a
// reference carries the language of the grammar that produced it, which differs
// whenever an embedded block was parsed by another one. Resolving embedded SQL under
// the host format's language is what turned every DML edge of a config XML carrying
// SQL into a File → File self-loop — the SQL grammar's `fallback: Table` rule was
// never consulted, and resolveNamed could not cross into the `plsql` declarations.
func (ri *rebuildIndex) resolveRefTarget(ref cachedReference, lang string) (uid, label string) {
	lang = langOr(ref.Lang, lang)
	rule := ri.refRule(ref, lang)
	if d, ok := ri.resolveNamed(ref.TargetUID, lang, rule); ok {
		return d.uid, d.label
	}
	switch kind := rule.fallbackKind(); kind {
	case TargetFallbackFile:
		return ref.Path, LabelFile
	case TargetFallbackStub:
		return ref.Path, LabelFile
	default:
		return ref.TargetUID, kind
	}
}

func (ri *rebuildIndex) refRule(ref cachedReference, lang string) TargetRule {
	if ri.refSourceLabel(ref.SourceUID) == LabelComment {
		return ri.rules.ForDocumentation(lang)
	}
	return ri.rules.ForRelation(lang, ref.RelType)
}

func (ri *rebuildIndex) stubTableJSON() []map[string]any {
	return collectRows(ri.streamStubTables)
}

func (ri *rebuildIndex) streamStubTables(emit func(map[string]any)) {
	for _, fe := range ri.fileEntries {
		for _, ref := range fe.entry.References {
			if ref.SourceUID == "" {
				continue
			}
			uid, label := ri.resolveRefTarget(ref, fe.entry.Language)
			if label != LabelTable || ri.emittedAny(uid) {
				continue
			}
			ri.emitUID(uid, LabelTable)
			emit(stubJSON(uid, fe.entry.Language, fe.entry.Cluster))
		}
	}
}

func (ri *rebuildIndex) streamAnnotationNodes(kind string, emit func(map[string]any)) {
	seen := make(map[string]bool)
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
					m := map[string]any{
						"uid": uid, "name": dec, "lang": ent.Lang, "is_stub": false,
					}
					if fe.entry.Cluster != "" {
						m["cluster"] = fe.entry.Cluster
					}
					emit(m)
				}
			}
		}
	}
}

func (ri *rebuildIndex) streamParamEdges(callerLabel string, emit func(map[string]any)) {
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
				emit(map[string]any{
					"func_uid": ce.ParentUID, "uid": ce.ChildUID,
					"source_file": fe.relPath, "line_number": 0,
				})
			}
		}

		for _, p := range fe.entry.Parameters {
			key := p.FuncUID + "→" + p.UID
			if !seen[key] && ri.emittedIn(p.FuncUID, callerLabel) && ri.emittedIn(p.UID, "Parameter") {
				seen[key] = true
				emit(map[string]any{
					"func_uid": p.FuncUID, "uid": p.UID,
					"source_file": fe.relPath, "line_number": p.Line,
				})
			}
		}
	}
}

func (ri *rebuildIndex) streamFieldEdges(parentType string, emit func(map[string]any)) {
	for _, fe := range ri.fileEntries {
		for _, f := range fe.entry.Fields {

			if ri.emittedIn(f.ParentUID, parentType) && ri.emittedIn(f.UID, "Field") {
				emit(map[string]any{
					"parent_uid": f.ParentUID, "uid": f.UID,
					"source_file": fe.relPath, "line_number": f.Line,
				})
			}
		}
	}
}

func (ri *rebuildIndex) streamImportEdges(emit func(map[string]any)) {
	for _, fe := range ri.fileEntries {
		for _, imp := range fe.entry.Imports {
			if ri.emittedAny(imp.FileUID) && ri.emittedAny(imp.ModuleUID) {
				emit(map[string]any{
					"file_uid": imp.FileUID, "module_uid": imp.ModuleUID,
					"alias": imp.Alias, "full_import_name": imp.RawImport,
					"imported_name": imp.ImportedName, "line_number": imp.Line,
					"source_file": imp.SourceFile,
				})
			}
		}
	}
}

func (ri *rebuildIndex) canWriteCallerLabel(callerLabel string) bool {
	if !ri.labelSet[LabelFunction] {
		return false
	}
	return callerLabel == LabelFile || ri.labelSet[callerLabel]
}

func (ri *rebuildIndex) callEdgeJSON(callerLabel, calleeLabel string) []map[string]any {
	return collectRows(func(emit func(map[string]any)) { ri.streamCallEdges(callerLabel, calleeLabel, emit) })
}

func (ri *rebuildIndex) streamCallEdges(callerLabel, calleeLabel string, emit func(map[string]any)) {
	for _, fe := range ri.fileEntries {
		for _, call := range fe.entry.Calls {
			if call.CallerUID == "" || call.SourceType != callerLabel ||
				!ri.emittedIn(call.CallerUID, callerLabel) {
				continue
			}
			// emittedIn with the resolved label, not emittedAny: the COPY declares
			// which node table the target lives in, so a call resolved to a Method
			// must not be written into the group that ends at Function.
			calleeUID, resolved := ri.resolveCallee(call.CalleeUID, langOr(call.Lang, fe.entry.Language))
			if resolved != calleeLabel || !ri.emittedIn(calleeUID, calleeLabel) {
				continue
			}
			emit(map[string]any{
				"caller_uid": call.CallerUID, "callee_uid": calleeUID,
				"source_file": call.Path, "line_number": call.Line,
				"full_call_name": "", "receiver_type": call.ReceiverType,
			})
		}
	}
}

func (ri *rebuildIndex) streamInheritEdges(relType, fromLabel, toLabel string, emit func(map[string]any)) {
	for _, fe := range ri.fileEntries {
		for _, inh := range fe.entry.Inheritance {
			if inh.RelType != relType {
				continue
			}
			if ri.entityUIDs[inh.ChildUID] == fromLabel && ri.entityUIDs[inh.ParentUID] == toLabel &&
				ri.emittedAny(inh.ChildUID) && ri.emittedAny(inh.ParentUID) {
				emit(map[string]any{
					"child_uid": inh.ChildUID, "parent_uid": inh.ParentUID,
					"source_file": inh.Path, "line_number": inh.Line,
				})
			}
		}
	}
}

func (ri *rebuildIndex) streamFieldAccessEdges(write bool, srcLabel string, emit func(map[string]any)) {
	for _, fe := range ri.fileEntries {
		for _, fa := range fe.entry.FieldAccess {
			if fa.IsWrite != write || !ri.emittedIn(fa.SourceUID, srcLabel) {
				continue
			}
			fieldUID := ri.resolveFieldTarget(fa.FieldUID, fe.entry.Language)
			if !ri.emittedIn(fieldUID, LabelField) {
				continue
			}
			emit(map[string]any{
				"source_uid": fa.SourceUID, "field_uid": fieldUID,
				"source_file": fa.Path, "line_number": fa.Line,
			})
		}
	}
}

func (ri *rebuildIndex) dmlEdgeJSON(relType, srcLabel, tgtLabel string) []map[string]any {
	return collectRows(func(emit func(map[string]any)) { ri.streamDMLEdges(relType, srcLabel, tgtLabel, emit) })
}

func (ri *rebuildIndex) streamDMLEdges(relType, srcLabel, tgtLabel string, emit func(map[string]any)) {
	for _, fe := range ri.fileEntries {
		for _, ref := range fe.entry.References {
			if ref.RelType != relType || ref.SourceUID == "" ||
				ri.refSourceLabel(ref.SourceUID) != srcLabel || !ri.emittedAny(ref.SourceUID) {
				continue
			}
			targetUID, targetLabel := ri.resolveRefTarget(ref, fe.entry.Language)
			// emittedIn, not emittedAny: the COPY declares which node table the
			// target lives in, so a target resolved to a View must not be written
			// into the group that ends at Table.
			//
			// File is the exception: its node does not go through emitUID — the table
			// is created unconditionally by the rebuild, see refSourceLabel — so the
			// proof it exists is being a relPath of this rebuild.
			if targetLabel != tgtLabel {
				continue
			}
			if tgtLabel == LabelFile {
				if !ri.filePaths[targetUID] {
					continue
				}
			} else if !ri.emittedIn(targetUID, tgtLabel) {
				continue
			}
			emit(map[string]any{
				"source_uid": ref.SourceUID, "target_uid": targetUID,
				"source_file": ref.Path, "line_number": ref.Line,
			})
		}
	}
}

func (ri *rebuildIndex) streamAnnotationEdges(kind, ownerLabel string, emit func(map[string]any)) {
	for _, fe := range ri.fileEntries {
		for _, ent := range fe.entry.Entities {
			if ent.Label != ownerLabel || annotationKind(ent.Lang) != kind {
				continue
			}
			for _, dec := range ent.Decorators {
				annUID := dec + ":" + ent.Lang
				if dec != "" && ri.emittedAny(ent.UID) && ri.emittedAny(annUID) {
					emit(map[string]any{
						"entity_uid": ent.UID, "annotation_uid": annUID,
						"source_file": ent.Path, "line_number": ent.Line,
					})
				}
			}
		}
	}
}

func (ri *rebuildIndex) streamContainsFileEntity(label string, emit func(map[string]any)) {
	for _, fe := range ri.fileEntries {
		for _, ent := range fe.entry.Entities {
			if ent.Label == label && ri.emittedIn(ent.Path, "File") && ri.emittedIn(ent.UID, label) {
				emit(map[string]any{"path": ent.Path, "uid": ent.UID})
			}
		}
	}
}

func (ri *rebuildIndex) streamContainsDirDir(emit func(map[string]any)) {
	for dp := range ri.dirPathSet {
		parent := filepath.Dir(dp)
		if strings.Contains(dp, "/") && ri.emittedAny(parent) && ri.emittedAny(dp) {
			emit(map[string]any{
				"parent_dir": parent, "child_dir": dp,
			})
		}
	}
}

func (ri *rebuildIndex) streamContainsDirFile(emit func(map[string]any)) {
	for _, fe := range ri.fileEntries {
		parent := filepath.Dir(fe.relPath)
		if strings.Contains(fe.relPath, "/") && ri.emittedAny(parent) && ri.emittedAny(fe.relPath) {
			emit(map[string]any{
				"parent_dir": parent, "file_path": fe.relPath,
			})
		}
	}
}

func (ri *rebuildIndex) streamContainsEntity(parentLabel, childLabel string, emit func(map[string]any)) {
	for _, fe := range ri.fileEntries {
		for _, ce := range fe.entry.ContainsEdges {
			if ce.ParentLabel == parentLabel && ce.ChildLabel == childLabel &&
				ri.emittedIn(ce.ParentUID, parentLabel) && ri.emittedIn(ce.ChildUID, childLabel) {
				emit(map[string]any{
					"parent_uid": ce.ParentUID, "child_uid": ce.ChildUID,
				})
			}
		}
	}
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
