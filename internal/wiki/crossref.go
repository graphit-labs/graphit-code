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

		if m := reBM25H1.FindStringSubmatch(content); m != nil {
			graph.Titles[slug] = strings.TrimSpace(m[1])
		} else {
			graph.Titles[slug] = slug
		}

		contentNoBacklinks := stripBacklinksSection(content)
		matches := reXRefWikiLink.FindAllStringSubmatch(contentNoBacklinks, -1)
		seen := make(map[string]bool)
		for _, m := range matches {
			target := m[1]
			resolvedTarget := ResolveSlug(target)
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
	matches := reXRefWikiLink.FindAllStringSubmatch(content, -1)
	seen := make(map[string]bool)
	var result []string
	for _, m := range matches {
		target := ResolveSlug(m[1])
		if target != "" && !seen[target] {
			seen[target] = true
			result = append(result, target)
		}
	}
	return result
}

func ResolveSlug(rawLink string) string {
	target := rawLink
	if idx := strings.Index(target, "|"); idx >= 0 {
		target = target[:idx]
	}
	target = strings.TrimSpace(target)
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
			_, _ = fmt.Fprintf(&b, "- [[%s]]\n", src)
		} else {
			_, _ = fmt.Fprintf(&b, "- [[%s]] — %s\n", src, title)
		}
	}

	return base + b.String()
}
