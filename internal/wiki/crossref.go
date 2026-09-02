package wiki

import (
	"fmt"
	"os"
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
	TotalPages     int
	TotalLinks     int
	BacklinksAdded int
	OrphanPages    int
	BrokenLinks    int
}

var reXRefWikiLink = regexp.MustCompile(`\[\[([^\]]+)\]\]`)
var reXRefMdLink = regexp.MustCompile(`\[([^\]]+)\]\(([^)]+)\)`)

var reWikiH1 = regexp.MustCompile(`(?m)^#\s+(.+)$`)

func BuildCrossRefGraph(wikiDir string) (*CrossRefGraph, error) {
	graph := &CrossRefGraph{
		Outbound: make(map[string][]string),
		Inbound:  make(map[string][]string),
		AllPages: make(map[string]bool),
		Titles:   make(map[string]string),
	}

	entries, err := os.ReadDir(wikiDir)
	if err != nil {
		return nil, fmt.Errorf("reading wiki dir: %w", err)
	}

	for _, e := range entries {
		if e.IsDir() || filepath.Ext(e.Name()) != ".md" {
			continue
		}
		slug := strings.TrimSuffix(e.Name(), ".md")
		graph.AllPages[slug] = true

		data, err := os.ReadFile(filepath.Join(wikiDir, e.Name()))
		if err != nil {
			continue
		}
		content := string(data)

		if m := reWikiH1.FindStringSubmatch(content); m != nil {
			graph.Titles[slug] = strings.TrimSpace(m[1])
		} else {
			graph.Titles[slug] = slug
		}

		contentNoBacklinks := stripBacklinksSection(content)
		matchesWiki := reXRefWikiLink.FindAllStringSubmatch(contentNoBacklinks, -1)
		matchesMd := reXRefMdLink.FindAllStringSubmatch(contentNoBacklinks, -1)
		seen := make(map[string]bool)

		for _, m := range matchesWiki {
			target := m[1]
			if !isBundlePageLink(target) {
				continue
			}
			resolvedTarget := ResolveSlug(target)
			if resolvedTarget == "" || resolvedTarget == slug || seen[resolvedTarget] {
				continue
			}
			seen[resolvedTarget] = true
			graph.Outbound[slug] = append(graph.Outbound[slug], resolvedTarget)
		}

		for _, m := range matchesMd {
			rawTarget := m[2]
			if !isBundlePageLink(rawTarget) {
				continue
			}
			resolvedTarget := ResolveSlug(rawTarget)
			if resolvedTarget == "" || resolvedTarget == slug || seen[resolvedTarget] {
				continue
			}
			seen[resolvedTarget] = true
			graph.Outbound[slug] = append(graph.Outbound[slug], resolvedTarget)
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

	return graph, nil
}

func InjectBacklinks(wikiDir string, graph *CrossRefGraph) (*CrossRefResult, error) {
	result := &CrossRefResult{
		TotalPages: len(graph.AllPages),
	}

	for _, targets := range graph.Outbound {
		result.TotalLinks += len(targets)
	}

	for page := range graph.AllPages {
		hasInbound := len(graph.Inbound[page]) > 0
		hasOutbound := len(graph.Outbound[page]) > 0
		if !hasInbound && !hasOutbound {
			result.OrphanPages++
		}
	}

	for _, targets := range graph.Outbound {
		for _, target := range targets {
			if !graph.AllPages[target] {
				result.BrokenLinks++
			}
		}
	}

	for page := range graph.AllPages {
		inbound := graph.Inbound[page]
		if len(inbound) == 0 {
			continue
		}

		filePath := filepath.Join(wikiDir, page+".md")
		data, err := os.ReadFile(filePath)
		if err != nil {
			continue
		}

		content := string(data)
		newContent := injectBacklinksSection(content, inbound, graph.Titles)

		if newContent != content {
			if err := os.WriteFile(filePath, []byte(newContent), 0o644); err != nil {
				continue
			}
			result.BacklinksAdded++
		}
	}

	return result, nil
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
	// [[target|label]] — the label is display text, only the target addresses a page.
	if i := strings.Index(target, "|"); i >= 0 {
		target = strings.TrimSpace(target[:i])
	}
	if target == "" {
		return false
	}
	// A fragment, alone or appended, addresses a position rather than a page.
	if i := strings.IndexAny(target, "#?"); i >= 0 {
		target = target[:i]
	}
	if target == "" {
		return false
	}
	// wiki:// is this project's own in-UI protocol and is stripped by ResolveSlug, so it is
	// recognised before the scheme test below rejects everything that looks like a URL.
	if after, ok := strings.CutPrefix(target, "wiki://"); ok {
		target = after
	} else if strings.Contains(target, "://") || strings.HasPrefix(target, "mailto:") || strings.HasPrefix(target, "tel:") {
		return false
	}
	// A bundle-relative link (OKF §6.1, the RECOMMENDED form) is rooted at the bundle,
	// and a compiled wiki IS the bundle root, so `/slug.md` addresses a page here.
	target = strings.TrimPrefix(target, "/")
	if strings.ContainsAny(target, "/\\") {
		return false
	}
	// A trailing extension disqualifies the target only when it LOOKS like a file
	// extension. Slugs are built from titles, and a title routinely carries a dot —
	// `graphit.lock.json_handling` is a page, `pipeline.go` is not — so the test is the
	// shape of the suffix rather than the presence of a dot.
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
