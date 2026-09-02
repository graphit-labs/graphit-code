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

	"github.com/graphit-labs/graphit-code/internal/ignorer"
	"github.com/graphit-labs/graphit-code/internal/wiki"
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
	// the index already holds content, skip the full rebuild entirely.
	// Both ignore files are watched: editing either changes what is in scope, and
	// a stat-unchanged tree would otherwise skip the rebuild that has to notice.
	watchFiles := []string{KnowledgeIgnoreFile}
	if rel, relErr := filepath.Rel(absRoot, walkRoot); relErr == nil && rel != "." {
		watchFiles = append(watchFiles, filepath.Join(rel, KnowledgeIgnoreFile))
	}
	if wiki.StatPreCheck(ctx, absRoot, wikiDir, processCache, wiki.StatPreCheckOpts{
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

	// What the index already holds, which is what "added", "updated" and "deleted" are measured
	// against. This used to be an `os.ReadDir` of the wiki directory: the pages were the record of
	// what existed, so a page on disk meant an indexed document. The pages are not written any
	// more, and the index was always the better answer — it is the set a search can reach, and it
	// carries each page's content hash, which the directory listing did not.
	indexedHashes, err := wiki.IndexedPageHashes(ctx, wikiDir)
	if err != nil {
		return nil, fmt.Errorf("reading the compiled index: %w", err)
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

	// FAST PATH: use wiki.FastPathCheck which checks processCache (O(1) per entry — no disk I/O)
	// and the index's own slug set to detect deletions.
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

	// There was a second skip gate here, for `processCache == nil`: it read the `content_hash`
	// out of each page's frontmatter and compared it against the source. It is gone rather than
	// ported to the index, because FastPathCheck above already asks the index the same question —
	// the only thing this branch did that the fast path does not is work without a process cache,
	// and a build with no cache is a build that has to run.

	// Full pipeline: something changed or first run.
	compiledTargets := wiki.BuildAutoLinkTargets(titlesMap)

	newSlugs := make(map[string]bool)
	var added []string
	var updated []string
	docDetails := make(map[string]wiki.LogDocDetails)

	// NO PAGE IS WRITTEN, and nothing is pruned.
	//
	// This loop used to end in `writePageIfChanged(<wikiDir>/<slug>.md, knowledgeEntityPage(doc))`,
	// followed by an `os.Remove` sweep over the slugs no document claimed any more. Both are gone:
	// RebuildDB below receives these same documents and is the only place one becomes readable, so
	// the page was a second copy of it — written, pruned, and then read back by the cross-reference
	// pass, the lint and the explorer, all of which now read the index instead.
	//
	// What the loop still does is the part that was never about files: autolinking a body against
	// every other document's title, which is why the body indexed for a document is not simply its
	// source text.
	//
	// added/updated/deleted are decided by comparing against the index, and the comparison is on
	// the SOURCE content hash. `writePageIfChanged` compared rendered bytes, which counted a
	// document as "updated" when a sibling was added and changed its autolinks. That distinction
	// existed to avoid a pointless file write; there is no file to write, and RebuildDB rewrites
	// every row regardless, so the cheaper and more honest signal is whether the document itself
	// changed.
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

		indexedHash, wasIndexed := indexedHashes[slug]
		switch {
		case !wasIndexed:
			added = append(added, slug)
		case indexedHash != docs[i].contentHash:
			updated = append(updated, slug)
		default:
			continue
		}
		result.ArticlesWritten++
	}

	var deleted []string
	for slug := range indexedHashes {
		if !newSlugs[slug] {
			deleted = append(deleted, slug)
		}
	}

	// Skip the expensive phases if nothing ended up changing. The gate is the index having
	// content, not a file existing beside it: an index that is present and empty is the state
	// both skip paths used to get permanently stuck in — see wiki.IndexHasContent.
	nothingChanged := result.ArticlesWritten == 0 && len(deleted) == 0
	if nothingChanged && wiki.IndexHasContent(ctx, wikiDir) {
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
	// `index.md` is not written. It was a catalogue of slugs, titles, types and clusters,
	// rewritten in full on every build — and rewritten twice more below, once for the clusters and
	// once for the staleness. Everything on it is a column, so the catalogue is a Browse query:
	// `wiki.WikiOverview` renders it for the AI consultation cycle, `graphit wiki browse` for a
	// person, and `graphit wiki export` writes the page itself for whoever wants the tree.

	// The cross-reference graph is built from the edges this pass just resolved, not by reading
	// pages back and re-extracting their links with two regexes. Same edge set — it is the one
	// written to the `xrefs` table below — derived once, by the pass that owns it.
	var graph *wiki.CrossRefGraph
	if !crossRefsOK {
		graph = wiki.BuildCrossRefGraphFromRefs(knowledgePageEdges(docs, docSlugs))
		stats := wiki.CrossRefStats(graph)
		result.BacklinksAdded = stats.BacklinksAdded
		result.OrphanPages = stats.OrphanPages
		result.BrokenLinks = stats.BrokenLinks
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

			// The cluster lands on the document, and that is the whole of it. There used to be a
			// second pass here re-rendering every clustered page and a third rewriting index.md,
			// because the cluster had to reach the file after the file had already been written.
			// It reaches the `cluster_id`/`cluster_name` columns instead, and those are filled
			// from these same documents when the chunks are built below.
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
		// Staleness lands on the document and travels into the `stale_since`/`stale_reason`
		// columns with everything else. The re-render of the affected pages, and the third
		// rewrite of index.md that followed it, are gone with the pages.
		for i := range docs {
			if info, ok := stalePages[docSlugs[i]]; ok {
				docs[i].staleSince = info.Since
				docs[i].staleReason = info.Reason
			}
		}
	}
	SaveManifest(wikiDir, newManifest)

	// Phase 4: Lint
	if graph != nil {
		lintResult := LintKnowledgeWiki(graph, docs, docSlugs)
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
			// The source's date. Stamping "today" here made every row claim it was updated on the
			// day of the last sync.
			Updated:     doc.updatedAt,
			Important:   important,
			StaleSince:  doc.staleSince,
			StaleReason: doc.staleReason,
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

	// `log.md` is not appended. The same information — the timestamp, the counts, and the three
	// slug lists with their titles and summaries — is the `sync_log` row written above by
	// RebuildDB; `graphit wiki log` and `graphit_wiki_log` read it, and `graphit wiki export`
	// renders the page for whoever wants the file.

	return result, nil
}

// knowledgePageEdges is the graph input: each page's slug, title, and the slugs it points at.
//
// The targets are `SafeSlug`-resolved here for the same reason they are resolved that way when the
// `xrefs` table is written a few lines below — the two must agree, or the graph and the table
// disagree about what a link means.
func knowledgePageEdges(docs []knowledgeDoc, slugs []string) []wiki.PageEdges {
	out := make([]wiki.PageEdges, 0, len(docs))
	for i, doc := range docs {
		targets := make([]string, 0, len(doc.crossRefs))
		for _, ref := range doc.crossRefs {
			targets = append(targets, wiki.SafeSlug(ref))
		}
		out = append(out, wiki.PageEdges{Slug: slugs[i], Title: doc.title, Targets: targets})
	}
	return out
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
