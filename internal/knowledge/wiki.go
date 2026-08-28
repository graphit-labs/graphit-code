package knowledge

import (
	"context"
	"crypto/sha256"
	"fmt"
	"os"
	"path"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/ignorer"
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

const maxKnowledgeDocBytes = 1024 * 1024

// knowledgeSourceFile reports whether a file found under absRoot is a document
// the wiki indexes, returning its cache key and extension.
func knowledgeSourceFile(absRoot, path string, info os.FileInfo, exts map[string]bool, ic ignorer.DirScope) (relPath, ext string, ok bool) {
	ext = strings.ToLower(filepath.Ext(path))
	if !exts[ext] {
		return "", "", false
	}
	relPath, err := filepath.Rel(absRoot, path)
	if err != nil || ic.IsIgnored(relPath, false) {
		return "", "", false
	}
	if info.Size() > maxKnowledgeDocBytes {
		return "", "", false
	}
	return relPath, ext, true
}

// knowledgeSource is a document the wiki indexes, as found on disk.
type knowledgeSource struct {
	relPath string
	ext     string
	mtime   int64
	size    int64
}

// WikiScope narrows a build to part of the tree under the root without changing
// what the root is.
//
// The distinction matters because every path the wiki reports — the `source:`
// field on a page, the manifest, the process-cache key — is relative to the
// root. Handing the docs directory in as the root instead would report a spec as
// `specs/config_module.md`, a path that resolves from nowhere the reader is
// standing, and would resolve `.gitignore`/`.wikiignore` from inside the docs
// tree, where a root-level pattern lands in a domain one level above itself and
// silently matches nothing.
//
// A zero WikiScope walks the whole root, which is what an imported context wants:
// its docs tree already *is* the root.
type WikiScope struct {
	// Subdir is the only directory walked, relative to the root. Empty or "."
	// walks everything. A Subdir that does not exist is not an error — it yields
	// no sources, and ExtraFiles are still indexed.
	Subdir string

	// ExtraFiles are single documents outside Subdir that are indexed anyway, as
	// paths relative to the root. Missing ones are skipped. This is how the
	// project's README stays in the wiki once the walk is scoped to docs/.
	ExtraFiles []string
}

// walkRoot returns the directory the walk starts from.
func (s WikiScope) walkRoot(absRoot string) string {
	sub := strings.TrimSpace(s.Subdir)
	if sub == "" || sub == "." {
		return absRoot
	}
	return filepath.Join(absRoot, filepath.FromSlash(sub))
}

// walkRoots returns every directory the walk starts from.
//
// There is one. The whitelist that used to make this a set existed for a single
// caller — the live search compiling several selected documentation sets into one
// wiki — and that compile is gone: a context now arrives already compiled and is
// searched where it lives. The plural shape is kept because the walk consumes a
// slice, not because a scope can name more than one tree.
func (s WikiScope) walkRoots(absRoot string) []string {
	return []string{s.walkRoot(absRoot)}
}

// enumerateKnowledgeSources walks the scope once. Both the added-document check
// in StatPreCheck and the generation pass read from this single result — walking
// twice per index is what this exists to avoid.
func enumerateKnowledgeSources(absRoot string, scope WikiScope, exts map[string]bool, ic ignorer.DirScope) ([]knowledgeSource, error) {
	var sources []knowledgeSource
	seen := make(map[string]bool)

	add := func(path string, info os.FileInfo) {
		relPath, ext, ok := knowledgeSourceFile(absRoot, path, info, exts, ic)
		if !ok || seen[relPath] {
			return
		}
		seen[relPath] = true
		sources = append(sources, knowledgeSource{
			relPath: relPath,
			ext:     ext,
			mtime:   info.ModTime().UnixNano(),
			size:    info.Size(),
		})
	}

	for _, walkFrom := range scope.walkRoots(absRoot) {
		err := filepath.Walk(walkFrom, func(path string, info os.FileInfo, walkErr error) error {
			if walkErr != nil {
				return nil
			}
			if info.IsDir() {
				relDir, _ := filepath.Rel(absRoot, path)
				if relDir != "." && ic.IsIgnored(relDir, true) && !ic.ShouldDescend(relDir) {
					return filepath.SkipDir
				}
				// A directory's own .gitignore/.wikiignore scopes to it, exactly
				// as git does; crossing into one before its children is what
				// makes that true.
				if relDir != "." {
					ic = ic.At(relDir)
				}
				return nil
			}
			add(path, info)
			return nil
		})
		if err != nil {
			return sources, err
		}
	}

	// After the walk, so a document the walk already found keeps the mtime the
	// walk read rather than being stat'ed a second time.
	for _, extra := range scope.ExtraFiles {
		abs := filepath.Join(absRoot, filepath.FromSlash(extra))
		info, statErr := os.Stat(abs)
		if statErr != nil || info.IsDir() {
			continue
		}
		add(abs, info)
	}

	return sources, nil
}

func knowledgeSourceRelPaths(sources []knowledgeSource) []string {
	names := make([]string, len(sources))
	for i, s := range sources {
		names[i] = s.relPath
	}
	return names
}

// GenerateKnowledgeWiki compiles the documents under rootPath into the wiki at
// wikiDir. scope narrows which of them are read; see WikiScope.
func GenerateKnowledgeWiki(ctx context.Context, rootPath, wikiDir string, allowedExts map[string]bool, scope WikiScope) (*WikiResult, error) {
	if err := os.MkdirAll(wikiDir, 0o755); err != nil {
		return nil, fmt.Errorf("creating wiki dir: %w", err)
	}

	absRoot, err := filepath.Abs(rootPath)
	if err != nil {
		return nil, fmt.Errorf("resolving root path: %w", err)
	}

	// Patterns resolve against the project, but they are collected from the docs
	// tree upward, so a .wikiignore kept inside the docs tree is read as well as
	// the one at the root.
	walkRoot := scope.walkRoot(absRoot)
	ic := ignorer.DirScope(NewKnowledgeIgnoreCheckerIn(absRoot, walkRoot))

	exts := allowedExts
	if len(exts) == 0 {
		exts = supportedKnowledgeExts
	}

	// Load process cache (JSON shards) for incremental rebuilds.
	processCache, _ := wiki.NewWikiProcessCache(wikiDir)

	candidates, err := enumerateKnowledgeSources(absRoot, scope, exts, ic)
	if err != nil {
		return nil, fmt.Errorf("walking docs: %w", err)
	}

	// STAT PRE-CHECK (shared AST pattern via wiki.StatPreCheck)
	// If all cached source files are stat-unchanged, no document was added and
	// wiki.db exists, skip the full rebuild entirely.
	// Both ignore files are watched: editing either changes what is in scope, and
	// a stat-unchanged tree would otherwise skip the rebuild that has to notice.
	watchFiles := []string{KnowledgeIgnoreFile}
	if rel, relErr := filepath.Rel(absRoot, walkRoot); relErr == nil && rel != "." {
		watchFiles = append(watchFiles, filepath.Join(rel, KnowledgeIgnoreFile))
	}
	if wiki.StatPreCheck(absRoot, wikiDir, processCache, wiki.StatPreCheckOpts{
		WatchFiles:         watchFiles,
		CurrentSourceFiles: func() []string { return knowledgeSourceRelPaths(candidates) },
	}) {
		return &WikiResult{OutputDir: wikiDir}, nil
	}

	// Track which source files exist for pruning stale cache entries.
	validPaths := make(map[string]bool)

	type sourceFile struct {
		relPath     string
		data        []byte
		contentHash string
		ext         string
		mtime       int64
		size        int64
	}
	sources := make([]sourceFile, 0, len(candidates))

	for _, c := range candidates {
		// Stat-cache fast-path: if mtime+size match the processCache, the
		// content hasn't changed. Skip ReadFile and use the cached hash.
		// This is the same technique as git's index stat caching.
		if processCache != nil {
			if cachedHash, ok := processCache.StatMatch(c.relPath, c.mtime, c.size); ok {
				validPaths[c.relPath] = true
				sources = append(sources, sourceFile{
					relPath:     c.relPath,
					contentHash: cachedHash,
					ext:         c.ext,
					mtime:       c.mtime,
					size:        c.size,
				})
				continue
			}
		}

		// Stat didn't match (or no cache): read the file and hash it.
		data, readErr := os.ReadFile(filepath.Join(absRoot, c.relPath))
		if readErr != nil {
			continue
		}

		contentHash := fmt.Sprintf("%x", sha256.Sum256(data))[:16]
		validPaths[c.relPath] = true
		sources = append(sources, sourceFile{
			relPath:     c.relPath,
			data:        data,
			contentHash: contentHash,
			ext:         c.ext,
			mtime:       c.mtime,
			size:        c.size,
		})
		// If hash matches cache, record the new mtime so next sync can skip
		// ReadFile entirely via StatMatch (same as AST's StoreMtime pattern).
		if processCache != nil && !processCache.HasChanged(c.relPath, contentHash) {
			processCache.StoreMtime(c.relPath, c.mtime, c.size)
		}
	}

	// Before pruning: identify deleted keys that had outgoing cross-refs.
	// If any exist, their targets will lose a backlink → graph rebuild needed.
	var deletedWithRefs []string
	if processCache != nil {
		for _, key := range processCache.AllCacheKeys() {
			if !validPaths[key] && len(processCache.GetOutRefs(key)) > 0 {
				deletedWithRefs = append(deletedWithRefs, key)
			}
		}
		processCache.Prune(validPaths)
	}

	// Process sources: one wiki document per source file, cached when unchanged.
	// Track changed keys and their cross-refs for incremental graph detection.
	// oldOutRefs is captured BEFORE WikiProcessCache.Store resets the entry.
	//
	// THE DOCUMENT IS THE UNIT. A source file is never split into per-heading
	// pieces. Splitting produced one page per heading, and a heading whose whole
	// content was subsections produced an EMPTY page — measured at 11,4% of the
	// index — which still carried a title into the ranking and outranked the
	// prose it was supposed to introduce. It also made a document's own page the
	// empty one whenever the document opened with a single H1.
	var changedKeys []string
	oldOutRefs := make(map[string][]string)
	newOutRefs := make(map[string][]string)
	var docs []knowledgeDoc
	for _, src := range sources {
		updatedAt := time.Unix(0, src.mtime).UTC().Format("2006-01-02")

		// Try cache first.
		if processCache != nil && !processCache.HasChanged(src.relPath, src.contentHash) {
			if cached := processCache.Get(src.relPath, src.contentHash); len(cached) > 0 {
				cc := cached[0]
				docs = append(docs, knowledgeDoc{
					title:       cc.Title,
					path:        src.relPath,
					summary:     cc.Summary,
					docType:     cc.DocType,
					body:        cc.Body,
					breadcrumb:  cc.Breadcrumb,
					contentHash: cc.ContentHash,
					crossRefs:   cc.CrossRefs,
					isMarkdown:  cc.IsMarkdown,
					updatedAt:   updatedAt,
				})
				continue
			}
		}

		// Cache miss or changed — process from source.
		//
		// The stat fast-path above leaves data nil when it trusted mtime+size, so
		// the content has to be read HERE rather than assumed present: reaching
		// this point with a stat hit and a cache miss would otherwise index the
		// document as empty, which reads as "the document says nothing" instead
		// of as a cache fault.
		if src.data == nil {
			data, readErr := os.ReadFile(filepath.Join(absRoot, src.relPath))
			if readErr != nil {
				continue
			}
			src.data = data
		}
		content := string(src.data)

		doc := knowledgeDoc{
			title:   wiki.ExtractTitle(content, src.relPath),
			path:    src.relPath,
			summary: wiki.ExtractSummary(content),
			docType: classifyDocType(src.relPath, content),
			body:    content,
			// The source path, so a query naming a file or a directory reaches the
			// page: `source` is not one of the indexed FTS columns and breadcrumb
			// no longer has a heading hierarchy to carry.
			breadcrumb:  filepath.ToSlash(src.relPath),
			contentHash: src.contentHash,
			crossRefs:   wiki.ExtractCrossRefs(content),
			isMarkdown:  src.ext == ".md" || src.ext == ".markdown" || src.ext == ".mdx",
			updatedAt:   updatedAt,
		}
		docs = append(docs, doc)

		// Store in cache.
		if processCache != nil {
			// Save old cross-refs BEFORE Store() resets the manifest entry.
			oldOutRefs[src.relPath] = processCache.GetOutRefs(src.relPath)

			processCache.Store(src.relPath, src.contentHash, []wiki.CachedChunk{{
				Title:       doc.title,
				Body:        doc.body,
				Summary:     doc.summary,
				DocType:     doc.docType,
				Breadcrumb:  doc.breadcrumb,
				ContentHash: doc.contentHash,
				CrossRefs:   doc.crossRefs,
				IsMarkdown:  doc.isMarkdown,
			}})
			// Record mtime+size so the next sync can skip ReadFile via StatMatch.
			processCache.StoreMtime(src.relPath, src.mtime, src.size)
			// Collect new cross-refs (stored to processCache after CrossRefsUnchanged).
			newOutRefs[src.relPath] = doc.crossRefs
		}
		changedKeys = append(changedKeys, src.relPath)
	}

	// Save process cache to disk.
	if processCache != nil {
		_ = processCache.Save()
	}

	// The path breaks ties, which makes the order TOTAL: sort.Slice is not stable,
	// so two documents sharing a type and a title would otherwise swap places
	// between builds, and slug assignment below reads this order.
	sort.Slice(docs, func(i, j int) bool {
		if docs[i].docType != docs[j].docType {
			return docs[i].docType < docs[j].docType
		}
		if docs[i].title != docs[j].title {
			return docs[i].title < docs[j].title
		}
		return docs[i].path < docs[j].path
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
	//
	// A slug must not depend on how many documents happen to precede it. The old
	// scheme numbered every collision _2, _3, … in iteration order, so adding one
	// document renumbered the rest and silently repointed the stored [[wikilinks]]
	// and xrefs of unrelated pages at different content — no error, no log. Here a
	// title that is unique in the corpus names its own page, and an ambiguous or
	// unusable one falls back to the source path, which is unique per document and
	// stable across builds by construction.
	titleCount := make(map[string]int, len(docs))
	for _, doc := range docs {
		titleCount[wiki.SafeSlug(doc.title)]++
	}
	docSlugs := make([]string, len(docs))
	titlesMap := make(map[string]string)
	usedSlugs := make(map[string]bool, len(docs))
	for i, doc := range docs {
		base := wiki.SafeSlug(doc.title)
		if base == "" || titleCount[base] > 1 {
			// ToSlash first, explicitly: doc.path comes from filepath.Rel and carries
			// backslashes on Windows. A slug that depended on the separator would give
			// the same document a different page name per platform, renaming every
			// page of a repository checked out on the other one.
			relSlash := filepath.ToSlash(doc.path)
			base = wiki.SafeSlug(strings.TrimSuffix(relSlash, path.Ext(relSlash)))
		}
		slug := wiki.UniqueSlug(base, usedSlugs)
		docSlugs[i] = slug
		titlesMap[doc.title] = slug
		fileBase := strings.TrimSuffix(filepath.Base(doc.path), filepath.Ext(doc.path))
		if fileBase != "" && fileBase != doc.title {
			titlesMap[fileBase] = slug
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
	if wiki.FastPathCheck(ctx, wikiDir, fastEntries, processCache) {
		// Nothing changed — skip ALL expensive phases, but keep the manifest
		// current so DetectStalePages has a baseline on the NEXT run.
		if len(candidates) > 0 {
			m := LoadManifest(wikiDir)
			m.SourceHashes = make(map[string]string, len(docs))
			m.PageSources = make(map[string]string, len(docs))
			for i, doc := range docs {
				m.SourceHashes[doc.path] = doc.contentHash
				m.PageSources[docSlugs[i]] = doc.path
			}
			SaveManifest(wikiDir, m)
		}
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

		written, existed := writePageIfChanged(filepath.Join(wikiDir, slug+".md"), knowledgeEntityPage(docs[i]))
		if !written {
			continue
		}
		if existed {
			updated = append(updated, slug)
		} else {
			added = append(added, slug)
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

	// Phase 1: Cross-ref graph + backlink injection
	// Skip if cross-refs haven't changed: no file gained/lost/changed any
	// wikilink reference, so backlinks are already correct on disk.
	crossRefsOK := processCache != nil &&
		wiki.CrossRefsUnchanged(deletedWithRefs, changedKeys, oldOutRefs, newOutRefs)

	// Persist new cross-refs now that the comparison is done.
	if processCache != nil {
		for _, key := range changedKeys {
			if refs, ok := newOutRefs[key]; ok {
				processCache.StoreOutRefs(key, refs)
			}
		}
		_ = processCache.Save()
	}
	indexContent := knowledgeIndexPage(docs, docSlugs, nil)
	if err := os.WriteFile(filepath.Join(wikiDir, "index.md"), []byte(indexContent), 0o644); err != nil {
		return result, err
	}

	var graph *wiki.CrossRefGraph
	if !crossRefsOK {
		var graphErr error
		graph, graphErr = wiki.BuildCrossRefGraph(wikiDir)
		if graphErr == nil {
			xrefResult, _ := wiki.InjectBacklinks(wikiDir, graph)
			if xrefResult != nil {
				result.BacklinksAdded = xrefResult.BacklinksAdded
				result.OrphanPages = xrefResult.OrphanPages
				result.BrokenLinks = xrefResult.BrokenLinks
			}
		}
	}

	// Phase 2: Community detection
	var communities []KnowledgeCommunity
	if crossRefsOK {
		// Cross-refs identical to last run → graph is the same → reuse cached
		// cluster assignments without rebuilding the community graph.
		if slugToCluster, slugToClusterName, clOK := loadClusterCache(wikiDir); clOK {
			for i := range docs {
				slug := docSlugs[i]
				if cid, ok := slugToCluster[slug]; ok {
					docs[i].cluster = cid
					docs[i].clusterName = slugToClusterName[slug]
				} else {
					docs[i].cluster = -1
				}
			}
		}
	} else if graph != nil {
		communities = DetectKnowledgeCommunities(graph)
		result.Communities = len(communities)

		if len(communities) > 0 {
			slugToCluster, slugToClusterName := AssignCommunities(communities)
			// Persist cluster assignments for reuse when cross-refs are unchanged.
			saveClusterCache(wikiDir, slugToCluster, slugToClusterName)

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
				writePageIfChanged(filepath.Join(wikiDir, slug+".md"), knowledgeEntityPage(docs[i]))
			}

			// Re-generate index with clusters
			indexContent = knowledgeIndexPage(docs, docSlugs, communities)
			_ = os.WriteFile(filepath.Join(wikiDir, "index.md"), []byte(indexContent), 0o644)
		}
	}

	// Phase 3: Staleness tracking
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
				writePageIfChanged(filepath.Join(wikiDir, slug+".md"), knowledgeEntityPage(docs[i]))
			}
		}
		// Re-generate index with stale info
		indexContent = knowledgeIndexPage(docs, docSlugs, communities)
		_ = os.WriteFile(filepath.Join(wikiDir, "index.md"), []byte(indexContent), 0o644)
	}
	SaveManifest(wikiDir, newManifest)

	// Phase 4: Lint
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

	// Phase 5: Build WikiDB for FTS5 search
	wikiChunks := make([]wiki.WikiChunk, 0, len(docs))
	xrefs := make(map[string][]string)
	for i, doc := range docs {
		slug := docSlugs[i]
		confidence := computeDocConfidence(doc)
		important := confidence >= 0.8
		wc := len(strings.Fields(doc.body))

		wikiChunks = append(wikiChunks, wiki.WikiChunk{
			Slug:        slug,
			Title:       doc.title,
			Body:        doc.body,
			Summary:     doc.summary,
			DocType:     doc.docType,
			Source:      doc.path,
			Breadcrumb:  doc.breadcrumb,
			ClusterID:   doc.cluster,
			ClusterName: doc.clusterName,
			Confidence:  confidence,
			ContentHash: doc.contentHash,
			WordCount:   wc,
			// The source's date, matching the page's frontmatter. Stamping "today"
			// here made every row claim it was updated on the day of the last sync.
			Updated:   doc.updatedAt,
			Important: important,
		})

		if len(doc.crossRefs) > 0 {
			var slugRefs []string
			for _, ref := range doc.crossRefs {
				slugRefs = append(slugRefs, wiki.SafeSlug(ref))
			}
			xrefs[slug] = slugRefs
		}

		// The corpus-level fields go into the cache here, and only here, because
		// this is the first point at which they all exist: the slug was assigned
		// after collision resolution, and the community after the whole graph was
		// built. Without them a shard is not a complete chunk, and a consumer that
		// installs this wiki without its sources — which is how a published
		// knowledge context arrives — would lose them.
		if processCache != nil {
			processCache.StoreDerived(doc.path, wiki.DerivedChunkFields{
				Slug:        slug,
				ClusterID:   doc.cluster,
				ClusterName: doc.clusterName,
				Confidence:  confidence,
				Updated:     doc.updatedAt,
				Important:   important,
			})
		}
	}

	// Details cover the pages this sync TOUCHED, not the whole corpus. Recording
	// every document made each log entry carry a title and summary for all of them,
	// and sync_log is append-only and re-copied on every rebuild: it had grown to
	// 99 MB of a 116 MB index, against 1,4 MB of actual indexed text.
	touchedDetails := make(map[string]wiki.LogDocDetails, len(added)+len(updated))
	for _, slug := range append(append([]string{}, added...), updated...) {
		if dd, ok := docDetails[slug]; ok {
			touchedDetails[slug] = dd
		}
	}

	var syncLogEntry *wiki.SyncLogEntry
	if len(added) > 0 || len(updated) > 0 || len(deleted) > 0 {
		syncLogEntry = &wiki.SyncLogEntry{
			Timestamp:       time.Now().UTC().Format(time.RFC3339),
			TotalDocs:       len(docs),
			ArticlesWritten: result.ArticlesWritten,
			BacklinksAdded:  result.BacklinksAdded,
			Added:           added,
			Updated:         updated,
			Deleted:         deleted,
			Details:         touchedDetails,
		}
	}

	_ = wiki.RebuildDB(ctx, wikiDir, wikiChunks, xrefs, syncLogEntry, processCache)

	// Record ignore file mtime so StatPreCheck detects changes next run.
	if processCache != nil {
		ignPath := filepath.Join(absRoot, KnowledgeIgnoreFile)
		if info, err := os.Stat(ignPath); err == nil {
			processCache.StoreWatchFile(KnowledgeIgnoreFile, info.ModTime().UnixNano(), info.Size())
		}
		_ = processCache.Save()
	}

	if len(added) > 0 || len(updated) > 0 || len(deleted) > 0 {
		appendKnowledgeLog(filepath.Join(wikiDir, "log.md"), len(docs), result.ArticlesWritten, result.BacklinksAdded, added, updated, deleted, touchedDetails)
	}

	return result, nil
}

// writePageIfChanged writes page to path only when the bytes differ from what is
// already there, and reports whether it wrote and whether the file existed.
//
// The comparison is against the RENDERED page, not against the source document's
// content hash, and that difference is the point. A page carries more than its
// source: autolinks resolved against every other document's title, injected
// backlinks, cluster and staleness annotations. Deciding by source hash left a
// page un-rewritten when a SIBLING document was added — the source was untouched,
// so the hash matched — while the database row was rebuilt from the fresh body.
// The file and the index then disagreed, and since backlinks are computed by
// reading the files, the stale side was the one feeding the graph.
//
// This only works because the page is a deterministic function of its inputs;
// `updated:` comes from the source file's mtime, never from time.Now().
func writePageIfChanged(path, page string) (written, existed bool) {
	existing, readErr := os.ReadFile(path)
	existed = readErr == nil
	if existed && string(existing) == page {
		return false, true
	}
	if err := os.WriteFile(path, []byte(page), 0o644); err != nil {
		return false, existed
	}
	return true, existed
}

func knowledgeEntityPage(doc knowledgeDoc) string {
	var b strings.Builder
	now := doc.updatedAt
	if now == "" {
		now = time.Now().UTC().Format("2006-01-02")
	}
	confidence := computeDocConfidence(doc)
	slug := wiki.SafeSlug(doc.title)
	if slug == "" {
		slug = wiki.SafeSlug(doc.path)
	}

	b.WriteString("---\n")
	_, _ = fmt.Fprintf(&b, "type: %s\n", doc.docType)
	_, _ = fmt.Fprintf(&b, "title: %s\n", doc.title)
	_, _ = fmt.Fprintf(&b, "generated.at: %s\n", now)
	b.WriteString("sources:\n")
	_, _ = fmt.Fprintf(&b, "  - %s\n", doc.path)
	if doc.summary != "" {
		summaryEscaped := strings.ReplaceAll(doc.summary, "\n", " ")
		_, _ = fmt.Fprintf(&b, "description: %s\n", summaryEscaped)
	}
	_, _ = fmt.Fprintf(&b, "tags:\n  - knowledge\n  - %s\n", doc.docType)
	_, _ = fmt.Fprintf(&b, "id: %s\n", slug)
	_, _ = fmt.Fprintf(&b, "confidence: %.2f\n", confidence)
	_, _ = fmt.Fprintf(&b, "content_hash: %s\n", doc.contentHash)
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

	_, _ = fmt.Fprintf(&b, "*Provenance: [%s](%s)*\n\n", doc.path, doc.path)

	if len(doc.crossRefs) > 0 {
		b.WriteString("## Cross-References\n\n")
		for _, ref := range doc.crossRefs {
			s := wiki.SafeSlug(ref)
			_, _ = fmt.Fprintf(&b, "- [%s](%s.md)\n", ref, s)
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

func knowledgeIndexPage(docs []knowledgeDoc, slugs []string, communities []KnowledgeCommunity) string {
	var b strings.Builder
	now := time.Now().UTC().Format("2006-01-02")
	b.WriteString("---\n")
	b.WriteString("type: navigation\n")
	b.WriteString("title: Knowledge Wiki Index\n")
	_, _ = fmt.Fprintf(&b, "generated.at: %s\n", now)
	b.WriteString("description: Navigation catalog for the Knowledge Wiki.\n")
	b.WriteString("tags:\n  - knowledge\n  - index\n")
	b.WriteString("id: index\n")
	b.WriteString("---\n\n")
	b.WriteString("# Knowledge Wiki Index\n\n")
	_, _ = fmt.Fprintf(&b, "> %s knowledge wiki. **Start here.** Scan the catalog below, then follow Markdown links to drill into specific pages.\n", brand.DisplayName)
	_, _ = fmt.Fprintf(&b, "> Check [log](log.md) for the timeline of updates. Last updated: %s\n\n", now)

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

	// Build doc lookup by slug, and the reverse for rendering entries.
	docBySlug := make(map[string]knowledgeDoc, len(docs))
	slugByPath := make(map[string]string, len(docs))
	for i, doc := range docs {
		slug := wiki.SafeSlug(doc.title)
		if i < len(slugs) && slugs[i] != "" {
			slug = slugs[i]
		}
		docBySlug[slug] = doc
		slugByPath[doc.path] = slug
	}

	writeDocEntry := func(doc knowledgeDoc) {
		s := slugByPath[doc.path]
		linkTitle := doc.title
		if linkTitle == "" {
			linkTitle = s
		}
		link := fmt.Sprintf("[%s](%s.md)", linkTitle, s)
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
					_, _ = fmt.Fprintf(&b, "- [%s](%s.md)\n", slug, slug)
				}
			}
			b.WriteString("\n")
		}

		// Unclustered docs
		var unclustered []knowledgeDoc
		for _, doc := range docs {
			if !clusteredSlugs[slugByPath[doc.path]] {
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
	b.WriteString("2. **Follow links** — each page has Markdown links to related pages.\n")
	b.WriteString("3. **Check backlinks** — each page lists what links *to* it (inbound references).\n")
	b.WriteString("4. **Check the log** — [log](log.md) shows the timeline of wiki updates.\n\n")
	b.WriteString("---\n")
	_, _ = fmt.Fprintf(&b, "*Generated by %s · %s*\n", brand.DisplayName, now)
	return b.String()
}

func appendKnowledgeLog(logPath string, totalDocs, articlesWritten, backlinksAdded int, added, updated, deleted []string, details map[string]wiki.LogDocDetails) {
	timestamp := time.Now().UTC().Format("2006-01-02 15:04:05")
	dateNow := time.Now().UTC().Format("2006-01-02")
	totalChanges := len(added) + len(updated) + len(deleted)

	var b strings.Builder
	_, _ = fmt.Fprintf(&b, "## [%s] sync | Compiled %d changes\n\n", timestamp, totalChanges)
	_, _ = fmt.Fprintf(&b, "- Total Documents: %d\n- Articles written/updated: %d\n- Backlinks injected: %d\n", totalDocs, articlesWritten, backlinksAdded)

	if len(added) > 0 {
		b.WriteString("- Added pages:\n")
		for _, slug := range added {
			title := slug
			if d, ok := details[slug]; ok && d.Title != "" {
				title = d.Title
			}
			link := fmt.Sprintf("[%s](%s.md)", title, slug)
			if d, ok := details[slug]; ok {
				summary := d.Summary
				if len(summary) > 120 {
					summary = summary[:120] + "…"
				}
				if summary != "" {
					_, _ = fmt.Fprintf(&b, "  - %s — %s\n", link, summary)
				} else {
					_, _ = fmt.Fprintf(&b, "  - %s\n", link)
				}
			} else {
				_, _ = fmt.Fprintf(&b, "  - %s\n", link)
			}
		}
	}
	if len(updated) > 0 {
		b.WriteString("- Updated pages:\n")
		for _, slug := range updated {
			title := slug
			if d, ok := details[slug]; ok && d.Title != "" {
				title = d.Title
			}
			link := fmt.Sprintf("[%s](%s.md)", title, slug)
			if d, ok := details[slug]; ok {
				summary := d.Summary
				if len(summary) > 120 {
					summary = summary[:120] + "…"
				}
				if summary != "" {
					_, _ = fmt.Fprintf(&b, "  - %s — %s\n", link, summary)
				} else {
					_, _ = fmt.Fprintf(&b, "  - %s\n", link)
				}
			} else {
				_, _ = fmt.Fprintf(&b, "  - %s\n", link)
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
		content = fmt.Sprintf("---\ntype: log\ntitle: Knowledge Wiki Log\ngenerated.at: %s\ndescription: Append-only chronological record of wiki compilation events.\ntags:\n  - knowledge\n  - log\nid: log\n---\n\n# Knowledge Wiki Log\n\n> Append-only chronological record. Parse with: `grep '^## \\[' log.md | tail -5`\n\n", dateNow)
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

// knowledgeDoc is one source document — the whole file, never a slice of one.
type knowledgeDoc struct {
	title       string
	path        string
	summary     string
	docType     string
	body        string
	contentHash string
	crossRefs   []string
	breadcrumb  string // source path, so a path query reaches the page
	cluster     int    // community ID from Louvain (-1 = unassigned)
	clusterName string // label of the community
	staleSince  string // ISO date if page is stale, empty otherwise
	staleReason string // why it's stale
	isMarkdown  bool   // true for .md/.markdown/.mdx — rendered as prose, not fenced
	updatedAt   string // source mtime as YYYY-MM-DD; keeps the page byte-stable
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
