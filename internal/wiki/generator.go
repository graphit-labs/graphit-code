package wiki

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"
	"unicode"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"golang.org/x/text/cases"
	"golang.org/x/text/language"
)

type WikiResult struct {
	ArticlesWritten int
	OutputDir       string
	CrossRefs       *CrossRefResult
}

type WikiConfig struct {
	OutputDir string

	EntityQuery string

	NodeLabels []string
	EdgeLabels []string

	Algo Algorithm

	Renderer WikiRenderer

	ModuleTag string

	EnableCrossRefs bool
}

func GenerateWiki(ctx context.Context, db GraphAdapter, cfg WikiConfig) (*WikiResult, error) {
	if err := os.MkdirAll(cfg.OutputDir, 0o755); err != nil {
		return nil, fmt.Errorf("creating wiki dir: %w", err)
	}

	communities, err := DetectCommunities(ctx, db, IndexConfig{
		NodeLabels: cfg.NodeLabels,
		EdgeLabels: cfg.EdgeLabels,
	}, cfg.Algo)
	if err != nil {
		communities = nil
	}

	godNodes, _ := GodNodes(ctx, db, cfg.NodeLabels, 10)
	totalNodes, totalEdges := GraphCounts(ctx, db, cfg.NodeLabels, cfg.EdgeLabels)
	entities := loadEntitySummaries(ctx, db, cfg.EntityQuery)

	result := &WikiResult{OutputDir: cfg.OutputDir}
	usedSlugs := make(map[string]bool)

	for _, ent := range entities {
		slug := uniqueSlug(SafeFilename(ent.Name), usedSlugs)
		var page string
		if cfg.Renderer != nil {
			page = cfg.Renderer.EntityPage(ctx, db, ent)
		} else {
			page = genericEntityPage(ent, cfg.ModuleTag)
		}
		path := filepath.Join(cfg.OutputDir, slug+".md")
		if err := os.WriteFile(path, []byte(page), 0o644); err != nil {
			continue
		}
		result.ArticlesWritten++
	}

	communityLabels := make(map[int]string)
	for _, c := range communities {
		communityLabels[c.ID] = c.Label
	}

	for _, c := range communities {
		slug := uniqueSlug(SafeFilename(c.Label), usedSlugs)
		page := communityPage(c, communityLabels, cfg.ModuleTag)
		path := filepath.Join(cfg.OutputDir, slug+".md")
		if err := os.WriteFile(path, []byte(page), 0o644); err != nil {
			continue
		}
		result.ArticlesWritten++
	}

	for _, gn := range godNodes {
		name, _ := gn["name"].(string)
		if name == "" {
			continue
		}
		slug := uniqueSlug(SafeFilename(name), usedSlugs)
		page := godNodePage(gn, communityLabels, cfg.ModuleTag)
		path := filepath.Join(cfg.OutputDir, slug+".md")
		if err := os.WriteFile(path, []byte(page), 0o644); err != nil {
			continue
		}
		result.ArticlesWritten++
	}

	var indexContent string
	if cfg.Renderer != nil {
		indexContent = cfg.Renderer.IndexPage(entities, communities, godNodes, totalNodes, totalEdges, cfg.ModuleTag)
	} else {
		indexContent = genericIndexPage(entities, communities, godNodes, totalNodes, totalEdges, cfg.ModuleTag)
	}
	if err := os.WriteFile(filepath.Join(cfg.OutputDir, "index.md"), []byte(indexContent), 0o644); err != nil {
		return result, err
	}

	if cfg.EnableCrossRefs {
		graph, err := BuildCrossRefGraph(cfg.OutputDir)
		if err == nil {
			xref, _ := InjectBacklinks(cfg.OutputDir, graph)
			result.CrossRefs = xref
		}
	}

	logTitle := cfg.ModuleTag + " Wiki Log"
	if cfg.Renderer != nil {
		logTitle = cfg.Renderer.LogTitle()
	}
	appendLog(filepath.Join(cfg.OutputDir, "log.md"), logTitle, totalNodes, totalEdges, len(communities), result.ArticlesWritten, cfg.ModuleTag)

	return result, nil
}

func loadEntitySummaries(ctx context.Context, db GraphAdapter, query string) []EntitySummary {
	if query == "" {
		return nil
	}
	res, err := db.Query(ctx, query, nil)
	if err != nil {
		return nil
	}
	var out []EntitySummary
	for _, r := range res.Records {
		uid, _ := r["uid"].(string)
		name, _ := r["name"].(string)
		if uid == "" || name == "" {
			continue
		}
		out = append(out, EntitySummary{
			UID:     uid,
			Name:    name,
			Path:    strVal(r["path"]),
			Summary: strVal(r["summary"]),
			Type:    strVal(r["type"]),
		})
	}
	return out
}

func genericEntityPage(ent EntitySummary, tag string) string {
	var b strings.Builder
	now := time.Now().UTC().Format("2006-01-02")
	confidence := computeConfidence(ent)

	b.WriteString("---\n")
	_, _ = fmt.Fprintf(&b, "title: %s\n", ent.Name)
	_, _ = fmt.Fprintf(&b, "type: %s\n", ent.Type)
	_, _ = fmt.Fprintf(&b, "source: %s\n", ent.Path)
	_, _ = fmt.Fprintf(&b, "updated: %s\n", now)
	_, _ = fmt.Fprintf(&b, "confidence: %.2f\n", confidence)
	b.WriteString("source_count: 1\n")
	_, _ = fmt.Fprintf(&b, "tags: [%s, %s]\n", tag, ent.Type)
	b.WriteString("---\n\n")
	_, _ = fmt.Fprintf(&b, "# %s\n\n", ent.Name)
	if ent.Summary != "" {
		_, _ = fmt.Fprintf(&b, "> %s\n\n", ent.Summary)
	}
	_, _ = fmt.Fprintf(&b, "**Source:** `%s`  \n", ent.Path)
	_, _ = fmt.Fprintf(&b, "**Type:** %s  \n", ent.Type)
	_, _ = fmt.Fprintf(&b, "**Confidence:** %.0f%%\n\n", confidence*100)

	if ent.Path != "" {
		_, _ = fmt.Fprintf(&b, "*Provenance: ^[%s]*\n\n", ent.Path)
	}

	b.WriteString("---\n*Navigate: [[index]] · [[log]]*\n")
	return b.String()
}

func computeConfidence(ent EntitySummary) float64 {
	score := 0.0
	if ent.Name != "" {
		score += 0.25
	}
	if ent.Path != "" {
		score += 0.25
	}
	if ent.Summary != "" {
		score += 0.30

		if len(ent.Summary) > 100 {
			score += 0.10
		}
	}
	if ent.Type != "" && ent.Type != "document" {
		score += 0.10
	}
	if score > 1.0 {
		score = 1.0
	}
	return score
}

func communityPage(c Community, labels map[int]string, tag string) string {
	var b strings.Builder
	now := time.Now().UTC().Format("2006-01-02")
	b.WriteString("---\n")
	_, _ = fmt.Fprintf(&b, "title: %s\n", c.Label)
	b.WriteString("type: community\n")
	_, _ = fmt.Fprintf(&b, "members: %d\n", len(c.Members))
	_, _ = fmt.Fprintf(&b, "cohesion: %.2f\n", c.Cohesion)
	_, _ = fmt.Fprintf(&b, "updated: %s\n", now)
	_, _ = fmt.Fprintf(&b, "tags: [%s, community]\n", tag)
	b.WriteString("---\n\n")
	_, _ = fmt.Fprintf(&b, "# %s\n\n", c.Label)
	_, _ = fmt.Fprintf(&b, "> **%d members** · cohesion %.2f\n\n", len(c.Members), c.Cohesion)
	b.WriteString("## Key Concepts\n\n")
	for i, uid := range c.Members {
		if i >= 30 {
			_, _ = fmt.Fprintf(&b, "\n*... and %d more members. See [[index]].*\n", len(c.Members)-30)
			break
		}
		name := extractNameFromUID(uid)
		_, _ = fmt.Fprintf(&b, "- [[%s]]\n", SafeFilename(name))
	}
	b.WriteString("\n## Related Communities\n\n")
	found := false
	for _, otherLabel := range labels {
		if otherLabel != c.Label {
			_, _ = fmt.Fprintf(&b, "- [[%s]]\n", SafeFilename(otherLabel))
			found = true
		}
	}
	if !found {
		b.WriteString("*No cross-community connections detected.*\n")
	}
	b.WriteString("\n---\n*Navigate: [[index]]*\n")
	return b.String()
}

func godNodePage(gn map[string]any, labels map[int]string, tag string) string {
	var b strings.Builder
	name, _ := gn["name"].(string)
	degree, _ := gn["degree"].(int)
	label, _ := gn["label"].(string)
	now := time.Now().UTC().Format("2006-01-02")
	b.WriteString("---\n")
	_, _ = fmt.Fprintf(&b, "title: %s\n", name)
	_, _ = fmt.Fprintf(&b, "type: %s\n", strings.ToLower(label))
	_, _ = fmt.Fprintf(&b, "connections: %d\n", degree)
	_, _ = fmt.Fprintf(&b, "updated: %s\n", now)
	_, _ = fmt.Fprintf(&b, "tags: [%s, god-node]\n", tag)
	b.WriteString("---\n\n")
	_, _ = fmt.Fprintf(&b, "# %s\n\n", name)
	_, _ = fmt.Fprintf(&b, "> **God node** — %d connections · type: %s\n\n", degree, label)
	b.WriteString("This is one of the most connected concepts in the graph.\n\n")
	b.WriteString("---\n*Navigate: [[index]]*\n")
	_ = labels
	return b.String()
}

func genericIndexPage(entities []EntitySummary, communities []Community, godNodes []map[string]any, totalNodes, totalEdges int, tag string) string {
	var b strings.Builder
	now := time.Now().UTC().Format("2006-01-02")
	title := cases.Title(language.English).String(tag) + " Wiki"

	b.WriteString("---\n")
	_, _ = fmt.Fprintf(&b, "title: %s\n", title)
	_, _ = fmt.Fprintf(&b, "updated: %s\n", now)
	_, _ = fmt.Fprintf(&b, "tags: [%s, index]\n", tag)
	b.WriteString("---\n\n")
	_, _ = fmt.Fprintf(&b, "# %s\n\n", title)
	_, _ = fmt.Fprintf(&b, "> %s knowledge wiki. **Start here.** Scan the catalog below, then follow [[wikilinks]] to drill into specific pages.\n", cases.Title(language.English).String(tag))
	_, _ = fmt.Fprintf(&b, "> Check [[log]] for the timeline of updates. Last updated: %s\n\n", now)
	_, _ = fmt.Fprintf(&b, "**%d nodes · %d edges · %d communities · %d articles**\n\n", totalNodes, totalEdges, len(communities), len(entities))

	byType := make(map[string][]EntitySummary)
	for _, ent := range entities {
		byType[ent.Type] = append(byType[ent.Type], ent)
	}
	var types []string
	for t := range byType {
		types = append(types, t)
	}
	sort.Strings(types)

	b.WriteString("---\n\n")
	b.WriteString("## Catalog\n\n")
	for _, t := range types {
		_, _ = fmt.Fprintf(&b, "### %s\n\n", cases.Title(language.English).String(t))
		for _, ent := range byType[t] {
			link := fmt.Sprintf("[[%s]]", SafeFilename(ent.Name))
			if ent.Summary != "" {

				summary := ent.Summary
				if len(summary) > 80 {
					summary = summary[:80] + "…"
				}
				_, _ = fmt.Fprintf(&b, "- %s — %s\n", link, summary)
			} else {
				_, _ = fmt.Fprintf(&b, "- %s (`%s`)\n", link, ent.Path)
			}
		}
		b.WriteString("\n")
	}

	if len(communities) > 0 {
		b.WriteString("## Communities\n\n")
		b.WriteString("> Thematic clusters detected by graph analysis. Each community page lists its members and cross-links.\n\n")
		for _, c := range communities {
			_, _ = fmt.Fprintf(&b, "- [[%s]] — %d members, cohesion %.2f\n", SafeFilename(c.Label), len(c.Members), c.Cohesion)
		}
		b.WriteString("\n")
	}

	if len(godNodes) > 0 {
		b.WriteString("## Hubs (God Nodes)\n\n")
		b.WriteString("> Most-connected concepts — load-bearing abstractions that link many topics. High-impact change targets.\n\n")
		for _, gn := range godNodes {
			name, _ := gn["name"].(string)
			degree, _ := gn["degree"].(int)
			_, _ = fmt.Fprintf(&b, "- [[%s]] — %d connections\n", SafeFilename(name), degree)
		}
		b.WriteString("\n")
	}

	b.WriteString("## How to Use This Wiki\n\n")
	b.WriteString("1. **Scan the Catalog** — find the topic you need, then click its [[wikilink]].\n")
	b.WriteString("2. **Follow cross-references** — each page links to related pages. Follow them to gain context.\n")
	b.WriteString("3. **Check backlinks** — each page lists what links *to* it (inbound references).\n")
	b.WriteString("4. **Explore Communities** — thematic clusters surface related concepts you might miss.\n")
	b.WriteString("5. **Find Hubs** — god nodes are the most connected concepts; changes near them carry high impact.\n")
	b.WriteString("6. **Read the Log** — [[log]] shows the timeline of wiki updates: what was indexed, when.\n\n")
	b.WriteString("> **Tip:** In Obsidian, open Graph View to see the full link structure visually.\n> Use Dataview to query `source_count`, `type`, `confidence`, and `updated` frontmatter fields.\n\n")
	b.WriteString("---\n")
	_, _ = fmt.Fprintf(&b, "*Generated by %s · %s · [[log]]*\n", brand.DisplayName, now)
	return b.String()
}

func appendLog(logPath, logTitle string, totalNodes, totalEdges, communities, articles int, tag string) {

	now := time.Now().UTC().Format("2006-01-02")
	entry := fmt.Sprintf("## [%s] index | Wiki regenerated\n\n"+
		"- Nodes: %d\n- Edges: %d\n- Communities: %d\n- Articles written: %d\n\n",
		now, totalNodes, totalEdges, communities, articles)

	existing, _ := os.ReadFile(logPath)
	var content string
	if len(existing) == 0 {
		content = fmt.Sprintf("---\ntitle: %s\ntags: [%s, log]\n---\n\n# %s\n\n"+
			"> Append-only chronological record. Parse with: `grep '^## \\[' log.md | tail -5`\n\n",
			logTitle, tag, logTitle)
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

func SafeFilename(name string) string {
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

func uniqueSlug(base string, used map[string]bool) string {
	slug := base
	n := 2
	for used[slug] {
		slug = fmt.Sprintf("%s_%d", base, n)
		n++
	}
	used[slug] = true
	return slug
}

func extractNameFromUID(uid string) string {
	parts := strings.SplitN(uid, "::", 2)
	if len(parts) == 2 {
		return strings.ReplaceAll(parts[1], "_", " ")
	}
	return strings.TrimSuffix(filepath.Base(uid), filepath.Ext(uid))
}

func strVal(v any) string {
	if v == nil {
		return ""
	}
	switch s := v.(type) {
	case string:
		return s
	case []byte:
		return string(s)
	default:
		return fmt.Sprintf("%v", s)
	}
}
