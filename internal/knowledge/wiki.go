package knowledge

import (
	"context"
	"crypto/sha256"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/wiki"
	"golang.org/x/text/cases"
	"golang.org/x/text/language"
)

type WikiResult struct {
	ArticlesWritten int
	OutputDir       string
	BacklinksAdded  int
	OrphanPages     int
	BrokenLinks     int
}

func GenerateKnowledgeWiki(_ context.Context, rootPath, wikiDir string) (*WikiResult, error) {
	if err := os.MkdirAll(wikiDir, 0o755); err != nil {
		return nil, fmt.Errorf("creating wiki dir: %w", err)
	}

	absRoot, err := filepath.Abs(rootPath)
	if err != nil {
		return nil, fmt.Errorf("resolving root path: %w", err)
	}

	ic := NewKnowledgeIgnoreChecker(absRoot)

	var docs []knowledgeDoc
	err = filepath.Walk(absRoot, func(path string, info os.FileInfo, walkErr error) error {
		if walkErr != nil {
			return nil
		}
		if info.IsDir() {
			relDir, _ := filepath.Rel(absRoot, path)
			if relDir != "." && ic.IsIgnored(relDir, true) {
				return filepath.SkipDir
			}
			return nil
		}

		ext := strings.ToLower(filepath.Ext(path))
		if !supportedKnowledgeExts[ext] {
			return nil
		}

		relPath, _ := filepath.Rel(absRoot, path)
		if ic.IsIgnored(relPath, false) {
			return nil
		}

		if info.Size() > 1024*1024 {
			return nil
		}

		data, readErr := os.ReadFile(path)
		if readErr != nil {
			return nil
		}

		title := extractDocTitle(string(data), relPath)
		summary := extractDocSummary(string(data))
		docType := classifyDocType(relPath, string(data))
		contentHash := fmt.Sprintf("%x", sha256.Sum256(data))[:16]

		crossRefs := extractDocCrossRefs(string(data))

		docs = append(docs, knowledgeDoc{
			title:       title,
			path:        relPath,
			summary:     summary,
			docType:     docType,
			body:        string(data),
			contentHash: contentHash,
			crossRefs:   crossRefs,
		})
		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("walking docs: %w", err)
	}

	sort.Slice(docs, func(i, j int) bool {
		if docs[i].docType != docs[j].docType {
			return docs[i].docType < docs[j].docType
		}
		return docs[i].title < docs[j].title
	})

	result := &WikiResult{OutputDir: wikiDir}
	usedSlugs := make(map[string]bool)

	for _, doc := range docs {
		slug := uniqueKSlug(safeFilename(doc.title), usedSlugs)
		page := knowledgeEntityPage(doc)
		path := filepath.Join(wikiDir, slug+".md")
		if err := os.WriteFile(path, []byte(page), 0o644); err != nil {
			continue
		}
		result.ArticlesWritten++
	}

	indexContent := knowledgeIndexPage(docs)
	if err := os.WriteFile(filepath.Join(wikiDir, "index.md"), []byte(indexContent), 0o644); err != nil {
		return result, err
	}

	graph, err := wiki.BuildCrossRefGraph(wikiDir)
	if err == nil {
		xrefResult, _ := wiki.InjectBacklinks(wikiDir, graph)
		if xrefResult != nil {
			result.BacklinksAdded = xrefResult.BacklinksAdded
			result.OrphanPages = xrefResult.OrphanPages
			result.BrokenLinks = xrefResult.BrokenLinks
		}
	}

	appendKnowledgeLog(filepath.Join(wikiDir, "log.md"), len(docs), result.ArticlesWritten, result.BacklinksAdded)

	return result, nil
}

func knowledgeEntityPage(doc knowledgeDoc) string {
	var b strings.Builder
	now := time.Now().UTC().Format("2006-01-02")
	confidence := computeDocConfidence(doc)

	b.WriteString("---\n")
	_, _ = fmt.Fprintf(&b, "title: %s\n", doc.title)
	_, _ = fmt.Fprintf(&b, "type: %s\n", doc.docType)
	_, _ = fmt.Fprintf(&b, "source: %s\n", doc.path)
	_, _ = fmt.Fprintf(&b, "updated: %s\n", now)
	_, _ = fmt.Fprintf(&b, "confidence: %.2f\n", confidence)
	_, _ = fmt.Fprintf(&b, "content_hash: %s\n", doc.contentHash)
	_, _ = fmt.Fprintf(&b, "tags: [knowledge, %s]\n", doc.docType)
	b.WriteString("---\n\n")
	_, _ = fmt.Fprintf(&b, "# %s\n\n", doc.title)
	if doc.summary != "" {
		_, _ = fmt.Fprintf(&b, "> %s\n\n", doc.summary)
	}
	_, _ = fmt.Fprintf(&b, "**Source:** `%s`  \n", doc.path)
	_, _ = fmt.Fprintf(&b, "**Type:** %s  \n", doc.docType)
	_, _ = fmt.Fprintf(&b, "**Confidence:** %.0f%%\n\n", confidence*100)

	_, _ = fmt.Fprintf(&b, "*Provenance: ^[%s]*\n\n", doc.path)

	if len(doc.crossRefs) > 0 {
		b.WriteString("## Cross-References\n\n")
		for _, ref := range doc.crossRefs {
			_, _ = fmt.Fprintf(&b, "- [[%s]]\n", safeFilename(ref))
		}
		b.WriteString("\n")
	}

	if doc.body != "" {
		body := stripFrontmatter(doc.body)
		b.WriteString("## Content\n\n")
		b.WriteString(body + "\n\n")
	}
	b.WriteString("---\n*Navigate: [[index]] · [[log]]*\n")
	return b.String()
}

func computeDocConfidence(doc knowledgeDoc) float64 {
	score := 0.0

	if doc.title != "" && doc.title != doc.path {
		score += 0.20
	}

	if doc.summary != "" {
		score += 0.20
		if len(doc.summary) > 50 {
			score += 0.10
		}
	}

	if doc.docType != "" && doc.docType != "document" {
		score += 0.15
	}

	bodyLen := len(stripFrontmatter(doc.body))
	switch {
	case bodyLen > 2000:
		score += 0.25
	case bodyLen > 500:
		score += 0.15
	case bodyLen > 100:
		score += 0.10
	}

	if len(doc.crossRefs) > 0 {
		score += 0.10
	}

	if score > 1.0 {
		score = 1.0
	}
	return score
}

func knowledgeIndexPage(docs []knowledgeDoc) string {
	var b strings.Builder
	now := time.Now().UTC().Format("2006-01-02")
	b.WriteString("---\n")
	b.WriteString("title: Knowledge Wiki\n")
	_, _ = fmt.Fprintf(&b, "updated: %s\n", now)
	b.WriteString("tags: [knowledge, index]\n")
	b.WriteString("---\n\n")
	b.WriteString("# Knowledge Wiki\n\n")
	_, _ = fmt.Fprintf(&b, "> %s knowledge wiki. **Start here.** Scan the catalog below, then follow [[wikilinks]] to drill into specific pages.\n", brand.DisplayName)
	_, _ = fmt.Fprintf(&b, "> Check [[log]] for the timeline of updates. Last updated: %s\n\n", now)
	_, _ = fmt.Fprintf(&b, "**%d documents**\n\n", len(docs))
	b.WriteString("---\n\n")
	b.WriteString("## Documents\n\n")

	byType := make(map[string][]knowledgeDoc)
	for _, doc := range docs {
		byType[doc.docType] = append(byType[doc.docType], doc)
	}
	var types []string
	for t := range byType {
		types = append(types, t)
	}
	sort.Strings(types)

	for _, docType := range types {
		_, _ = fmt.Fprintf(&b, "### %s\n\n", cases.Title(language.English).String(docType))
		for _, doc := range byType[docType] {
			link := fmt.Sprintf("[[%s]]", safeFilename(doc.title))
			if doc.summary != "" {

				summary := doc.summary
				if len(summary) > 80 {
					summary = summary[:80] + "…"
				}
				_, _ = fmt.Fprintf(&b, "- %s — %s\n", link, summary)
			} else {
				_, _ = fmt.Fprintf(&b, "- %s (`%s`)\n", link, doc.path)
			}
		}
		b.WriteString("\n")
	}

	b.WriteString("## How to Navigate\n\n")
	b.WriteString("1. **Start here** — scan the Documents section above for the topic you need.\n")
	b.WriteString("2. **Follow links** — each page has [[wikilinks]] to related pages.\n")
	b.WriteString("3. **Check backlinks** — each page lists what links *to* it (inbound references).\n")
	b.WriteString("4. **Check the log** — [[log]] shows the timeline of wiki updates.\n\n")
	b.WriteString("---\n")
	_, _ = fmt.Fprintf(&b, "*Generated by %s · %s*\n", brand.DisplayName, now)
	return b.String()
}

func appendKnowledgeLog(logPath string, totalDocs, articlesWritten, backlinksAdded int) {
	now := time.Now().UTC().Format("2006-01-02")
	entry := fmt.Sprintf("## [%s] index | Wiki regenerated\n\n"+
		"- Documents: %d\n- Articles written: %d\n- Backlinks injected: %d\n\n",
		now, totalDocs, articlesWritten, backlinksAdded)

	existing, _ := os.ReadFile(logPath)
	var content string
	if len(existing) == 0 {
		content = "---\ntitle: Knowledge Wiki Log\ntags: [knowledge, log]\n---\n\n# Knowledge Wiki Log\n\n" +
			"> Append-only chronological record. Parse with: `grep '^## \\[' log.md | tail -5`\n\n"
	} else {
		content = string(existing)
	}

	parts := strings.SplitN(content, "\n---\n", 2)
	if len(parts) == 2 {
		content = parts[0] + "\n---\n\n" + entry + parts[1]
	} else {
		content += "\n" + entry
	}
	_ = os.WriteFile(logPath, []byte(content), 0o644)
}

type knowledgeDoc struct {
	title       string
	path        string
	summary     string
	docType     string
	body        string
	contentHash string
	crossRefs   []string
}

func safeFilename(name string) string {
	r := strings.NewReplacer("/", "-", " ", "_", ":", "-", "\\", "-", "?", "", "*", "")
	return r.Replace(name)
}

func uniqueKSlug(base string, used map[string]bool) string {
	slug := base
	n := 2
	for used[slug] {
		slug = fmt.Sprintf("%s_%d", base, n)
		n++
	}
	used[slug] = true
	return slug
}

func extractDocTitle(content, relPath string) string {
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
	base := filepath.Base(relPath)
	return strings.TrimSuffix(base, filepath.Ext(base))
}

func extractDocSummary(content string) string {

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

	stripped := stripFrontmatter(content)
	for _, line := range strings.Split(stripped, "\n") {
		trimmed := strings.TrimSpace(line)
		if trimmed == "" || strings.HasPrefix(trimmed, "#") {
			continue
		}
		if len(trimmed) > 200 {
			return trimmed[:200] + "…"
		}
		return trimmed
	}
	return ""
}

func extractDocCrossRefs(content string) []string {
	content = stripFrontmatter(content)
	var refs []string
	seen := make(map[string]bool)

	matches := wiki.FindWikiLinks(content)
	for _, m := range matches {
		if !seen[m] {
			seen[m] = true
			refs = append(refs, m)
		}
	}

	return refs
}

func classifyDocType(relPath, content string) string {
	lower := strings.ToLower(relPath)

	if paradigm := classifyParadigm(relPath, content); paradigm != "" {
		return paradigm
	}

	switch {
	case strings.Contains(lower, "decision") || strings.Contains(lower, "adr"):
		return "decision"
	case strings.Contains(lower, "spec") || strings.Contains(lower, "specification"):
		return "specification"
	case strings.Contains(lower, "guide") || strings.Contains(lower, "howto") || strings.Contains(lower, "tutorial"):
		return "guide"
	case strings.Contains(lower, "api"):
		return "api"
	case strings.Contains(lower, "readme"):
		return "readme"
	case strings.Contains(lower, "changelog") || strings.Contains(lower, "release"):
		return "changelog"
	case strings.Contains(lower, "architecture"):
		return "architecture"
	default:
		return "document"
	}
}

func classifyParadigm(relPath, content string) string {
	lower := strings.ToLower(relPath)
	lowerContent := strings.ToLower(content)
	switch {
	case strings.HasSuffix(lower, ".proto"):
		return "grpc"
	case strings.HasSuffix(lower, ".graphql") || strings.HasSuffix(lower, ".gql"):
		return "graphql"
	case strings.HasSuffix(lower, ".wsdl"):
		return "soap"
	case (strings.HasSuffix(lower, ".yaml") || strings.HasSuffix(lower, ".yml") || strings.HasSuffix(lower, ".json")) &&
		(strings.Contains(lowerContent, "openapi") || strings.Contains(lowerContent, "swagger")):
		return "rest"
	case (strings.HasSuffix(lower, ".yaml") || strings.HasSuffix(lower, ".yml") || strings.HasSuffix(lower, ".json")) &&
		strings.Contains(lowerContent, "asyncapi"):
		return "async"
	default:
		return ""
	}
}

func stripFrontmatter(content string) string {
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
