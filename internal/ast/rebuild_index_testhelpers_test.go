package ast

// newRebuildIndexForTest builds the index with the permissive default rules, for tests
// whose subject is not target resolution.
//
// nil rules is a real state, not a test shortcut: a project whose grammar files fail to
// load still has to rebuild, and every TargetRules method treats nil as "any label this
// grammar declares, falling back to a stub".
func newRebuildIndexForTest(entries map[string]*parseCacheEntry) *rebuildIndex {
	return newRebuildIndex(entries, nil)
}

// dmlTargetRules mirrors what the SQL-family grammars declare: a DML target is a schema
// object, and one that nothing declares is still a table.
func dmlTargetRules(langs ...string) *TargetRules {
	decl := TargetRuleDecl{Labels: []string{LabelTable, LabelView}, Fallback: LabelTable}
	rules := map[string]TargetRuleDecl{}
	for _, rel := range []string{RelSelects, RelInserts, RelUpdates, RelDeletes} {
		rules[rel] = decl
	}
	files := make([]ExternalQueryFile, 0, len(langs))
	for _, l := range langs {
		files = append(files, ExternalQueryFile{Language: l, TargetRules: rules})
	}
	return BuildTargetRules(files)
}

// newRebuildIndexWithDML is the index a SQL corpus gets: DML targets resolve to schema
// objects and fall back to a table.
func newRebuildIndexWithDML(entries map[string]*parseCacheEntry) *rebuildIndex {
	return newRebuildIndex(entries, dmlTargetRules("sql", "plsql"))
}
