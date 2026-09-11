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

func (s WikiScope) walkRoot(absRoot string) string {
	sub := strings.TrimSpace(s.Subdir)
	if sub == "" || sub == "." {
		return absRoot
	}
	return filepath.Join(absRoot, filepath.FromSlash(sub))
}

func (s WikiScope) walkRoots(absRoot string) []string {
	return []string{s.walkRoot(absRoot)}
}

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

	var docs []knowledgeDoc
	for _, src := range sources {
		updatedAt := time.Unix(0, src.mtime).UTC().Format("2006-01-02")
		content := string(src.data)

		doc := knowledgeDoc{
			title:       wiki.ExtractTitle(content, src.relPath),
			path:        src.relPath,
			summary:     wiki.ExtractSummary(content),
			docType:     classifyDocType(src.relPath, content),
			body:        content,
			breadcrumb:  filepath.ToSlash(src.relPath),
			contentHash: src.contentHash,
			crossRefs:   wiki.ExtractCrossRefs(content),
			updatedAt:   updatedAt,
		}
		docs = append(docs, doc)
	}

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

	indexedHashes, err := wiki.IndexedPageHashes(ctx, wikiDir)
	if err != nil {
		return nil, fmt.Errorf("reading the compiled index: %w", err)
	}
	indexedChunks, err := wiki.IndexedChunks(ctx, wikiDir)
	if err != nil {
		return nil, fmt.Errorf("reading the compiled corpus: %w", err)
	}

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

		autoLinkedBody, autoRefs := wiki.AutoLinkContent(docs[i].body, compiledTargets, slug)
		docs[i].body = autoLinkedBody

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

	nothingChanged := result.ArticlesWritten == 0 && len(deleted) == 0
	if nothingChanged && wiki.IndexHasContent(ctx, wikiDir) {
		return result, nil
	}

	graph := wiki.BuildCrossRefGraphFromRefs(knowledgePageEdges(docs, docSlugs))
	stats := wiki.CrossRefStats(graph)
	result.BacklinksAdded = stats.BacklinksAdded
	result.OrphanPages = stats.OrphanPages
	result.BrokenLinks = stats.BrokenLinks

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
		for i := range docs {
			if info, ok := stalePages[docSlugs[i]]; ok {
				docs[i].staleSince = info.Since
				docs[i].staleReason = info.Reason
			}
		}
	}

	if graph != nil {
		lintResult := LintKnowledgeWiki(graph, docs, docSlugs)
		if lintResult != nil {
			result.LintFindings = len(lintResult.Findings)
		}
	}

	sort.Strings(added)
	sort.Strings(updated)
	sort.Strings(deleted)

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
			Updated:     doc.updatedAt,
			Important:   important,
			Tags:        knowledgeWikiTags(doc.docType, important, doc.staleSince != ""),
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

	return result, nil
}

func knowledgeWikiTags(docType string, important, stale bool) []string {
	tags := []string{"wiki"}
	if docType != "" {
		tags = append(tags, docType)
	}
	if important {
		tags = append(tags, "important")
	}
	if stale {
		tags = append(tags, "stale")
	}
	return tags
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
	breadcrumb  string
	cluster     int
	clusterName string
	staleSince  string
	staleReason string
	updatedAt   string
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
