package wiki

import (
	"fmt"
	"os"
	"strings"
	"unicode"

	"gopkg.in/yaml.v3"
)

// Slug helpers — shared by wiki generators

// SafeSlug converts a title or path into a safe filesystem slug.
// Replaces spaces/slashes/colons with underscores/dashes, strips non-alphanumeric
// runes, and collapses repeated separators.
func SafeSlug(name string) string {
	r := strings.NewReplacer("/", "-", " ", "_", ":", "-", "\\", "-", "?", "", "*", "")
	name = r.Replace(name)

	var b strings.Builder
	for _, r := range name {
		if unicode.IsLetter(r) || unicode.IsNumber(r) || r == '_' || r == '-' || r == '.' {
			b.WriteRune(r)
		}
	}
	name = b.String()

	for strings.Contains(name, "__") {
		name = strings.ReplaceAll(name, "__", "_")
	}
	for strings.Contains(name, "--") {
		name = strings.ReplaceAll(name, "--", "-")
	}
	name = strings.Trim(name, "_-")
	return name
}

// UniqueSlug returns a unique slug derived from base, appending _2, _3, … as
// needed. It marks the returned slug as used in the provided map.
func UniqueSlug(base string, used map[string]bool) string {
	slug := base
	n := 2
	for used[slug] {
		slug = fmt.Sprintf("%s_%d", base, n)
		n++
	}
	used[slug] = true
	return slug
}

// Frontmatter helpers — shared by wiki generators

// FrontmatterBlock returns the text of the document's leading YAML frontmatter
// block, without the --- delimiters. ok is false when the document does not
// open with one.
func FrontmatterBlock(content string) (string, bool) {
	if !strings.HasPrefix(content, "---") {
		return "", false
	}
	rest := content[3:]
	end := strings.Index(rest, "\n---")
	if end < 0 {
		return "", false
	}
	return rest[:end], true
}

// FrontmatterField returns a scalar field from the document's leading YAML
// frontmatter, letting the YAML parser resolve quoting and escaping instead of
// guessing at them: a single-quoted value with a doubled apostrophe, or a
// double-quoted one with a \" in it, arrives as the author wrote it.
//
// The scalar's literal text is returned, without YAML type resolution, so a
// content hash that happens to be all digits stays the string it was.
//
// ok is false when there is no frontmatter, when the block does not parse, and
// when the field is absent, null or not a scalar — each a signal for the caller
// to fall back to its own scan rather than to trust an empty answer.
func FrontmatterField(content, field string) (string, bool) {
	block, ok := FrontmatterBlock(content)
	if !ok {
		return "", false
	}
	var doc yaml.Node
	if err := yaml.Unmarshal([]byte(block), &doc); err != nil || len(doc.Content) == 0 {
		return "", false
	}
	mapping := doc.Content[0]
	if mapping.Kind != yaml.MappingNode {
		return "", false
	}
	for i := 0; i+1 < len(mapping.Content); i += 2 {
		if mapping.Content[i].Value != field {
			continue
		}
		value := mapping.Content[i+1]
		if value.Kind != yaml.ScalarNode || value.Tag == "!!null" {
			return "", false
		}
		return value.Value, value.Value != ""
	}
	return "", false
}

// ReadFrontmatterField reads a single YAML frontmatter field from a .md file.
// Returns "" if the file doesn't exist, the field is absent, or the file
// doesn't start with "---".
func ReadFrontmatterField(path, field string) string {
	data, err := os.ReadFile(path)
	if err != nil {
		return ""
	}
	content := string(data)
	if value, ok := FrontmatterField(content, field); ok {
		return value
	}
	block, ok := FrontmatterBlock(content)
	if !ok {
		return ""
	}
	prefix := field + ": "
	for _, line := range strings.Split(block, "\n") {
		if strings.HasPrefix(line, prefix) {
			return strings.TrimSpace(line[len(prefix):])
		}
	}
	return ""
}

// StripFrontmatter removes YAML frontmatter (--- ... ---) from the beginning
// of a markdown document and returns the trimmed body.
func StripFrontmatter(content string) string {
	if !strings.HasPrefix(strings.TrimSpace(content), "---") {
		return content
	}
	lines := strings.Split(content, "\n")
	inFM := false
	var out []string
	for i, line := range lines {
		trimmed := strings.TrimSpace(line)
		if i == 0 && trimmed == "---" {
			inFM = true
			continue
		}
		if inFM {
			if trimmed == "---" {
				inFM = false
			}
			continue
		}
		out = append(out, line)
	}
	return strings.TrimSpace(strings.Join(out, "\n"))
}

// Trigram similarity — shared by search and wiki-link resolution

// CleanForFuzzy normalizes a string for trigram comparison: lowercase, letters
// and digits only.
func CleanForFuzzy(s string) string {
	var b strings.Builder
	for _, r := range strings.ToLower(s) {
		if (r >= 'a' && r <= 'z') || (r >= '0' && r <= '9') {
			b.WriteRune(r)
		}
	}
	return b.String()
}

// GetTrigrams returns the set of 3-character substrings for s.
func GetTrigrams(s string) map[string]bool {
	s = strings.ToLower(s)
	trigrams := make(map[string]bool)
	if len(s) < 3 {
		trigrams[s] = true
		return trigrams
	}
	for i := 0; i <= len(s)-3; i++ {
		trigrams[s[i:i+3]] = true
	}
	return trigrams
}

// TrigramSimilarity computes the Jaccard similarity between the trigram sets
// of s1 and s2 (range 0–1).
func TrigramSimilarity(s1, s2 string) float64 {
	t1 := GetTrigrams(s1)
	t2 := GetTrigrams(s2)
	intersection := 0
	for k := range t1 {
		if t2[k] {
			intersection++
		}
	}
	union := len(t1) + len(t2) - intersection
	if union == 0 {
		return 0.0
	}
	return float64(intersection) / float64(union)
}

// FindBestFuzzyTitleMatch finds the slug from titlesMap whose title or slug
// has the best trigram similarity to target. Returns ("", false) when the
// best score is below the 0.55 threshold.
// titlesMap maps title/label → canonical slug.
func FindBestFuzzyTitleMatch(target string, titlesMap map[string]string) (string, bool) {
	targetClean := CleanForFuzzy(target)
	if targetClean == "" {
		return "", false
	}
	bestSlug := ""
	bestScore := 0.0
	for title, slug := range titlesMap {
		if score := TrigramSimilarity(targetClean, CleanForFuzzy(title)); score > bestScore {
			bestScore = score
			bestSlug = slug
		}
		if score := TrigramSimilarity(targetClean, CleanForFuzzy(slug)); score > bestScore {
			bestScore = score
			bestSlug = slug
		}
	}
	if bestScore >= 0.55 {
		return bestSlug, true
	}
	return "", false
}
