package ast

import (
	"regexp"
	"strconv"
	"strings"

	ladybug "github.com/graphit-labs/graphit-code/internal/ladybugstore"
)

var (
	canonicalNodeCountQuery = regexp.MustCompile(`(?is)^\s*MATCH\s*\(\s*([A-Za-z_][A-Za-z0-9_]*)(?:\s*:\s*` + "`?" + `([A-Za-z_][A-Za-z0-9_]*)` + "`?" + `)?\s*\)\s+RETURN\s+(count\s*\(\s*(\*|[A-Za-z_][A-Za-z0-9_]*)\s*\))(?:\s+AS\s+` + "`?" + `([A-Za-z_][A-Za-z0-9_]*)` + "`?" + `)?\s*;?\s*$`)
	canonicalRelCountQuery  = regexp.MustCompile(`(?is)^\s*MATCH\s*\(\s*\)\s*-\s*\[\s*([A-Za-z_][A-Za-z0-9_]*)\s*\]\s*->\s*\(\s*\)\s+RETURN\s+(count\s*\(\s*(\*|[A-Za-z_][A-Za-z0-9_]*)\s*\))(?:\s+AS\s+` + "`?" + `([A-Za-z_][A-Za-z0-9_]*)` + "`?" + `)?\s*;?\s*$`)
	canonicalNodeStatsQuery = regexp.MustCompile(`(?is)^\s*MATCH\s*\(\s*([A-Za-z_][A-Za-z0-9_]*)\s*\)\s+RETURN\s+DISTINCT\s+label\s*\(\s*([A-Za-z_][A-Za-z0-9_]*)\s*\)\s+AS\s+` + "`?" + `([A-Za-z_][A-Za-z0-9_]*)` + "`?" + `\s*,\s*count\s*\(\s*([A-Za-z_][A-Za-z0-9_]*)\s*\)\s+AS\s+` + "`?" + `([A-Za-z_][A-Za-z0-9_]*)` + "`?" + `\s+ORDER\s+BY\s+` + "`?" + `([A-Za-z_][A-Za-z0-9_]*)` + "`?" + `\s+DESC\s*;?\s*$`)
	canonicalRelStatsQuery  = regexp.MustCompile(`(?is)^\s*MATCH\s*\(\s*\)\s*-\s*\[\s*([A-Za-z_][A-Za-z0-9_]*)\s*\]\s*->\s*\(\s*\)\s+RETURN\s+DISTINCT\s+label\s*\(\s*([A-Za-z_][A-Za-z0-9_]*)\s*\)\s+AS\s+` + "`?" + `([A-Za-z_][A-Za-z0-9_]*)` + "`?" + `\s*,\s*count\s*\(\s*([A-Za-z_][A-Za-z0-9_]*)\s*\)\s+AS\s+` + "`?" + `([A-Za-z_][A-Za-z0-9_]*)` + "`?" + `\s+ORDER\s+BY\s+` + "`?" + `([A-Za-z_][A-Za-z0-9_]*)` + "`?" + `\s+DESC(?:\s+LIMIT\s+(\d+))?\s*;?\s*$`)
)

// canonicalMetadataQuery recognizes only complete read-only statements whose result is
// already exact in icebug.json. Keeping this as a result-producing planner (instead of a
// textual rewrite) is what lets Query answer before mounting LadybugDB at all.
func canonicalMetadataQuery(man *ladybug.CanonicalManifest, cypher string) (*QueryResult, bool) {
	stats := canonicalStatsFromManifest(man)

	if m := canonicalNodeCountQuery.FindStringSubmatch(cypher); m != nil {
		if m[4] != "*" && m[1] != m[4] {
			return nil, false
		}
		count := stats.NodeCount
		if m[2] != "" {
			found := false
			for _, stat := range stats.Nodes {
				if stat.Label == m[2] {
					count = stat.Count
					found = true
					break
				}
			}
			if !found {
				return nil, false
			}
		}
		return canonicalCountResult(m[3], m[5], count), true
	}
	if m := canonicalRelCountQuery.FindStringSubmatch(cypher); m != nil {
		if m[3] != "*" && m[1] != m[3] {
			return nil, false
		}
		return canonicalCountResult(m[2], m[4], stats.EdgeCount), true
	}
	if m := canonicalNodeStatsQuery.FindStringSubmatch(cypher); m != nil {
		if !sameIdentifier(m[1], m[2], m[4]) || m[5] != m[6] {
			return nil, false
		}
		records := make([]QueryRecord, 0, len(stats.Nodes))
		for _, stat := range stats.Nodes {
			records = append(records, QueryRecord{m[3]: stat.Label, m[5]: stat.Count})
		}
		return &QueryResult{Records: records}, true
	}
	if m := canonicalRelStatsQuery.FindStringSubmatch(cypher); m != nil {
		if !sameIdentifier(m[1], m[2], m[4]) || m[5] != m[6] {
			return nil, false
		}
		limit := len(stats.Relationships)
		if m[7] != "" {
			parsed, err := strconv.Atoi(m[7])
			if err != nil {
				return nil, false
			}
			limit = min(limit, parsed)
		}
		records := make([]QueryRecord, 0, limit)
		for _, stat := range stats.Relationships[:limit] {
			records = append(records, QueryRecord{m[3]: stat.Type, m[5]: stat.Count})
		}
		return &QueryResult{Records: records}, true
	}
	return nil, false
}

func canonicalCountResult(expression, alias string, count int64) *QueryResult {
	column := alias
	if column == "" {
		column = strings.TrimSpace(expression)
	}
	return &QueryResult{Records: []QueryRecord{{column: count}}}
}

func sameIdentifier(values ...string) bool {
	for i := 1; i < len(values); i++ {
		if values[0] != values[i] {
			return false
		}
	}
	return true
}
