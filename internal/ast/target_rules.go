package ast

import "strings"

// Fallback kinds for TargetRule.Fallback. Anything else is taken as a graph label.
const (
	TargetFallbackStub = "stub"
	TargetFallbackFile = "file"
)

type TargetRule struct {
	// Labels the target may resolve to. Empty means every label the grammar declares.
	Labels map[string]bool
	// Fallback is "stub", "file", or a graph label. Empty means "stub".
	Fallback string
}

func (r TargetRule) allows(label string) bool {
	if len(r.Labels) == 0 {
		return true
	}
	return r.Labels[label]
}

func (r TargetRule) fallbackKind() string {
	if r.Fallback == "" {
		return TargetFallbackStub
	}
	return r.Fallback
}

// TargetRules is the resolution table for every language the grammars describe.
type TargetRules struct {
	byLang   map[string]map[string]TargetRule
	declared map[string]map[string]bool
}

// BuildTargetRules reads the resolution rules out of the loaded grammar files.
//
// Later files win per (language, relation): the loader has already merged the runtime,
// user and project levels, so what arrives here is one decision per language.
func BuildTargetRules(files []ExternalQueryFile) *TargetRules {
	t := &TargetRules{
		byLang:   make(map[string]map[string]TargetRule, len(files)),
		declared: make(map[string]map[string]bool, len(files)),
	}

	for _, f := range files {
		lang := f.Language
		if lang == "" {
			continue
		}
		if t.declared[lang] == nil {
			t.declared[lang] = make(map[string]bool)
		}
		if t.byLang[lang] == nil {
			t.byLang[lang] = make(map[string]TargetRule)
		}

		for rel, decl := range f.TargetRules {
			rule := TargetRule{Fallback: decl.Fallback}
			for _, l := range decl.Labels {
				if l == "" {
					continue
				}
				if rule.Labels == nil {
					rule.Labels = make(map[string]bool)
				}
				rule.Labels[l] = true
			}
			t.byLang[lang][strings.TrimSpace(rel)] = rule
		}

		for _, q := range f.Queries {
			for _, l := range []string{q.GraphLabel, q.ValueLabel, q.ParentLabel} {
				if l != "" {
					t.declared[lang][l] = true
				}
			}

			rel := strings.TrimSpace(q.RelationType)
			if rel == "" {
				continue
			}
			rule := t.byLang[lang][rel]
			for _, l := range q.TargetLabels {
				if l == "" {
					continue
				}
				if rule.Labels == nil {
					rule.Labels = make(map[string]bool)
				}
				rule.Labels[l] = true
			}
			if q.TargetFallback != "" {
				rule.Fallback = q.TargetFallback
			}
			t.byLang[lang][rel] = rule
		}
	}
	return t
}

// ForRelation is the rule for a relation the grammar declared.
func (t *TargetRules) ForRelation(lang, relType string) TargetRule {
	if t == nil {
		return TargetRule{}
	}
	rule, ok := t.byLang[lang][relType]
	if !ok {
		// A relation the grammar never declared still has to resolve — the engine
		// emits some of them itself. Default to the grammar's own labels.
		return TargetRule{Labels: t.declared[lang]}
	}
	if len(rule.Labels) == 0 {
		rule.Labels = t.declared[lang]
	}
	return rule
}

// ForDocumentation is the rule for the comment → documented entity edge.
//
// That edge is the ENGINE's, not any grammar's: every language gets it from
// comment_types, and no yaml declares it. So its rule is fixed here rather than read
// from a query — but only its policy is fixed. The labels it resolves against are
// still whatever the grammar declares, so a new grammar's comments find that
// grammar's declarations.
//
// The fallback is the file, because a comment that documents nothing nameable still
// belongs to a file, and that is a fact rather than an invention. It is also why this
// rule cannot be shared with a grammar's own REFERENCES: nine grammars declare that
// relation, and for the SQL family an unresolved one legitimately means a Table.
func (t *TargetRules) ForDocumentation(lang string) TargetRule {
	if t == nil {
		return TargetRule{Fallback: TargetFallbackFile}
	}
	return TargetRule{Labels: t.declared[lang], Fallback: TargetFallbackFile}
}

// targetRulesFor reads the grammar rules for a project.
//
// It reads the EFFECTIVE files — the project's own merged onto the user's and the
// runtime's — and not LoadExternalQueries, which is the project directory alone. Most
// projects ship no grammar of their own, so that one answers with nothing and every
// rule silently falls back to the permissive default: the symptom is a call resolving
// to a Comment, because "any label this grammar declares" was never narrowed.
//
// Called once per rebuild rather than cached here: the loader below already caches per
// directory, and a rule table held across rebuilds would resolve targets by a grammar
// the user has since edited. nil is a valid answer — every TargetRules method treats it
// as the permissive default, so a project with no grammar files still rebuilds.
func targetRulesFor(projectDir string) *TargetRules {
	return BuildTargetRules(allEffectiveQueryFiles(projectDir))
}
