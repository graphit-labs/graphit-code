package ladybugstore

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/apache/arrow-go/v18/arrow"
	"github.com/apache/arrow-go/v18/arrow/array"
)

const canonicalFormat = "icebug-canonical"

const canonicalSchemaFile = "schema.cypher"

// CanonicalManifestVersion is bumped when the manifest's shape changes in a way consumers
// must notice.
const CanonicalManifestVersion = 2

type CanonicalNodeTable struct {
	Label      string  `json:"label"`
	File       string  `json:"file"`
	Rows       int64   `json:"rows"`
	PrimaryKey string  `json:"primary_key"`
	Columns    []Field `json:"columns"`
}

type CanonicalMember struct {
	From    string `json:"from"`
	To      string `json:"to"`
	Table   string `json:"table"`
	Indices string `json:"indices"`
	Indptr  string `json:"indptr"`
	Rows    int64  `json:"rows"`
}

type CanonicalRelGroup struct {
	// Type is the LOGICAL relationship name public queries use (`CALLS`). It has no physical
	// table; the members below are what the engine mounts.
	Type           string            `json:"type"`
	Members        []CanonicalMember `json:"members"`
	ReverseMembers []CanonicalMember `json:"reverse_members,omitempty"`
}

type CanonicalInvariants struct {
	IndptrRowGroups int `json:"indptr_row_groups"`
	// SelfLoops documents where a source==target edge lives: once, in the forward member's
	// CSR. Mirrors never duplicate it.
	SelfLoops string `json:"self_loops"`
}

type CanonicalManifest struct {
	Version         int                  `json:"manifest_version"`
	Format          string               `json:"format"`
	Storage         string               `json:"storage"`
	Finished        bool                 `json:"finished"`
	Reverse         bool                 `json:"add_reverse_edges"`
	Schema          string               `json:"schema_file"`
	NodeTables      []CanonicalNodeTable `json:"node_tables"`
	RelGroups       []CanonicalRelGroup  `json:"relationships"`
	EdgeCount       int64                `json:"n_edges"`
	RepairedStrings int64                `json:"repaired_strings"`
	Invariants      CanonicalInvariants  `json:"invariants"`
}

// ExportIcebugCanonical writes the store as a CANONICAL icebug-disk directory: real node
// tables per label, one rel table per (type, from, to) pair over the real endpoints,
// optional `<member>_reverse` mirrors, schema.cypher, and the v2 manifest mapping logical
// types back to their member tables.
func ExportIcebugCanonical(c Conn, outDir string, opts IcebugOptions) (*CanonicalManifest, error) {
	if err := os.MkdirAll(outDir, 0o755); err != nil {
		return nil, fmt.Errorf("icebug: dir: %w", err)
	}
	utf8Repairs.Store(0)

	tables, err := listTables(c)
	if err != nil {
		return nil, err
	}
	labels := make([]string, 0, len(tables))
	relTypes := make([]string, 0, len(tables))
	for _, tb := range tables {
		if tb.kind == "NODE" {
			labels = append(labels, tb.name)
		} else {
			relTypes = append(relTypes, tb.name)
		}
	}
	sort.Strings(labels)
	sort.Strings(relTypes)

	man := &CanonicalManifest{
		Version: CanonicalManifestVersion,
		Format:  canonicalFormat,
		Storage: opts.StorageURI,
		Schema:  canonicalSchemaFile,
		Reverse: opts.reverseEdgesEnabled(),
		Invariants: CanonicalInvariants{
			IndptrRowGroups: 1,
			SelfLoops:       "forward-once",
		},
	}

	labelIDs := map[string]map[int64]uint64{}

	for _, label := range labels {
		node, ids, err := exportCanonicalNodes(c, outDir, label)
		if err != nil {
			return nil, err
		}
		if node.Rows == 0 {
			continue
		}
		man.NodeTables = append(man.NodeTables, *node)
		labelIDs[label] = ids
	}

	usedMembers := map[string]string{}

	var ddlMembers []canonicalMemberDDL

	for _, relType := range relTypes {
		pairs, err := connectionsOf(c, relType)
		if err != nil {
			return nil, err
		}
		props, _, err := icebugColumns(c, relType)
		if err != nil {
			return nil, err
		}
		group := CanonicalRelGroup{Type: relType}
		for _, p := range pairs {
			srcIDs, okS := labelIDs[p.src]
			dstIDs, okD := labelIDs[p.dst]
			if !okS || !okD {
				continue
			}
			for _, m := range []*struct{ from, to string }{{p.src, p.dst}, {p.dst, p.src}} {
				name := canonicalMemberName(relType, m.from, m.to)
				if prev, seen := usedMembers[name]; seen && prev != m.from+"->"+m.to {
					return nil, fmt.Errorf("icebug: canonical members %q collide on table name %q", prev+"->"+m.to+" / "+m.from+"->"+m.to, name)
				}
				usedMembers[name] = m.from + "->" + m.to
			}
			member, rev, err := exportCanonicalPair(c, outDir, relType,
				p.src, p.dst, props, srcIDs, dstIDs, len(labelIDs[p.src]), len(labelIDs[p.dst]), opts)
			if err != nil {
				return nil, err
			}
			if member == nil {
				continue
			}
			group.Members = append(group.Members, *member)
			if rev != nil {
				group.ReverseMembers = append(group.ReverseMembers, *rev)
			}
			man.EdgeCount += member.Rows
			for _, m := range []*CanonicalMember{member, rev} {
				if m == nil {
					continue
				}
				ddlMembers = append(ddlMembers, canonicalMemberDDL{
					stmt: canonicalRelDDL(m, props, opts.StorageURI),
					rows: m.Rows,
					name: m.Table,
				})
			}
		}
		if len(group.Members) > 0 {
			man.RelGroups = append(man.RelGroups, group)
		}
	}

	if err := writeCanonicalSchema(outDir, man.Storage, man.NodeTables, ddlMembers); err != nil {
		return nil, err
	}
	man.RepairedStrings = utf8Repairs.Load()
	man.Finished = true
	raw, err := json.MarshalIndent(man, "", "  ")
	if err != nil {
		return nil, fmt.Errorf("icebug: manifest: %w", err)
	}
	if err := os.WriteFile(filepath.Join(outDir, IcebugManifestFile), raw, 0o644); err != nil {
		return nil, fmt.Errorf("icebug: write manifest: %w", err)
	}
	return man, nil
}

func exportCanonicalNodes(c Conn, outDir, label string) (*CanonicalNodeTable, map[int64]uint64, error) {
	cols, pk, err := icebugColumns(c, label)
	if err != nil {
		return nil, nil, fmt.Errorf("icebug: columns %s: %w", label, err)
	}
	proj := []string{"offset(id(n)) AS " + QuoteIdent(icebugOffsetAlias)}
	for _, f := range cols {
		proj = append(proj, fmt.Sprintf("n.%s AS %s", QuoteIdent(f.Name), QuoteIdent(f.Name)))
	}
	rows, err := c.Query(fmt.Sprintf("MATCH (n:%s) RETURN %s ORDER BY %s",
		QuoteIdent(label), strings.Join(proj, ", "), QuoteIdent(icebugOffsetAlias)), nil)
	if err != nil {
		return nil, nil, fmt.Errorf("icebug: read nodes of %s: %w", label, err)
	}

	ids := make(map[int64]uint64, len(rows))
	values := make([]map[string]any, 0, len(rows))
	for _, r := range rows {
		off := Int64(r[icebugOffsetAlias])
		if _, clash := ids[off]; clash {
			return nil, nil, fmt.Errorf("icebug: %s returned internal offset %d twice — the store is being written while it is read",
				label, off)
		}
		ids[off] = uint64(len(values))
		row := make(map[string]any, len(cols))
		for _, f := range cols {
			row[f.Name] = r[f.Name]
		}
		values = append(values, row)
	}

	fields := make([]arrow.Field, len(cols))
	for ci, f := range cols {
		fields[ci] = arrow.Field{Name: f.Name, Type: arrowTypeForCypher(f.Type), Nullable: true}
	}
	file := "nodes_" + label + ".parquet"
	schema := arrow.NewSchema(fields, icebugMetadata())
	if err := writeParquet(filepath.Join(outDir, file), schema, len(values),
		func(b *array.RecordBuilder, from, to int) {
			for i := from; i < to; i++ {
				for ci, f := range cols {
					appendArrowValue(b.Field(ci), values[i][f.Name])
				}
			}
		}); err != nil {
		return nil, nil, fmt.Errorf("icebug: write %s: %w", file, err)
	}
	return &CanonicalNodeTable{
		Label:      label,
		File:       file,
		Rows:       int64(len(values)),
		PrimaryKey: pk,
		Columns:    cols,
	}, ids, nil
}

func exportCanonicalPair(c Conn, outDir, relType, from, to string, props []Field,
	srcIDs, dstIDs map[int64]uint64, nsrc, ndst int,
	opts IcebugOptions) (*CanonicalMember, *CanonicalMember, error) {

	pattern := fmt.Sprintf("MATCH (a:%s)-[r:%s]->(b:%s)",
		QuoteIdent(from), QuoteIdent(relType), QuoteIdent(to))
	proj := []string{"offset(id(a)) AS __src", "offset(id(b)) AS __dst"}
	for _, f := range props {
		proj = append(proj, fmt.Sprintf("r.%s AS %s", QuoteIdent(f.Name), QuoteIdent(f.Name)))
	}
	rows, err := c.Query(pattern+" RETURN "+strings.Join(proj, ", "), nil)
	if err != nil {
		return nil, nil, fmt.Errorf("icebug: read %s %s->%s: %w", relType, from, to, err)
	}

	var edges []csrEdge
	propValues := make([][]any, len(props))
	for _, r := range rows {
		s, okS := srcIDs[Int64(r["__src"])]
		d, okD := dstIDs[Int64(r["__dst"])]
		if !okS || !okD {
			continue
		}
		edges = append(edges, csrEdge{s, d})
		for ci, f := range props {
			propValues[ci] = append(propValues[ci], r[f.Name])
		}
	}
	if len(edges) == 0 {
		return nil, nil, nil
	}
	sortEdges(&edges, propValues)

	fwdName := canonicalMemberName(relType, from, to)
	indicesFile := "indices_" + fwdName + ".parquet"
	indptrFile := "indptr_" + fwdName + ".parquet"
	if err := writeIndices(filepath.Join(outDir, indicesFile), edges, arrowFieldsFor(props), propValues); err != nil {
		return nil, nil, fmt.Errorf("icebug: indices %s: %w", fwdName, err)
	}
	if err := writeIndptr(filepath.Join(outDir, indptrFile), edges, uint64(nsrc)); err != nil {
		return nil, nil, fmt.Errorf("icebug: indptr %s: %w", fwdName, err)
	}
	fwd := &CanonicalMember{
		From: from, To: to, Table: fwdName,
		Indices: indicesFile, Indptr: indptrFile,
		Rows: int64(len(edges)),
	}
	if !opts.reverseEdgesEnabled() {
		return fwd, nil, nil
	}

	revEdges, revProps := reverseEdges(edges, propValues)
	sortEdges(&revEdges, revProps)
	revName := fwdName + "_reverse"
	revIndices := "indices_" + revName + ".parquet"
	revIndptr := "indptr_" + revName + ".parquet"
	if err := writeIndices(filepath.Join(outDir, revIndices), revEdges, arrowFieldsFor(props), revProps); err != nil {
		return nil, nil, fmt.Errorf("icebug: indices %s: %w", revName, err)
	}
	if err := writeIndptr(filepath.Join(outDir, revIndptr), revEdges, uint64(ndst)); err != nil {
		return nil, nil, fmt.Errorf("icebug: indptr %s: %w", revName, err)
	}
	rev := &CanonicalMember{
		From: to, To: from, Table: revName,
		Indices: revIndices, Indptr: revIndptr,
		Rows: int64(len(revEdges)),
	}
	return fwd, rev, nil
}

func arrowFieldsFor(props []Field) []arrow.Field {
	fields := make([]arrow.Field, len(props))
	for i, f := range props {
		fields[i] = arrow.Field{Name: f.Name, Type: arrowTypeForCypher(f.Type), Nullable: true}
	}
	return fields
}

func canonicalMemberName(relType, from, to string) string {
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

func canonicalNodeDDL(n CanonicalNodeTable, storage string) string {
	cols := make([]string, 0, len(n.Columns))
	for _, f := range n.Columns {
		cols = append(cols, fmt.Sprintf("%s %s", QuoteIdent(f.Name), f.Type))
	}
	pk := n.PrimaryKey
	if pk == "" && len(n.Columns) > 0 {
		pk = n.Columns[0].Name
	}
	return fmt.Sprintf("CREATE NODE TABLE %s(%s, PRIMARY KEY(%s)) WITH (storage = '%s', format = 'icebug-disk');\n",
		QuoteIdent(n.Label), strings.Join(cols, ", "), QuoteIdent(pk), storage)
}

func canonicalRelDDL(m *CanonicalMember, props []Field, storage string) string {
	parts := []string{fmt.Sprintf("FROM %s TO %s", QuoteIdent(m.From), QuoteIdent(m.To))}
	for _, f := range props {
		parts = append(parts, fmt.Sprintf("%s %s", QuoteIdent(f.Name), f.Type))
	}
	return fmt.Sprintf("CREATE REL TABLE %s(%s) WITH (storage = '%s', format = 'icebug-disk');\n",
		QuoteIdent(m.Table), strings.Join(parts, ", "), storage)
}

// writeCanonicalSchema emits node DDL first (labels sorted, matching enumeration order),
// then every member ordered largest-first — creation order is table-id order, and the
// upstream alternatives defect bounds any union by its lowest-id member, so the biggest
// member must carry the lowest id even in the canonical layout.
type canonicalMemberDDL struct {
	stmt string
	rows int64
	name string
}

func writeCanonicalSchema(outDir, storage string, nodes []CanonicalNodeTable, members []canonicalMemberDDL) error {
	sort.SliceStable(members, func(i, j int) bool {
		if members[i].rows != members[j].rows {
			return members[i].rows > members[j].rows
		}
		return members[i].name < members[j].name
	})
	var b strings.Builder
	for _, n := range nodes {
		b.WriteString(canonicalNodeDDL(n, storage))
	}
	for _, m := range members {
		b.WriteString(m.stmt)
	}
	return os.WriteFile(filepath.Join(outDir, canonicalSchemaFile), []byte(b.String()), 0o644)
}
