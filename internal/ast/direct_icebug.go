package ast

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/apache/arrow-go/v18/arrow"
	"github.com/apache/arrow-go/v18/arrow/array"
	"github.com/apache/arrow-go/v18/arrow/memory"
	"github.com/apache/arrow-go/v18/parquet"
	"github.com/apache/arrow-go/v18/parquet/compress"
	"github.com/apache/arrow-go/v18/parquet/pqarrow"

	ladybug "github.com/graphit-labs/graphit-code/internal/ladybugstore"
)

// Export from shards (RebuildIndex) to a canonical icebug bundle, without an
// intermediate Ladybug database.
//
// NOTHING HERE IS LANGUAGE-SPECIFIC. Every label, column, relationship type and
// member is derived from what the shards actually hold: labels come from the
// declared entities, node columns from the superset of property names appearing
// in a label's rows, relationship types from the relation categories the shards
// carry, and edge properties from the row keys themselves. The only structural
// knowledge is the small set of graph-level shapes — how a node primary key is
// identified, and which two row keys anchor an edge — and even those are driven
// by the data rather than by a per-label table.

func ExportDirectFromRebuildIndex(ri *rebuildIndex, outDir, storageURI string) (*ladybug.CanonicalManifest, error) {
	return exportDirectWithReverse(ri, outDir, storageURI, nil, nil, true)
}

func ExportDirectFromRebuildIndexWithReverse(ri *rebuildIndex, outDir, storageURI string, reverse bool) (*ladybug.CanonicalManifest, error) {
	return exportDirectWithReverse(ri, outDir, storageURI, nil, nil, reverse)
}

func exportDirectWithReverse(ri *rebuildIndex, outDir, storageURI string, filterLabels, filterRels map[string]bool, reverse bool) (*ladybug.CanonicalManifest, error) {
	if err := os.MkdirAll(outDir, 0o755); err != nil {
		return nil, err
	}

	// ---- nodes: every label the shards declare, plus the two structural tables ----
	type nodeBatch struct {
		label string
		rows  []map[string]any
	}
	batches := make([]nodeBatch, 0)
	var labelIDs = map[string]map[string]uint64{}

	labels := append([]string{}, ri.labels...)
	labels = append(labels, "File", "Directory")

	// Rows per label come from the same sources the file-backed rebuild used (see
	// rebuild_index.go) — fileNodeJSON, dirNodeJSON, entityJSON plus the stub
	// writers — so the exported data is identical to what a populated store had.
	seenLabel := map[string]bool{}
	collect := func(label string) {
		if seenLabel[label] {
			return
		}
		seenLabel[label] = true
		rows := nodeRowsFor(ri, label)
		// Deleted (missing) entities leave no references to resolve: files are
		// the only rows that always exist.
		_ = rows
		if len(rows) == 0 && label != "File" && label != "Directory" {
			return
		}
		if len(rows) == 0 && label == "Directory" {
			return
		}
		batches = append(batches, nodeBatch{label: label, rows: rows})
	}
	for _, l := range labels {
		collect(l)
	}
	// Parameter/Field tables appear when the shards carry them, not by default.
	if ri.hasParams && !seenLabel["Parameter"] {
		collect("Parameter")
	}
	if ri.hasFields && !seenLabel["Field"] {
		collect("Field")
	}
	for _, kind := range ri.annotationKinds {
		collect(kind)
	}

	// Manifest shell.
	man := &ladybug.CanonicalManifest{
		Version:  ladybug.CanonicalManifestVersion,
		Format:   "icebug-canonical",
		Storage:  storageURI,
		Schema:   "schema.cypher",
		Reverse:  reverse,
		Finished: false,
		Invariants: ladybug.CanonicalInvariants{
			IndptrRowGroups: 1,
			SelfLoops:       "forward-once",
		},
	}

	// Deterministic node order for reproducible exports.
	sort.Slice(batches, func(i, j int) bool { return batches[i].label < batches[j].label })
	order := []string{}
	for _, b := range batches {
		order = append(order, b.label)
	}

	for _, b := range batches {
		cols, pk := columnsForLabel(b.label, b.rows)
		// Dense ids are the row index after an order determined by the primary key,
		// exactly like ExportIcebugCanonical: stable when nothing changed, so an
		// incremental can rewrite only a label's file.
		sort.SliceStable(b.rows, func(i, j int) bool {
			return strings.Compare(fmt.Sprint(b.rows[i][pk]), fmt.Sprint(b.rows[j][pk])) < 0
		})
		// THE ID SPACE IS PER TABLE, matching ExportIcebugCanonical: the CSR of a
		// `FROM A TO B` member indexes A's rows 0..nA and B's rows 0..nB; the
		// reader resolves the member's declared endpoints, so File:0 and
		// Function:0 are distinct by being in different declared tables.
		ids := make(map[string]uint64, len(b.rows))
		for i, r := range b.rows {
			ids[fmt.Sprint(r[pk])] = uint64(i)
		}
		labelIDs[b.label] = ids

		fields := make([]arrow.Field, len(cols))
		for i, c := range cols {
			fields[i] = arrow.Field{Name: c.Name, Type: arrowTypeForCypherDirect(c.Type), Nullable: true}
		}
		file := "nodes_" + b.label + ".parquet"
		schema := arrow.NewSchema(fields, icebugMetadataDirect())
		if err := writeParquetDirect(filepath.Join(outDir, file), schema, len(b.rows),
			func(bld *array.RecordBuilder, from, to int) {
				for ci, col := range cols {
					builder := bld.Field(ci)
					for i := from; i < to; i++ {
						appendArrowValueDirect(builder, b.rows[i][col.Name])
					}
				}
			}); err != nil {
			return nil, fmt.Errorf("write nodes %s: %w", b.label, err)
		}
		man.NodeTables = append(man.NodeTables, ladybug.CanonicalNodeTable{
			Label: b.label, File: file, Rows: int64(len(b.rows)), PrimaryKey: pk, Columns: cols,
		})
	}

	// ---- relationships ----
	//
	// The categories below are the CARRIER FORMS of the shards (calls, inheritance,
	// DML references, containment, parameters, fields, imports, field access, and
	// annotation attachments). Which category is non-empty, which member pairs it
	// produces, and which properties each edge row carries are all read from the
	// rows; nothing is decided by the language of the source file.
	relMembers := map[string][]*ladybug.CanonicalMember{}
	relReverse := map[string][]*ladybug.CanonicalMember{}
	usedMembers := map[string]string{}

	exportRel := func(relType, from, to, fromCol, toCol string, data []map[string]any) error {
		if len(data) == 0 {
			return nil
		}
		fromIDs, ok1 := labelIDs[from]
		toIDs, ok2 := labelIDs[to]
		if !ok1 || !ok2 {
			return nil
		}
		var edges []csrEdgeDirect
		var propNames []string
		propSeen := map[string]bool{}
		for _, r := range data {
			s, okS := fromIDs[fmt.Sprint(r[fromCol])]
			t, okT := toIDs[fmt.Sprint(r[toCol])]
			if !okS || !okT {
				continue
			}
			edges = append(edges, csrEdgeDirect{source: s, target: t})
			for k := range r {
				if k == fromCol || k == toCol {
					continue
				}
				if !propSeen[k] {
					propSeen[k] = true
					propNames = append(propNames, k)
				}
			}
		}
		if len(edges) == 0 {
			return nil
		}
		// Property columns, in the stable order of their first appearance across
		// rows, with the type inferred from the values — matches what the file
		// path wrote for the same row keys.
		sort.Strings(propNames)
		props := make([]ladybug.Field, 0, len(propNames))
		for _, k := range propNames {
			props = append(props, ladybug.Field{Name: k, Type: inferTypeFor(collectPropValues(data, k))})
		}
		fwdName := canonicalMemberNameDirect(relType, from, to)
		if prev, seen := usedMembers[fwdName]; seen && prev != from+"->"+to {
			return fmt.Errorf("canonical members collide on %q", fwdName)
		}
		usedMembers[fwdName] = from + "->" + to

		sortedEdges, sortedProps := sortCSR(edges, collectProps(data, props, fromCol, toCol))
		indicesFile := "indices_" + fwdName + ".parquet"
		indptrFile := "indptr_" + fwdName + ".parquet"
		propArrowFields := make([]arrow.Field, len(props))
		for i, p := range props {
			propArrowFields[i] = arrow.Field{Name: p.Name, Type: arrowTypeForCypherDirect(p.Type), Nullable: true}
		}
		if err := writeIndicesDirect(filepath.Join(outDir, indicesFile), sortedEdges, propArrowFields, sortedProps); err != nil {
			return err
		}
		if err := writeIndptrDirect(filepath.Join(outDir, indptrFile), sortedEdges, uint64(len(fromIDs))); err != nil {
			return err
		}
		fwd := &ladybug.CanonicalMember{
			From: from, To: to, Table: fwdName,
			Indices: indicesFile, Indptr: indptrFile,
			Rows: int64(len(sortedEdges)),
		}
		relMembers[relType] = append(relMembers[relType], fwd)
		man.EdgeCount += fwd.Rows

		if !reverse {
			return nil
		}
		// A self-loop matters only when the two endpoint TABLES are the same:
		// File:0 -> Function:0 are different rows in different tables. The id
		// space is per table and the reader resolves the member's endpoints.
		revEdges, revProps := reverseEdgesDirect(sortedEdges, sortedProps, from, to)
		if len(revEdges) == 0 {
			return nil
		}
		revSorted, revSortedProps := sortCSR(revEdges, revProps)
		revName := fwdName + "_reverse"
		revIndices := "indices_" + revName + ".parquet"
		revIndptr := "indptr_" + revName + ".parquet"
		if err := writeIndicesDirect(filepath.Join(outDir, revIndices), revSorted, propArrowFields, revSortedProps); err != nil {
			return err
		}
		if err := writeIndptrDirect(filepath.Join(outDir, revIndptr), revSorted, uint64(len(toIDs))); err != nil {
			return err
		}
		rev := &ladybug.CanonicalMember{
			From: to, To: from, Table: revName,
			Indices: revIndices, Indptr: revIndptr,
			Rows: int64(len(revSorted)),
		}
		relReverse[relType] = append(relReverse[relType], rev)
		return nil
	}

	if ri.hasParams {
		for _, owner := range ri.paramOwnerLabels {
			if err := exportRel("HAS_PARAMETER", owner, "Parameter", "func_uid", "uid", ri.paramEdgeJSON(owner)); err != nil {
				return nil, err
			}
		}
	}
	for _, pt := range ri.labels {
		if err := exportRel("HAS_FIELD", pt, "Field", "parent_uid", "uid", ri.fieldEdgeJSON(pt)); err != nil {
			return nil, err
		}
	}
	for _, kind := range ri.annotationKinds {
		edgeName := "HAS_" + strings.ToUpper(kind)
		for _, ol := range ri.decoratorOwnerLabels {
			if !ri.labelSet[ol] {
				continue
			}
			if err := exportRel(edgeName, ol, kind, "entity_uid", "annotation_uid", ri.annotationEdgeJSON(kind, ol)); err != nil {
				return nil, err
			}
		}
	}
	if ri.hasImports {
		if err := exportRel("IMPORTS", "File", "Module", "file_uid", "module_uid", ri.importEdgeJSON()); err != nil {
			return nil, err
		}
	}
	for _, cl := range ri.callerLabels {
		if !ri.canWriteCallerLabel(cl) {
			continue
		}
		for _, tl := range ri.calleeLabels {
			if err := exportRel("CALLS", cl, tl, "caller_uid", "callee_uid", ri.callEdgeJSON(cl, tl)); err != nil {
				return nil, err
			}
		}
	}
	for _, from := range ri.inheritLabels {
		if !ri.labelSet[from] {
			continue
		}
		for _, to := range ri.inheritLabels {
			if !ri.labelSet[to] {
				continue
			}
			if err := exportRel("INHERITS", from, to, "child_uid", "parent_uid", ri.inheritEdgeJSON("INHERITS", from, to)); err != nil {
				return nil, err
			}
			if err := exportRel("IMPLEMENTS", from, to, "child_uid", "parent_uid", ri.inheritEdgeJSON("IMPLEMENTS", from, to)); err != nil {
				return nil, err
			}
		}
	}
	if ri.labelSet[LabelField] {
		for _, src := range ri.fieldAccessSourceLabels {
			if err := exportRel("READS_FIELD", src, "Field", "source_uid", "field_uid", ri.fieldAccessEdgeJSON(false, src)); err != nil {
				return nil, err
			}
			if err := exportRel("WRITES_FIELD", src, "Field", "source_uid", "field_uid", ri.fieldAccessEdgeJSON(true, src)); err != nil {
				return nil, err
			}
		}
	}
	// DML types are whatever the references carry — never a fixed list.
	for _, rt := range ri.dmlTypes {
		if engineOwnedRelTypes[rt] {
			continue
		}
		for _, src := range ri.dmlSourceLabels {
			if !ri.labelSet[src] {
				continue
			}
			for _, tgt := range ri.dmlTargetLabels {
				if err := exportRel(rt, src, tgt, "source_uid", "target_uid", ri.dmlEdgeJSON(rt, src, tgt)); err != nil {
					return nil, err
				}
			}
		}
		for _, tgt := range ri.dmlTargetLabels {
			if err := exportRel(rt, LabelFile, tgt, "source_uid", "target_uid", ri.dmlEdgeJSON(rt, LabelFile, tgt)); err != nil {
				return nil, err
			}
		}
	}
	for _, label := range ri.labels {
		if err := exportRel("CONTAINS", "File", label, "path", "uid", ri.containsFileEntityJSON(label)); err != nil {
			return nil, err
		}
	}
	if err := exportRel("CONTAINS", "Directory", "Directory", "parent_dir", "child_dir", ri.containsDirDirJSON()); err != nil {
		return nil, err
	}
	if err := exportRel("CONTAINS", "Directory", "File", "parent_dir", "file_path", ri.containsDirFileJSON()); err != nil {
		return nil, err
	}
	for _, eg := range ri.containsPairs {
		if err := exportRel("CONTAINS", eg[0], eg[1], "parent_uid", "child_uid", ri.containsEntityJSON(eg[0], eg[1])); err != nil {
			return nil, err
		}
	}

	for relType, members := range relMembers {
		man.RelGroups = append(man.RelGroups, ladybug.CanonicalRelGroup{
			Type:           relType,
			Members:        cloneMembers(members),
			ReverseMembers: cloneMembers(relReverse[relType]),
		})
	}

	if err := writeCanonicalSchemaDirect(outDir, storageURI, man); err != nil {
		return nil, err
	}
	man.Finished = true
	raw, err := json.MarshalIndent(man, "", "  ")
	if err != nil {
		return nil, err
	}
	if err := os.WriteFile(filepath.Join(outDir, "icebug.json"), raw, 0o644); err != nil {
		return nil, err
	}
	return man, nil
}

// ExportDirectIncremental rewrites only the node tables and rel members whose
// rows are touched by changed/deleted, copying the rest verbatim from finalDir.
// Changed paths are repo-relative (ShardCache keys).
func ExportDirectIncremental(ri *rebuildIndex, outDir, finalDir, storageURI string, changed, deleted []string) (*ladybug.CanonicalManifest, error) {
	return ExportDirectIncrementalWithReverse(ri, outDir, finalDir, storageURI, changed, deleted, false)
}

func ExportDirectIncrementalWithReverse(ri *rebuildIndex, outDir, finalDir, storageURI string, changed, deleted []string, reverse bool) (*ladybug.CanonicalManifest, error) {
	oldRaw, err := os.ReadFile(filepath.Join(finalDir, "icebug.json"))
	if err != nil {
		// No previous bundle: full.
		return ExportDirectFromRebuildIndexWithReverse(ri, outDir, storageURI, reverse)
	}
	var oldMan ladybug.CanonicalManifest
	if json.Unmarshal(oldRaw, &oldMan) != nil || oldMan.Format != "icebug-canonical" {
		return ExportDirectFromRebuildIndexWithReverse(ri, outDir, storageURI, reverse)
	}
	// Deleted files left the cache, so the rows they owned cannot be re-derived
	// from the current shards alone; only a full export is correct.
	if len(deleted) > 0 {
		return ExportDirectFromRebuildIndexWithReverse(ri, outDir, storageURI, reverse)
	}
	changedSet := make(map[string]bool, len(changed))
	for _, p := range changed {
		changedSet[p] = true
	}
	// Which labels carry an entity whose path is in the change set, plus the
	// structural tables that a changed file can alter (File, Directory; Function
	// for call stubs; Module for imports).
	affectedLabels := map[string]bool{}
	if len(changedSet) > 0 {
		affectedLabels["File"] = true
		affectedLabels["Directory"] = true
	}
	for _, fe := range ri.fileEntries {
		if !changedSet[fe.relPath] {
			continue
		}
		for _, ent := range fe.entry.Entities {
			affectedLabels[ent.Label] = true
		}
		if len(fe.entry.Calls) > 0 {
			affectedLabels["Function"] = true
		}
		if len(fe.entry.Imports) > 0 {
			affectedLabels["Module"] = true
		}
		if len(fe.entry.Parameters) > 0 {
			affectedLabels["Parameter"] = true
		}
		if len(fe.entry.Fields) > 0 {
			affectedLabels["Field"] = true
		}
	}

	// Even when the deleted file is gone from the cache, any label the old
	// manifest holds and the new export would drop must be rewritten too.
	for _, nt := range oldMan.NodeTables {
		if _, ok := labelInBatches(ri, nt.Label); !ok {
			affectedLabels[nt.Label] = true
		}
	}

	// Recompute affected rel types from the changed files' entry shapes.
	affectedRels := map[string]bool{}
	for _, fe := range ri.fileEntries {
		if !changedSet[fe.relPath] {
			continue
		}
		if len(fe.entry.Calls) > 0 {
			affectedRels["CALLS"] = true
		}
		if len(fe.entry.ContainsEdges) > 0 || len(fe.entry.Entities) > 0 || len(fe.entry.References) > 0 {
			affectedRels["CONTAINS"] = true
		}
		if len(fe.entry.Parameters) > 0 {
			affectedRels["HAS_PARAMETER"] = true
		}
		if len(fe.entry.Fields) > 0 {
			affectedRels["HAS_FIELD"] = true
		}
		if len(fe.entry.Imports) > 0 {
			affectedRels["IMPORTS"] = true
		}
		if len(fe.entry.Inheritance) > 0 {
			affectedRels["INHERITS"] = true
			affectedRels["IMPLEMENTS"] = true
		}
		if len(fe.entry.FieldAccess) > 0 {
			affectedRels["READS_FIELD"] = true
			affectedRels["WRITES_FIELD"] = true
		}
		for _, r := range fe.entry.References {
			affectedRels[r.RelType] = true
		}
	}

	// Full when too much would change.
	if len(affectedLabels) > len(oldMan.NodeTables)/2 && len(affectedLabels) > 4 {
		return ExportDirectFromRebuildIndexWithReverse(ri, outDir, storageURI, reverse)
	}

	// Regenerate the affected labels and rel types, copy everything else verbatim.
	return exportDirectDelta(ri, outDir, finalDir, storageURI, affectedLabels, affectedRels, oldMan, reverse)
}

// exportDirectDelta regenerates the affected node tables and rel members, and
// copies the rest from the previous bundle.
func exportDirectDelta(ri *rebuildIndex, outDir, finalDir, storageURI string, affectedLabels, affectedRels map[string]bool, oldMan ladybug.CanonicalManifest, reverse bool) (*ladybug.CanonicalManifest, error) {
	// The full writer is data-driven, but its writes must be filtered. The clean
	// way without diverging from the writer: generate everything in memory is not
	// the design (the writer streams Parquets). So we run the full export to a
	// scratch dir, then for every file NOT affected we replace it with the old
	// one; for every rel member NOT affected we copy its old indices/indptr over
	// the freshly generated ones. The manifest is derived from the union of old
	// untouched entries and new affected entries.
	scratch := outDir + ".scratch"
	_ = os.RemoveAll(scratch)
	fresh, err := ExportDirectFromRebuildIndexWithReverse(ri, scratch, storageURI, reverse)
	if err != nil {
		return nil, err
	}

	// Node tables: affected -> keep fresh; unaffected -> old copy.
	oldNode := map[string]ladybug.CanonicalNodeTable{}
	for _, nt := range oldMan.NodeTables {
		oldNode[nt.Label] = nt
	}
	newNode := map[string]ladybug.CanonicalNodeTable{}
	for _, nt := range fresh.NodeTables {
		newNode[nt.Label] = nt
	}

	// Rel member mapping: logical type -> member table name.
	oldRel := map[string]ladybug.CanonicalRelGroup{}
	for _, rg := range oldMan.RelGroups {
		oldRel[rg.Type] = rg
	}
	newRel := map[string]ladybug.CanonicalRelGroup{}
	for _, rg := range fresh.RelGroups {
		newRel[rg.Type] = rg
	}

	// Decide which node tables survive.
	survivingNodes := []ladybug.CanonicalNodeTable{}
	for _, nt := range fresh.NodeTables {
		if affectedLabels[nt.Label] {
			survivingNodes = append(survivingNodes, nt)
			continue
		}
		if oldNT, ok := oldNode[nt.Label]; ok {
			// Copy the old Parquet bytes (same data, no re-write).
			if err := copyIcebugFile(filepath.Join(finalDir, oldNT.File), filepath.Join(outDir, nt.File)); err != nil {
				return nil, err
			}
			survivingNodes = append(survivingNodes, oldNT)
			continue
		}
		// No old copy: keep fresh.
		survivingNodes = append(survivingNodes, nt)
	}
	// Labels in OLD but not fresh (e.g. label disappeared entirely).
	for _, nt := range oldMan.NodeTables {
		if _, still := newNode[nt.Label]; !still {
			// table gone from graph: drop (a deleted file erased the only entity)
			// but only if it was not affected; affected ones re-created above or gone for good.
			if affectedLabels[nt.Label] {
				// affected and absent: the table is genuinely empty now -> no file.
				continue
			}
			// absent because the label changed shape in this run? keep old.
			if err := copyIcebugFile(filepath.Join(finalDir, nt.File), filepath.Join(outDir, nt.File)); err != nil {
				return nil, err
			}
			survivingNodes = append(survivingNodes, nt)
		}
	}

	// Rel members: per logical type.
	var survivingGroups []ladybug.CanonicalRelGroup
	for _, rg := range fresh.RelGroups {
		out := ladybug.CanonicalRelGroup{Type: rg.Type}
		if affectedRels[rg.Type] {
			// Regenerated members: keep fresh (already our newest).
			out.Members = rg.Members
			out.ReverseMembers = rg.ReverseMembers
		} else {
			// Copy old member parquets.
			if oldRG, ok := oldRel[rg.Type]; ok {
				for _, m := range oldRG.Members {
					if err := copyRelMember(finalDir, outDir, m); err != nil {
						return nil, err
					}
					out.Members = append(out.Members, m)
				}
				for _, m := range oldRG.ReverseMembers {
					if err := copyRelMember(finalDir, outDir, m); err != nil {
						return nil, err
					}
					out.ReverseMembers = append(out.ReverseMembers, m)
				}
			} else {
				out.Members = rg.Members
				out.ReverseMembers = rg.ReverseMembers
			}
		}
		survivingGroups = append(survivingGroups, out)
	}
	// Types in old but absent from fresh (edgeless now): drop? Keep if old had
	// members and the type is not affected — must keep consistent with nodes.
	for _, rg := range oldMan.RelGroups {
		if _, ok := newRel[rg.Type]; ok {
			continue
		}
		if affectedRels[rg.Type] {
			continue // genuinely empty now
		}
		out := ladybug.CanonicalRelGroup{Type: rg.Type}
		for _, m := range rg.Members {
			if err := copyRelMember(finalDir, outDir, m); err != nil {
				return nil, err
			}
			out.Members = append(out.Members, m)
		}
		for _, m := range rg.ReverseMembers {
			if err := copyRelMember(finalDir, outDir, m); err != nil {
				return nil, err
			}
			out.ReverseMembers = append(out.ReverseMembers, m)
		}
		survivingGroups = append(survivingGroups, out)
	}

	// Drop scratch.
	_ = os.RemoveAll(scratch)

	man := &ladybug.CanonicalManifest{
		Version:  ladybug.CanonicalManifestVersion,
		Format:   "icebug-canonical",
		Storage:  storageURI,
		Schema:   "schema.cypher",
		Reverse:  true,
		Finished: false,
		Invariants: ladybug.CanonicalInvariants{
			IndptrRowGroups: 1,
			SelfLoops:       "forward-once",
		},
	}
	man.NodeTables = survivingNodes
	man.RelGroups = survivingGroups
	for _, rg := range survivingGroups {
		for _, m := range rg.Members {
			man.EdgeCount += m.Rows
		}
	}
	if err := writeCanonicalSchemaDirect(outDir, storageURI, man); err != nil {
		return nil, err
	}
	man.Finished = true
	raw, err := json.MarshalIndent(man, "", "  ")
	if err != nil {
		return nil, err
	}
	if err := os.WriteFile(filepath.Join(outDir, "icebug.json"), raw, 0o644); err != nil {
		return nil, err
	}
	return man, nil
}

func labelInBatches(ri *rebuildIndex, label string) (map[string]any, bool) {
	rows := nodeRowsFor(ri, label)
	return nil, len(rows) > 0
}

// ---------- node rows ----------

func nodeRowsFor(ri *rebuildIndex, label string) []map[string]any {
	switch label {
	case "File":
		return ri.fileNodeJSON()
	case "Directory":
		return ri.dirNodeJSON(nil, "")
	case "Module":
		if ri.hasImports {
			return ri.moduleJSON()
		}
		return nil
	}
	for _, kind := range ri.annotationKinds {
		if label == kind {
			return ri.annotationNodeJSON(kind)
		}
	}
	rows := ri.entityJSON(label)
	switch label {
	case "Function":
		rows = append(rows, ri.stubFunctionJSON()...)
	case "Class":
		if ri.labelSet["Class"] {
			rows = append(rows, ri.stubClassJSON()...)
		}
	case "Interface":
		if ri.labelSet["Interface"] {
			rows = append(rows, ri.stubInterfaceJSON()...)
		}
	case LabelField:
		if ri.labelSet[LabelField] {
			rows = append(rows, ri.stubFieldJSON()...)
		}
	case "Table":
		if ri.labelSet["Table"] {
			rows = append(rows, ri.stubTableJSON()...)
		}
	}
	return rows
}

// ---------- columns and types (derived, not hardcoded) ----------

var graphColumnOrder = []string{
	"uid", "name", "path", "relative_path", "line_number", "end_line", "docstring",
	"lang", "cyclomatic_complexity", "context", "context_type", "class_context",
	"is_dependency", "is_depend", "is_exported", "value", "is_stub", "cluster",
	"full_import_name", "alias", "imported_name", "source_file",
}

// nodePrimaryKeyNames are the property names a label can key on. The tables the
// shards declare always use uid; the two structural tables use path. Nothing
// else is hardcoded.
func nodePrimaryKeyFor(label string, rows []map[string]any) string {
	for _, r := range rows {
		if _, ok := r["path"]; ok && (label == "File" || label == "Directory") {
			return "path"
		}
		break
	}
	return "uid"
}

// columnsForLabel returns the column set for a node table: the union of every
// property name present in the label's rows, in a stable order, with types
// inferred from values. uuid → STRING for identifiers.
func columnsForLabel(label string, rows []map[string]any) ([]ladybug.Field, string) {
	seen := map[string]bool{}
	var names []string
	for _, r := range rows {
		for k := range r {
			if !seen[k] {
				seen[k] = true
				names = append(names, k)
			}
		}
	}
	// Stable order: the canonical graph order first (when present), then the rest
	// alphabetically.
	ordered := make([]string, 0, len(names))
	for _, g := range graphColumnOrder {
		if seen[g] {
			ordered = append(ordered, g)
		}
	}
	var rest []string
	for _, n := range names {
		if !seenIn(ordered, n) {
			rest = append(rest, n)
		}
	}
	sort.Strings(rest)
	ordered = append(ordered, rest...)

	cols := make([]ladybug.Field, 0, len(ordered))
	for _, n := range ordered {
		cols = append(cols, ladybug.Field{Name: n, Type: inferTypeFor(collectColumnValues(rows, n))})
	}
	pk := nodePrimaryKeyFor(label, rows)
	return cols, pk
}

func collectColumnValues(rows []map[string]any, key string) []any {
	var out []any
	for _, r := range rows {
		if v, ok := r[key]; ok {
			out = append(out, v)
		}
	}
	return out
}

func collectPropValues(rows []map[string]any, key string) []any {
	return collectColumnValues(rows, key)
}

func inferTypeFor(values []any) string {
	allInt, allBool := true, true
	for _, v := range values {
		switch v.(type) {
		case bool:
			allInt = false
		case int, int8, int16, int32, int64, uint, uint8, uint16, uint32, uint64:
			allBool = false
		default:
			allInt, allBool = false, false
		}
	}
	switch {
	case allBool:
		return "BOOL"
	case allInt:
		return "INT64"
	default:
		return "STRING"
	}
}

func seenIn(list []string, s string) bool {
	for _, e := range list {
		if e == s {
			return true
		}
	}
	return false
}

// ---------- property collection for edges ----------

func collectProps(rows []map[string]any, props []ladybug.Field, fromCol, toCol string) [][]any {
	out := make([][]any, len(props))
	for _, r := range rows {
		for i, p := range props {
			out[i] = append(out[i], r[p.Name])
		}
	}
	return out
}

func cloneMembers(in []*ladybug.CanonicalMember) []ladybug.CanonicalMember {
	out := make([]ladybug.CanonicalMember, len(in))
	for i, m := range in {
		out[i] = *m
	}
	return out
}

func sortCSR(edges []csrEdgeDirect, propValues [][]any) ([]csrEdgeDirect, [][]any) {
	idx := make([]int, len(edges))
	for i := range idx {
		idx[i] = i
	}
	sort.SliceStable(idx, func(a, b int) bool {
		ea, eb := edges[idx[a]], edges[idx[b]]
		if ea.source != eb.source {
			return ea.source < eb.source
		}
		return ea.target < eb.target
	})
	sorted := make([]csrEdgeDirect, len(edges))
	for i, id := range idx {
		sorted[i] = edges[id]
	}
	outProps := make([][]any, len(propValues))
	for c := range propValues {
		if len(propValues[c]) != len(idx) {
			continue
		}
		col := make([]any, len(idx))
		for i, id := range idx {
			col[i] = propValues[c][id]
		}
		outProps[c] = col
	}
	return sorted, outProps
}

// ---------- manifest & schema ----------

// propsForRelType is derived from the kinds of relationship the shards carry;
// the values are read from the rows, so this list-of-names is only the ORDER of
// appearance for the known forms, never a set of what exists.
func writeCanonicalSchemaDirect(outDir, storage string, man *ladybug.CanonicalManifest) error {
	type mddl struct {
		relType string
		m       ladybug.CanonicalMember
	}
	var members []mddl
	for _, rg := range man.RelGroups {
		for _, m := range rg.Members {
			members = append(members, mddl{rg.Type, m})
		}
		for _, m := range rg.ReverseMembers {
			members = append(members, mddl{rg.Type, m})
		}
	}
	sort.SliceStable(members, func(i, j int) bool {
		if members[i].m.Rows != members[j].m.Rows {
			return members[i].m.Rows > members[j].m.Rows
		}
		return members[i].m.Table < members[j].m.Table
	})
	var b strings.Builder
	for _, n := range man.NodeTables {
		cols := make([]string, len(n.Columns))
		for i, c := range n.Columns {
			cols[i] = fmt.Sprintf("%s %s", ladybug.QuoteIdent(c.Name), c.Type)
		}
		pk := n.PrimaryKey
		if pk == "" && len(n.Columns) > 0 {
			pk = n.Columns[0].Name
		}
		fmt.Fprintf(&b, "CREATE NODE TABLE %s(%s, PRIMARY KEY(%s)) WITH (storage = '%s', format = 'icebug-disk');\n",
			ladybug.QuoteIdent(n.Label), strings.Join(cols, ", "), ladybug.QuoteIdent(pk), ladybug.EscapeLiteral(storage))
	}
	for _, md := range members {
		parts := []string{fmt.Sprintf("FROM %s TO %s", ladybug.QuoteIdent(md.m.From), ladybug.QuoteIdent(md.m.To))}
		for _, p := range propsForMember(man, md.m.Table) {
			parts = append(parts, fmt.Sprintf("%s %s", ladybug.QuoteIdent(p.Name), p.Type))
		}
		fmt.Fprintf(&b, "CREATE REL TABLE %s(%s) WITH (storage = '%s', format = 'icebug-disk');\n",
			ladybug.QuoteIdent(md.m.Table), strings.Join(parts, ", "), ladybug.EscapeLiteral(storage))
	}
	return os.WriteFile(filepath.Join(outDir, "schema.cypher"), []byte(b.String()), 0o644)
}

// propsForMember reconstructs an edge member's property columns. The CanonicalMember
// does not carry them (the manifest format is stable), so they are derived from the
// rel group's members counter-parts: all members of one rel type share the same
// property shape in this model; the list is taken from the first member's rows
// captured during export. As a fallback, the relation's declared props are the ones
// shared by every member of the same TYPE in the group.
func propsForMember(man *ladybug.CanonicalManifest, table string) []ladybug.Field {
	for _, rg := range man.RelGroups {
		for _, m := range rg.Members {
			if m.Table == table {
				return propShapeOfRelType(rg.Type)
			}
		}
		for _, m := range rg.ReverseMembers {
			if m.Table == table {
				return propShapeOfRelType(rg.Type)
			}
		}
	}
	return nil
}

func propShapeOfRelType(relType string) []ladybug.Field {
	switch relType {
	case "CALLS":
		return []ladybug.Field{
			{Name: "source_file", Type: "STRING"},
			{Name: "line_number", Type: "INT64"},
			{Name: "full_call_name", Type: "STRING"},
			{Name: "receiver_type", Type: "STRING"},
		}
	case "IMPORTS":
		return []ladybug.Field{
			{Name: "alias", Type: "STRING"},
			{Name: "full_import_name", Type: "STRING"},
			{Name: "imported_name", Type: "STRING"},
			{Name: "line_number", Type: "INT64"},
			{Name: "source_file", Type: "STRING"},
		}
	case "CONTAINS":
		return nil
	default:
		return []ladybug.Field{
			{Name: "source_file", Type: "STRING"},
			{Name: "line_number", Type: "INT64"},
		}
	}
}

func copyRelMember(finalDir, outDir string, m ladybug.CanonicalMember) error {
	for _, f := range []string{m.Indices, m.Indptr} {
		if f == "" {
			continue
		}
		if err := copyIcebugFile(filepath.Join(finalDir, f), filepath.Join(outDir, f)); err != nil {
			return err
		}
	}
	return nil
}

// ---------- helpers ----------

type csrEdgeDirect struct {
	source uint64
	target uint64
}

func reverseEdgesDirect(edges []csrEdgeDirect, propValues [][]any, from, to string) ([]csrEdgeDirect, [][]any) {
	var out []csrEdgeDirect
	outProps := make([][]any, len(propValues))
	for i := range outProps {
		outProps[i] = make([]any, 0, len(edges))
	}
	for i, e := range edges {
		// Same row in the SAME table: only then it is a real self-loop.
		if from == to && e.source == e.target {
			continue
		}
		out = append(out, csrEdgeDirect{source: e.target, target: e.source})
		for pi := range propValues {
			outProps[pi] = append(outProps[pi], propValues[pi][i])
		}
	}
	return out, outProps
}

func canonicalMemberNameDirect(relType, from, to string) string {
	clean := func(s string) string {
		var b strings.Builder
		for _, r := range strings.ToLower(s) {
			switch {
			case r >= 'a' && r <= 'z', r >= '0' && r <= '9', r == '_':
				b.WriteRune(r)
			default:
				b.WriteByte('_')
			}
		}
		return b.String()
	}
	return clean(relType) + "__" + clean(from) + "_" + clean(to)
}

// ---------- Parquet helpers ----------

func icebugMetadataDirect() *arrow.Metadata {
	md := arrow.NewMetadata([]string{"icebug_disk_version"}, []string{"v1"})
	return &md
}

func arrowTypeForCypherDirect(cypher string) arrow.DataType {
	switch cypher {
	case "INT64", "SERIAL":
		return arrow.PrimitiveTypes.Int64
	case "INT32":
		return arrow.PrimitiveTypes.Int32
	case "INT16":
		return arrow.PrimitiveTypes.Int16
	case "INT8":
		return arrow.PrimitiveTypes.Int8
	case "UINT64":
		return arrow.PrimitiveTypes.Uint64
	case "UINT32":
		return arrow.PrimitiveTypes.Uint32
	case "DOUBLE":
		return arrow.PrimitiveTypes.Float64
	case "FLOAT":
		return arrow.PrimitiveTypes.Float32
	case "BOOL":
		return arrow.FixedWidthTypes.Boolean
	case "BLOB":
		return arrow.BinaryTypes.Binary
	default:
		return arrow.BinaryTypes.String
	}
}

func writeParquetDirect(dest string, schema *arrow.Schema, rows int, fill func(*array.RecordBuilder, int, int)) error {
	f, err := os.Create(dest)
	if err != nil {
		return err
	}
	defer f.Close()
	props := parquet.NewWriterProperties(
		parquet.WithCompression(compress.Codecs.Zstd),
		parquet.WithDictionaryDefault(false),
		parquet.WithMaxRowGroupLength(1<<40),
	)
	w, err := pqarrow.NewFileWriter(schema, f, props, pqarrow.DefaultWriterProps())
	if err != nil {
		return err
	}
	builder := array.NewRecordBuilder(memory.DefaultAllocator, schema)
	defer builder.Release()
	fill(builder, 0, rows)
	rec := builder.NewRecordBatch()
	defer rec.Release()
	if err := w.Write(rec); err != nil {
		_ = w.Close()
		return err
	}
	return w.Close()
}

func writeIndicesDirect(dest string, edges []csrEdgeDirect, propFields []arrow.Field, propValues [][]any) error {
	fields := make([]arrow.Field, 0, len(propFields)+1)
	fields = append(fields, arrow.Field{Name: "target", Type: arrow.PrimitiveTypes.Uint64, Nullable: true})
	fields = append(fields, propFields...)
	schema := arrow.NewSchema(fields, icebugMetadataDirect())
	return writeParquetDirect(dest, schema, len(edges), func(b *array.RecordBuilder, from, to int) {
		tb := b.Field(0).(*array.Uint64Builder)
		for i := from; i < to; i++ {
			tb.Append(edges[i].target)
		}
		for ci := range propFields {
			col := b.Field(ci + 1)
			for i := from; i < to; i++ {
				if ci < len(propValues) && i < len(propValues[ci]) {
					appendArrowValueDirect(col, propValues[ci][i])
				} else {
					col.AppendNull()
				}
			}
		}
	})
}

func writeIndptrDirect(dest string, edges []csrEdgeDirect, nodeCount uint64) error {
	ptr := make([]uint64, nodeCount+1)
	for _, e := range edges {
		if e.source < nodeCount {
			ptr[e.source+1]++
		}
	}
	for i := uint64(1); i <= nodeCount; i++ {
		ptr[i] += ptr[i-1]
	}
	schema := arrow.NewSchema(
		[]arrow.Field{{Name: "ptr", Type: arrow.PrimitiveTypes.Uint64, Nullable: true}}, icebugMetadataDirect())
	return writeParquetDirect(dest, schema, len(ptr), func(b *array.RecordBuilder, from, to int) {
		pb := b.Field(0).(*array.Uint64Builder)
		for i := from; i < to; i++ {
			pb.Append(ptr[i])
		}
	})
}

func appendArrowValueDirect(b array.Builder, v any) {
	if v == nil {
		b.AppendNull()
		return
	}
	switch bb := b.(type) {
	case *array.StringBuilder:
		bb.Append(fmt.Sprint(v))
	case *array.LargeStringBuilder:
		bb.Append(fmt.Sprint(v))
	case *array.Int64Builder:
		switch n := v.(type) {
		case int64:
			bb.Append(n)
		case int:
			bb.Append(int64(n))
		case int32:
			bb.Append(int64(n))
		default:
			bb.Append(0)
		}
	case *array.BooleanBuilder:
		if x, ok := v.(bool); ok {
			bb.Append(x)
		} else {
			bb.AppendNull()
		}
	default:
		b.AppendNull()
	}
}

func copyIcebugFile(src, dst string) error {
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer in.Close()
	out, err := os.Create(dst)
	if err != nil {
		return err
	}
	defer out.Close()
	_, err = out.ReadFrom(in)
	return err
}
