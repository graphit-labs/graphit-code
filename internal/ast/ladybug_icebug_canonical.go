package ast

import (
	"context"
	"fmt"
	"sort"
	"regexp"
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
		`(?:\s+WHERE\s+(.+?))?\s+RETURN\s+(DISTINCT\s+)?(.+?)\s*;?\s*$`)
	canonicalCountPattern      = regexp.MustCompile(`(?is)^count\s*\(\s*(distinct\s+)?([A-Za-z_][A-Za-z0-9_]*)\.uid\s*\)$`)
	canonicalProjectionPattern = regexp.MustCompile(`(?is)^([A-Za-z_][A-Za-z0-9_]*)\.([A-Za-z_][A-Za-z0-9_]*)(\s+AS\s+([A-Za-z_][A-Za-z0-9_]*))?$`)
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
		q := fmt.Sprintf("MATCH (%s:%s) WHERE %s RETURN DISTINCT %s.uid AS %s",
			plan.anchor.variable, ladybug.QuoteIdent(table), strings.Join(conds, " AND "),
			plan.anchor.variable, ladybug.QuoteIdent(icebugUIDColumn))
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
			list := sortedUIDs(frontier)
			for start := 0; start < len(list); start += icebugTraversalBatchSize {
				end := min(start+icebugTraversalBatchSize, len(list))
				q := fmt.Sprintf(
					"MATCH (a:%s)-[:%s]->(b:%s) WHERE a.uid IN [%s] RETURN DISTINCT b.uid AS %s",
					ladybug.QuoteIdent(m.From), ladybug.QuoteIdent(m.Table), ladybug.QuoteIdent(m.To),
					icebugStringList(list[start:end]), ladybug.QuoteIdent(icebugUIDColumn))
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
	labels = canonicalTablesFor(k.canonical, plan.reached.label,
		plan.reached.variable, reachedProps)
	result := &QueryResult{}
	seen := map[string]bool{}
	for _, uid := range uids {
		if err := ctx.Err(); err != nil {
			return nil, true, err
		}
		for _, label := range labels {
			conds := append(canonicalConditions(plan.reached, plan.reachedPreds),
				fmt.Sprintf("%s.uid IN [%s]", plan.reached.variable, icebugStringList([]string{uid})))
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
					result.Records = append(result.Records, record)
				}
			}
		}
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

// canonicalUIDMembers keeps only the members whose BOTH endpoint tables carry a `uid`
// column: uid-frontier traversal presumes a global uid identity that tables keyed by other
// columns (File.path, Directory.path) do not have.
func canonicalUIDMembers(m *ladybug.CanonicalManifest,
	g *ladybug.CanonicalRelGroup, reverse, directionless bool) []ladybug.CanonicalMember {

	hasUID := func(label string) bool {
		for _, n := range m.NodeTables {
			if n.Label == label {
				for _, c := range n.Columns {
					if strings.EqualFold(c.Name, "uid") {
						return true
					}
				}
				return false
			}
		}
		return false
	}
	keep := func(in []ladybug.CanonicalMember) []ladybug.CanonicalMember {
		out := make([]ladybug.CanonicalMember, 0, len(in))
		for _, mm := range in {
			if hasUID(mm.From) && hasUID(mm.To) {
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

func canonicalAnchorTables(m *ladybug.CanonicalManifest, label string) []string {
	if label != "" {
		return []string{label}
	}
	return canonicalNodeLabels(m)
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
	if m == nil || m[2] != "" {
		return canonicalPlan{}, false
	}
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
