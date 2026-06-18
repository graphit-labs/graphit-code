package wiki

import (
	"fmt"
	"regexp"
	"strings"
)

// reWikiLinkResolvable matches [[target]] and [[target|label]] wikilinks.
var reWikiLinkResolvable = regexp.MustCompile(`\[\[([^\]|]+)(?:\|([^\]]+))?\]\]`)

// ResolveWikiLinksInBody rewrites every [[wikilink]] in body using titlesMap,
// attempting resolution in order:
//  1. Exact title match
//  2. Slugified match (SafeSlug)
//  3. Case-insensitive title or slug match
//  4. Case-insensitive slugified match
//  5. Trigram fuzzy match (threshold 0.55)
//
// Unresolvable links are left unchanged.
// titlesMap maps title OR slug → canonical slug.
func ResolveWikiLinksInBody(body string, titlesMap map[string]string) string {
	return reWikiLinkResolvable.ReplaceAllStringFunc(body, func(match string) string {
		submatches := reWikiLinkResolvable.FindStringSubmatch(match)
		if len(submatches) < 2 {
			return match
		}
		target := strings.TrimSpace(submatches[1])
		label := ""
		if len(submatches) > 2 && submatches[2] != "" {
			label = strings.TrimSpace(submatches[2])
		}

		resolvedSlug, ok := titlesMap[target]
		if !ok {
			resolvedSlug, ok = titlesMap[SafeSlug(target)]
		}
		if !ok {
			targetLower := strings.ToLower(target)
			for t, s := range titlesMap {
				if strings.ToLower(t) == targetLower || strings.ToLower(s) == targetLower {
					resolvedSlug = s
					ok = true
					break
				}
			}
		}
		if !ok {
			targetSlugLower := strings.ToLower(SafeSlug(target))
			for t, s := range titlesMap {
				if strings.ToLower(SafeSlug(t)) == targetSlugLower || strings.ToLower(s) == targetSlugLower {
					resolvedSlug = s
					ok = true
					break
				}
			}
		}
		if !ok {
			resolvedSlug, ok = FindBestFuzzyTitleMatch(target, titlesMap)
		}

		if ok {
			if label != "" {
				return fmt.Sprintf("[[%s|%s]]", resolvedSlug, label)
			}
			return fmt.Sprintf("[[%s]]", resolvedSlug)
		}
		return match
	})
}
