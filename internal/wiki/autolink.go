package wiki

import (
	"fmt"
	"regexp"
	"sort"
	"strings"
)

// Pre-compiled regexes for AutoLinkLine — protects existing links and code spans
// from being double-linked.
var (
	reAutoLinkCode = regexp.MustCompile("`[^`]+`")
	reAutoLinkWiki = regexp.MustCompile(`\[\[([^\]]+)\]\]`)
	reAutoLinkMd   = regexp.MustCompile(`\[[^\]]+\]\([^)]+\)`)
)

// CompiledTarget is a pre-compiled auto-link target for a single wiki page.
type CompiledTarget struct {
	Slug  string
	re    *regexp.Regexp
	lower string // pre-lowered term for fast strings.Contains pre-filter
}

// BuildAutoLinkTargets pre-compiles regex targets from a title→slug map.
// The resulting slice is sorted longest-first so that longer terms are
// matched before their shorter substrings.
// Call once per wiki generation cycle.
func BuildAutoLinkTargets(titles map[string]string) []CompiledTarget {
	type rawTarget struct {
		term string
		slug string
	}

	var targets []rawTarget
	for title, slug := range titles {
		targets = append(targets, rawTarget{term: title, slug: slug})
		if title != slug {
			targets = append(targets, rawTarget{term: slug, slug: slug})
			targets = append(targets, rawTarget{term: strings.ReplaceAll(slug, "_", " "), slug: slug})
		}
	}

	sort.Slice(targets, func(i, j int) bool {
		return len(targets[i].term) > len(targets[j].term)
	})

	seenTerms := make(map[string]string)
	var result []CompiledTarget
	for _, t := range targets {
		termLower := strings.ToLower(t.term)
		if termLower == "" || len(termLower) < 3 {
			continue
		}
		if _, ok := seenTerms[termLower]; !ok {
			seenTerms[termLower] = t.slug
			pattern := `\b(?i)` + regexp.QuoteMeta(t.term) + `\b`
			re, err := regexp.Compile(pattern)
			if err != nil {
				continue
			}
			result = append(result, CompiledTarget{
				Slug:  t.slug,
				re:    re,
				lower: termLower,
			})
		}
	}
	return result
}

// AutoLinkContent walks body line-by-line and inserts [term](slug.md) markdown links
// for every CompiledTarget whose term appears in the text, skipping existing
// wikilinks, markdown links, code spans, fenced code blocks, and frontmatter.
//
// The link form is the one OKF specifies for links between concepts (§6.1). Legacy
// [[wikilinks]] are still RECOGNISED — they are protected from double-linking below, and
// crossref.go still reads them — but they are no longer PRODUCED.
//
// Returns the modified body and a sorted slice of slugs that were auto-linked.
func AutoLinkContent(body string, targets []CompiledTarget, currentSlug string) (string, []string) {
	lines := strings.Split(body, "\n")
	inCodeBlock := false
	inFrontmatter := false

	var newLines []string
	autoLinkedRefs := make(map[string]bool)

	for _, line := range lines {
		trimmed := strings.TrimSpace(line)
		if trimmed == "---" {
			inFrontmatter = !inFrontmatter
			newLines = append(newLines, line)
			continue
		}
		if inFrontmatter {
			newLines = append(newLines, line)
			continue
		}
		if strings.HasPrefix(trimmed, "```") {
			inCodeBlock = !inCodeBlock
			newLines = append(newLines, line)
			continue
		}
		if inCodeBlock {
			newLines = append(newLines, line)
			continue
		}

		newLine := AutoLinkLine(line, targets, currentSlug, autoLinkedRefs)
		newLines = append(newLines, newLine)
	}

	var refs []string
	for r := range autoLinkedRefs {
		refs = append(refs, r)
	}
	sort.Strings(refs)

	return strings.Join(newLines, "\n"), refs
}

// autoLinkLine processes a single line, protecting existing links and code spans
// with placeholder tokens before applying auto-link regexes.
func AutoLinkLine(line string, targets []CompiledTarget, currentSlug string, autoLinkedRefs map[string]bool) string {
	var wlPlaceholders []string
	var mlPlaceholders []string
	var cdPlaceholders []string

	line = reAutoLinkCode.ReplaceAllStringFunc(line, func(match string) string {
		wlPlaceholders = append(wlPlaceholders, match)
		return fmt.Sprintf("___CD_PLACEHOLDER_%d___", len(wlPlaceholders)-1)
	})

	line = reAutoLinkWiki.ReplaceAllStringFunc(line, func(match string) string {
		mlPlaceholders = append(mlPlaceholders, match)
		return fmt.Sprintf("___WL_PLACEHOLDER_%d___", len(mlPlaceholders)-1)
	})

	line = reAutoLinkMd.ReplaceAllStringFunc(line, func(match string) string {
		cdPlaceholders = append(cdPlaceholders, match)
		return fmt.Sprintf("___ML_PLACEHOLDER_%d___", len(cdPlaceholders)-1)
	})

	lineLower := strings.ToLower(line)
	var autoWlPlaceholders []string
	for _, target := range targets {
		if target.Slug == currentSlug {
			continue
		}
		if !strings.Contains(lineLower, target.lower) {
			continue
		}
		line = target.re.ReplaceAllStringFunc(line, func(match string) string {
			autoLinkedRefs[target.Slug] = true
			replacement := fmt.Sprintf("[%s](%s.md)", match, target.Slug)
			autoWlPlaceholders = append(autoWlPlaceholders, replacement)
			return fmt.Sprintf("___AUTO_WL_PLACEHOLDER_%d___", len(autoWlPlaceholders)-1)
		})
	}

	for i, val := range autoWlPlaceholders {
		line = strings.ReplaceAll(line, fmt.Sprintf("___AUTO_WL_PLACEHOLDER_%d___", i), val)
	}
	for i, val := range cdPlaceholders {
		line = strings.ReplaceAll(line, fmt.Sprintf("___ML_PLACEHOLDER_%d___", i), val)
	}
	for i, val := range mlPlaceholders {
		line = strings.ReplaceAll(line, fmt.Sprintf("___WL_PLACEHOLDER_%d___", i), val)
	}
	for i, val := range wlPlaceholders {
		line = strings.ReplaceAll(line, fmt.Sprintf("___CD_PLACEHOLDER_%d___", i), val)
	}

	return line
}
