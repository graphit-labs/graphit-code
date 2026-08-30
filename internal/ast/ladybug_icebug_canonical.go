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

// The CANONICAL catalog has real node tables per label and one rel table per
// (type, from, to) pair, so the folded planner's assumptions do not hold: there is no
// Entity table, no label column, and CALLS names N physical members. This planner resolves
// the logical TYPE against the manifest's member map, runs UNBOUNDED breadth-first
// frontiers — termination comes from visited saturation and the caller's deadline, never
// from a hop ceiling — and answers reachability plus basic endpoint counts itself.
//
// Everything ≥2 hops on a canonical catalog belongs here BY RULE. A var-length form this
// planner cannot preserve fails CLOSED with the member names, because forwarding it would
// hand the query to an upstream recursive plan that MEASURED enumerates the whole graph.

var (
	canonicalTraversalPattern = regexp.MustCompile(`(?is)^\s*MATCH\s+(\([^)]*\))\s*-\s*\[\s*(?:([A-Za-z_][A-Za-z0-9_]*)\s*)?:\s*` +
		"`?([A-Za-z_][A-Za-z0-9_]*)`?" +
		`(?:\s*(\*)?\s*(?:(\d+)\s*)?(?:\.\.\s*(\d+))?)?\s*\]\s*(->|-)\s*(\([^)]*\))` +
		`(?:\s+WHERE\s+(.+?))?\s+RETURN\s+(DISTINCT\s+)?((?:.|\n)*?)(?:\s+(?:ORDER\s+BY|LIMIT)\b|\s*;\s*|\s*)$`)
	canonicalCountPattern      = regexp.MustCompile(`(?is)^count\s*\(\s*(distinct\s+)?([A-Za-z_][A-Za-z0-9_]*)\.uid\s*\)(?:\s+AS\s+[A-Za-z_][A-Za-z0-9_]*)?$`)
	canonicalProjectionPattern = regexp.MustCompile(`(?is)^([A-Za-z_][A-Za-z0-9_]*)\.([A-Za-z_][A-Za-z0-9_]*)(\s+AS\s+([A-Za-z_][A-Za-z0-9_]*))?$`)
	// canonicalTraversalTail strips trailing `ORDER BY ...` (and LIMIT) from the
	// RETURN projection, because a traversal's ordering is applied by the caller
	// over the materialized set, not by the engine over mounted object files.
)

type canonicalPlan struct {
	anchor, reached  icebugNodePattern
	relType          string
	minHops, maxHops int // maxHops 0 == unbounded
	directionless    bool
	reverse          bool // the selective endpoint sits on the FROM side of public syntax
	distinct         bool
	returnClause     string
	countDistinct    bool
	count            bool
	anchorPreds      []string
	reachedPreds     []string
}

func (k *LadybugBackend) tryCanonicalBoundedTraversal(
	ctx context.Context, cypher string, params map[string]any,
) (*QueryResult, bool, error) {
	plan, ok := parseCanonicalTraversal(cypher)
	if !ok {
		return nil, false, nil
	}
	group := k.canonicalGroup(plan.relType)
	if group == nil {
		return nil, false, nil
	}

	members := canonicalUIDMembers(k.canonical, group, plan.reverse, plan.directionless)
	nodeTables := canonicalNodeLabels(k.canonical)

	anchorConds := canonicalConditions(plan.anchor, plan.anchorPreds)
	anchorTables := canonicalTablesFor(k.canonical, plan.anchor.label,
		plan.anchor.variable, append(append([]string{}, plan.anchor.properties...), plan.anchorPreds...))
	frontier := map[string]bool{}
	for _, table := range anchorTables {
		conds := anchorConds
		pk := canonicalPKFor(k.canonical, table)
		// Rewrite `=` against this table's primary key to IN, exactly as the engine
		// requires — equality against an icebug-disk PK answers zero rows.
		conds = sanitizeCondPK(conds, plan.anchor.variable, pk)
		q := fmt.Sprintf("MATCH (%s:%s) WHERE %s RETURN DISTINCT %s.%s AS %s",
			plan.anchor.variable, ladybug.QuoteIdent(table), strings.Join(conds, " AND "),
			plan.anchor.variable, ladybug.QuoteIdent(pk), ladybug.QuoteIdent(icebugUIDColumn))
		records, err := k.queryRecordsLocked(q, params)
		if err != nil {
			return nil, true, err
		}
		for _, uid := range uidValues(records) {
			frontier[uid] = true
		}
	}
	if len(frontier) == 0 {
		return canonicalEmpty(plan), true, nil
	}

	visitedDepth := map[string]int{}
	for uid := range frontier {
		visitedDepth[uid] = 0
	}
	reached := map[string]bool{}
	for hop := 1; len(frontier) > 0; hop++ {
		if err := ctx.Err(); err != nil {
			return nil, true, err
		}
		next := map[string]bool{}
		for _, m := range members {
			pkFrom := canonicalPKFor(k.canonical, m.From)
			pkTo := canonicalPKFor(k.canonical, m.To)
			list := sortedUIDs(frontier)
			for start := 0; start < len(list); start += icebugTraversalBatchSize {
				end := min(start+icebugTraversalBatchSize, len(list))
				q := fmt.Sprintf(
					"MATCH (a:%s)-[:%s]->(b:%s) WHERE a.%s IN [%s] RETURN DISTINCT b.%s AS %s",
					ladybug.QuoteIdent(m.From), ladybug.QuoteIdent(m.Table), ladybug.QuoteIdent(m.To),
					ladybug.QuoteIdent(pkFrom), icebugStringList(list[start:end]),
					ladybug.QuoteIdent(pkTo), ladybug.QuoteIdent(icebugUIDColumn))
				records, qerr := k.queryRecordsLocked(q, nil)
				if qerr != nil {
					return nil, true, qerr
				}
				for _, uid := range uidValues(records) {
					if _, seen := visitedDepth[uid]; !seen {
						visitedDepth[uid] = hop
						next[uid] = true
					}
				}
			}
		}
		for uid := range next {
			if hop >= plan.minHops && (plan.maxHops == 0 || hop <= plan.maxHops) {
				reached[uid] = true
			}
		}
		frontier = next
		// A bounded plan has everything it can use once it has run maxHops hops: the
		// filter above admits nothing deeper. Without this the loop kept expanding to
		// visited saturation and threw the result away.
		if plan.maxHops != 0 && hop >= plan.maxHops {
			break
		}
	}

	if len(reached) == 0 {
		return canonicalEmpty(plan), true, nil
	}
	return k.finishCanonicalTraversal(ctx, plan, nodeTables, sortedUIDs(reached), params)
}

func canonicalEmpty(plan canonicalPlan) *QueryResult {
	if plan.count || plan.countDistinct {
		return &QueryResult{Records: []QueryRecord{{"count": int64(0)}}}
	}
	return &QueryResult{}
}

func (k *LadybugBackend) finishCanonicalTraversal(ctx context.Context, plan canonicalPlan,
	labels []string, uids []string, params map[string]any,
) (*QueryResult, bool, error) {
	if plan.count || plan.countDistinct {
		return &QueryResult{Records: []QueryRecord{{"count": int64(len(uids))}}}, true, nil
	}
	if column, ok := plan.uidProjection(); ok {
		result := &QueryResult{Records: make([]QueryRecord, 0, len(uids))}
		for _, uid := range uids {
			result.Records = append(result.Records, QueryRecord{column: uid})
		}
		return result, true, nil
	}
	reachedProps := append(append([]string{}, plan.reached.properties...), plan.reachedPreds...)
	reachedProps = append(reachedProps, plan.returnClause)
	resolvedLabels := canonicalTablesFor(k.canonical, plan.reached.label,
		plan.reached.variable, reachedProps)
	// SAFETY: the row order is SPECIFIED, not inherited. It used to fall out of the
	// iteration — reached uid, then candidate label, then whatever the engine returned —
	// which is an order keyed on something the caller cannot see, and it moved the moment
	// the queries were batched. Sorting on the record's own canonical key makes it
	// reproducible whatever the planner does underneath, and it matches the uid-projection
	// path above, which already answers in sorted uid order.
	type keyedRecord struct {
		key    string
		record QueryRecord
	}
	var collected []keyedRecord
	seen := map[string]bool{}
	// Batched at the same width as the traversal above. One query per reached uid meant a
	// result of N rows cost N round trips per candidate label, which is the whole cost of
	// any traversal that projects properties rather than uids.
	for _, label := range resolvedLabels {
		pk := canonicalPKFor(k.canonical, label)
		for start := 0; start < len(uids); start += icebugTraversalBatchSize {
			if err := ctx.Err(); err != nil {
				return nil, true, err
			}
			end := min(start+icebugTraversalBatchSize, len(uids))
			conds := append(canonicalConditions(plan.reached, plan.reachedPreds),
				fmt.Sprintf("%s.%s IN [%s]", plan.reached.variable, ladybug.QuoteIdent(pk), icebugStringList(uids[start:end])))
			q := fmt.Sprintf("MATCH (%s:%s) WHERE %s RETURN DISTINCT %s",
				plan.reached.variable, ladybug.QuoteIdent(label), strings.Join(conds, " AND "), plan.returnClause)
			records, err := k.queryRecordsLocked(q, params)
			if err != nil {
				return nil, true, err
			}
			for _, record := range records {
				key := icebugRecordKey(record)
				if !seen[key] {
					seen[key] = true
					collected = append(collected, keyedRecord{key: key, record: record})
				}
			}
		}
	}
	sort.Slice(collected, func(i, j int) bool { return collected[i].key < collected[j].key })
	result := &QueryResult{Records: make([]QueryRecord, 0, len(collected))}
	for _, c := range collected {
		result.Records = append(result.Records, c.record)
	}
	return result, true, nil
}

func (k *LadybugBackend) canonicalGroup(relType string) *ladybug.CanonicalRelGroup {
	for i := range k.canonical.RelGroups {
		if strings.EqualFold(k.canonical.RelGroups[i].Type, relType) {
			return &k.canonical.RelGroups[i]
		}
	}
	return nil
}

// canonicalPKFor returns the PRIMARY KEY column of a node table. Every node table has
// one by construction (the manifest records it); the common case is `uid`, the two
// structural tables use `path`.
func canonicalPKFor(m *ladybug.CanonicalManifest, label string) string {
	for _, n := range m.NodeTables {
		if n.Label == label {
			if n.PrimaryKey != "" {
				return n.PrimaryKey
			}
			if len(n.Columns) > 0 {
				return n.Columns[0].Name
			}
		}
	}
	return "uid"
}

// canonicalUIDMembers keeps only the members whose BOTH endpoint tables carry a
// primary key: frontier traversal presumes a global identity that tables keyed by
// other columns (File.path, Directory.path) share — which is exactly what the PK is.
func canonicalUIDMembers(m *ladybug.CanonicalManifest,
	g *ladybug.CanonicalRelGroup, reverse, directionless bool) []ladybug.CanonicalMember {

	hasKey := func(label string) bool {
		for _, n := range m.NodeTables {
			if n.Label == label {
				return n.PrimaryKey != "" || len(n.Columns) > 0
			}
		}
		return false
	}
	keep := func(in []ladybug.CanonicalMember) []ladybug.CanonicalMember {
		out := make([]ladybug.CanonicalMember, 0, len(in))
		for _, mm := range in {
			if hasKey(mm.From) && hasKey(mm.To) {
				out = append(out, mm)
			}
		}
		// Smallest member first: batches fill with useful uids before the big CSRs are
		// touched, using the row counts the manifest already carries.
		sort.SliceStable(out, func(i, j int) bool { return out[i].Rows < out[j].Rows })
		return out
	}
	switch {
	case directionless:
		return append(keep(g.Members), keep(g.ReverseMembers)...)
	case reverse:
		return keep(g.ReverseMembers)
	default:
		return keep(g.Members)
	}
}

func canonicalNodeLabels(m *ladybug.CanonicalManifest) []string {
	out := make([]string, 0, len(m.NodeTables))
	for _, n := range m.NodeTables {
		out = append(out, n.Label)
	}
	return out
}

func canonicalConditions(n icebugNodePattern, preds []string) []string {
	conds := make([]string, 0, len(n.properties)+len(preds)+1)
	conds = append(conds, n.properties...)
	for _, p := range preds {
		conds = append(conds, "("+p+")")
	}
	if len(conds) == 0 {
		conds = append(conds, "true")
	}
	return conds
}

func (p canonicalPlan) uidProjection() (string, bool) {
	expression := strings.TrimSpace(p.returnClause)
	prefix := p.reached.variable + ".uid"
	if strings.EqualFold(expression, prefix) {
		return expression, true
	}
	parts := strings.Fields(expression)
	if len(parts) == 3 && strings.EqualFold(parts[0], prefix) &&
		strings.EqualFold(parts[1], "as") && isIdentifier(parts[2]) {
		return parts[2], true
	}
	return "", false
}

// parseCanonicalTraversal recognizes the bounded-reachability shapes whose semantics the
// canonical planner can preserve exactly, including the aggregation forms it computes over
// the reached set. Everything else fails closed.
func parseCanonicalTraversal(cypher string) (canonicalPlan, bool) {
	m := canonicalTraversalPattern.FindStringSubmatch(cypher)
	if m == nil {
		return canonicalPlan{}, false
	}
	// m[2] is the relationship variable (`[r:CALLS]`); it is metadata for the
	// planner, whose traversal does not need to bind the relationship itself.
	left, okL := parseIcebugNodePattern(m[1])
	right, okR := parseIcebugNodePattern(m[8])
	if !okL || !okR || left.variable == right.variable {
		return canonicalPlan{}, false
	}

	plan := canonicalPlan{
		relType: m[3], minHops: 1,
		directionless: m[7] == "-",
		distinct:      strings.TrimSpace(m[10]) != "",
		returnClause:  strings.TrimSpace(m[11]),
	}
	if m[4] == "" {
		// A BARE relationship pattern is exactly one hop.
		plan.minHops, plan.maxHops = 1, 1
	} else {
		if m[5] != "" {
			plan.minHops, _ = strconv.Atoi(m[5])
		}
		if m[6] != "" {
			plan.maxHops, _ = strconv.Atoi(m[6])
			if plan.maxHops < plan.minHops {
				return canonicalPlan{}, false
			}
		}
	}
	returnClause := plan.returnClause
	returnLeft := referencesVariable(returnClause, left.variable)
	returnRight := referencesVariable(returnClause, right.variable)
	if returnLeft == returnRight || strings.Contains(strings.ToLower(returnClause), "label(") ||
		strings.Contains(strings.ToLower(returnClause), "."+strings.ToLower(ladybug.IcebugLabelColumn)) {
		return canonicalPlan{}, false
	}

	// Whichever endpoint the RETURN projects is the REACHED side; the other carries the
	// selective anchor and the traversal direction flips accordingly.
	plan.anchor, plan.reached = left, right
	if returnLeft {
		plan.anchor, plan.reached = right, left
		plan.reverse = !plan.directionless
	}
	reachedVar := plan.reached.variable

	if cm := canonicalCountPattern.FindStringSubmatch(returnClause); cm != nil {
		if cm[2] != reachedVar {
			return canonicalPlan{}, false
		}
		plan.countDistinct = strings.TrimSpace(cm[1]) != ""
		plan.count = true
		plan.distinct = false
		plan.returnClause = ""
	} else if !plan.distinct {
		return canonicalPlan{}, false
	} else {
		// Every projected item must be `reached.prop [AS alias]`; the materializer runs them
		// per node, so anything richer (collect, arithmetic, paths) would silently change
		// semantics and is refused instead.
		for _, item := range splitCommaList(returnClause) {
			pm := canonicalProjectionPattern.FindStringSubmatch(item)
			if pm == nil || pm[1] != reachedVar ||
				strings.EqualFold(pm[2], ladybug.IcebugLabelColumn) {
				return canonicalPlan{}, false
			}
		}
	}

	for _, predicate := range splitTopLevel(strings.TrimSpace(m[9]), "AND") {
		a := referencesVariable(predicate, plan.anchor.variable)
		r := referencesVariable(predicate, plan.reached.variable)
		if a && r {
			return canonicalPlan{}, false
		}
		switch {
		case r:
			plan.reachedPreds = append(plan.reachedPreds, predicate)
		case a:
			plan.anchorPreds = append(plan.anchorPreds, predicate)
		default:
			return canonicalPlan{}, false
		}
	}
	if !plan.anchor.selective(plan.anchorPreds) {
		return canonicalPlan{}, false
	}
	return plan, true
}

// sanitizeCanonicalUIDEquality rewrites `X.uid = 'lit'` into `X.uid IN ['lit']` outside
// string literals. MEASURED, equality against an icebug-disk primary key answers zero rows
// even when the row exists, while an IN list answers it.
func sanitizeCanonicalUIDEquality(cypher string) string {
	eq := regexp.MustCompile(`([A-Za-z_][A-Za-z0-9_]*)\.uid\s*=\s*('[^']*'|"[^"]*")`)
	var b strings.Builder
	inString := byte(0)
	i := 0
	for i < len(cypher) {
		ch := cypher[i]
		if inString == 0 && (ch == '\'' || ch == '"') {
			inString = ch
		} else if ch == inString {
			inString = 0
		}
		if inString == 0 {
			if loc := eq.FindStringSubmatchIndex(cypher[i:]); loc != nil {
				m := eq.FindStringSubmatch(cypher[i:])
				b.WriteString(cypher[i : i+loc[0]])
				fmt.Fprintf(&b, "%s.uid IN [%s]", m[1], m[2])
				i += loc[1]
				continue
			}
		}
		b.WriteByte(ch)
		i++
	}
	return b.String()
}

// splitCommaList splits on top-level commas, tracking quotes and nesting depth. Unlike
// splitTopLevel it does not require identifier boundaries around the separator, because a
// projection item legitimately ENDS in an identifier (`caller.name, caller.path`).
func splitCommaList(expression string) []string {
	expression = strings.TrimSpace(expression)
	if expression == "" {
		return nil
	}
	var parts []string
	start, depth := 0, 0
	inString := byte(0)
	for i := 0; i < len(expression); i++ {
		ch := expression[i]
		switch {
		case inString != 0:
			if ch == inString {
				inString = 0
			}
			continue
		case ch == '\'' || ch == '"':
			inString = ch
			continue
		case ch == '(' || ch == '[' || ch == '{':
			depth++
		case ch == ')' || ch == ']' || ch == '}':
			if depth > 0 {
				depth--
			}
		case ch == ',' && depth == 0:
			parts = append(parts, strings.TrimSpace(expression[start:i]))
			start = i + 1
		}
	}
	parts = append(parts, strings.TrimSpace(expression[start:]))
	return parts
}

// canonicalTablesFor narrows the candidate node tables to those actually carrying every
// `variable.property` reference in the given fragments. A canonical catalog stores real
// schemas per label, so a table without the column cannot answer the predicate — MEASURED,
// including it fails the whole anchor scan with "Cannot find property".
func canonicalTablesFor(m *ladybug.CanonicalManifest, label, variable string, fragments []string) []string {
	if label != "" {
		return []string{label}
	}
	need := map[string]bool{}
	propRe := regexp.MustCompile(`(?:^|[^A-Za-z0-9_])` + regexp.QuoteMeta(variable) + `\.([A-Za-z_][A-Za-z0-9_]*)`)
	for _, f := range fragments {
		for _, sub := range propRe.FindAllStringSubmatch(f, -1) {
			need[strings.ToLower(sub[1])] = true
		}
	}
	var out []string
	for _, n := range m.NodeTables {
		ok := true
		for prop := range need {
			found := false
			for _, col := range n.Columns {
				if strings.EqualFold(col.Name, prop) {
					found = true
					break
				}
			}
			if !found {
				ok = false
				break
			}
		}
		if ok {
			out = append(out, n.Label)
		}
	}
	if len(out) == 0 {
		out = canonicalNodeLabels(m)
	}
	return out
}

// namesLogicalRel reports whether a Cypher query names a LOGICAL relationship type
// (CALLS, CONTAINS, …) in a pattern position. Physical member tables — the ones the
// engine actually knows — are never in the manifest types, so they pass through.
func namesLogicalRel(man *ladybug.CanonicalManifest, cypher string) bool {
	if man == nil {
		return false
	}
	for _, g := range man.RelGroups {
		// Pattern positions like -[r:CALLS]-> or -[:CALLS]->, plus the compact
		// form used by tests. Anchored by the brackets AND the colon so an
		// identifier merely mentioning the word elsewhere does not trip it.
		if containsRelPattern(cypher, g.Type) {
			return true
		}
	}
	return false
}

func containsRelPattern(cypher, relType string) bool {
	idx := strings.Index(cypher, relType)
	for idx >= 0 {
		// The token must follow a colon: `:CALLS` or `r:CALLS` … and be preceded
		// by an optional variable. `(n:Function)` is a NODE pattern, so ensure the
		// colon is inside brackets immediately followed by the type.
		before := cypher[:idx]
		pos := idx - 1
		for pos >= 0 && cypher[pos] == ' ' {
			pos--
		}
		if pos >= 0 && cypher[pos] == ':' {
			// find the bracket before the variable/colon
			br := strings.LastIndex(before[:idx], "[")
			if br >= 0 && strings.LastIndex(before[:idx], "(") < br {
				return true
			}
		}
		nxt := strings.Index(cypher[idx+1:], relType)
		if nxt < 0 {
			break
		}
		idx += 1 + nxt
	}
	return false
}

// sanitizeCanonicalPKEquality rewrites `X.<pk> = 'lit'` into `X.<pk> IN ['lit']` for
// every node table's primary key. MEASURED, equality against an icebug-disk primary
// key answers zero rows even when the row exists, while an IN list answers it.
func sanitizeCanonicalPKEquality(man *ladybug.CanonicalManifest, cypher string) string {
	for _, n := range man.NodeTables {
		pk := n.PrimaryKey
		if pk == "" && len(n.Columns) > 0 {
			pk = n.Columns[0].Name
		}
		if pk == "" {
			continue
		}
		cypher = rewritePKEqToIN(cypher, pk)
	}
	return cypher
}

func rewritePKEqToIN(cypher, pk string) string {
	eq := regexp.MustCompile(`([A-Za-z_][A-Za-z0-9_]*)\.` + regexp.QuoteMeta(pk) + `\s*=\s*('[^']*'|"[^"]*")`)
	var b strings.Builder
	inString := byte(0)
	i := 0
	for i < len(cypher) {
		ch := cypher[i]
		if inString == 0 && (ch == '\'' || ch == '"') {
			inString = ch
		} else if ch == inString {
			inString = 0
		}
		if inString == 0 {
			if loc := eq.FindStringSubmatchIndex(cypher[i:]); loc != nil {
				m := eq.FindStringSubmatch(cypher[i:])
				b.WriteString(cypher[i : i+loc[0]])
				fmt.Fprintf(&b, "%s.%s IN [%s]", m[1], pk, m[2])
				i += loc[1]
				continue
			}
		}
		b.WriteByte(ch)
		i++
	}
	return b.String()
}

func sanitizeCondPK(conds []string, variable, pk string) []string {
	if len(conds) == 0 || pk == "" {
		return conds
	}
	out := make([]string, 0, len(conds))
	for _, c := range conds {
		out = append(out, rewritePKEqToIN(c, pk))
	}
	return out
}
