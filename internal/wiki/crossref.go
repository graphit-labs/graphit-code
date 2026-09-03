package wiki

import (
	"context"
	"fmt"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

type CrossRefGraph struct {
	Outbound map[string][]string

	Inbound map[string][]string

	AllPages map[string]bool

	Titles map[string]string
}

type CrossRefResult struct {
	TotalPages  int
	TotalLinks  int
	OrphanPages int
	BrokenLinks int

	BacklinksAdded int
}

var reXRefWikiLink = regexp.MustCompile(`\[\[([^\]]+)\]\]`)
var reXRefMdLink = regexp.MustCompile(`\[([^\]]+)\]\(([^)]+)\)`)

// PageEdges is one page's identity and the pages it points at, as the producer already knows them.
//
// The generator resolves a document's cross-references while it is compiling it — autolinks
// included — and writes exactly this set to the `xrefs` table. Handing it here is what replaced
// re-reading every rendered page and re-extracting the links with two regexes: the same edges,
// derived once, by the pass that owns them.
type PageEdges struct {
	Slug    string
	Title   string
	Targets []string
}

// BuildCrossRefGraphFromRefs assembles the graph from resolved edges.
//
// Targets are slugs already: the caller resolved them, because the caller is the only side that
// knows which title a link meant. Self-references and duplicates are dropped here rather than at
// every call site, and a target that names no page is KEPT — that is what makes it a broken link
// instead of a missing one.
func BuildCrossRefGraphFromRefs(pages []PageEdges) *CrossRefGraph {
	graph := &CrossRefGraph{
		Outbound: make(map[string][]string),
		Inbound:  make(map[string][]string),
		AllPages: make(map[string]bool, len(pages)),
		Titles:   make(map[string]string, len(pages)),
	}

	for _, p := range pages {
		if p.Slug == "" {
			continue
		}
		graph.AllPages[p.Slug] = true
		if p.Title != "" {
			graph.Titles[p.Slug] = p.Title
		} else {
			graph.Titles[p.Slug] = p.Slug
		}

		seen := make(map[string]bool, len(p.Targets))
		for _, target := range p.Targets {
			if target == "" || target == p.Slug || seen[target] {
				continue
			}
			seen[target] = true
			graph.Outbound[p.Slug] = append(graph.Outbound[p.Slug], target)
		}
	}

	for source, targets := range graph.Outbound {
		for _, target := range targets {
			graph.Inbound[target] = append(graph.Inbound[target], source)
		}
	}
	for k := range graph.Outbound {
		sort.Strings(graph.Outbound[k])
	}
	for k := range graph.Inbound {
		sort.Strings(graph.Inbound[k])
	}
	return graph
}

// BuildCrossRefGraphFromIndex assembles the graph for a caller that holds only a wiki directory.
//
// The pages come from `chunks` and the edges from `xrefs`, which is where the producer wrote them.
// Lint is the reason this exists: it audits a compiled wiki it did not build, so it has no
// in-memory edge set to be handed.
func BuildCrossRefGraphFromIndex(ctx context.Context, wikiDir string) (*CrossRefGraph, error) {
	db, err := OpenWikiDB(ctx, wikiDir)
	if err != nil {
		return nil, err
	}
	defer func() { _ = db.Close() }()

	titles, err := db.PageTitles(ctx)
	if err != nil {
		return nil, err
	}
	edges, err := db.AllXRefs(ctx)
	if err != nil {
		return nil, err
	}

	pages := make([]PageEdges, 0, len(titles))
	for slug, title := range titles {
		pages = append(pages, PageEdges{Slug: slug, Title: title, Targets: edges[slug]})
	}
	sort.Slice(pages, func(i, j int) bool { return pages[i].Slug < pages[j].Slug })
	return BuildCrossRefGraphFromRefs(pages), nil
}

// CrossRefStats reports what the graph says about the wiki's connectedness.
//
// It replaced `InjectBacklinks`, which computed exactly these four numbers and then, as a side
// effect, appended a `## Backlinks` section into every page with an inbound reference. The numbers
// were the useful half; the writing was the duplication this work removes.
func CrossRefStats(graph *CrossRefGraph) *CrossRefResult {
	if graph == nil {
		return &CrossRefResult{}
	}
	result := &CrossRefResult{TotalPages: len(graph.AllPages)}

	for _, targets := range graph.Outbound {
		result.TotalLinks += len(targets)
		for _, target := range targets {
			if !graph.AllPages[target] {
				result.BrokenLinks++
			}
		}
	}

	for page := range graph.AllPages {
		if len(graph.Inbound[page]) > 0 {
			result.BacklinksAdded++
			continue
		}
		if len(graph.Outbound[page]) == 0 {
			result.OrphanPages++
		}
	}

	return result
}

func OrphanPages(graph *CrossRefGraph) []string {
	var orphans []string
	for page := range graph.AllPages {
		if len(graph.Inbound[page]) == 0 && len(graph.Outbound[page]) == 0 {
			orphans = append(orphans, page)
		}
	}
	sort.Strings(orphans)
	return orphans
}

func BrokenLinks(graph *CrossRefGraph) []BrokenLinkInfo {
	var broken []BrokenLinkInfo
	seen := make(map[string]bool)
	for source, targets := range graph.Outbound {
		for _, target := range targets {
			if !graph.AllPages[target] && !seen[target] {
				seen[target] = true
				broken = append(broken, BrokenLinkInfo{
					Target: target,
					Source: source,
				})
			}
		}
	}
	sort.Slice(broken, func(i, j int) bool { return broken[i].Target < broken[j].Target })
	return broken
}

type BrokenLinkInfo struct {
	Target string
	Source string
}

func stripCodeBlocks(content string) string {
	var sb strings.Builder
	inFenced := false
	inInline := false

	lines := strings.Split(content, "\n")
	for _, line := range lines {
		trimmed := strings.TrimSpace(line)
		if strings.HasPrefix(trimmed, "```") {
			inFenced = !inFenced
			continue
		}
		if inFenced {
			continue
		}

		var lineBuilder strings.Builder
		runes := []rune(line)
		for j := 0; j < len(runes); j++ {
			if runes[j] == '`' {
				inInline = !inInline
				continue
			}
			if !inInline {
				lineBuilder.WriteRune(runes[j])
			}
		}
		sb.WriteString(lineBuilder.String() + "\n")
	}
	return sb.String()
}

func FindWikiLinks(content string) []string {
	content = stripCodeBlocks(content)
	matchesWiki := reXRefWikiLink.FindAllStringSubmatch(content, -1)
	matchesMd := reXRefMdLink.FindAllStringSubmatch(content, -1)
	seen := make(map[string]bool)
	var result []string

	for _, m := range matchesWiki {
		if !isBundlePageLink(m[1]) {
			continue
		}
		target := ResolveSlug(m[1])
		if target != "" && !seen[target] {
			seen[target] = true
			result = append(result, target)
		}
	}
	for _, m := range matchesMd {
		rawTarget := m[2]
		if !isBundlePageLink(rawTarget) {
			continue
		}
		target := ResolveSlug(rawTarget)
		if target != "" && !seen[target] {
			seen[target] = true
			result = append(result, target)
		}
	}
	return result
}

// isBundlePageLink says whether a markdown link target names a page of THIS wiki.
//
// OKF §6.1 lets a concept link anywhere with a plain markdown link, and §6.2 lets
// path-valued fields hold absolute URLs and relative paths. So after the move off
// [[wikilinks]], "it is a markdown link" stopped being evidence of anything: the
// Provenance line every generated page carries — `*Provenance: [docs/x.md](docs/x.md)*` —
// is a link to a REPOSITORY file, and body links point at source files too.
//
// Treating those as cross-references is not a cosmetic mistake. ResolveSlug flattens a
// path into a slug, so `../../internal/ast/pipeline.go` became the page
// `..-..-internal-ast-pipeline.go`, which exists nowhere: 354 of this project's 354
// "broken links" were this, the backlink graph gained an edge per page, and lint reported
// a wiki that was in fact intact.
//
// The discriminator is the wiki's own shape rather than a list of schemes to exclude: a
// compiled wiki is FLAT — one directory of `<slug>.md` — so a target that carries a path
// separator, a directory hop, or a non-markdown extension cannot be a page in it. That
// holds for anything the generator emits and for anything a human writes, which a
// blocklist of URL schemes never did.
//
// It applies to [[wikilinks]] for the same reason. Source documents under the docs tree
// contain hand-written `[[../architecture/storage_layout.md#section]]` links, and a page
// body is copied into the wiki verbatim; those resolve to nothing here either.
func isBundlePageLink(rawTarget string) bool {
	target := strings.TrimSpace(rawTarget)
	if i := strings.Index(target, "|"); i >= 0 {
		target = strings.TrimSpace(target[:i])
	}
	if target == "" {
		return false
	}
	if i := strings.IndexAny(target, "#?"); i >= 0 {
		target = target[:i]
	}
	if target == "" {
		return false
	}
	if after, ok := strings.CutPrefix(target, "wiki://"); ok {
		target = after
	} else if strings.Contains(target, "://") || strings.HasPrefix(target, "mailto:") || strings.HasPrefix(target, "tel:") {
		return false
	}
	target = strings.TrimPrefix(target, "/")
	if strings.ContainsAny(target, "/\\") {
		return false
	}
	if ext := filepath.Ext(target); ext != "" && !strings.EqualFold(ext, ".md") && looksLikeFileExt(ext) {
		return false
	}
	return true
}

var reFileExt = regexp.MustCompile(`^\.[A-Za-z0-9]{1,5}$`)

func looksLikeFileExt(ext string) bool { return reFileExt.MatchString(ext) }

func ResolveSlug(rawLink string) string {
	target := rawLink
	if idx := strings.Index(target, "|"); idx >= 0 {
		target = target[:idx]
	}
	target = strings.TrimSpace(target)
	target = strings.TrimPrefix(target, "wiki://")
	if strings.HasSuffix(strings.ToLower(target), ".md") {
		target = target[:len(target)-3]
	}
	return SafeSlug(target)
}

const backlinksHeader = "## Backlinks"
const backlinksSeparator = "\n" + backlinksHeader + "\n"

func stripBacklinksSection(content string) string {
	idx := strings.Index(content, backlinksSeparator)
	if idx < 0 {

		if strings.HasPrefix(content, backlinksHeader+"\n") {
			return ""
		}
		return content
	}
	return content[:idx]
}

func injectBacklinksSection(content string, inbound []string, titles map[string]string) string {

	base := stripBacklinksSection(content)
	base = strings.TrimRight(base, "\n")

	var b strings.Builder
	b.WriteString("\n\n")
	b.WriteString(backlinksHeader)
	b.WriteString("\n\n")
	b.WriteString("> Pages that reference this article:\n\n")
	for _, src := range inbound {
		title := titles[src]
		if title == "" || title == src {
			_, _ = fmt.Fprintf(&b, "- [%s](%s.md)\n", src, src)
		} else {
			_, _ = fmt.Fprintf(&b, "- [%s](%s.md) — %s\n", title, src, title)
		}
	}

	return base + b.String()
}
