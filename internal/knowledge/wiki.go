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
		title := wiki.ExtractTitle(content, src.relPath)
		summary := wiki.ExtractSummary(content)
		docType := classifyDocType(src.relPath, content)
		crossRefs := wiki.ExtractCrossRefs(content)

		var processedDocs []knowledgeDoc
		if isMarkdown {
			chunks, chunkErr := wiki.ChunkMarkdown(content, wiki.ChunkOpts{
				MaxTokens: 512,
				MinTokens: 32,
				DocTitle:  title,
				DocSlug:   wiki.SafeSlug(title),
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
		slug := wiki.UniqueSlug(wiki.SafeSlug(doc.title), tempUsedSlugs)
		docSlugs[i] = slug
		titlesMap[doc.title] = slug
		base := strings.TrimSuffix(filepath.Base(doc.path), filepath.Ext(doc.path))
		if base != "" && base != doc.title {
			titlesMap[base] = slug
		}
	}

	// FAST PATH: use wiki.FastPathCheck which checks processCache (O(1) per
	// entry — no disk I/O) and a single ReadDir to detect deletions.
	// Only enter the full pipeline if something actually changed.
	fastEntries := make([]wiki.DocHashEntry, len(docs))
	for i, doc := range docs {
		fastEntries[i] = wiki.DocHashEntry{
			CacheKey:    doc.path,
			ContentHash: doc.contentHash,
			Slug:        docSlugs[i],
		}
	}
	if wiki.FastPathCheck(wikiDir, fastEntries, processCache) {
		// Nothing changed — skip ALL expensive phases.
		return result, nil
	}

	// Fallback for processCache == nil: check frontmatter hashes from disk.
	if _, dbStatErr := os.Stat(filepath.Join(wikiDir, "wiki.db")); dbStatErr == nil && processCache == nil {
		newSlugsCheck := make(map[string]bool, len(docs))
		allHashesMatch := true
		for i, doc := range docs {
			slug := docSlugs[i]
			newSlugsCheck[slug] = true
			if doc.contentHash != "" {
				path := filepath.Join(wikiDir, slug+".md")
				if wiki.ReadFrontmatterField(path, "content_hash") != doc.contentHash {
					allHashesMatch = false
					break
				}
			} else {
				allHashesMatch = false
				break
			}
		}
		if allHashesMatch {
			noDeletes := true
			for slug := range existingSlugs {
				if !newSlugsCheck[slug] {
					noDeletes = false
					break
				}
			}
			if noDeletes {
				return result, nil
			}
		}
	}

	// Full pipeline: something changed or first run.
	compiledTargets := wiki.BuildAutoLinkTargets(titlesMap)

	newSlugs := make(map[string]bool)
	var added []string
	var updated []string
	docDetails := make(map[string]wiki.LogDocDetails)

	for i := range docs {
		slug := docSlugs[i]
		newSlugs[slug] = true
		docDetails[slug] = wiki.LogDocDetails{
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
		autoLinkedBody, autoRefs := wiki.AutoLinkContent(docs[i].body, compiledTargets, slug)
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
		docs[i].body = wiki.ResolveWikiLinksInBody(docs[i].body, titlesMap)

		page := knowledgeEntityPage(docs[i])
		path := filepath.Join(wikiDir, slug+".md")

		exists := func() bool { _, err := os.Stat(path); return err == nil }()

		if docs[i].contentHash != "" {
			if existingHash := wiki.ReadFrontmatterField(path, "content_hash"); existingHash == docs[i].contentHash {
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
	_, dbStatErr2 := os.Stat(filepath.Join(wikiDir, "wiki.db"))
	dbExists := dbStatErr2 == nil
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
			ParentSlug:  wiki.SafeSlug(doc.parentTitle),
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
				slugRefs = append(slugRefs, wiki.SafeSlug(ref))
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
			_, _ = fmt.Fprintf(&b, "- [[%s]]\n", wiki.SafeSlug(ref))
		}
		b.WriteString("\n")
	}

	if doc.body != "" {
		body := wiki.StripFrontmatter(doc.body)
		b.WriteString("## Content\n\n")
		if doc.isMarkdown {
			b.WriteString(body + "\n\n")
		} else {
			lang := wiki.ExtToLang(filepath.Ext(doc.path))
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

	bodyLen := len(wiki.StripFrontmatter(doc.body))
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
		docBySlug[wiki.SafeSlug(doc.title)] = doc
	}

	writeDocEntry := func(doc knowledgeDoc) {
		link := fmt.Sprintf("[[%s]]", wiki.SafeSlug(doc.title))
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
			slug := wiki.SafeSlug(doc.title)
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

func appendKnowledgeLog(logPath string, totalDocs, articlesWritten, backlinksAdded int, added, updated, deleted []string, details map[string]wiki.LogDocDetails) {
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
