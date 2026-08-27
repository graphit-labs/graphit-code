package wiki

import (
	"fmt"
	"regexp"
	"strings"
)

// reWikiLinkResolvable matches [[target]] and [[target|label]] wikilinks.
var reWikiLinkResolvable = regexp.MustCompile(`\[\[([^\]|]+)(?:\|([^\]]+))?\]\]`)

// ResolveWikiLinksInBody rewrites every [[wikilink]] in body using titlesMap into
// OKF compliant standard Markdown links [label](resolvedSlug.md).
func ResolveWikiLinksInBody(body string, titlesMap map[string]string) string {
	return reWikiLinkResolvable.ReplaceAllStringFunc(body, func(match string) string {
		submatches := reWikiLinkResolvable.FindStringSubmatch(match)
		if len(submatches) < 2 {
			return match
		}
		target := strings.TrimSpace(submatches[1])
		label := target
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
			return fmt.Sprintf("[%s](%s.md)", label, resolvedSlug)
		}
		return match
	})
}
