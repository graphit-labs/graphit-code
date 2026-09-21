package ast

import (
	"context"
	"fmt"
	"sort"
	"strings"

	ladybug "github.com/graphit-labs/graphit-code/internal/ladybugstore"
)

// Canonical reverse tables are traversal accelerators, not additional edges.
// Scan each forward member separately: Icebug 0.19 also reuses a Parquet reader
// incorrectly when a wildcard scan switches physical relationship tables.
func queryGraphEdgeSample(ctx context.Context, db GraphDB) (*QueryResult, error) {
	provider, ok := db.(canonicalManifestProvider)
	if !ok {
		return querySample(ctx, db, defaultGraphEdgeQuery, graphEdgeSampleQuery(false))
	}
	manifest, ok := provider.canonicalManifestSnapshot()
	if !ok {
		return querySample(ctx, db, defaultGraphEdgeQuery, graphEdgeSampleQuery(false))
	}
	type member struct{ from, table, to string }
	var members []member
	for _, group := range manifest.RelGroups {
		for _, m := range group.Members {
			if m.Rows > 0 {
				members = append(members, member{m.From, m.Table, m.To})
			}
		}
	}
	sort.Slice(members, func(i, j int) bool { return members[i].table < members[j].table })
	result := &QueryResult{}
	quote := func(s string) string { return "`" + strings.ReplaceAll(s, "`", "``") + "`" }
	for _, m := range members {
		remaining := graphSampleEdges - len(result.Records)
		if remaining <= 0 {
			break
		}
		if err := ctx.Err(); err != nil {
			return nil, err
		}
		query := fmt.Sprintf("MATCH (n:%s)-[r:%s]->(m:%s) WITH n,r,m LIMIT %d RETURN %s, %s, label(r) AS rel_type",
			quote(m.from), quote(m.table), quote(m.to), remaining,
			canonicalGraphSideColumns(manifest, m.from, "n", "src"), canonicalGraphSideColumns(manifest, m.to, "m", "dst"))
		rows, err := db.Query(ctx, query, nil)
		if err != nil {
			return nil, fmt.Errorf("sample relationship %s: %w", m.table, err)
		}
		result.Records = append(result.Records, rows.Records...)
	}
	return result, nil
}

func canonicalGraphSideColumns(manifest *ladybug.CanonicalManifest, label, variable, prefix string) string {
	columns := map[string]bool{}
	for _, table := range manifest.NodeTables {
		if table.Label == label {
			for _, column := range table.Columns {
				columns[column.Name] = true
			}
		}
	}
	parts := []string{fmt.Sprintf("CAST(id(%s) AS STRING) AS %s_id", variable, prefix), fmt.Sprintf("label(%s) AS %s_label", variable, prefix)}
	for _, prop := range [][2]string{{"name", "name"}, {"path", "path"}, {"cluster", "cluster"}, {"lang", "lang"}, {"line_number", "line"}} {
		value := "NULL"
		if columns[prop[0]] {
			value = variable + "." + prop[0]
		}
		parts = append(parts, fmt.Sprintf("%s AS %s_%s", value, prefix, prop[1]))
	}
	return strings.Join(parts, ", ")
}
