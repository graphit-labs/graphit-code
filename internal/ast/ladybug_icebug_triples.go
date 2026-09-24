package ast

import (
	"context"
	"fmt"
	"regexp"
	"sort"
	"strconv"
	"strings"

	ladybug "github.com/graphit-labs/graphit-code/internal/ladybugstore"
)

// A full n,r,m projection must retain actual relationship rows. The ordinary
// canonical reachability planner intentionally reduces matches to a set of
// reached nodes, so it cannot answer this form or preserve parallel edges.
var canonicalTriplesPattern = regexp.MustCompile("(?is)^\\s*MATCH\\s*\\(\\s*([A-Za-z_][A-Za-z0-9_]*)(?::`?([A-Za-z_][A-Za-z0-9_]*)`?)?\\s*\\)\\s*-\\s*\\[\\s*([A-Za-z_][A-Za-z0-9_]*)(?::`?([A-Za-z_][A-Za-z0-9_]*)`?)?\\s*\\]\\s*(->|-)\\s*\\(\\s*([A-Za-z_][A-Za-z0-9_]*)(?::`?([A-Za-z_][A-Za-z0-9_]*)`?)?\\s*\\)\\s*(?:WHERE\\s+([A-Za-z_][A-Za-z0-9_]*)\\.(uid|path)\\s*=\\s*('(?:\\\\.|[^'\\\\])*'))?\\s*RETURN\\s+([A-Za-z_][A-Za-z0-9_]*(?:\\s*,\\s*[A-Za-z_][A-Za-z0-9_]*){0,2})\\s*(?:LIMIT\\s+(\\d+))?\\s*;?\\s*$")

func (k *LadybugBackend) tryCanonicalTriples(ctx context.Context, cypher string, maxRows int, complete bool) (*QueryResult, bool, error) {
	m := canonicalTriplesPattern.FindStringSubmatch(cypher)
	if m == nil || k.canonical == nil {
		return nil, false, nil
	}
	if !complete && maxRows > 10000 {
		return nil, true, fmt.Errorf("canonical catalog: triple page exceeds 10000 rows; filter by uid/path")
	}
	leftVar, leftLabel, relVar, relType := m[1], m[2], m[3], m[4]
	rightVar, rightLabel := m[6], m[7]
	if leftVar == relVar || leftVar == rightVar || relVar == rightVar {
		return nil, false, nil
	}
	returnVars := strings.Split(m[11], ",")
	requested := map[string]bool{}
	for i, variable := range returnVars {
		variable = strings.TrimSpace(variable)
		returnVars[i] = variable
		if variable != leftVar && variable != relVar && variable != rightVar {
			return nil, false, nil
		}
		requested[variable] = true
	}
	if len(requested) != len(returnVars) {
		return nil, false, nil
	}
	if m[8] != "" && (m[8] != leftVar && m[8] != rightVar) {
		return nil, false, nil
	}
	limit := 0
	if m[12] != "" {
		var err error
		limit, err = strconv.Atoi(m[12])
		if err != nil || limit < 1 {
			return nil, true, fmt.Errorf("canonical catalog: LIMIT must be positive")
		}
	}
	if complete && limit > maxRows {
		return nil, true, fmt.Errorf("canonical catalog: triple result exceeds %d rows; use a smaller LIMIT or filter by uid/path", maxRows)
	}
	rowCap := maxRows
	if limit > 0 && limit < rowCap {
		rowCap = limit
	}
	fetchCap := rowCap
	if complete && limit == 0 {
		fetchCap++ // Detect an unbounded result without materializing the whole graph.
	}
	if !k.canonical.RelationUIDs {
		return nil, true, fmt.Errorf("this index has no stable relationship UIDs; reindex before returning relationships")
	}
	type member struct {
		from, table, to, logical string
		reverse                  bool
	}
	var members []member
	for _, group := range k.canonical.RelGroups {
		if relType != "" && !strings.EqualFold(group.Type, relType) {
			continue
		}
		for _, physical := range group.Members {
			if physical.Rows == 0 {
				continue
			}
			if (leftLabel == "" || physical.From == leftLabel) && (rightLabel == "" || physical.To == rightLabel) {
				members = append(members, member{physical.From, physical.Table, physical.To, group.Type, false})
			}
			if m[5] == "-" && (leftLabel == "" || physical.To == leftLabel) && (rightLabel == "" || physical.From == rightLabel) {
				members = append(members, member{physical.From, physical.Table, physical.To, group.Type, true})
			}
		}
	}
	if relType != "" && len(members) == 0 {
		// A physical relation table is intentionally handled by the ordinary engine.
		if k.canonicalGroup(relType) == nil {
			return nil, false, nil
		}
	}
	sort.Slice(members, func(i, j int) bool {
		if members[i].table == members[j].table {
			return !members[i].reverse && members[j].reverse
		}
		return members[i].table < members[j].table
	})
	result := &QueryResult{}
	for _, physical := range members {
		if err := ctx.Err(); err != nil {
			return nil, true, err
		}
		startVar, endVar := leftVar, rightVar
		if physical.reverse {
			startVar, endVar = rightVar, leftVar
		}
		query := fmt.Sprintf("MATCH (%s:%s)-[%s:%s]->(%s:%s)", startVar, ladybug.QuoteIdent(physical.from), relVar, ladybug.QuoteIdent(physical.table), endVar, ladybug.QuoteIdent(physical.to))
		if m[8] != "" {
			pk := canonicalPKFor(k.canonical, physical.from)
			if m[8] == endVar {
				pk = canonicalPKFor(k.canonical, physical.to)
			}
			if pk != m[9] {
				continue
			}
			query += fmt.Sprintf(" WHERE %s.%s IN [%s]", m[8], pk, m[10])
		}
		query += fmt.Sprintf(" RETURN %s,%s,%s", leftVar, relVar, rightVar)
		query += fmt.Sprintf(" LIMIT %d", fetchCap-len(result.Records))
		rows, err := k.queryRecordsLocked(query, nil)
		if err != nil {
			return nil, true, fmt.Errorf("canonical triple member %s: %w", physical.table, err)
		}
		for _, row := range rows {
			rel, ok := row[relVar].(map[string]any)
			if !ok {
				return nil, true, fmt.Errorf("canonical triple member %s returned no relationship", physical.table)
			}
			if physical.reverse {
				if ladybugIDStr(rel["SourceID"]) == ladybugIDStr(rel["DestinationID"]) {
					continue
				}
			}
			props, _ := rel["Properties"].(map[string]any)
			if safeStr(props["uid"]) == "" {
				return nil, true, fmt.Errorf("canonical triple member %s has no stable relationship UID; reindex", physical.table)
			}
			rel["Label"] = physical.logical
			projected := QueryRecord{}
			for _, variable := range returnVars {
				projected[variable] = row[variable]
			}
			result.Records = append(result.Records, projected)
		}
		if len(result.Records) >= fetchCap {
			break
		}
	}
	if complete && len(result.Records) > rowCap {
		return nil, true, fmt.Errorf("canonical catalog: triple result exceeds %d rows; add LIMIT or filter by uid/path", rowCap)
	}
	return result, true, nil
}
