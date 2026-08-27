package ast

import (
	"path/filepath"
	"sort"
	"strings"
)

type rebuildIndex struct {
	entries map[string]*parseCacheEntry

	labels        []string
	labelSet      map[string]bool
	containsPairs [][2]string
	callerLabels  []string
	// calleeLabels are the node tables call targets live in. Function is always one of
	// them — it is where the stubs go — and Method joins it as soon as some call
	// resolves to a declared method.
	calleeLabels    []string
	dmlTypes        []string
	dmlTargetLabels []string
	dmlSourceLabels []string
	// filePaths are this rebuild's relPaths, so refSourceLabel can recognise a
	// reference whose source is the file rather than an entity.
	filePaths map[string]bool
	// hasCallTargets says some call has a target, and therefore that stubFunctionJSON
	// may emit Function nodes — the table has to exist even with no caller at all.
	hasCallTargets   bool
	paramOwnerLabels []string
	fieldOwnerLabels []string
	// fieldAccessSourceLabels are the node tables a READS_FIELD/WRITES_FIELD starts from.
	fieldAccessSourceLabels []string
	inheritLabels           []string
	decoratorOwnerLabels    []string
	annotationKinds         []string
	hasParams               bool
	hasFields               bool
	hasInherits             bool
	hasDecorators           bool
	hasImports              bool

	entityUIDs  map[string]string
	fieldUIDs   map[string]bool
	dirPathSet  map[string]bool
	fileEntries []fileEntry
	emittedUIDs map[string]map[string]bool
	// decls resolves a target's name to the declarations it may mean. It indexes EVERY
	// declared label, with no fixed list: which subset counts is the grammar's
	// decision, through TargetRule.
	decls map[string][]declRef
	// rules are the resolution rules the grammars declare.
	rules *TargetRules
}

// declRef is one declaration a target's name may mean.
//
// The index keeps EVERY candidate for a name rather than a count: which labels count
// depends on the relation being resolved — see TargetRule — so ambiguity can only be
// judged at lookup time. With the right set, a name the whole list would reject
// resolves: a call to `Rule` is unique among functions even when a `Rule` struct also
// exists.
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
		entries:     entries,
		rules:       rules,
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

// refSourceLabel says which node table a reference starts from.
//
// Normally the entity containing it. When there is no entity — a statement at the top
// of a script, a `var(--x)` at the top of a rule — the UID is the file's path and the
// source is the File. `LabelFile` deliberately stays out of ri.labels: the File table
// is created unconditionally by the rebuild, and adding it there would emit the node's
// COPY twice.
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
	// Filled from the KEYS of ri.entries rather than from ri.fileEntries: that one is
	// built inside the loop below, so an init before it would see an empty list.
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
				// The file's language is the fallback for the entity's: not every
				// extractor fills ent.Lang, and without it the same-language guard
				// would reject every candidate — resolving nothing.
				declLang := ent.Lang
				if declLang == "" {
					declLang = entry.Language
				}
				ref := declRef{uid: ent.UID, label: ent.Label, lang: declLang}
				decls[ent.Name] = append(decls[ent.Name], ref)

				// Also under `owner.name`, so a target that names what it belongs to
				// resolves to ONE declaration instead of to the dozens that share a
				// bare name. `PEDIDO.ORDER_ID` is a different key from `ORDER_ID`, and
				// only a query that qualified its target ever asks for it — see
				// QualifierCapture. Both keys are kept: an unqualified target still
				// resolves the way it always did.
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
			if call.CalleeUID != "" {
				ri.hasCallTargets = true
			}
			if call.CallerUID != "" && call.SourceType != "" {
				// Whatever label the caller carries, including File — a call with no
				// entity around it is made by the FILE (an `init()` at the end of a
				// module, a bare script statement), and unlike DML the CALLS schema
				// has no fixed File step: it derives its pairs from this set, so a
				// label missing here is a group that never gets declared and an edge
				// with nowhere to go.
				//
				// This used to be a FIXED LIST — Function, Method, Procedure, Trigger,
				// Package, File — and that made the caller of a call un-extensible:
				// an embedded block attributed to a unit its own grammar declared
				// (a screen's trigger, a flow's processor, a report's query) carried a
				// label absent from the list, so the DML edge from that block appeared
				// and the CALLS edge from the SAME block did not. Nothing here has to
				// vet the label: `canWriteCallerLabel` requires it to have been
				// emitted, `callRelPairs` intersects with the node tables that exist,
				// and `callEdgeJSON` demands the caller uid live in that very table.
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
				// File deliberately stays OUT of dmlSrcSet: the schema already
				// declares `FROM File TO <target>` unconditionally for every DML type
				// (ladybug.go), so announcing it here would emit the pair twice and
				// LadybugDB rejects the whole group with "duplicate FROM-TO pairs".
				// File-sourced edges have their own COPY step in json_rebuild,
				// mirroring the step the schema already had.
				if lbl := ri.entityUIDs[ref.SourceUID]; lbl != "" {
					dmlSrcSet[lbl] = true
				}
			}
		}

		for _, ce := range entry.ContainsEdges {
			containsSet[[2]string{ce.ParentLabel, ce.ChildLabel}] = true
		}
	}

	// The Function table has to exist when ANY call has a target, not only when some
	// call has a caller. stubFunctionJSON creates a Function node for every unresolved
	// target regardless of who calls it — and a call at the top of a script has no
	// entity around it, so `callerSet` stays empty. In that state the stub rows were
	// emitted against a table that did not exist and the COPY failed with "Table
	// Function does not exist", aborting the whole rebuild. Latent until a bare
	// embedded SQL block made the case reachable.
	if len(callerSet) > 0 || ri.hasCallTargets {
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
	ri.decls = decls

	// Segundo passo, depois de ri.decls estar completo: resolver um alvo de chamada
	// needs the whole index, and inside the loop above it is still being built.
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
		Labels:                  ri.labels,
		ContainsPairs:           ri.containsPairs,
		CallerLabels:            ri.callerLabels,
		CalleeLabels:            ri.calleeLabels,
		DMLTypes:                ri.dmlTypes,
		DMLTargetLabels:         ri.dmlTargetLabels,
		DMLSourceLabels:         ri.dmlSourceLabels,
		ParamOwnerLabels:        ri.paramOwnerLabels,
		FieldOwnerLabels:        ri.fieldOwnerLabels,
		FieldAccessSourceLabels: ri.fieldAccessSourceLabels,
		InheritLabels:           ri.inheritLabels,
		DecoratorOwnerLabels:    ri.decoratorOwnerLabels,
		AnnotationKinds:         ri.annotationKinds,
		HasFields:               ri.hasFields,
		HasParams:               ri.hasParams,
		HasInherits:             ri.hasInherits,
		HasDecorators:           ri.hasDecorators,
	}
}

func (ri *rebuildIndex) fileNodeJSON() []map[string]any {
	rows := make([]map[string]any, 0, len(ri.fileEntries))
	for _, fe := range ri.fileEntries {
		ri.emitUID(fe.relPath, "File")
		rows = append(rows, map[string]any{
			"path": fe.relPath, "name": filepath.Base(fe.relPath),
			"relative_path": fe.relPath, "is_dependency": fe.entry.IsDepend,
			"lang": fe.entry.Language, "cluster": fe.entry.Cluster,
		})
	}
	return rows
}

func (ri *rebuildIndex) dirNodeJSON(clusterPathMap map[string]string, defaultCluster string) []map[string]any {
	rows := make([]map[string]any, 0, len(ri.dirPathSet))
	for dp := range ri.dirPathSet {
		ri.emitUID(dp, "Directory")
		cluster := resolveClusterForPath(dp, "", clusterPathMap, defaultCluster)
		rows = append(rows, map[string]any{
			"path": dp, "name": filepath.Base(dp), "cluster": cluster,
		})
	}
	return rows
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

// stubJSON is the node left behind when a target is declared nowhere in the corpus —
// a call to `fmt.Println`, an inheritance from a library class.
//
// lang comes from the file the reference originated in, never empty: a stub has no
// file of its own, but the language of whoever invoked it is the only correct answer
// and is what lets it be grouped. It used to be "" and every stub fell into a group
// with no language, indistinguishable from a directory node.
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
	var rows []map[string]any
	for _, fe := range ri.fileEntries {
		for _, ent := range fe.entry.Entities {
			if ent.Label == label && ri.emitUID(ent.UID, label) {
				rows = append(rows, entityToJSON(ent, false, fe.entry.Cluster))
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
				m := map[string]any{
					"uid": imp.ModuleUID, "name": imp.ModuleName,
					"lang": imp.Lang, "full_import_name": imp.RawImport, "is_stub": false,
				}
				if fe.entry.Cluster != "" {
					m["cluster"] = fe.entry.Cluster
				}
				rows = append(rows, m)
			}
		}
	}
	return rows
}

// resolveNamed joins a target captured by name to the declaration it means, under the
// rule the grammar declared for that relation.
//
// Two guards stay in the engine because they are correctness invariants, not language
// policy:
//
//   - exactly one candidate. With homonyms inside the allowed set, picking one would
//     invent an edge nobody wrote.
//   - same language. A canvas `fill()` in .tsx does not call the Go function of the
//     same name that happens to be the only one in the repository.
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
	// An unresolved call stays a stub whatever fallback was declared: it is written
	// into the Function table, and pointing it at the file would lose the name of what
	// was called, which is the only information left.
	return calleeUID, LabelFunction
}

func (ri *rebuildIndex) stubFunctionJSON() []map[string]any {
	var rows []map[string]any
	for _, fe := range ri.fileEntries {
		for _, call := range fe.entry.Calls {
			if call.CalleeUID == "" {
				continue
			}
			// A resolved target was already emitted as an entity — entities come
			// before stubs in write order — so emittedAny discards it here.
			uid, label := ri.resolveCallee(call.CalleeUID, langOr(call.Lang, fe.entry.Language))
			if label != LabelFunction || ri.emittedAny(uid) {
				continue
			}
			ri.emitUID(uid, LabelFunction)
			rows = append(rows, stubJSON(uid, fe.entry.Language, fe.entry.Cluster))
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
				rows = append(rows, stubJSON(inh.ParentUID, fe.entry.Language, fe.entry.Cluster))
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
				rows = append(rows, stubJSON(inh.ParentUID, fe.entry.Language, fe.entry.Cluster))
			}
		}
	}
	return rows
}

// resolveFieldTarget says which node a field access reaches.
//
// The access is cached as the field's bare name, so it needs the same join a call does
// — and the same guards: a field of the same name in ten structs, or a `length` from
// another language, is worse resolved than left alone. Unresolved, the stub remains,
// recording an access to a field declared outside this corpus.
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

func (ri *rebuildIndex) stubFieldJSON() []map[string]any {
	var rows []map[string]any
	for _, fe := range ri.fileEntries {
		for _, fa := range fe.entry.FieldAccess {
			uid := ri.resolveFieldTarget(fa.FieldUID, fe.entry.Language)
			if ri.emittedAny(uid) {
				continue
			}
			ri.emitUID(uid, LabelField)
			rows = append(rows, stubJSON(uid, fe.entry.Language, fe.entry.Cluster))
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
		// With no declared label there is no table to write the stub into, and the
		// reference has nowhere to go: the file is the target that always exists.
		return ref.Path, LabelFile
	default:
		return ref.TargetUID, kind
	}
}

// refRule chooses between the grammar's rule and the engine's.
//
// The documentation edge — a comment to what it documents — is emitted by the ENGINE,
// for every language, and no yaml declares it. It cannot simply share the grammar's
// REFERENCES rule: nine grammars declare that relation, and for the SQL family an
// unresolved REFERENCES really does mean a table. The source is what separates the
// two — only the engine's starts from a Comment.
func (ri *rebuildIndex) refRule(ref cachedReference, lang string) TargetRule {
	if ri.refSourceLabel(ref.SourceUID) == LabelComment {
		return ri.rules.ForDocumentation(lang)
	}
	return ri.rules.ForRelation(lang, ref.RelType)
}

func (ri *rebuildIndex) stubTableJSON() []map[string]any {
	var rows []map[string]any
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
			rows = append(rows, stubJSON(uid, fe.entry.Language, fe.entry.Cluster))
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
					m := map[string]any{
						"uid": uid, "name": dec, "lang": ent.Lang, "is_stub": false,
					}
					if fe.entry.Cluster != "" {
						m["cluster"] = fe.entry.Cluster
					}
					rows = append(rows, m)
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

// canWriteCallerLabel says whether the CALLS group has both ends for this caller.
//
// File passes without being in labelSet, and the asymmetry is deliberate: the File
// node table is created unconditionally by the rebuild, so putting it in labelSet would
// emit the node's COPY twice — but the original gate consulted labelSet alone, and so
// top-level calls kept being dropped even after the file became a caller.
//
// It exists as a function rather than an inline condition because that is precisely
// what a test over callEdgeJSON did not cover: that test calls the row generator
// directly and skips the gate, so it passed while the real path was broken.
func (ri *rebuildIndex) canWriteCallerLabel(callerLabel string) bool {
	if !ri.labelSet[LabelFunction] {
		return false
	}
	return callerLabel == LabelFile || ri.labelSet[callerLabel]
}

func (ri *rebuildIndex) callEdgeJSON(callerLabel, calleeLabel string) []map[string]any {
	var rows []map[string]any
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
			rows = append(rows, map[string]any{
				"caller_uid": call.CallerUID, "callee_uid": calleeUID,
				"source_file": call.Path, "line_number": call.Line,
				"full_call_name": "", "receiver_type": call.ReceiverType,
			})
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

// fieldAccessEdgeJSON takes the source label because a method reads fields as much as
// a function does — in Go, more. The COPY pinned Function as the source, so every
// access made from inside a method had no group to be written into.
func (ri *rebuildIndex) fieldAccessEdgeJSON(write bool, srcLabel string) []map[string]any {
	var rows []map[string]any
	for _, fe := range ri.fileEntries {
		for _, fa := range fe.entry.FieldAccess {
			if fa.IsWrite != write || !ri.emittedIn(fa.SourceUID, srcLabel) {
				continue
			}
			fieldUID := ri.resolveFieldTarget(fa.FieldUID, fe.entry.Language)
			if !ri.emittedIn(fieldUID, LabelField) {
				continue
			}
			rows = append(rows, map[string]any{
				"source_uid": fa.SourceUID, "field_uid": fieldUID,
				"source_file": fa.Path, "line_number": fa.Line,
			})
		}
	}
	return rows
}

func (ri *rebuildIndex) dmlEdgeJSON(relType, srcLabel, tgtLabel string) []map[string]any {
	var rows []map[string]any
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
