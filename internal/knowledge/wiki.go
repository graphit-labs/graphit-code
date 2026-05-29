package knowledge

import (
	"context"
	"crypto/sha256"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
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

	// Split long docs into parent/children based on H2 headers
	var splitDocs []knowledgeDoc
	for _, doc := range docs {
		splitDocs = append(splitDocs, splitDocByHeaders(doc)...)
	}
	docs = splitDocs

	sort.Slice(docs, func(i, j int) bool {
		if docs[i].docType != docs[j].docType {
			return docs[i].docType < docs[j].docType
		}
		return docs[i].title < docs[j].title
	})

	result := &WikiResult{OutputDir: wikiDir}

	// Read existing slugs on disk to identify additions vs updates
	existingSlugs := make(map[string]bool)
	if entries, err := os.ReadDir(wikiDir); err == nil {
		for _, entry := range entries {
			if entry.IsDir() || filepath.Ext(entry.Name()) != ".md" {
				continue
			}
			slug := strings.TrimSuffix(entry.Name(), ".md")
			if slug == "index" || slug == "log" {
				continue
			}
			existingSlugs[slug] = true
		}
	}

	// First pass: resolve slugs and map titles
	docSlugs := make([]string, len(docs))
	titlesMap := make(map[string]string)
	tempUsedSlugs := make(map[string]bool)
	for i, doc := range docs {
		slug := uniqueKSlug(safeFilename(doc.title), tempUsedSlugs)
		docSlugs[i] = slug
		titlesMap[doc.title] = slug
		base := strings.TrimSuffix(filepath.Base(doc.path), filepath.Ext(doc.path))
		if base != "" && base != doc.title {
			titlesMap[base] = slug
		}
	}

	newSlugs := make(map[string]bool)
	var added []string
	var updated []string
	docDetails := make(map[string]logDocDetails)

	for i := range docs {
		slug := docSlugs[i]
		newSlugs[slug] = true
		docDetails[slug] = logDocDetails{
			Title:   docs[i].title,
			Summary: docs[i].summary,
		}

		// Prepend parent link for child documents
		if docs[i].parentTitle != "" {
			parentSlug := titlesMap[docs[i].parentTitle]
			if parentSlug != "" {
				docs[i].body = fmt.Sprintf("**Parent:** [[%s]]\n\n%s", parentSlug, docs[i].body)
			}
		}

		// Auto-link body content using the titlesMap
		autoLinkedBody, autoRefs := autoLinkContent(docs[i].body, titlesMap, slug)
		docs[i].body = autoLinkedBody

		// Add auto-linked references to doc's crossRefs
		existingRefs := make(map[string]bool)
		for _, ref := range docs[i].crossRefs {
			existingRefs[ref] = true
		}
		for _, ref := range autoRefs {
			if !existingRefs[ref] {
				docs[i].crossRefs = append(docs[i].crossRefs, ref)
				existingRefs[ref] = true
			}
		}

		// Resolve manual/child wikilinks in the body to their resolved slugs
		docs[i].body = resolveWikiLinksInBody(docs[i].body, titlesMap)

		page := knowledgeEntityPage(docs[i])
		path := filepath.Join(wikiDir, slug+".md")

		existingData, readErr := os.ReadFile(path)
		exists := readErr == nil

		// Only write to disk and track if content changed
		if exists && string(existingData) == page {
			continue
		}

		if exists {
			updated = append(updated, slug)
		} else {
			added = append(added, slug)
		}

		if err := os.WriteFile(path, []byte(page), 0o644); err != nil {
			continue
		}
		result.ArticlesWritten++
	}

	// Prune deleted files
	var deleted []string
	for slug := range existingSlugs {
		if !newSlugs[slug] {
			deleted = append(deleted, slug)
			_ = os.Remove(filepath.Join(wikiDir, slug+".md"))
		}
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

	sort.Strings(added)
	sort.Strings(updated)
	sort.Strings(deleted)

	if len(added) > 0 || len(updated) > 0 || len(deleted) > 0 {
		appendKnowledgeLog(filepath.Join(wikiDir, "log.md"), len(docs), result.ArticlesWritten, result.BacklinksAdded, added, updated, deleted, docDetails)
	}

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

func appendKnowledgeLog(logPath string, totalDocs, articlesWritten, backlinksAdded int, added, updated, deleted []string, details map[string]logDocDetails) {
	timestamp := time.Now().UTC().Format("2006-01-02 15:04:05")
	totalChanges := len(added) + len(updated) + len(deleted)

	var b strings.Builder
	_, _ = fmt.Fprintf(&b, "## [%s] sync | Compiled %d changes\n\n", timestamp, totalChanges)
	_, _ = fmt.Fprintf(&b, "- Total Documents: %d\n- Articles written/updated: %d\n- Backlinks injected: %d\n", totalDocs, articlesWritten, backlinksAdded)

	if len(added) > 0 {
		b.WriteString("- Added pages:\n")
		for _, slug := range added {
			if d, ok := details[slug]; ok && d.Title != "" {
				summary := d.Summary
				if len(summary) > 120 {
					summary = summary[:120] + "…"
				}
				if summary != "" {
					_, _ = fmt.Fprintf(&b, "  - [[%s]] (%s) — %s\n", slug, d.Title, summary)
				} else {
					_, _ = fmt.Fprintf(&b, "  - [[%s]] (%s)\n", slug, d.Title)
				}
			} else {
				_, _ = fmt.Fprintf(&b, "  - [[%s]]\n", slug)
			}
		}
	}
	if len(updated) > 0 {
		b.WriteString("- Updated pages:\n")
		for _, slug := range updated {
			if d, ok := details[slug]; ok && d.Title != "" {
				summary := d.Summary
				if len(summary) > 120 {
					summary = summary[:120] + "…"
				}
				if summary != "" {
					_, _ = fmt.Fprintf(&b, "  - [[%s]] (%s) — %s\n", slug, d.Title, summary)
				} else {
					_, _ = fmt.Fprintf(&b, "  - [[%s]] (%s)\n", slug, d.Title)
				}
			} else {
				_, _ = fmt.Fprintf(&b, "  - [[%s]]\n", slug)
			}
		}
	}
	if len(deleted) > 0 {
		b.WriteString("- Removed pages:\n")
		for _, slug := range deleted {
			_, _ = fmt.Fprintf(&b, "  - %s\n", slug)
		}
	}
	b.WriteString("\n")

	entry := b.String()

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
	parentTitle string
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



type linkTarget struct {
	term string
	slug string
}

func autoLinkContent(body string, titles map[string]string, currentSlug string) (string, []string) {
	var targets []linkTarget
	for title, slug := range titles {
		if slug == currentSlug {
			continue
		}
		targets = append(targets, linkTarget{term: title, slug: slug})
		if title != slug {
			targets = append(targets, linkTarget{term: slug, slug: slug})
			targets = append(targets, linkTarget{term: strings.ReplaceAll(slug, "_", " "), slug: slug})
		}
	}

	sort.Slice(targets, func(i, j int) bool {
		return len(targets[i].term) > len(targets[j].term)
	})

	seenTerms := make(map[string]string)
	var uniqueTargets []linkTarget
	for _, t := range targets {
		termLower := strings.ToLower(t.term)
		if termLower == "" || len(termLower) < 3 {
			continue
		}
		if _, ok := seenTerms[termLower]; !ok {
			seenTerms[termLower] = t.slug
			uniqueTargets = append(uniqueTargets, t)
		}
	}

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

		newLine := autoLinkLine(line, uniqueTargets, autoLinkedRefs)
		newLines = append(newLines, newLine)
	}

	var refs []string
	for r := range autoLinkedRefs {
		refs = append(refs, r)
	}
	sort.Strings(refs)

	return strings.Join(newLines, "\n"), refs
}

func autoLinkLine(line string, targets []linkTarget, autoLinkedRefs map[string]bool) string {
	var wlPlaceholders []string
	var mlPlaceholders []string
	var cdPlaceholders []string

	reCode := regexp.MustCompile("`[^`]+`")
	line = reCode.ReplaceAllStringFunc(line, func(match string) string {
		wlPlaceholders = append(wlPlaceholders, match)
		return fmt.Sprintf("___CD_PLACEHOLDER_%d___", len(wlPlaceholders)-1)
	})

	reWiki := regexp.MustCompile(`\[\[([^\]]+)\]\]`)
	line = reWiki.ReplaceAllStringFunc(line, func(match string) string {
		mlPlaceholders = append(mlPlaceholders, match)
		return fmt.Sprintf("___WL_PLACEHOLDER_%d___", len(mlPlaceholders)-1)
	})

	reMd := regexp.MustCompile(`\[[^\]]+\]\([^)]+\)`)
	line = reMd.ReplaceAllStringFunc(line, func(match string) string {
		cdPlaceholders = append(cdPlaceholders, match)
		return fmt.Sprintf("___ML_PLACEHOLDER_%d___", len(cdPlaceholders)-1)
	})

	var autoWlPlaceholders []string
	for _, target := range targets {
		pattern := `\b(?i)` + regexp.QuoteMeta(target.term) + `\b`
		re, err := regexp.Compile(pattern)
		if err != nil {
			continue
		}

		line = re.ReplaceAllStringFunc(line, func(match string) string {
			autoLinkedRefs[target.slug] = true
			replacement := fmt.Sprintf("[[%s|%s]]", target.slug, match)
			autoWlPlaceholders = append(autoWlPlaceholders, replacement)
			return fmt.Sprintf("___AUTO_WL_PLACEHOLDER_%d___", len(autoWlPlaceholders)-1)
		})
	}

	for i, val := range autoWlPlaceholders {
		ph := fmt.Sprintf("___AUTO_WL_PLACEHOLDER_%d___", i)
		line = strings.ReplaceAll(line, ph, val)
	}

	for i, val := range cdPlaceholders {
		ph := fmt.Sprintf("___ML_PLACEHOLDER_%d___", i)
		line = strings.ReplaceAll(line, ph, val)
	}

	for i, val := range mlPlaceholders {
		ph := fmt.Sprintf("___WL_PLACEHOLDER_%d___", i)
		line = strings.ReplaceAll(line, ph, val)
	}

	for i, val := range wlPlaceholders {
		ph := fmt.Sprintf("___CD_PLACEHOLDER_%d___", i)
		line = strings.ReplaceAll(line, ph, val)
	}

	return line
}

func splitDocByHeaders(doc knowledgeDoc) []knowledgeDoc {
	body := stripFrontmatter(doc.body)
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
		return []knowledgeDoc{doc}
	}

	// Count how many sections will actually be split (word count >= 150)
	splitCount := 0
	for idx, startLine := range h2Indices {
		endLine := len(lines)
		if idx+1 < len(h2Indices) {
			endLine = h2Indices[idx+1]
		}
		sectionContent := strings.Join(lines[startLine+1:endLine], "\n")
		trimmedContent := strings.TrimSpace(sectionContent)
		if len(strings.Fields(trimmedContent)) >= 150 {
			splitCount++
		}
	}

	if splitCount == 0 {
		return []knowledgeDoc{doc}
	}

	var result []knowledgeDoc

	// Extract the intro (parent content before first H2)
	parentBody := strings.Join(lines[:h2Indices[0]], "\n")
	parentDoc := doc

	var parentBuf strings.Builder
	parentBuf.WriteString(parentBody)
	if !strings.HasSuffix(parentBody, "\n") && parentBody != "" {
		parentBuf.WriteString("\n")
	}

	for idx, startLine := range h2Indices {
		headerLine := lines[startLine]
		sectionTitle := strings.TrimSpace(strings.TrimPrefix(headerLine, "##"))

		endLine := len(lines)
		if idx+1 < len(h2Indices) {
			endLine = h2Indices[idx+1]
		}

		sectionContent := strings.Join(lines[startLine+1:endLine], "\n")
		trimmedContent := strings.TrimSpace(sectionContent)

		// If section is empty, keep it in the parent and don't split
		if trimmedContent == "" {
			parentBuf.WriteString("\n" + headerLine + "\n")
			continue
		}

		// If section content has less than 150 words, keep it inline in parent
		if len(strings.Fields(trimmedContent)) < 150 {
			parentBuf.WriteString("\n" + headerLine + "\n" + sectionContent + "\n")
			continue
		}

		childTitle := doc.title + " - " + sectionTitle

		// In the parent, replace the section content with a link to the child page
		_, _ = fmt.Fprintf(&parentBuf, "\n## %s\nSee: [[%s]]\n", sectionTitle, childTitle)

		childDoc := knowledgeDoc{
			title:       childTitle,
			path:        doc.path,
			summary:     extractDocSummary(sectionContent),
			docType:     doc.docType,
			body:        trimmedContent,
			parentTitle: doc.title,
			contentHash: fmt.Sprintf("%x", sha256.Sum256([]byte(trimmedContent)))[:16],
		}
		result = append(result, childDoc)
	}

	parentDoc.body = parentBuf.String()
	parentDoc.summary = extractDocSummary(parentDoc.body)
	parentDoc.contentHash = fmt.Sprintf("%x", sha256.Sum256([]byte(parentDoc.body)))[:16]

	// Prepend parentDoc so it is processed first
	result = append([]knowledgeDoc{parentDoc}, result...)
	return result
}

var reWikiLink = regexp.MustCompile(`\[\[([^\]|]+)(?:\|([^\]]+))?\]\]`)

func resolveWikiLinksInBody(body string, titlesMap map[string]string) string {
	return reWikiLink.ReplaceAllStringFunc(body, func(match string) string {
		submatches := reWikiLink.FindStringSubmatch(match)
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
			resolvedSlug, ok = titlesMap[safeFilename(target)]
		}
		if !ok {
			// Case-insensitive lookup fallback
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
			// Case-insensitive slugified lookup fallback
			targetSlugLower := strings.ToLower(safeFilename(target))
			for t, s := range titlesMap {
				if strings.ToLower(safeFilename(t)) == targetSlugLower || strings.ToLower(s) == targetSlugLower {
					resolvedSlug = s
					ok = true
					break
				}
			}
		}
		if !ok {
			// Trigram fuzzy match fallback
			resolvedSlug, ok = findBestFuzzyTitleMatch(target, titlesMap)
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

type logDocDetails struct {
	Title   string
	Summary string
}

func cleanForFuzzy(s string) string {
	var b strings.Builder
	for _, r := range strings.ToLower(s) {
		if (r >= 'a' && r <= 'z') || (r >= '0' && r <= '9') {
			b.WriteRune(r)
		}
	}
	return b.String()
}

func getTrigrams(s string) map[string]bool {
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

func trigramSimilarity(s1, s2 string) float64 {
	t1 := getTrigrams(s1)
	t2 := getTrigrams(s2)

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

func findBestFuzzyTitleMatch(target string, titlesMap map[string]string) (string, bool) {
	targetClean := cleanForFuzzy(target)
	if targetClean == "" {
		return "", false
	}

	bestSlug := ""
	bestScore := 0.0

	for title, slug := range titlesMap {
		titleClean := cleanForFuzzy(title)
		score := trigramSimilarity(targetClean, titleClean)
		if score > bestScore {
			bestScore = score
			bestSlug = slug
		}

		slugClean := cleanForFuzzy(slug)
		scoreSlug := trigramSimilarity(targetClean, slugClean)
		if scoreSlug > bestScore {
			bestScore = scoreSlug
			bestSlug = slug
		}
	}

	if bestScore >= 0.55 {
		return bestSlug, true
	}
	return "", false
}

