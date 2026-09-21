package ast

import "testing"

func TestCanonicalUnsafeScan(t *testing.T) {
	for _, query := range []string{
		"MATCH (a)-[r]->(b) RETURN a,r,b",
		"MATCH (n)-[r*1..1 (e, v | WHERE v.name IN ['Checkout','GetOrder'])]->(m) RETURN n.name,m.name",
		"MATCH (a)-[]-(b) RETURN b",
		"MATCH (a)-->(b) RETURN b",
		"MATCH (a)<--(b) RETURN b",
		"MATCH (a)--(b) RETURN b",
		"MATCH (a)-[r:TYPE_A|TYPE_B]->(b) RETURN b",
		"MATCH (a)-[r {note: 'contains:colon'}]->(b) RETURN b",
	} {
		if canonicalUnsafeScan(query) == nil {
			t.Errorf("unsafe query accepted: %s", query)
		}
	}
	for _, query := range []string{
		"MATCH (a:Function) RETURN a.name",
		"RETURN 3--2 AS difference",
		"MATCH (a:Function)-[r:calls__function_function]->(b:Function) RETURN a,r,b",
		"MATCH (a)-[:`type|name`]->(b) RETURN b",
		"RETURN '(a)-[r]->(b)' AS example",
		"MATCH (a) /* -[r]-> */ RETURN a.name // -->\n",
		"MATCH (a)-[:A]->(b) RETURN b UNION MATCH (a)-[:B]->(b) RETURN b",
	} {
		if err := canonicalUnsafeScan(query); err != nil {
			t.Errorf("safe query refused: %s: %v", query, err)
		}
	}
}
