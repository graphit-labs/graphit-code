package wiki

import (
	"crypto/sha256"
	"fmt"
	"path/filepath"
	"strings"
)

// ---------------------------------------------------------------------------
// Document parsing utilities — shared by wiki generators
// ---------------------------------------------------------------------------

// ExtractTitle extracts a document title from markdown content by looking for,
// in order: a YAML frontmatter title: field, a top-level # Heading, or the
// file basename (without extension) derived from fallbackPath.
func ExtractTitle(content, fallbackPath string) string {
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
	// Prefer explicit frontmatter description.
	for _, line := range strings.Split(content, "\n") {
		trimmed := strings.TrimSpace(line)
		if strings.HasPrefix(trimmed, "description:") {
			desc := strings.TrimSpace(strings.TrimPrefix(trimmed, "description:"))
			desc = strings.Trim(desc, "\"'")
			if desc != "" {
				if len(desc) > 200 {
					return desc[:200] + "…"
				}
				return desc
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
		// Skip thematic breaks (---, ***, ___).
		trimmedHR := strings.TrimLeft(trimmed, "-*_")
		if trimmedHR == "" && len(trimmed) >= 3 {
			continue
		}
		// Skip setext underlines.
		stripped2 := strings.TrimLeft(trimmed, "=-")
		if stripped2 == "" {
			continue
		}
		// Skip table separator lines.
		if strings.HasPrefix(trimmed, "|") && strings.Count(trimmed, "|") >= 2 && strings.Contains(trimmed, "---") {
			withoutStructural := strings.NewReplacer("-", "", "|", "", " ", "").Replace(trimmed)
			if withoutStructural == "" || withoutStructural == ":" || strings.Trim(withoutStructural, ":") == "" {
				continue
			}
		}
		if len(trimmed) > 200 {
			return trimmed[:200] + "…"
		}
		return trimmed
	}
	return ""
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

// ---------------------------------------------------------------------------
// Document splitting — H2-based page splitting for wiki page generation
// ---------------------------------------------------------------------------

// SplitDoc represents one document produced by SplitByH2Headers.
type SplitDoc struct {
	Title       string
	Body        string
	Summary     string
	ParentTitle string // "" for the root/parent doc
	Breadcrumb  string // e.g. "Parent > Section"
	ContentHash string
}

// SplitByH2Headers splits a markdown body into a parent document (with ToC)
// and one child document per H2 section. This enables long documents to be
// represented as linked wiki pages rather than one monolithic page.
//
// If the body has no H2 sections (or all sections are empty), a single SplitDoc
// containing the original body is returned.
func SplitByH2Headers(title, body string) []SplitDoc {
	lines := strings.Split(body, "\n")

	inCodeBlock := false
	var h2Indices []int
	for i, line := range lines {
		trimmed := strings.TrimSpace(line)
		if strings.HasPrefix(trimmed, "```") {
			inCodeBlock = !inCodeBlock
			continue
		}
		if inCodeBlock {
			continue
		}
		if strings.HasPrefix(line, "## ") {
			h2Indices = append(h2Indices, i)
		}
	}

	if len(h2Indices) == 0 {
		return []SplitDoc{{
			Title:       title,
			Body:        body,
			Summary:     ExtractSummary(body),
			ContentHash: contentHash16(body),
		}}
	}

	// Count non-empty H2 sections.
	splitCount := 0
	for idx, startLine := range h2Indices {
		endLine := len(lines)
		if idx+1 < len(h2Indices) {
			endLine = h2Indices[idx+1]
		}
		if strings.TrimSpace(strings.Join(lines[startLine+1:endLine], "\n")) != "" {
			splitCount++
		}
	}

	if splitCount == 0 {
		return []SplitDoc{{
			Title:       title,
			Body:        body,
			Summary:     ExtractSummary(body),
			ContentHash: contentHash16(body),
		}}
	}

	var result []SplitDoc
	parentBody := strings.Join(lines[:h2Indices[0]], "\n")
	var parentBuf strings.Builder
	parentBuf.WriteString(parentBody)
	if !strings.HasSuffix(parentBody, "\n") && parentBody != "" {
		parentBuf.WriteString("\n")
	}

	var tocEntries []string

	for idx, startLine := range h2Indices {
		headerLine := lines[startLine]
		sectionTitle := strings.TrimSpace(strings.TrimPrefix(headerLine, "##"))

		endLine := len(lines)
		if idx+1 < len(h2Indices) {
			endLine = h2Indices[idx+1]
		}

		sectionContent := strings.Join(lines[startLine+1:endLine], "\n")
		trimmedContent := strings.TrimSpace(sectionContent)

		if trimmedContent == "" {
			parentBuf.WriteString("\n" + headerLine + "\n")
			continue
		}

		childTitle := title + " - " + sectionTitle
		fmt.Fprintf(&parentBuf, "\n## %s\nSee: [[%s]]\n", sectionTitle, childTitle)

		tocEntry := fmt.Sprintf("- [[%s|%s]]", childTitle, sectionTitle)
		for _, sl := range strings.Split(trimmedContent, "\n") {
			if strings.HasPrefix(sl, "### ") {
				subTitle := strings.TrimSpace(strings.TrimPrefix(sl, "###"))
				tocEntry += fmt.Sprintf("\n  - %s", subTitle)
			}
		}
		tocEntries = append(tocEntries, tocEntry)

		result = append(result, SplitDoc{
			Title:       childTitle,
			Body:        trimmedContent,
			Summary:     ExtractSummary(sectionContent),
			ParentTitle: title,
			Breadcrumb:  title + " > " + sectionTitle,
			ContentHash: contentHash16(trimmedContent),
		})
	}

	if len(tocEntries) > 0 {
		var tocBuf strings.Builder
		tocBuf.WriteString("\n## 📋 Table of Contents\n\n")
		for _, entry := range tocEntries {
			tocBuf.WriteString(entry + "\n")
		}
		tocBuf.WriteString("\n")
		rest := parentBuf.String()[len(parentBody):]
		parentBuf.Reset()
		parentBuf.WriteString(parentBody)
		parentBuf.WriteString(tocBuf.String())
		parentBuf.WriteString(rest)
	}

	parentFinalBody := parentBuf.String()
	parent := SplitDoc{
		Title:       title,
		Body:        parentFinalBody,
		Summary:     ExtractSummary(parentFinalBody),
		ContentHash: contentHash16(parentFinalBody),
	}
	return append([]SplitDoc{parent}, result...)
}

// contentHash16 returns the first 16 hex characters of the SHA-256 of s.
func contentHash16(s string) string {
	return fmt.Sprintf("%x", sha256.Sum256([]byte(s)))[:16]
}
