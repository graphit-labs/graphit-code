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

const maxRowGroupRows = 1 << 40

const parquetChunkRows = 64 << 10

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

	var labelIDs = map[string]map[string]uint64{}

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

	labels := append([]string{}, ri.labels...)
	labels = append(labels, "File", "Directory")
	if ri.hasParams {
		labels = append(labels, "Parameter")
	}
	if ri.hasFields {
		labels = append(labels, "Field")
	}
	labels = append(labels, ri.annotationKinds...)

	type collectedNodeTable struct {
		label string
		table *nodeColumns
	}
	seenLabel := map[string]bool{}
	var collected []collectedNodeTable
	for _, label := range labels {
		if seenLabel[label] {
			continue
		}
		seenLabel[label] = true
		table := newNodeColumns()
		streamNodeRowsFor(ri, label, table.appendRow)
		if table.rows == 0 && label != "File" {
			continue
		}
		collected = append(collected, collectedNodeTable{label: label, table: table})
	}

	type nodeWriteResult struct {
		label string
		ids   map[string]uint64
		row   ladybug.CanonicalNodeTable
		err   error
	}
	writeCollectedNodeTable := func(c collectedNodeTable) nodeWriteResult {
		res := nodeWriteResult{label: c.label}
		cols, columns, pk := c.table.fields(c.label)
		order, keys := c.table.sortedOrder(pk)
		ids := make(map[string]uint64, c.table.rows)
		for i, at := range order {
			ids[keys[at]] = uint64(i)
		}
		res.ids = ids

		fields := make([]arrow.Field, len(cols))
		for i, col := range cols {
			fields[i] = arrow.Field{Name: col.Name, Type: arrowTypeForCypherDirect(col.Type), Nullable: true}
		}
		file := "nodes_" + c.label + ".parquet"
		schema := arrow.NewSchema(fields, icebugMetadataDirect())
		if err := writeParquetDirect(filepath.Join(outDir, file), schema, c.table.rows,
			func(bld *array.RecordBuilder, from, to int) {
				for ci, col := range columns {
					builder := bld.Field(ci)
					for i := from; i < to; i++ {
						col.appendTo(builder, int(order[i]))
					}
				}
			}); err != nil {
			res.err = fmt.Errorf("write nodes %s: %w", c.label, err)
			return res
		}
		res.row = ladybug.CanonicalNodeTable{
			Label: c.label, File: file, Rows: int64(c.table.rows), PrimaryKey: pk, Columns: cols,
		}
		return res
	}

	var firstNodeErr error
	parallelForEach(collected, SafeWorkers(0), writeCollectedNodeTable, func(res nodeWriteResult) {
		if res.err != nil {
			if firstNodeErr == nil {
				firstNodeErr = res.err
			}
			return
		}
		labelIDs[res.label] = res.ids
		man.NodeTables = append(man.NodeTables, res.row)
	})
	if firstNodeErr != nil {
		return nil, firstNodeErr
	}
	sort.Slice(man.NodeTables, func(i, j int) bool { return man.NodeTables[i].Label < man.NodeTables[j].Label })

	relMembers := map[string][]*ladybug.CanonicalMember{}
	relReverse := map[string][]*ladybug.CanonicalMember{}
	usedMembers := map[string]string{}

	type relJob struct {
		idx                               int
		relType, from, to, fromCol, toCol string
		stream                            func(emit func(map[string]any))
	}
	type relResult struct {
		idx               int
		relType, from, to string
		fwdName           string
		fwd, rev          *ladybug.CanonicalMember
		err               error
	}

	computeRelJob := func(job relJob) relResult {
		res := relResult{idx: job.idx, relType: job.relType, from: job.from, to: job.to}
		fromIDs, ok1 := labelIDs[job.from]
		toIDs, ok2 := labelIDs[job.to]
		if !ok1 || !ok2 {
			return res
		}
		// The property columns are accumulated ALONGSIDE the edge they belong to, and
		// only for the rows whose endpoints resolved. A second pass over the rows would
		// include the ones skipped here, and every property from the first skipped row
		// on would land on the wrong edge. The rows themselves are never retained.
		var edges []csrEdgeDirect
		propTable := newNodeColumns()
		job.stream(func(r map[string]any) {
			s, okS := fromIDs[fmt.Sprint(r[job.fromCol])]
			t, okT := toIDs[fmt.Sprint(r[job.toCol])]
			if !okS || !okT {
				return
			}
			edges = append(edges, csrEdgeDirect{source: s, target: t})
			propTable.appendRowExcept(r, job.fromCol, job.toCol)
		})
		if len(edges) == 0 {
			return res
		}
		props, propColumns := propTable.sortedFields()

		res.fwdName = canonicalMemberNameDirect(job.relType, job.from, job.to)
		fwdCSR := csrMemberDirect{edges: edges, props: propColumns}
		fwdCSR.order = csrOrderDirect(edges)
		indicesFile := "indices_" + res.fwdName + ".parquet"
		indptrFile := "indptr_" + res.fwdName + ".parquet"
		propArrowFields := make([]arrow.Field, len(props))
		for i, p := range props {
			propArrowFields[i] = arrow.Field{Name: p.Name, Type: arrowTypeForCypherDirect(p.Type), Nullable: true}
		}
		if err := writeIndicesDirect(filepath.Join(outDir, indicesFile), fwdCSR, propArrowFields); err != nil {
			res.err = err
			return res
		}
		if err := writeIndptrDirect(filepath.Join(outDir, indptrFile), fwdCSR.edges, uint64(len(fromIDs))); err != nil {
			res.err = err
			return res
		}
		res.fwd = &ladybug.CanonicalMember{
			From: job.from, To: job.to, Table: res.fwdName,
			Indices: indicesFile, Indptr: indptrFile,
			Rows: int64(len(fwdCSR.edges)),
		}

		if !reverse {
			return res
		}
		revCSR := reverseMemberDirect(fwdCSR, job.from == job.to)
		if len(revCSR.edges) == 0 {
			return res
		}
		revName := res.fwdName + "_reverse"
		revIndices := "indices_" + revName + ".parquet"
		revIndptr := "indptr_" + revName + ".parquet"
		if err := writeIndicesDirect(filepath.Join(outDir, revIndices), revCSR, propArrowFields); err != nil {
			res.err = err
			return res
		}
		if err := writeIndptrDirect(filepath.Join(outDir, revIndptr), revCSR.edges, uint64(len(toIDs))); err != nil {
			res.err = err
			return res
		}
		res.rev = &ladybug.CanonicalMember{
			From: job.to, To: job.from, Table: revName,
			Indices: revIndices, Indptr: revIndptr,
			Rows: int64(len(revCSR.edges)),
		}
		return res
	}

	// Every job below only reads ri and writes files unique to itself, so the SET of
	// them runs in parallel below — what must stay sequential is the ORDER results are
	// merged in: usedMembers' collision check and every Members slice's insertion
	// order are meaningful (see the sort comment near man.RelGroups further down), so
	// `jobs` is built in EXACTLY the order the old sequential code called exportRel,
	// and the merge loop after parallelForEach replays that same order regardless of
	// which goroutine happens to finish first.
	var jobs []relJob
	addJob := func(relType, from, to, fromCol, toCol string, stream func(emit func(map[string]any))) {
		jobs = append(jobs, relJob{idx: len(jobs), relType: relType, from: from, to: to, fromCol: fromCol, toCol: toCol, stream: stream})
	}

	if ri.hasParams {
		for _, owner := range ri.paramOwnerLabels {
			addJob("HAS_PARAMETER", owner, "Parameter", "func_uid", "uid", func(emit func(map[string]any)) { ri.streamParamEdges(owner, emit) })
		}
	}
	for _, pt := range ri.labels {
		addJob("HAS_FIELD", pt, "Field", "parent_uid", "uid", func(emit func(map[string]any)) { ri.streamFieldEdges(pt, emit) })
	}
	for _, kind := range ri.annotationKinds {
		edgeName := "HAS_" + strings.ToUpper(kind)
		for _, ol := range ri.decoratorOwnerLabels {
			if !ri.labelSet[ol] {
				continue
			}
			addJob(edgeName, ol, kind, "entity_uid", "annotation_uid", func(emit func(map[string]any)) { ri.streamAnnotationEdges(kind, ol, emit) })
		}
	}
	if ri.hasImports {
		addJob("IMPORTS", "File", "Module", "file_uid", "module_uid", ri.streamImportEdges)
	}
	for _, cl := range ri.callerLabels {
		if !ri.canWriteCallerLabel(cl) {
			continue
		}
		for _, tl := range ri.calleeLabels {
			addJob("CALLS", cl, tl, "caller_uid", "callee_uid", func(emit func(map[string]any)) { ri.streamCallEdges(cl, tl, emit) })
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
			addJob("INHERITS", from, to, "child_uid", "parent_uid", func(emit func(map[string]any)) { ri.streamInheritEdges("INHERITS", from, to, emit) })
			addJob("IMPLEMENTS", from, to, "child_uid", "parent_uid", func(emit func(map[string]any)) { ri.streamInheritEdges("IMPLEMENTS", from, to, emit) })
		}
	}
	if ri.labelSet[LabelField] {
		for _, src := range ri.fieldAccessSourceLabels {
			addJob("READS_FIELD", src, "Field", "source_uid", "field_uid", func(emit func(map[string]any)) { ri.streamFieldAccessEdges(false, src, emit) })
			addJob("WRITES_FIELD", src, "Field", "source_uid", "field_uid", func(emit func(map[string]any)) { ri.streamFieldAccessEdges(true, src, emit) })
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
				addJob(rt, src, tgt, "source_uid", "target_uid", func(emit func(map[string]any)) { ri.streamDMLEdges(rt, src, tgt, emit) })
			}
		}
		for _, tgt := range ri.dmlTargetLabels {
			addJob(rt, LabelFile, tgt, "source_uid", "target_uid", func(emit func(map[string]any)) { ri.streamDMLEdges(rt, LabelFile, tgt, emit) })
		}
	}
	for _, label := range ri.labels {
		addJob("CONTAINS", "File", label, "path", "uid", func(emit func(map[string]any)) { ri.streamContainsFileEntity(label, emit) })
	}
	addJob("CONTAINS", "Directory", "Directory", "parent_dir", "child_dir", ri.streamContainsDirDir)
	addJob("CONTAINS", "Directory", "File", "parent_dir", "file_path", ri.streamContainsDirFile)
	for _, eg := range ri.containsPairs {
		addJob("CONTAINS", eg[0], eg[1], "parent_uid", "child_uid", func(emit func(map[string]any)) { ri.streamContainsEntity(eg[0], eg[1], emit) })
	}

	results := make([]relResult, len(jobs))
	parallelForEach(jobs, SafeWorkers(0), computeRelJob, func(res relResult) { results[res.idx] = res })

	for _, res := range results {
		if res.err != nil {
			return nil, res.err
		}
	}
	for _, res := range results {
		if res.fwd == nil {
			continue
		}
		if prev, seen := usedMembers[res.fwdName]; seen && prev != res.from+"->"+res.to {
			return nil, fmt.Errorf("canonical members collide on %q", res.fwdName)
		}
		usedMembers[res.fwdName] = res.from + "->" + res.to
		relMembers[res.relType] = append(relMembers[res.relType], res.fwd)
		man.EdgeCount += res.fwd.Rows
		if res.rev != nil {
			relReverse[res.relType] = append(relReverse[res.relType], res.rev)
		}
	}

	relTypes := make([]string, 0, len(relMembers))
	for relType := range relMembers {
		relTypes = append(relTypes, relType)
	}
	sort.Strings(relTypes)
	for _, relType := range relTypes {
		man.RelGroups = append(man.RelGroups, ladybug.CanonicalRelGroup{
			Type:           relType,
			Members:        cloneMembers(relMembers[relType]),
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
		return ExportDirectFromRebuildIndexWithReverse(ri, outDir, storageURI, reverse)
	}
	var oldMan ladybug.CanonicalManifest
	if json.Unmarshal(oldRaw, &oldMan) != nil || oldMan.Format != "icebug-canonical" {
		return ExportDirectFromRebuildIndexWithReverse(ri, outDir, storageURI, reverse)
	}
	if len(deleted) > 0 {
		return ExportDirectFromRebuildIndexWithReverse(ri, outDir, storageURI, reverse)
	}
	changedSet := make(map[string]bool, len(changed))
	for _, p := range changed {
		changedSet[p] = true
	}
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

	restoreEmitState := ri.detachEmitState()
	for _, nt := range oldMan.NodeTables {
		if _, ok := labelInBatches(ri, nt.Label); !ok {
			affectedLabels[nt.Label] = true
		}
	}
	restoreEmitState()

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

	if len(affectedLabels) > len(oldMan.NodeTables)/2 && len(affectedLabels) > 4 {
		return ExportDirectFromRebuildIndexWithReverse(ri, outDir, storageURI, reverse)
	}

	return exportDirectDelta(ri, outDir, finalDir, storageURI, affectedLabels, affectedRels, oldMan, reverse)
}

func exportDirectDelta(ri *rebuildIndex, outDir, finalDir, storageURI string, affectedLabels, affectedRels map[string]bool, oldMan ladybug.CanonicalManifest, reverse bool) (*ladybug.CanonicalManifest, error) {
	if err := os.MkdirAll(outDir, 0o755); err != nil {
		return nil, fmt.Errorf("icebug delta: mkdir out: %w", err)
	}

	scratch := outDir + ".scratch"
	_ = os.RemoveAll(scratch)
	defer func() { _ = os.RemoveAll(scratch) }()
	fresh, err := ExportDirectFromRebuildIndexWithReverse(ri, scratch, storageURI, reverse)
	if err != nil {
		return nil, err
	}

	oldNode := map[string]ladybug.CanonicalNodeTable{}
	for _, nt := range oldMan.NodeTables {
		oldNode[nt.Label] = nt
	}
	newNode := map[string]ladybug.CanonicalNodeTable{}
	for _, nt := range fresh.NodeTables {
		newNode[nt.Label] = nt
	}

	oldRel := map[string]ladybug.CanonicalRelGroup{}
	for _, rg := range oldMan.RelGroups {
		oldRel[rg.Type] = rg
	}
	newRel := map[string]ladybug.CanonicalRelGroup{}
	for _, rg := range fresh.RelGroups {
		newRel[rg.Type] = rg
	}

	survivingNodes := []ladybug.CanonicalNodeTable{}
	for _, nt := range fresh.NodeTables {
		if affectedLabels[nt.Label] {
			if err := copyIcebugFile(filepath.Join(scratch, nt.File), filepath.Join(outDir, nt.File)); err != nil {
				return nil, fmt.Errorf("publishing regenerated table %s: %w", nt.Label, err)
			}
			survivingNodes = append(survivingNodes, nt)
			continue
		}
		if oldNT, ok := oldNode[nt.Label]; ok {
			if err := copyIcebugFile(filepath.Join(finalDir, oldNT.File), filepath.Join(outDir, nt.File)); err != nil {
				return nil, err
			}
			survivingNodes = append(survivingNodes, oldNT)
			continue
		}
		if err := copyIcebugFile(filepath.Join(scratch, nt.File), filepath.Join(outDir, nt.File)); err != nil {
			return nil, fmt.Errorf("publishing new table %s: %w", nt.Label, err)
		}
		survivingNodes = append(survivingNodes, nt)
	}
	for _, nt := range oldMan.NodeTables {
		if _, still := newNode[nt.Label]; !still {
			if affectedLabels[nt.Label] {
				continue
			}
			if err := copyIcebugFile(filepath.Join(finalDir, nt.File), filepath.Join(outDir, nt.File)); err != nil {
				return nil, err
			}
			survivingNodes = append(survivingNodes, nt)
		}
	}

	var survivingGroups []ladybug.CanonicalRelGroup
	for _, rg := range fresh.RelGroups {
		out := ladybug.CanonicalRelGroup{Type: rg.Type}
		if affectedRels[rg.Type] {
			for _, m := range rg.Members {
				if err := copyRelMember(scratch, outDir, m); err != nil {
					return nil, fmt.Errorf("publishing regenerated %s member: %w", rg.Type, err)
				}
			}
			for _, m := range rg.ReverseMembers {
				if err := copyRelMember(scratch, outDir, m); err != nil {
					return nil, fmt.Errorf("publishing regenerated %s reverse member: %w", rg.Type, err)
				}
			}
			out.Members = rg.Members
			out.ReverseMembers = rg.ReverseMembers
		} else {
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
				for _, m := range rg.Members {
					if err := copyRelMember(scratch, outDir, m); err != nil {
						return nil, fmt.Errorf("publishing new %s member: %w", rg.Type, err)
					}
				}
				for _, m := range rg.ReverseMembers {
					if err := copyRelMember(scratch, outDir, m); err != nil {
						return nil, fmt.Errorf("publishing new %s reverse member: %w", rg.Type, err)
					}
				}
				out.Members = rg.Members
				out.ReverseMembers = rg.ReverseMembers
			}
		}
		survivingGroups = append(survivingGroups, out)
	}
	for _, rg := range oldMan.RelGroups {
		if _, ok := newRel[rg.Type]; ok {
			continue
		}
		if affectedRels[rg.Type] {
			continue
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
	var found bool
	streamNodeRowsFor(ri, label, func(map[string]any) { found = true })
	return nil, found
}

func streamNodeRowsFor(ri *rebuildIndex, label string, emit func(map[string]any)) {
	switch label {
	case "File":
		ri.streamFileNodes(emit)
		return
	case "Directory":
		ri.streamDirNodes(nil, "", emit)
		return
	case "Module":
		if ri.hasImports {
			ri.streamModules(emit)
		}
		return
	}
	for _, kind := range ri.annotationKinds {
		if label == kind {
			ri.streamAnnotationNodes(kind, emit)
			return
		}
	}
	ri.streamEntities(label, emit)
	switch label {
	case "Function":
		ri.streamStubFunctions(emit)
	case "Class":
		if ri.labelSet["Class"] {
			ri.streamStubClasses(emit)
		}
	case "Interface":
		if ri.labelSet["Interface"] {
			ri.streamStubInterfaces(emit)
		}
	case LabelField:
		if ri.labelSet[LabelField] {
			ri.streamStubFields(emit)
		}
	case "Table":
		if ri.labelSet["Table"] {
			ri.streamStubTables(emit)
		}
	}
}

var graphColumnOrder = []string{
	"uid", "name", "path", "relative_path", "line_number", "end_line", "docstring",
	"lang", "cyclomatic_complexity", "context", "context_type", "class_context",
	"is_dependency", "is_depend", "is_exported", "value", "is_stub", "cluster",
	"full_import_name", "alias", "imported_name", "source_file",
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

func cloneMembers(in []*ladybug.CanonicalMember) []ladybug.CanonicalMember {
	out := make([]ladybug.CanonicalMember, len(in))
	for i, m := range in {
		out[i] = *m
	}
	return out
}

type csrMemberDirect struct {
	edges  []csrEdgeDirect
	props  []*nodeColumn
	propAt []int32
	order  []int32
}

func (m csrMemberDirect) propRow(edge int32) int32 {
	if m.propAt == nil {
		return edge
	}
	return m.propAt[edge]
}

func csrOrderDirect(edges []csrEdgeDirect) []int32 {
	order := make([]int32, len(edges))
	for i := range order {
		order[i] = int32(i)
	}
	sort.SliceStable(order, func(a, b int) bool {
		ea, eb := edges[order[a]], edges[order[b]]
		if ea.source != eb.source {
			return ea.source < eb.source
		}
		return ea.target < eb.target
	})
	return order
}

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

func copyRelMember(srcDir, outDir string, m ladybug.CanonicalMember) error {
	for _, f := range []string{m.Indices, m.Indptr} {
		if f == "" {
			continue
		}
		if err := copyIcebugFile(filepath.Join(srcDir, f), filepath.Join(outDir, f)); err != nil {
			return err
		}
	}
	return nil
}

type csrEdgeDirect struct {
	source uint64
	target uint64
}

func reverseMemberDirect(m csrMemberDirect, sameTable bool) csrMemberDirect {
	rev := csrMemberDirect{
		props:  m.props,
		edges:  make([]csrEdgeDirect, 0, len(m.edges)),
		propAt: make([]int32, 0, len(m.edges)),
	}
	for i, e := range m.edges {
		if sameTable && e.source == e.target {
			continue
		}
		rev.edges = append(rev.edges, csrEdgeDirect{source: e.target, target: e.source})
		rev.propAt = append(rev.propAt, m.propRow(int32(i)))
	}
	rev.order = csrOrderDirect(rev.edges)
	return rev
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
		parquet.WithMaxRowGroupLength(maxRowGroupRows),
	)
	w, err := pqarrow.NewFileWriter(schema, f, props, pqarrow.DefaultWriterProps())
	if err != nil {
		return err
	}
	builder := array.NewRecordBuilder(memory.DefaultAllocator, schema)
	defer builder.Release()

	writeChunk := func(from, to int) error {
		fill(builder, from, to)
		rec := builder.NewRecordBatch()
		defer rec.Release()
		return w.WriteBuffered(rec)
	}
	for from := 0; from < rows; from += parquetChunkRows {
		to := from + parquetChunkRows
		if to > rows {
			to = rows
		}
		if err := writeChunk(from, to); err != nil {
			_ = w.Close()
			return err
		}
	}
	if rows == 0 {
		if err := writeChunk(0, 0); err != nil {
			_ = w.Close()
			return err
		}
	}
	return w.Close()
}

func writeIndicesDirect(dest string, m csrMemberDirect, propFields []arrow.Field) error {
	fields := make([]arrow.Field, 0, len(propFields)+1)
	fields = append(fields, arrow.Field{Name: "target", Type: arrow.PrimitiveTypes.Uint64, Nullable: true})
	fields = append(fields, propFields...)
	schema := arrow.NewSchema(fields, icebugMetadataDirect())
	return writeParquetDirect(dest, schema, len(m.edges), func(b *array.RecordBuilder, from, to int) {
		tb := b.Field(0).(*array.Uint64Builder)
		for i := from; i < to; i++ {
			tb.Append(m.edges[m.order[i]].target)
		}
		for ci := range propFields {
			col := b.Field(ci + 1)
			for i := from; i < to; i++ {
				row := int(m.propRow(m.order[i]))
				if ci < len(m.props) && row < m.props[ci].len() {
					m.props[ci].appendTo(col, row)
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
