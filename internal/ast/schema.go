package ast

import (
	"context"
	"fmt"
	"strings"
)

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

func CreateGraphSchema(ctx context.Context, db GraphDB, labels ...string) error {
	for _, label := range labels {
		q := fmt.Sprintf(`CREATE CONSTRAINT %s_unique IF NOT EXISTS FOR (n:%s) REQUIRE n.name IS UNIQUE`, label, label)
		if _, err := db.Execute(ctx, q, nil); err != nil {
			q2 := fmt.Sprintf(`CREATE INDEX FOR (n:%s) ON (n.name)`, label)
			db.Execute(ctx, q2, nil)
		}

		q = fmt.Sprintf(`CREATE INDEX %s_lang IF NOT EXISTS FOR (n:%s) ON (n.lang)`, label, label)
		if _, err := db.Execute(ctx, q, nil); err != nil {
			q2 := fmt.Sprintf(`CREATE INDEX FOR (n:%s) ON (n.lang)`, label)
			db.Execute(ctx, q2, nil)
		}

		qn := fmt.Sprintf(`CREATE INDEX %s_name IF NOT EXISTS FOR (n:%s) ON (n.name)`, label, label)
		if _, err := db.Execute(ctx, qn, nil); err != nil {
			qn2 := fmt.Sprintf(`CREATE INDEX FOR (n:%s) ON (n.name)`, label)
			db.Execute(ctx, qn2, nil)
		}

		qc := fmt.Sprintf(`CREATE INDEX %s_cluster IF NOT EXISTS FOR (n:%s) ON (n.cluster)`, label, label)
		if _, err := db.Execute(ctx, qc, nil); err != nil {
			qc2 := fmt.Sprintf(`CREATE INDEX FOR (n:%s) ON (n.cluster)`, label)
			db.Execute(ctx, qc2, nil)
		}
	}

	return nil
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
	buf.WriteString("Node labels and key properties:\n")

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
		fmt.Fprintf(&buf, "- %s(%s)\n", t, strings.Join(props, ", "))
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
