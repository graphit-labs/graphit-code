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
	"unicode"

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
	Communities     int
	StalePages      int
	LintFindings    int
}

func GenerateKnowledgeWiki(_ context.Context, rootPath, wikiDir string, allowedExts map[string]bool) (*WikiResult, error) {
	if err := os.MkdirAll(wikiDir, 0o755); err != nil {
		return nil, fmt.Errorf("creating wiki dir: %w", err)
	}

	absRoot, err := filepath.Abs(rootPath)
	if err != nil {
		return nil, fmt.Errorf("resolving root path: %w", err)
	}

	ic := NewKnowledgeIgnoreChecker(absRoot)

	exts := allowedExts
	if len(exts) == 0 {
		exts = supportedKnowledgeExts
	}

	// Load process cache (JSON shards) for incremental rebuilds.
	processCache, _ := wiki.NewWikiProcessCache(wikiDir)

	// Track which source files exist for pruning stale cache entries.
	validPaths := make(map[string]bool)

	type sourceFile struct {
		relPath     string
		data        []byte
		contentHash string
		ext         string
	}
	var sources []sourceFile

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
		if !exts[ext] {
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

		contentHash := fmt.Sprintf("%x", sha256.Sum256(data))[:16]
		validPaths[relPath] = true
		sources = append(sources, sourceFile{
			relPath:     relPath,
			data:        data,
			contentHash: contentHash,
			ext:         ext,
		})
		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("walking docs: %w", err)
	}

	// Prune stale cache entries for deleted files.
	if processCache != nil {
		processCache.Prune(validPaths)
	}

	// Process sources: use cache for unchanged files, re-parse changed ones.
	var docs []knowledgeDoc
	for _, src := range sources {
		isMarkdown := src.ext == ".md" || src.ext == ".markdown" || src.ext == ".mdx"
		content := string(src.data)

		// Try cache first.
		if processCache != nil && !processCache.HasChanged(src.relPath, src.contentHash) {
			cached := processCache.Get(src.relPath, src.contentHash)
			if cached != nil {
				for _, cc := range cached {
					docs = append(docs, knowledgeDoc{
						title:       cc.Title,
						path:        src.relPath,
						summary:     cc.Summary,
						docType:     cc.DocType,
						body:        cc.Body,
						breadcrumb:  cc.Breadcrumb,
						parentTitle: cc.ParentTitle,
						contentHash: cc.ContentHash,
						crossRefs:   cc.CrossRefs,
						isMarkdown:  cc.IsMarkdown,
					})
				}
				continue
			}
		}

		// Cache miss or changed — process from source.
		title := extractDocTitle(content, src.relPath)
		summary := extractDocSummary(content)
		docType := classifyDocType(src.relPath, content)
		crossRefs := extractDocCrossRefs(content)

		var processedDocs []knowledgeDoc
		if isMarkdown {
			chunks, chunkErr := wiki.ChunkMarkdown(content, wiki.ChunkOpts{
				MaxTokens: 512,
				MinTokens: 32,
				DocTitle:  title,
				DocSlug:   safeFilename(title),
			})
			if chunkErr != nil || len(chunks) == 0 {
				processedDocs = append(processedDocs, knowledgeDoc{
					title: title, path: src.relPath, summary: summary,
					docType: docType, body: content, contentHash: src.contentHash,
					crossRefs: crossRefs, isMarkdown: isMarkdown,
				})
			} else {
				for i, chunk := range chunks {
					chunkDoc := knowledgeDoc{
						title:       chunk.Title,
						path:        src.relPath,
						summary:     chunk.Summary,
						docType:     docType,
						body:        chunk.Body,
						breadcrumb:  chunk.Breadcrumb,
						contentHash: fmt.Sprintf("%x", sha256.Sum256([]byte(chunk.Body)))[:16],
						crossRefs:   chunk.CrossRefs,
						isMarkdown:  true,
					}
					if chunk.ParentIdx >= 0 && chunk.ParentIdx < len(chunks) {
						chunkDoc.parentTitle = chunks[chunk.ParentIdx].Title
					}
					if i == 0 && chunk.NodeType == "intro" {
						chunkDoc.title = title
					}
					processedDocs = append(processedDocs, chunkDoc)
				}
			}
		} else {
			processedDocs = append(processedDocs, knowledgeDoc{
				title: title, path: src.relPath, summary: summary,
				docType: docType, body: content, contentHash: src.contentHash,
				crossRefs: crossRefs, isMarkdown: isMarkdown,
			})
		}

		// Store in cache.
		if processCache != nil {
			cachedChunks := make([]wiki.CachedChunk, len(processedDocs))
			for i, d := range processedDocs {
				cachedChunks[i] = wiki.CachedChunk{
					Title:       d.title,
					Body:        d.body,
					Summary:     d.summary,
					DocType:     d.docType,
					Breadcrumb:  d.breadcrumb,
					ParentTitle: d.parentTitle,
					ContentHash: d.contentHash,
					CrossRefs:   d.crossRefs,
					IsMarkdown:  d.isMarkdown,
				}
			}
			processCache.Store(src.relPath, src.contentHash, cachedChunks)
		}

		docs = append(docs, processedDocs...)
	}

	// Save process cache to disk.
	if processCache != nil {
		_ = processCache.Save()
	}

	sort.Slice(docs, func(i, j int) bool {
		if docs[i].docType != docs[j].docType {
			return docs[i].docType < docs[j].docType
		}
		return docs[i].title < docs[j].title
	})

	result := &WikiResult{OutputDir: wikiDir}

	// Read existing slugs on disk to identify deletions.
	existingSlugs := make(map[string]bool)
	if entries, err := os.ReadDir(wikiDir); err == nil {
		for _, entry := range entries {
			if entry.IsDir() || filepath.Ext(entry.Name()) != ".md" {
				continue
			}
			slug := strings.TrimSuffix(entry.Name(), ".md")
			existingSlugs[slug] = true
		}
	}

	// First pass: resolve slugs and map titles (cheap — no I/O).
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

	// FAST PATH: pre-scan content hashes + check for deletions BEFORE any
	// expensive work (autoLinkContent, regex compilation, DB rebuild, etc.).
	// Only enter the full pipeline if something actually changed.
	dbPath := filepath.Join(wikiDir, "wiki.db")
	_, dbStatErr := os.Stat(dbPath)
	dbExists := dbStatErr == nil

	if dbExists {
		newSlugsCheck := make(map[string]bool, len(docs))
		allHashesMatch := true
		for i, doc := range docs {
			slug := docSlugs[i]
			newSlugsCheck[slug] = true
			if doc.contentHash != "" {
				path := filepath.Join(wikiDir, slug+".md")
				if readKnowledgeFrontmatterField(path, "content_hash") != doc.contentHash {
					allHashesMatch = false
					break
				}
			} else {
				allHashesMatch = false
				break
			}
		}
		if allHashesMatch {
			// Also check for deleted pages.
			noDeletes := true
			for slug := range existingSlugs {
				if !newSlugsCheck[slug] {
					noDeletes = false
					break
				}
			}
			if noDeletes {
				// Nothing changed — skip ALL expensive phases.
				return result, nil
			}
		}
	}

	// Full pipeline: something changed or first run.
	compiledTargets := buildAutoLinkTargets(titlesMap)

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
		autoLinkedBody, autoRefs := autoLinkContent(docs[i].body, compiledTargets, slug)
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

		exists := func() bool { _, err := os.Stat(path); return err == nil }()

		if docs[i].contentHash != "" {
			if existingHash := readKnowledgeFrontmatterField(path, "content_hash"); existingHash == docs[i].contentHash {
				continue
			}
		} else {
			existingData, readErr := os.ReadFile(path)
			if readErr == nil && string(existingData) == page {
				continue
			}
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

	// Skip expensive phases if nothing ended up changing.
	nothingChanged := result.ArticlesWritten == 0 && len(deleted) == 0
	if nothingChanged && dbExists {
		return result, nil
	}

	// --- Phase 1: Initial index + cross-ref graph ---
	indexContent := knowledgeIndexPage(docs, nil)
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

	// --- Phase 2: Community detection ---
	var communities []KnowledgeCommunity
	if graph != nil {
		communities = DetectKnowledgeCommunities(graph)
		result.Communities = len(communities)

		if len(communities) > 0 {
			slugToCluster, slugToClusterName := AssignCommunities(communities)

			// Assign cluster info to docs
			for i := range docs {
				slug := docSlugs[i]
				if cid, ok := slugToCluster[slug]; ok {
					docs[i].cluster = cid
					docs[i].clusterName = slugToClusterName[slug]
				} else {
					docs[i].cluster = -1
				}
			}

			// Re-generate entity pages with cluster info
			for i := range docs {
				slug := docSlugs[i]
				if docs[i].cluster < 0 {
					continue
				}
				page := knowledgeEntityPage(docs[i])
				path := filepath.Join(wikiDir, slug+".md")
				_ = os.WriteFile(path, []byte(page), 0o644)
			}

			// Re-generate index with clusters
			indexContent = knowledgeIndexPage(docs, communities)
			_ = os.WriteFile(filepath.Join(wikiDir, "index.md"), []byte(indexContent), 0o644)
		}
	}

	// --- Phase 3: Staleness tracking ---
	oldManifest := LoadManifest(wikiDir)
	newManifest := &Manifest{
		SourceHashes: make(map[string]string),
		PageSources:  make(map[string]string),
	}
	for i, doc := range docs {
		newManifest.SourceHashes[doc.path] = doc.contentHash
		newManifest.PageSources[docSlugs[i]] = doc.path
	}

	stalePages := DetectStalePages(oldManifest, newManifest, graph)
	if len(stalePages) > 0 {
		result.StalePages = len(stalePages)
		// Apply stale info to docs and re-generate affected pages
		for i := range docs {
			slug := docSlugs[i]
			if info, ok := stalePages[slug]; ok {
				docs[i].staleSince = info.Since
				docs[i].staleReason = info.Reason
				page := knowledgeEntityPage(docs[i])
				path := filepath.Join(wikiDir, slug+".md")
				_ = os.WriteFile(path, []byte(page), 0o644)
			}
		}
		// Re-generate index with stale info
		indexContent = knowledgeIndexPage(docs, communities)
		_ = os.WriteFile(filepath.Join(wikiDir, "index.md"), []byte(indexContent), 0o644)
	}
	SaveManifest(wikiDir, newManifest)

	// --- Phase 4: Lint ---
	var sourcePaths []string
	for _, doc := range docs {
		sourcePaths = append(sourcePaths, doc.path)
	}
	if graph != nil {
		lintResult := LintKnowledgeWiki(wikiDir, graph, sourcePaths)
		if lintResult != nil {
			result.LintFindings = len(lintResult.Findings)
		}
	}

	sort.Strings(added)
	sort.Strings(updated)
	sort.Strings(deleted)

	// --- Phase 5: Build WikiDB for FTS5 search ---
	wikiChunks := make([]wiki.WikiChunk, 0, len(docs))
	xrefs := make(map[string][]string)
	for i, doc := range docs {
		slug := docSlugs[i]
		confidence := computeDocConfidence(doc)
		important := confidence >= 0.8
		wc := len(strings.Fields(doc.body))
		now := time.Now().UTC().Format("2006-01-02")

		wikiChunks = append(wikiChunks, wiki.WikiChunk{
			Slug:        slug,
			Title:       doc.title,
			Body:        doc.body,
			Summary:     doc.summary,
			DocType:     doc.docType,
			Source:      doc.path,
			Breadcrumb:  doc.breadcrumb,
			ParentSlug:  safeFilename(doc.parentTitle),
			ClusterID:   doc.cluster,
			ClusterName: doc.clusterName,
			Confidence:  confidence,
			ContentHash: doc.contentHash,
			WordCount:   wc,
			Updated:     now,
			Important:   important,
		})

		if len(doc.crossRefs) > 0 {
			var slugRefs []string
			for _, ref := range doc.crossRefs {
				slugRefs = append(slugRefs, safeFilename(ref))
			}
			xrefs[slug] = slugRefs
		}
	}

	var syncLogEntry *wiki.SyncLogEntry
	if len(added) > 0 || len(updated) > 0 || len(deleted) > 0 {
		details := make(map[string]wiki.LogDocDetails)
		for slug, dd := range docDetails {
			details[slug] = wiki.LogDocDetails{
				Title:   dd.Title,
				Summary: dd.Summary,
			}
		}
		syncLogEntry = &wiki.SyncLogEntry{
			Timestamp:       time.Now().UTC().Format(time.RFC3339),
			TotalDocs:       len(docs),
			ArticlesWritten: result.ArticlesWritten,
			BacklinksAdded:  result.BacklinksAdded,
			Added:           added,
			Updated:         updated,
			Deleted:         deleted,
			Details:         details,
		}
	}

	_ = wiki.RebuildDB(wikiDir, wikiChunks, xrefs, syncLogEntry, processCache)

	// Also write legacy log.md for backward compatibility
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
	if doc.breadcrumb != "" {
		_, _ = fmt.Fprintf(&b, "breadcrumb: %s\n", doc.breadcrumb)
	}
	if doc.cluster >= 0 {
		_, _ = fmt.Fprintf(&b, "cluster: %d\n", doc.cluster)
		_, _ = fmt.Fprintf(&b, "cluster_name: %s\n", doc.clusterName)
	}
	if doc.staleSince != "" {
		_, _ = fmt.Fprintf(&b, "stale_since: %s\n", doc.staleSince)
		_, _ = fmt.Fprintf(&b, "stale_reason: %s\n", doc.staleReason)
	}
	b.WriteString("---\n\n")
	_, _ = fmt.Fprintf(&b, "# %s\n\n", doc.title)
	if doc.staleSince != "" {
		_, _ = fmt.Fprintf(&b, "> ⚠️ **Stale since %s** — %s. Content may be outdated.\n\n", doc.staleSince, doc.staleReason)
	}
	if doc.breadcrumb != "" {
		_, _ = fmt.Fprintf(&b, "*📍 %s*\n\n", doc.breadcrumb)
	}
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
		if doc.isMarkdown {
			b.WriteString(body + "\n\n")
		} else {
			lang := extToLang(filepath.Ext(doc.path))
			_, _ = fmt.Fprintf(&b, "```%s\n%s\n```\n\n", lang, body)
		}
	}
	b.WriteString("---\n")
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

func knowledgeIndexPage(docs []knowledgeDoc, communities []KnowledgeCommunity) string {
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

	// Count stale pages
	staleCount := 0
	for _, doc := range docs {
		if doc.staleSince != "" {
			staleCount++
		}
	}

	if len(communities) > 0 {
		_, _ = fmt.Fprintf(&b, "**%d documents** in **%d clusters**", len(docs), len(communities))
	} else {
		_, _ = fmt.Fprintf(&b, "**%d documents**", len(docs))
	}
	if staleCount > 0 {
		_, _ = fmt.Fprintf(&b, " · ⚠️ **%d stale**", staleCount)
	}
	b.WriteString("\n\n---\n\n")

	// Build doc lookup by slug
	docBySlug := make(map[string]knowledgeDoc)
	for _, doc := range docs {
		docBySlug[safeFilename(doc.title)] = doc
	}

	writeDocEntry := func(doc knowledgeDoc) {
		link := fmt.Sprintf("[[%s]]", safeFilename(doc.title))
		badge := fmt.Sprintf("`%s`", doc.docType)
		staleMarker := ""
		if doc.staleSince != "" {
			staleMarker = " ⚠️"
		}
		if doc.summary != "" {
			summary := doc.summary
			if len(summary) > 80 {
				summary = summary[:80] + "…"
			}
			_, _ = fmt.Fprintf(&b, "- %s — %s %s%s\n", link, summary, badge, staleMarker)
		} else {
			_, _ = fmt.Fprintf(&b, "- %s %s%s\n", link, badge, staleMarker)
		}
	}

	if len(communities) > 0 {
		// Cluster-based layout
		b.WriteString("## Clusters\n\n")

		clusteredSlugs := make(map[string]bool)
		for _, comm := range communities {
			_, _ = fmt.Fprintf(&b, "### 🔗 %s (%d pages, cohesion: %.0f%%)\n\n",
				comm.Label, len(comm.Members), comm.Cohesion*100)
			for _, slug := range comm.Members {
				clusteredSlugs[slug] = true
				if doc, ok := docBySlug[slug]; ok {
					writeDocEntry(doc)
				} else {
					_, _ = fmt.Fprintf(&b, "- [[%s]]\n", slug)
				}
			}
			b.WriteString("\n")
		}

		// Unclustered docs
		var unclustered []knowledgeDoc
		for _, doc := range docs {
			slug := safeFilename(doc.title)
			if !clusteredSlugs[slug] {
				unclustered = append(unclustered, doc)
			}
		}
		if len(unclustered) > 0 {
			_, _ = fmt.Fprintf(&b, "### 📄 Unclustered (%d pages)\n\n", len(unclustered))
			for _, doc := range unclustered {
				writeDocEntry(doc)
			}
			b.WriteString("\n")
		}
	} else {
		// Fallback: type-based layout (no communities detected)
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
				writeDocEntry(doc)
			}
			b.WriteString("\n")
		}
	}

	b.WriteString("## How to Navigate\n\n")
	b.WriteString("1. **Start here** — scan the catalog above for the topic you need.\n")
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
	breadcrumb  string // "Parent > Section" hierarchy path
	cluster     int    // community ID from Louvain (-1 = unassigned)
	clusterName string // label of the community
	staleSince  string // ISO date if page is stale, empty otherwise
	staleReason string // why it's stale
	isMarkdown  bool   // true for .md/.markdown/.mdx — eligible for header splitting
}

// extToLang maps file extensions to code fence language identifiers.
// Only returns a language tag if the wiki renderer (Prism) supports it.
// Unsupported languages return "" so the fence renders as plain text.
func extToLang(ext string) string {
	switch strings.ToLower(ext) {
	case ".yaml", ".yml":
		return "yaml"
	case ".json":
		return "json"
	case ".graphql", ".gql":
		return "graphql"
	case ".xml", ".wsdl":
		return "xml"
	default:
		// .proto, .rst, .adoc, .puml, .plantuml, .txt — no Prism support
		return ""
	}
}

func safeFilename(name string) string {
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
	inFenced := false
	for _, line := range strings.Split(stripped, "\n") {
		trimmed := strings.TrimSpace(line)

		// Track fenced code block boundaries.
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


// Pre-compiled regexes for autoLinkLine — avoids re-compilation on every line.
var (
	reAutoLinkCode = regexp.MustCompile("`[^`]+`")
	reAutoLinkWiki = regexp.MustCompile(`\[\[([^\]]+)\]\]`)
	reAutoLinkMd   = regexp.MustCompile(`\[[^\]]+\]\([^)]+\)`)
)

type compiledTarget struct {
	slug  string
	re    *regexp.Regexp
	lower string // pre-lowered term for fast strings.Contains check
}

// buildAutoLinkTargets pre-compiles all auto-link targets from the titles map.
// Called once per wiki generation cycle instead of once per document.
func buildAutoLinkTargets(titles map[string]string) []compiledTarget {
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
	var result []compiledTarget
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
			result = append(result, compiledTarget{
				slug:  t.slug,
				re:    re,
				lower: termLower,
			})
		}
	}
	return result
}

func autoLinkContent(body string, targets []compiledTarget, currentSlug string) (string, []string) {
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

		newLine := autoLinkLine(line, targets, currentSlug, autoLinkedRefs)
		newLines = append(newLines, newLine)
	}

	var refs []string
	for r := range autoLinkedRefs {
		refs = append(refs, r)
	}
	sort.Strings(refs)

	return strings.Join(newLines, "\n"), refs
}

func autoLinkLine(line string, targets []compiledTarget, currentSlug string, autoLinkedRefs map[string]bool) string {
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
		if target.slug == currentSlug {
			continue
		}
		// Fast pre-filter: skip regex if term not present (case-insensitive)
		if !strings.Contains(lineLower, target.lower) {
			continue
		}
		line = target.re.ReplaceAllStringFunc(line, func(match string) string {
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

	// Count H2 sections with non-empty content
	splitCount := 0
	for idx, startLine := range h2Indices {
		endLine := len(lines)
		if idx+1 < len(h2Indices) {
			endLine = h2Indices[idx+1]
		}
		sectionContent := strings.Join(lines[startLine+1:endLine], "\n")
		if strings.TrimSpace(sectionContent) != "" {
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

	// Build ToC and children
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

		// If section is empty, keep it in the parent and don't split
		if trimmedContent == "" {
			parentBuf.WriteString("\n" + headerLine + "\n")
			continue
		}

		childTitle := doc.title + " - " + sectionTitle

		// In the parent, replace the section content with a link to the child page
		_, _ = fmt.Fprintf(&parentBuf, "\n## %s\nSee: [[%s]]\n", sectionTitle, childTitle)

		// Build ToC entry with H3 sub-items
		tocEntry := fmt.Sprintf("- [[%s|%s]]", childTitle, sectionTitle)
		sectionLines := strings.Split(trimmedContent, "\n")
		for _, sl := range sectionLines {
			if strings.HasPrefix(sl, "### ") {
				subTitle := strings.TrimSpace(strings.TrimPrefix(sl, "###"))
				tocEntry += fmt.Sprintf("\n  - %s", subTitle)
			}
		}
		tocEntries = append(tocEntries, tocEntry)

		childDoc := knowledgeDoc{
			title:       childTitle,
			path:        doc.path,
			summary:     extractDocSummary(sectionContent),
			docType:     doc.docType,
			body:        trimmedContent,
			parentTitle: doc.title,
			breadcrumb:  doc.title + " > " + sectionTitle,
			contentHash: fmt.Sprintf("%x", sha256.Sum256([]byte(trimmedContent)))[:16],
			isMarkdown:  doc.isMarkdown,
		}
		result = append(result, childDoc)
	}

	// Insert ToC into parent if we have children
	if len(tocEntries) > 0 {
		var tocBuf strings.Builder
		tocBuf.WriteString("\n## 📋 Table of Contents\n\n")
		for _, entry := range tocEntries {
			tocBuf.WriteString(entry + "\n")
		}
		tocBuf.WriteString("\n")

		// Insert ToC right after intro, before the H2 links
		introEnd := parentBody
		rest := parentBuf.String()[len(introEnd):]
		parentBuf.Reset()
		parentBuf.WriteString(introEnd)
		parentBuf.WriteString(tocBuf.String())
		parentBuf.WriteString(rest)
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

// readKnowledgeFrontmatterField reads a single YAML frontmatter field from a .md file
// without full parsing. Returns "" if the file doesn't exist or the field is absent.
func readKnowledgeFrontmatterField(path, field string) string {
	data, err := os.ReadFile(path)
	if err != nil {
		return ""
	}
	content := string(data)
	if !strings.HasPrefix(content, "---") {
		return ""
	}
	rest := content[3:]
	end := strings.Index(rest, "\n---")
	if end < 0 {
		return ""
	}
	fm := rest[:end]
	prefix := field + ": "
	for _, line := range strings.Split(fm, "\n") {
		if strings.HasPrefix(line, prefix) {
			return strings.TrimSpace(line[len(prefix):])
		}
	}
	return ""
}
