package ast

import (
	"context"
	"fmt"
	"sort"
	"strings"
)

// NoLangGroup names the group holding nodes that carry no `lang` — call-target stubs,
// directories, and anything else the parsers create without attributing it to a file.
// It is deliberately not a valid language name, so it can never collide with one.
const NoLangGroup = "(no language)"

// SchemaLabelCount is one node label and how many nodes of it a single language produced.
type SchemaLabelCount struct {
	Label string
	Count int
}

// SchemaLangGroup is a language together with the node labels its files actually produced.
type SchemaLangGroup struct {
	Lang   string
	Count  int
	Labels []SchemaLabelCount
}

// DisplayLang is the group's name as a human or an agent should read it, which for the
// language-less group is a label rather than an empty string.
// schemaSharedShapeMin is how many labels must share a property set before they are
// printed as a group. Two is a wash — the grouped form is as long as two plain lines —
// so grouping starts paying at three.
const schemaSharedShapeMin = 3

func (g SchemaLangGroup) DisplayLang() string {
	if g.Lang == "" {
		return NoLangGroup
	}
	return g.Lang
}

// SchemaLangGroups groups the graph's node labels by the language that produced them,
// so a caller can tell which labels belong to Go and which to CSS instead of reading one
// flat run of every label in the database.
//
// The counts are live: a label with no nodes does not appear here at all, and the same
// label appears under every language that produced it — the grouping splits the flat
// count, it does not pick one owner per label.
//
// NOTE: best-effort on purpose — `n.lang` only binds while a label that carries it is
// in the schema, so a rebuild-time partial schema degrades to an empty grouping instead
// of failing the caller.
func SchemaLangGroups(ctx context.Context, db GraphDB) []SchemaLangGroup {
	q := `MATCH (n) RETURN DISTINCT n.lang AS lang, label(n) AS label, count(n) AS count ORDER BY count DESC`
	res, err := db.Query(ctx, q, nil)
	if err != nil {
		return []SchemaLangGroup{}
	}

	byLang := make(map[string]*SchemaLangGroup)
	order := make([]string, 0, len(res.Records))

	for _, rec := range res.Records {
		label, ok := rec["label"].(string)
		if !ok || label == "" {
			continue
		}
		lang, _ := rec["lang"].(string)
		count := toInt(rec["count"])

		g := byLang[lang]
		if g == nil {
			g = &SchemaLangGroup{Lang: lang}
			byLang[lang] = g
			order = append(order, lang)
		}
		g.Count += count
		g.Labels = append(g.Labels, SchemaLabelCount{Label: label, Count: count})
	}

	// Biggest language first, but the language-less group is never a language, so it
	// sorts last regardless of how many nodes it holds.
	sort.SliceStable(order, func(i, j int) bool {
		a, b := order[i], order[j]
		if (a == "") != (b == "") {
			return b == ""
		}
		return byLang[a].Count > byLang[b].Count
	})

	out := make([]SchemaLangGroup, 0, len(order))
	for _, lang := range order {
		out = append(out, *byLang[lang])
	}
	return out
}

func ActiveNodeLabels(ctx context.Context, db GraphDB) []string {
	res, err := db.Query(ctx, "CALL show_tables() RETURN *", nil)
	if err != nil {
		return nil
	}
	var labels []string
	for _, rec := range res.Records {
		name, ok1 := rec["name"].(string)
		typ, ok2 := rec["type"].(string)
		if !ok1 || !ok2 || typ != "NODE" {
			continue
		}
		labels = append(labels, name)
	}
	return labels
}

// writeSchemaLangSection renders the per-language grouping that opens the schema, so a
// reader sees which labels belong to which language before meeting the flat property
// reference below it.
//
// An empty grouping writes nothing at all: a graph mid-rebuild, or one with no nodes yet,
// should fall through to the property reference rather than show an empty heading.
func writeSchemaLangSection(buf *strings.Builder, groups []SchemaLangGroup) {
	if len(groups) == 0 {
		return
	}

	buf.WriteString("Node labels by language (live node counts):\n")
	for _, g := range groups {
		parts := make([]string, 0, len(g.Labels))
		for _, l := range g.Labels {
			parts = append(parts, fmt.Sprintf("%s(%d)", l.Label, l.Count))
		}
		fmt.Fprintf(buf, "- %s [%d]: %s\n", g.DisplayLang(), g.Count, strings.Join(parts, ", "))
	}

	buf.WriteString("\nHow to read the grouping above:\n")
	buf.WriteString("- A label is listed under every language that produced it, so the same label can appear more than once; the grouping splits the total, it does not assign one owner per label.\n")
	fmt.Fprintf(buf, "- %q holds nodes whose label carries no `lang` at all, such as Directory — not nodes whose language is unknown. An unresolved call target keeps the language of the file that called it.\n", NoLangGroup)
	buf.WriteString("- A label absent from this grouping has no nodes in the graph, so matching it returns nothing even though its table exists below.\n\n")
}

func SchemaText(ctx context.Context, db GraphDB) (string, error) {
	res, err := db.Query(ctx, "CALL show_tables() RETURN *", nil)
	if err != nil {
		return "", err
	}

	var nodeTables []string
	var relTables []string

	for _, rec := range res.Records {
		name, ok1 := rec["name"].(string)
		typ, ok2 := rec["type"].(string)
		if !ok1 || !ok2 {
			continue
		}
		switch typ {
		case "NODE":
			nodeTables = append(nodeTables, name)
		case "REL":
			relTables = append(relTables, name)
		}
	}

	var sortStrs = func(s []string) {
		for i := 0; i < len(s); i++ {
			for j := i + 1; j < len(s); j++ {
				if s[i] > s[j] {
					s[i], s[j] = s[j], s[i]
				}
			}
		}
	}
	sortStrs(nodeTables)
	sortStrs(relTables)

	var buf strings.Builder

	writeSchemaLangSection(&buf, SchemaLangGroups(ctx, db))

	buf.WriteString("Node labels and key properties:\n")

	// Labels are grouped by their property set instead of one line each.
	//
	// Almost every label in this schema is an ENTITY label, and they all carry the
	// identical 16-property row — only File, Directory and Module differ. Printed one
	// per line that is ~25 repetitions of the same list, which an agent pays for in
	// context on every session while learning nothing after the first one. Grouping
	// loses no information: a label's properties are still stated exactly, once.
	sigOrder := make([]string, 0, len(nodeTables))
	byLabel := make(map[string]string, len(nodeTables))
	labelsBySig := make(map[string][]string, len(nodeTables))
	for _, t := range nodeTables {
		info, err := db.Query(ctx, fmt.Sprintf("CALL table_info('%s') RETURN *", t), nil)
		if err != nil {
			continue
		}

		var props []string
		for _, irec := range info.Records {
			pname, ok := irec["name"].(string)
			if ok {
				props = append(props, pname)
			}
		}
		sig := strings.Join(props, ", ")
		if _, seen := labelsBySig[sig]; !seen {
			sigOrder = append(sigOrder, sig)
		}
		byLabel[t] = sig
		labelsBySig[sig] = append(labelsBySig[sig], t)
	}

	// Unique shapes first and one per line, because those are the ones worth reading
	// individually — File.path vs an entity's path is exactly the distinction that
	// makes a query crash with "Cannot find property".
	for _, t := range nodeTables {
		if sig, ok := byLabel[t]; ok && len(labelsBySig[sig]) < schemaSharedShapeMin {
			fmt.Fprintf(&buf, "- %s(%s)\n", t, sig)
		}
	}
	for _, sig := range sigOrder {
		shared := labelsBySig[sig]
		if len(shared) < schemaSharedShapeMin {
			continue
		}
		fmt.Fprintf(&buf, "- These %d labels all carry the SAME properties: %s\n  (%s)\n",
			len(shared), strings.Join(shared, ", "), sig)
	}

	buf.WriteString("\nRelationships:\n")
	for _, r := range relTables {
		info, err := db.Query(ctx, fmt.Sprintf("CALL table_info('%s') RETURN *", r), nil)
		if err != nil {
			continue
		}

		var props []string
		for _, irec := range info.Records {
			pname, ok := irec["name"].(string)
			if ok {
				props = append(props, pname)
			}
		}

		if len(props) > 0 {
			_, _ = fmt.Fprintf(&buf, "- (any)-[:%s {%s}]->(any)\n", r, strings.Join(props, ", "))
		} else {
			_, _ = fmt.Fprintf(&buf, "- (any)-[:%s]->(any)\n", r)
		}
	}

	buf.WriteString("\n*(Note: Relationship source/destination node types are strictly typed in LadybugDB but simplified as `any` here for brevity. You can infer valid connections from common sense, e.g., File-CONTAINS->Function)*\n")

	return buf.String(), nil
}
