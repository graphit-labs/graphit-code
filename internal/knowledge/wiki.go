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

// enumerateKnowledgeSources walks the scope once for the generation pass.
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

	candidates, err := enumerateKnowledgeSources(absRoot, scope, exts, ic)
	if err != nil {
		return nil, fmt.Errorf("walking docs: %w", err)
	}

	type sourceFile struct {
		relPath     string
		data        []byte
		contentHash string
		mtime       int64
	}
	sources := make([]sourceFile, 0, len(candidates))

	for _, c := range candidates {
		data, readErr := os.ReadFile(filepath.Join(absRoot, c.relPath))
		if readErr != nil {
			continue
		}

		contentHash := fmt.Sprintf("%x", sha256.Sum256(data))[:16]
		sources = append(sources, sourceFile{
			relPath:     c.relPath,
			data:        data,
			contentHash: contentHash,
			mtime:       c.mtime,
		})
	}

	// Process sources: one wiki document per source file. The compiled table is the cache: the
	// incremental writer below compares these rows with what is already stored and preserves
	// unchanged rows and their embeddings.
	//
	// THE DOCUMENT IS THE UNIT. A source file is never split into per-heading
	// pieces. Splitting produced one page per heading, and a heading whose whole
	// content was subsections produced an EMPTY page — measured at 11,4% of the
	// index — which still carried a title into the ranking and outranked the
	// prose it was supposed to introduce. It also made a document's own page the
	// empty one whenever the document opened with a single H1.
	var docs []knowledgeDoc
	for _, src := range sources {
		updatedAt := time.Unix(0, src.mtime).UTC().Format("2006-01-02")
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
			updatedAt:   updatedAt,
		}
		docs = append(docs, doc)
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
	indexedChunks, err := wiki.IndexedChunks(ctx, wikiDir)
	if err != nil {
		return nil, fmt.Errorf("reading the compiled corpus: %w", err)
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

	// FAST PATH: the table's own slug/hash projection is the evidence that no work is needed.
	fastEntries := make([]wiki.DocHashEntry, len(docs))
	for i, doc := range docs {
		fastEntries[i] = wiki.DocHashEntry{
			ContentHash: doc.contentHash,
			Slug:        docSlugs[i],
		}
	}
	if wiki.FastPathCheck(ctx, wikiDir, fastEntries) {
		return result, nil
	}

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
	// SyncDB below receives these same documents and is the only place one becomes readable, so
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
	// existed to avoid a pointless file write; there is no file to write, so the cheaper and more
	// honest signal is whether the document itself
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

	// Phase 1: Cross-reference graph. It is rebuilt in memory; the table sync below updates only
	// source slugs whose edge set changed, so no sidecar graph cache is needed.
	// `index.md` is not written. It was a catalogue of slugs, titles, types and clusters,
	// rewritten in full on every build — and rewritten twice more below, once for the clusters and
	// once for the staleness. Everything on it is a column, so the catalogue is a Browse query:
	// `wiki.WikiOverview` renders it for the AI consultation cycle, `graphit wiki browse` for a
	// person, and `graphit wiki export` writes the page itself for whoever wants the tree.

	// The cross-reference graph is built from the edges this pass just resolved, not by reading
	// pages back and re-extracting their links with two regexes. Same edge set — it is the one
	// written to the `xrefs` table below — derived once, by the pass that owns it.
	graph := wiki.BuildCrossRefGraphFromRefs(knowledgePageEdges(docs, docSlugs))
	stats := wiki.CrossRefStats(graph)
	result.BacklinksAdded = stats.BacklinksAdded
	result.OrphanPages = stats.OrphanPages
	result.BrokenLinks = stats.BrokenLinks

	// Phase 2: Community detection
	communities := DetectKnowledgeCommunities(graph)
	result.Communities = len(communities)
	if len(communities) > 0 {
		slugToCluster, slugToClusterName := AssignCommunities(communities)
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

	// Phase 3: Staleness tracking
	oldManifest := ManifestFromChunks(indexedChunks)
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

	// Phase 5: incrementally synchronize the LanceDB wiki.
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

	if err := wiki.SyncDB(ctx, wikiDir, wikiChunks, xrefs, syncLogEntry); err != nil {
		return nil, err
	}

	// `log.md` is not appended. The same information — the timestamp, the counts, and the three
	// slug lists with their titles and summaries — is the `sync_log` row written above by
	// SyncDB; `graphit wiki log` and `graphit_wiki_log` read it, and `graphit wiki export`
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
