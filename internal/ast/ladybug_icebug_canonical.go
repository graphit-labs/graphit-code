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
	// The tail CONSUMES the arguments of ORDER BY and LIMIT rather than just their
	// keywords. Matching the keyword alone left the arguments inside the projection, where
	// they were rejected as "not a plain property projection" — a refusal that named the
	// wrong rule and hid a clause the planner is able to honour.
	canonicalTraversalPattern = regexp.MustCompile(`(?is)^\s*MATCH\s+(\([^)]*\))\s*-\s*\[\s*(?:([A-Za-z_][A-Za-z0-9_]*)\s*)?:\s*` +
		"`?([A-Za-z_][A-Za-z0-9_]*)`?" +
		`(?:\s*(\*)?\s*(?:(\d+)\s*)?(?:\.\.\s*(\d+))?)?\s*\]\s*(->|-)\s*(\([^)]*\))` +
		`(?:\s+WHERE\s+(.+?))?\s+RETURN\s+(DISTINCT\s+)?((?:.|\n)*?)` +
		`(?:\s+ORDER\s+BY\s+((?:.|\n)+?))?(?:\s+LIMIT\s+(\d+))?\s*;?\s*$`)
	canonicalCountPattern      = regexp.MustCompile(`(?is)^count\s*\(\s*(distinct\s+)?([A-Za-z_][A-Za-z0-9_]*)\.uid\s*\)(?:\s+AS\s+[A-Za-z_][A-Za-z0-9_]*)?$`)
	canonicalProjectionPattern = regexp.MustCompile(`(?is)^([A-Za-z_][A-Za-z0-9_]*)\.([A-Za-z_][A-Za-z0-9_]*)(\s+AS\s+([A-Za-z_][A-Za-z0-9_]*))?$`)
	// canonicalOrderTermPattern reads one ORDER BY term: `x.prop`, `alias`, either with an
	// optional direction. The term is resolved against the projection, never against the
	// table, because the sort runs over the materialized rows.
	canonicalOrderTermPattern = regexp.MustCompile(`(?is)^([A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*)?)(?:\s+(ASC|DESC))?$`)
)

// canonicalOrderKey is one resolved ORDER BY term: the RECORD COLUMN to read and the
// direction to read it in. Resolution happens at parse time so that a term naming
// something the query does not project is refused before any work is done.
type canonicalOrderKey struct {
	column string
	desc   bool
}

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
	orderBy          []canonicalOrderKey
	limit            int
	hasLimit         bool
}

func (k *LadybugBackend) tryCanonicalBoundedTraversal(
	ctx context.Context, cypher string, params map[string]any,
) (*QueryResult, bool, error) {
	plan, refusal, ok := parseCanonicalTraversal(cypher)
	if !ok {
		// A refusal binds only for a LOGICAL relationship type, where this planner is
		// the only route and forwarding the query would enumerate the whole component.
		// A traversal naming a physical member table is the engine's to run exactly as
		// written, so this planner's rules say nothing about it — as does a query that
		// is not a traversal at all.
		if refusal != nil && k.canonicalGroup(refusal.relType) != nil {
			return nil, true, refusal
		}
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
		records := make([]QueryRecord, 0, len(uids))
		for _, uid := range uids {
			records = append(records, QueryRecord{column: uid})
		}
		return &QueryResult{Records: applyCanonicalOrdering(plan, records)}, true, nil
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
	records := make([]QueryRecord, 0, len(collected))
	for _, c := range collected {
		records = append(records, c.record)
	}
	return &QueryResult{Records: applyCanonicalOrdering(plan, records)}, true, nil
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

// resolveCanonicalOrder turns an ORDER BY clause into record columns to sort on.
//
// A term must name something the query PROJECTS, by its text or its alias. Ordering by a
// column the projection does not carry would mean widening the projection behind the
// caller's back and hiding the extra column again, so it is refused with the fix stated.
func resolveCanonicalOrder(clause string, projected map[string]string, reachedVar string,
) ([]canonicalOrderKey, *canonicalRefusal) {
	var keys []canonicalOrderKey
	for _, raw := range splitCommaList(clause) {
		term := strings.TrimSpace(raw)
		if term == "" {
			continue
		}
		om := canonicalOrderTermPattern.FindStringSubmatch(term)
		if om == nil {
			return nil, &canonicalRefusal{
				what: "`" + term + "` is not an ORDER BY term this planner can read.",
				fix:  "Order by a projected property or its alias, optionally with ASC or DESC.",
			}
		}
		column, ok := projected[strings.ToLower(om[1])]
		if !ok {
			return nil, &canonicalRefusal{
				what: "ORDER BY names `" + om[1] + "`, which this query does not project, and the sort runs " +
					"over the rows the traversal materialized rather than over the table.",
				fix: "Project it — `RETURN DISTINCT " + reachedVar + "." +
					strings.TrimPrefix(om[1], reachedVar+".") + ", ... ORDER BY " + om[1] + "` — or order by something you do project.",
			}
		}
		keys = append(keys, canonicalOrderKey{
			column: column,
			desc:   strings.EqualFold(strings.TrimSpace(om[2]), "DESC"),
		})
	}
	return keys, nil
}

// compareCanonicalValues orders two record values of unknown dynamic type. Numbers compare
// numerically — a lexical compare would put line 10 before line 9 — strings compare
// lexically, and a missing value sorts first so that a null never lands in the middle of an
// otherwise ordered column.
func compareCanonicalValues(a, b any) int {
	if a == nil || b == nil {
		switch {
		case a == nil && b == nil:
			return 0
		case a == nil:
			return -1
		default:
			return 1
		}
	}
	af, aNum := canonicalNumeric(a)
	bf, bNum := canonicalNumeric(b)
	if aNum && bNum {
		switch {
		case af < bf:
			return -1
		case af > bf:
			return 1
		default:
			return 0
		}
	}
	as, bs := fmt.Sprintf("%v", a), fmt.Sprintf("%v", b)
	return strings.Compare(as, bs)
}

func canonicalNumeric(v any) (float64, bool) {
	switch n := v.(type) {
	case int:
		return float64(n), true
	case int32:
		return float64(n), true
	case int64:
		return float64(n), true
	case uint32:
		return float64(n), true
	case uint64:
		return float64(n), true
	case float32:
		return float64(n), true
	case float64:
		return n, true
	}
	return 0, false
}

// applyCanonicalOrdering sorts the materialized rows and truncates them to LIMIT.
//
// The canonical key stays as the final tiebreak even when ORDER BY is present: two rows
// equal on every sort key would otherwise come back in whatever order the batched member
// queries happened to produce, which is not stable across runs.
func applyCanonicalOrdering(plan canonicalPlan, records []QueryRecord) []QueryRecord {
	if len(plan.orderBy) > 0 {
		keys := make([]string, len(records))
		for i, r := range records {
			keys[i] = icebugRecordKey(r)
		}
		indexed := make([]int, len(records))
		for i := range indexed {
			indexed[i] = i
		}
		sort.SliceStable(indexed, func(x, y int) bool {
			i, j := indexed[x], indexed[y]
			for _, key := range plan.orderBy {
				c := compareCanonicalValues(records[i][key.column], records[j][key.column])
				if c == 0 {
					continue
				}
				if key.desc {
					return c > 0
				}
				return c < 0
			}
			return keys[i] < keys[j]
		})
		ordered := make([]QueryRecord, len(records))
		for x, i := range indexed {
			ordered[x] = records[i]
		}
		records = ordered
	}
	if plan.hasLimit && plan.limit < len(records) {
		records = records[:plan.limit]
	}
	return records
}

// canonicalRefusal names the rule a query broke and the form that works instead. The
// planner knows which of its rules rejected a query; returning a bare bool threw that
// away and left the caller guessing at the reason from the query text.
//
// relType is carried because a refusal only BINDS for a logical relationship type. A
// traversal naming a physical member table is answered by the engine as written, so no
// rule of this planner has any bearing on it — the caller checks the type against the
// manifest before surfacing any of this.
type canonicalRefusal struct {
	relType string
	what    string
	fix     string
}

func (r *canonicalRefusal) Error() string {
	if r.fix == "" {
		return "canonical catalog: " + r.what
	}
	return "canonical catalog: " + r.what + " " + r.fix
}

// parseCanonicalTraversal recognizes the bounded-reachability shapes whose semantics the
// canonical planner can preserve exactly, including the aggregation forms it computes over
// the reached set. Everything else fails closed.
//
// The three outcomes are distinct and the caller depends on all three: a plan; a refusal,
// which the caller must surface rather than retry; and neither, which means the query is
// not a traversal at all and belongs to the engine.
func parseCanonicalTraversal(cypher string) (canonicalPlan, *canonicalRefusal, bool) {
	m := canonicalTraversalPattern.FindStringSubmatch(cypher)
	if m == nil {
		return canonicalPlan{}, nil, false
	}
	relType := m[3]
	refuse := func(what, fix string) (canonicalPlan, *canonicalRefusal, bool) {
		return canonicalPlan{}, &canonicalRefusal{relType: relType, what: what, fix: fix}, false
	}
	// m[2] is the relationship variable (`[r:CALLS]`); it is metadata for the
	// planner, whose traversal does not need to bind the relationship itself.
	left, okL := parseIcebugNodePattern(m[1])
	right, okR := parseIcebugNodePattern(m[8])
	if !okL || !okR {
		return refuse("both ends of the pattern must be node patterns this planner can read.",
			"Write them as `(v)`, `(v:Label)` or `(v:Label {prop: 'value'})`.")
	}
	if left.variable == right.variable {
		return refuse("both ends of the pattern bind the same variable.",
			"Give them different names: one end is filtered and the other is projected.")
	}

	plan := canonicalPlan{
		relType: relType, minHops: 1,
		directionless: m[7] == "-",
		distinct:      strings.TrimSpace(m[10]) != "",
		returnClause:  strings.TrimSpace(m[11]),
	}
	orderClause := strings.TrimSpace(m[12])
	if raw := strings.TrimSpace(m[13]); raw != "" {
		limit, err := strconv.Atoi(raw)
		if err != nil || limit < 0 {
			return refuse("the LIMIT `"+raw+"` is not a non-negative integer.", "Write a whole number.")
		}
		plan.limit = limit
		plan.hasLimit = true
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
				return refuse(fmt.Sprintf("the hop range *%d..%d is inverted.", plan.minHops, plan.maxHops),
					"Write the lower bound first.")
			}
		}
	}
	returnClause := plan.returnClause
	returnLeft := referencesVariable(returnClause, left.variable)
	returnRight := referencesVariable(returnClause, right.variable)
	if lowered := strings.ToLower(returnClause); strings.Contains(lowered, "label(") ||
		strings.Contains(lowered, "."+strings.ToLower(ladybug.IcebugLabelColumn)) {
		return refuse("a label is not projectable here: on a canonical catalog the label IS the physical table, "+
			"and the logical type "+relType+" spans several of them, so a traversal has no label column to return.",
			"Pin the label in the pattern instead and run one query per label: "+
				"`MATCH (a)-[:"+relType+"]->(b:Function) ... RETURN DISTINCT b.name`.")
	}
	if returnLeft == returnRight {
		return refuse("the RETURN must project exactly one end of the pattern, and this one projects "+
			map[bool]string{true: "both", false: "neither"}[returnLeft]+".",
			"Project one end; the other one carries the filter that anchors the traversal.")
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
		// No check that the counted variable is the reached one: the count pattern is
		// anchored to the WHOLE return clause, so its variable is the only one the
		// clause references, and reachedVar was just derived from that same reference.
		// The two are the same by construction.
		plan.countDistinct = strings.TrimSpace(cm[1]) != ""
		plan.count = true
		plan.distinct = false
		plan.returnClause = ""
	} else if !plan.distinct {
		return refuse("a projection over a traversal must be DISTINCT: the planner materializes the SET of reached "+
			"nodes, so it cannot reproduce one row per path.",
			"Add DISTINCT: `RETURN DISTINCT "+reachedVar+".name`.")
	} else {
		// Every projected item must be `reached.prop [AS alias]`; the materializer runs them
		// per node, so anything richer (collect, arithmetic, paths) would silently change
		// semantics and is refused instead.
		//
		// projected maps every name the caller may legitimately sort by — the projection text
		// and its alias — to the column the RECORD will actually carry.
		projected := map[string]string{}
		for _, item := range splitCommaList(returnClause) {
			pm := canonicalProjectionPattern.FindStringSubmatch(item)
			if pm == nil {
				return refuse("`"+item+"` is not a plain property projection, and the planner evaluates the RETURN "+
					"per reached node — collect(), arithmetic and path expressions would answer a different question.",
					"Project properties only: `"+reachedVar+".property [AS alias]`.")
			}
			if pm[1] != reachedVar {
				return refuse("`"+item+"` projects `"+pm[1]+"`, which is the end the pattern filters rather than the end it reaches.",
					"Project the reached end, `"+reachedVar+"`, or swap which end carries the filter.")
			}
			if strings.EqualFold(pm[2], ladybug.IcebugLabelColumn) {
				return refuse("a label is not projectable here: on a canonical catalog the label IS the physical table.",
					"Pin the label in the pattern instead: `(:"+"Function)`.")
			}
			column := pm[1] + "." + pm[2]
			if alias := strings.TrimSpace(pm[4]); alias != "" {
				column = alias
				projected[strings.ToLower(alias)] = column
			}
			projected[strings.ToLower(pm[1]+"."+pm[2])] = column
		}
		if orderClause != "" {
			keys, refusal := resolveCanonicalOrder(orderClause, projected, reachedVar)
			if refusal != nil {
				refusal.relType = relType
				return canonicalPlan{}, refusal, false
			}
			plan.orderBy = keys
		}
	}
	if orderClause != "" && plan.count {
		return refuse("ORDER BY has nothing to order here: the RETURN is a count, which is a single row.",
			"Drop the ORDER BY, or project the nodes instead of counting them.")
	}

	for _, predicate := range splitTopLevel(strings.TrimSpace(m[9]), "AND") {
		a := referencesVariable(predicate, plan.anchor.variable)
		r := referencesVariable(predicate, plan.reached.variable)
		if a && r {
			return refuse("the predicate `"+predicate+"` compares the two ends of the pattern, which the planner "+
				"resolves as two independent sets and cannot join.",
				"Filter one end at a time.")
		}
		switch {
		case r:
			plan.reachedPreds = append(plan.reachedPreds, predicate)
		case a:
			plan.anchorPreds = append(plan.anchorPreds, predicate)
		default:
			return refuse("the predicate `"+predicate+"` references neither end of the pattern.",
				"Every predicate must filter `"+plan.anchor.variable+"` or `"+plan.reached.variable+"`.")
		}
	}
	if !plan.anchor.selective(plan.anchorPreds) {
		return refuse("nothing filters `"+plan.anchor.variable+"`, the end the traversal starts from, so the plan "+
			"would begin with every node of that label.",
			"Filter the starting end — a property in the pattern or a WHERE predicate on it.")
	}
	return plan, nil, true
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
