package ast

import (
	"regexp"
	"strings"
	"testing"
)

// skillQueryTemplate is one Cypher example lifted out of the generated skill text,
// carried with enough context to name it in a failure.
type skillQueryTemplate struct {
	line  int
	query string
}

var (
	skillMatchLine = regexp.MustCompile(`(?i)\bMATCH\s*\(`)
	// A template is embedded in prose, tables and fenced blocks. What identifies it is
	// a MATCH ... RETURN on one line, which is how every example in the skill is written.
	skillReturnLine = regexp.MustCompile(`(?i)\bRETURN\b`)
)

// extractSkillQueries pulls every Cypher example out of the generated skill text.
// The skill embeds them three ways — inside fenced blocks, inside markdown table cells
// wrapped in backticks, and inline in prose — so extraction works line by line and
// unwraps backticks rather than assuming a single container.
func extractSkillQueries(text string) []skillQueryTemplate {
	var out []skillQueryTemplate
	for i, raw := range strings.Split(text, "\n") {
		for _, candidate := range skillQueryCandidates(raw) {
			if skillMatchLine.MatchString(candidate) && skillReturnLine.MatchString(candidate) {
				out = append(out, skillQueryTemplate{line: i + 1, query: candidate})
			}
		}
	}
	return out
}

func skillQueryCandidates(line string) []string {
	trimmed := strings.TrimSpace(line)
	if trimmed == "" || strings.HasPrefix(trimmed, "#") {
		return nil
	}
	// Table rows and prose wrap a query in backticks; a fenced block does not.
	if strings.Contains(trimmed, "`") {
		var out []string
		parts := strings.Split(trimmed, "`")
		for i := 1; i < len(parts); i += 2 {
			out = append(out, strings.TrimSpace(parts[i]))
		}
		return out
	}
	return []string{trimmed}
}

// The skill text is the instruction set every agent copies from, and its Cypher examples
// were never executed by anything. The canonical planner lives in this same package, so
// the documentation can be validated by the code it documents: any example the planner
// would refuse is an example that teaches a failing query.
func TestSkillQueryTemplatesAreAcceptedByTheCanonicalPlanner(t *testing.T) {
	t.Parallel()

	templates := extractSkillQueries(ASTRuleContent())
	if len(templates) == 0 {
		t.Fatal("extracted no Cypher examples from the generated skill text: the extractor is broken, not the skill")
	}

	var refused int
	for _, tpl := range templates {
		_, refusal, ok := parseCanonicalTraversal(tpl.query)
		if ok || refusal == nil {
			// Either a valid traversal plan, or not a traversal at all — a node-only
			// query is unaffected by the canonical rules and is the engine's to run.
			continue
		}
		refused++
		t.Errorf("skill line %d teaches a query the planner refuses:\n  query:   %s\n  refusal: %s",
			tpl.line, tpl.query, refusal.Error())
	}
	t.Logf("checked %d Cypher examples from the skill, %d refused", len(templates), refused)
}

// An untyped relationship is the failure the planner cannot protect anyone from: the
// pattern never reaches the canonical planner, so it is answered against the PHYSICAL
// member tables — reverse ones included — and returns rows that look plausible while
// projecting the wrong end and leaking internal table names. A refusal is visible; this
// is not, which is why the skill must never show one.
func TestSkillQueryTemplatesAlwaysNameTheRelationshipType(t *testing.T) {
	t.Parallel()

	untyped := regexp.MustCompile(`-\s*\[\s*[A-Za-z_][A-Za-z0-9_]*\s*\]\s*->|-\s*\[\s*\]\s*->`)

	for _, tpl := range extractSkillQueries(ASTRuleContent()) {
		if untyped.MatchString(tpl.query) {
			t.Errorf("skill line %d teaches an untyped traversal, which silently answers from physical member tables:\n  %s",
				tpl.line, tpl.query)
		}
	}
}
