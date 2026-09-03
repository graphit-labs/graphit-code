package ast

func newRebuildIndexForTest(entries map[string]*parseCacheEntry) *rebuildIndex {
	return newRebuildIndex(entries, nil)
}

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

func newRebuildIndexWithDML(entries map[string]*parseCacheEntry) *rebuildIndex {
	return newRebuildIndex(entries, dmlTargetRules("sql", "plsql"))
}
