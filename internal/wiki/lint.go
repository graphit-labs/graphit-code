package wiki

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"time"
)

type LintConfig struct {
	Deep bool

	Fix bool

	StaleDays int
}

type LintReport struct {
	TotalPages    int
	Orphans       []string
	BrokenLinks   []BrokenLinkInfo
	StalePages    []string
	EmptyPages    []string
	MissingFields []FieldIssue
	// WeakFields lists pages missing a RECOMMENDED OKF field. It is reported and never
	// counted in Errors: OKF v0.2 §11 forbids rejecting a concept over an optional field,
	// so these are quality hints, not conformance failures.
	WeakFields   []FieldIssue
	Errors       int
	FixesApplied int
}

type FieldIssue struct {
	Page          string
	MissingFields []string
}

func (r *LintReport) HasIssues() bool {
	return r.Errors > 0
}

func (r *LintReport) Summary() string {
	if r.Errors == 0 {
		return fmt.Sprintf("✓ %d pages — no issues found", r.TotalPages)
	}
	return fmt.Sprintf("⚠ %d pages — %d issue(s) found", r.TotalPages, r.Errors)
}

func LintWiki(wikiDir string, cfg LintConfig) (*LintReport, error) {
	report := &LintReport{}

	graph, err := BuildCrossRefGraph(wikiDir)
	if err != nil {
		return nil, fmt.Errorf("building cross-ref graph: %w", err)
	}

	report.TotalPages = len(graph.AllPages)

	report.Orphans = OrphanPages(graph)
	report.Errors += len(report.Orphans)

	report.BrokenLinks = BrokenLinks(graph)
	report.Errors += len(report.BrokenLinks)

	for page := range graph.AllPages {
		if page == "index" || page == "log" {
			continue
		}

		filePath := filepath.Join(wikiDir, page+".md")
		data, err := os.ReadFile(filePath)
		if err != nil {
			continue
		}
		content := string(data)

		if cfg.StaleDays > 0 {
			if isStale(content, cfg.StaleDays) {
				report.StalePages = append(report.StalePages, page)
				report.Errors++
			}
		}

		body := StripFrontmatter(content)
		body = stripBacklinksSection(body)
		wordCount := len(strings.Fields(body))
		if wordCount <= 10 {
			report.EmptyPages = append(report.EmptyPages, page)
			report.Errors++
		}

		missing := checkFrontmatter(content)
		if len(missing) > 0 {
			report.MissingFields = append(report.MissingFields, FieldIssue{
				Page:          page,
				MissingFields: missing,
			})
			report.Errors++
		}

		if weak := missingRecommendedFields(content); len(weak) > 0 {
			report.WeakFields = append(report.WeakFields, FieldIssue{
				Page:          page,
				MissingFields: weak,
			})
		}
	}

	sort.Strings(report.StalePages)
	sort.Strings(report.EmptyPages)

	if cfg.Fix {

		xrefResult, err := InjectBacklinks(wikiDir, graph)
		if err == nil {
			report.FixesApplied += xrefResult.BacklinksAdded
		}
	}

	return report, nil
}

// reFMField matches a top-level frontmatter key. The character class is not `\w`
// because OKF keys are not all bare words: a producer may write `generated.at` or
// `usage_window`, and `\w+` stops at the dot, so the key reads as absent rather than
// as present-with-a-dotted-name. This project shipped exactly that: every page carried
// a key the scanner could not see.
var reFMField = regexp.MustCompile(`(?m)^([A-Za-z_][\w.-]*):`)

// requiredFields is OKF's conformance contract, not this project's taste.
//
// OKF v0.2 §11 requires one thing of a concept document: a parseable frontmatter block
// containing a non-empty `type`. Everything else — `title`, `description`, `tags`,
// `sources`, `generated` — is RECOMMENDED, and §11 is explicit that a consumer MUST NOT
// reject a document for missing an optional field.
//
// The list used to be {title, tags, updated}, which predates OKF and outlived it: after
// the generator moved to `generated.at` no page carried `updated` any more, so the lint
// reported 240 of 242 pages as malformed while the wiki was fine.
var requiredFields = []string{"type"}

// recommendedFields are reported separately: their absence is a quality signal, never a
// conformance failure, so they must not be counted as errors.
var recommendedFields = []string{"title", "description"}

func checkFrontmatter(content string) []string {
	fm := extractFrontmatter(content)
	if fm == "" {
		return requiredFields
	}

	present := make(map[string]bool)
	for _, m := range reFMField.FindAllStringSubmatch(fm, -1) {
		present[strings.ToLower(m[1])] = true
	}

	var missing []string
	for _, f := range requiredFields {
		if !present[f] {
			missing = append(missing, f)
		}
	}
	return missing
}

func missingRecommendedFields(content string) []string {
	fm := extractFrontmatter(content)
	if fm == "" {
		return recommendedFields
	}
	present := make(map[string]bool)
	for _, m := range reFMField.FindAllStringSubmatch(fm, -1) {
		present[strings.ToLower(m[1])] = true
	}
	var missing []string
	for _, f := range recommendedFields {
		if !present[f] {
			missing = append(missing, f)
		}
	}
	return missing
}

func extractFrontmatter(content string) string {
	trimmed := strings.TrimSpace(content)
	if !strings.HasPrefix(trimmed, "---") {
		return ""
	}
	lines := strings.Split(trimmed, "\n")
	var fmLines []string
	started := false
	for _, line := range lines {
		t := strings.TrimSpace(line)
		if t == "---" {
			if !started {
				started = true
				continue
			}
			break
		}
		if started {
			fmLines = append(fmLines, line)
		}
	}
	return strings.Join(fmLines, "\n")
}

// The two shapes OKF allows for the trust family's timestamp (§5.2): `generated` is a
// mapping, written inline or as a block. There is no flat `generated.at` key and no
// `updated` key — the first was this project's misreading of the spec's prose path
// notation, the second predates OKF entirely. Neither is read: the wiki is regenerated
// from its sources, so there is no old page to stay compatible with.
var (
	reFMGeneratedInline = regexp.MustCompile(`(?m)^generated:\s*\{[^}]*\bat:\s*([^,}]+)`)
	reFMGeneratedBlock  = regexp.MustCompile(`(?m)^generated:\s*$[\s\S]*?^\s+at:\s*(.+)$`)
)

// generatedAt reads the instant a page's content last meaningfully changed (§5.2).
func generatedAt(fm string) (time.Time, bool) {
	for _, re := range []*regexp.Regexp{reFMGeneratedInline, reFMGeneratedBlock} {
		m := re.FindStringSubmatch(fm)
		if m == nil {
			continue
		}
		if t, ok := parseFMInstant(m[1]); ok {
			return t, true
		}
	}
	return time.Time{}, false
}

var reFMStaleAfter = regexp.MustCompile(`(?m)^stale_after:\s*(.+)$`)

func parseFMInstant(raw string) (time.Time, bool) {
	s := strings.TrimSpace(raw)
	s = strings.Trim(s, "\"'")
	s = strings.TrimSpace(s)
	for _, layout := range []string{time.RFC3339, "2006-01-02T15:04:05Z", "2006-01-02 15:04:05", "2006-01-02"} {
		if t, err := time.Parse(layout, s); err == nil {
			return t, true
		}
	}
	return time.Time{}, false
}

func isStale(content string, staleDays int) bool {
	fm := extractFrontmatter(content)

	if m := reFMStaleAfter.FindStringSubmatch(fm); m != nil {
		if t, ok := parseFMInstant(m[1]); ok {
			return !time.Now().Before(t)
		}
	}

	t, ok := generatedAt(fm)
	if !ok {
		// No timestamp at all. OKF §11 forbids rejecting a concept for a missing
		// optional field, and a page whose age is unknown is not a page known to be
		// old — reporting it as stale is inventing a fact.
		return false
	}
	return time.Since(t).Hours() > float64(staleDays*24)
}
