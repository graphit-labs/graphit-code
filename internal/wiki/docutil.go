package wiki

import (
	"path/filepath"
	"strings"
	"unicode/utf8"
)

// ExtractTitle extracts a document title from markdown content by looking for,
// in order: a YAML frontmatter title: field, a top-level # Heading, or the
// file basename (without extension) derived from fallbackPath.
func ExtractTitle(content, fallbackPath string) string {
	if title, ok := FrontmatterField(content, "title"); ok {
		return title
	}
	for _, line := range strings.Split(content, "\n") {
		trimmed := strings.TrimSpace(line)
		if trimmed == "---" {
			continue
		}
		if strings.HasPrefix(trimmed, "# ") {
			return strings.TrimPrefix(trimmed, "# ")
		}
		if strings.HasPrefix(trimmed, "title:") {
			title := strings.TrimSpace(strings.TrimPrefix(trimmed, "title:"))
			title = strings.Trim(title, "\"'")
			if title != "" {
				return title
			}
		}
	}
	base := filepath.Base(fallbackPath)
	return strings.TrimSuffix(base, filepath.Ext(base))
}

// ExtractSummary returns the first meaningful prose line from markdown content.
// It prefers a YAML frontmatter description: field; otherwise it scans the body
// skipping headings, fenced code blocks, setext underlines, thematic breaks,
// and table separator rows.
func ExtractSummary(content string) string {
	if desc, ok := FrontmatterField(content, "description"); ok {
		return truncateSummary(desc)
	}

	for _, line := range strings.Split(content, "\n") {
		trimmed := strings.TrimSpace(line)
		if strings.HasPrefix(trimmed, "description:") {
			desc := strings.TrimSpace(strings.TrimPrefix(trimmed, "description:"))
			desc = strings.Trim(desc, "\"'")
			if desc != "" {
				return truncateSummary(desc)
			}
		}
	}

	stripped := StripFrontmatter(content)
	inFenced := false
	for _, line := range strings.Split(stripped, "\n") {
		trimmed := strings.TrimSpace(line)

		if strings.HasPrefix(trimmed, "```") || strings.HasPrefix(trimmed, "~~~") {
			inFenced = !inFenced
			continue
		}
		if inFenced {
			continue
		}
		if trimmed == "" || strings.HasPrefix(trimmed, "#") {
			continue
		}
		trimmedHR := strings.TrimLeft(trimmed, "-*_")
		if trimmedHR == "" && len(trimmed) >= 3 {
			continue
		}
		stripped2 := strings.TrimLeft(trimmed, "=-")
		if stripped2 == "" {
			continue
		}
		if strings.HasPrefix(trimmed, "|") && strings.Count(trimmed, "|") >= 2 && strings.Contains(trimmed, "---") {
			withoutStructural := strings.NewReplacer("-", "", "|", "", " ", "").Replace(trimmed)
			if withoutStructural == "" || withoutStructural == ":" || strings.Trim(withoutStructural, ":") == "" {
				continue
			}
		}
		return truncateSummary(trimmed)
	}
	return ""
}

// truncateSummary caps a summary at 200 characters, counting runes so a
// multi-byte character is never cut in half.
func truncateSummary(s string) string {
	const maxSummaryRunes = 200
	if utf8.RuneCountInString(s) <= maxSummaryRunes {
		return s
	}
	return string([]rune(s)[:maxSummaryRunes]) + "…"
}

// ExtractCrossRefs returns all wikilink slugs found in content (after stripping
// frontmatter). Duplicates are removed.
func ExtractCrossRefs(content string) []string {
	content = StripFrontmatter(content)
	seen := make(map[string]bool)
	var refs []string
	for _, slug := range FindWikiLinks(content) {
		if !seen[slug] {
			seen[slug] = true
			refs = append(refs, slug)
		}
	}
	return refs
}

// ExtToLang maps a file extension to a Prism.js language identifier for
// syntax-highlighted fenced code blocks in wiki pages.
// Returns "" when the extension is not supported.
func ExtToLang(ext string) string {
	switch ext {
	case ".go":
		return "go"
	case ".ts", ".tsx":
		return "typescript"
	case ".js", ".jsx", ".mjs", ".cjs":
		return "javascript"
	case ".py":
		return "python"
	case ".rs":
		return "rust"
	case ".java":
		return "java"
	case ".cs":
		return "csharp"
	case ".cpp", ".cc", ".cxx", ".c":
		return "cpp"
	case ".rb":
		return "ruby"
	case ".php":
		return "php"
	case ".swift":
		return "swift"
	case ".kt", ".kts":
		return "kotlin"
	case ".scala":
		return "scala"
	case ".sh", ".bash", ".zsh":
		return "bash"
	case ".sql":
		return "sql"
	case ".yaml", ".yml":
		return "yaml"
	case ".json":
		return "json"
	case ".xml", ".wsdl":
		return "xml"
	case ".html", ".htm":
		return "html"
	case ".css":
		return "css"
	case ".md", ".markdown":
		return "markdown"
	case ".graphql", ".gql":
		return "graphql"
	default:
		return ""
	}
}
