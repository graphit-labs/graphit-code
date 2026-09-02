package wiki

import (
	"context"
	"fmt"
	"github.com/graphit-labs/graphit-code/internal/textslice"
	"strings"
)

type WikiSource struct {
	ID    string `json:"id"`
	Label string `json:"label"`
	Dir   string `json:"dir"`
}

type MultiWikiSearchConfig struct {
	Sources []WikiSource

	MaxTurns int

	BM25TopNPerSource int

	UseBM25 bool
}

type BM25ResultWithSource struct {
	BM25Result
	SourceID    string `json:"source_id"`
	SourceLabel string `json:"source_label"`
}

func SearchMultiWiki(ctx context.Context, client AIClient, query string, cfg MultiWikiSearchConfig) (*SearchResult, error) {
	if len(cfg.Sources) == 0 {
		return nil, fmt.Errorf("no wiki sources provided")
	}

	if len(cfg.Sources) == 1 {
		return SearchWiki(ctx, client, query, SearchConfig{
			WikiDir:   cfg.Sources[0].Dir,
			ModuleTag: cfg.Sources[0].Label,
			MaxTurns:  cfg.MaxTurns,
			BM25TopN:  cfg.BM25TopNPerSource,
			UseBM25:   cfg.UseBM25,
		})
	}

	if cfg.MaxTurns <= 0 {
		cfg.MaxTurns = 6
	}

	result := &SearchResult{}

	var contextBuilder strings.Builder
	for _, src := range cfg.Sources {
		// The catalogue comes from the index. It used to be `index.md` read off disk, a page
		// rewritten on every build; the table already knows every slug, so the overview is a
		// query and cannot disagree with what is searchable.
		overview := wikiOverview(ctx, src.Dir)
		if overview == "" {
			continue
		}
		_, _ = fmt.Fprintf(&contextBuilder, "=== [%s] catalogue (%s) ===\n%s\n\n",
			src.ID, src.Label, overview)
		result.TokensSent += len(overview) / 4
	}

	context_ := contextBuilder.String()

	if cfg.UseBM25 {
		bm25Ctx := bm25PreFilterMulti(ctx, cfg.Sources, query, cfg.BM25TopNPerSource)
		if bm25Ctx != "" {
			context_ += "\n" + bm25Ctx
			result.TokensSent += len(bm25Ctx) / 4
		}
	}

	systemPrompt := buildMultiSearchSystemPrompt(cfg.Sources)

	loadedPages := make(map[string]bool)

	for turn := 0; turn < cfg.MaxTurns; turn++ {
		result.Turns = turn + 1

		userMsg := fmt.Sprintf(
			"Query: %s\n\nAvailable context:\n%s\n\nReply with EITHER:\n"+
				"- A list of pages to read (format: [source-id]/page-name, one per line), OR\n"+
				"- DONE: <your comprehensive Markdown answer synthesizing all context>",
			query, context_,
		)
		result.TokensSent += len(userMsg) / 4

		reply, err := client.Complete(ctx, systemPrompt, userMsg)
		if err != nil {
			return nil, fmt.Errorf("AI error on turn %d: %w", turn+1, err)
		}

		reply = strings.TrimSpace(reply)

		if after, ok := strings.CutPrefix(reply, "DONE:"); ok {
			result.Answer = strings.TrimSpace(after)
			if isPageRefOnlyAnswer(result.Answer) {
				retryMsg := buildSynthesisRetryPrompt(query, context_)
				result.TokensSent += len(retryMsg) / 4
				if retryAnswer, err := client.Complete(ctx, synthesisSystemPrompt, retryMsg); err == nil {
					if !isPageRefOnlyAnswer(strings.TrimSpace(retryAnswer)) {
						result.Answer = strings.TrimSpace(retryAnswer)
					}
				}
			}
			return result, nil
		}
		if after, ok := strings.CutPrefix(reply, "DONE "); ok {
			result.Answer = strings.TrimSpace(after)
			if isPageRefOnlyAnswer(result.Answer) {
				retryMsg := buildSynthesisRetryPrompt(query, context_)
				result.TokensSent += len(retryMsg) / 4
				if retryAnswer, err := client.Complete(ctx, synthesisSystemPrompt, retryMsg); err == nil {
					if !isPageRefOnlyAnswer(strings.TrimSpace(retryAnswer)) {
						result.Answer = strings.TrimSpace(retryAnswer)
					}
				}
			}
			return result, nil
		}

		pages := parseMultiPageList(reply, cfg.Sources)
		if len(pages) == 0 {

			result.Answer = reply
			if isPageRefOnlyAnswer(result.Answer) {
				retryMsg := buildSynthesisRetryPrompt(query, context_)
				result.TokensSent += len(retryMsg) / 4
				if retryAnswer, err := client.Complete(ctx, synthesisSystemPrompt, retryMsg); err == nil {
					if !isPageRefOnlyAnswer(strings.TrimSpace(retryAnswer)) {
						result.Answer = strings.TrimSpace(retryAnswer)
					}
				}
			}
			return result, nil
		}

		var loaded []string
		foundAny := false
		for _, pg := range pages {
			content, resolvedSlug := loadWikiPageFromIndex(ctx, pg.dir, pg.page)
			if content != "" {
				foundAny = true
				if !loadedPages[resolvedSlug] {
					loadedPages[resolvedSlug] = true
					header := fmt.Sprintf("=== [%s] %s.md ===\n%s", pg.sourceID, resolvedSlug, content)
					loaded = append(loaded, header)
					result.TokensSent += len(content) / 4
				}
			}
		}

		if !foundAny {
			result.Answer = "(no matching pages found for requested pages)"
			return result, nil
		}

		if len(loaded) > 0 {
			context_ = fmt.Sprintf("%s\n\n%s", context_, strings.Join(loaded, "\n\n"))
		} else {
			var requestedNames []string
			for _, pg := range pages {
				requestedNames = append(requestedNames, fmt.Sprintf("[%s]/%s", pg.sourceID, pg.page))
			}
			context_ = fmt.Sprintf("%s\n\nSystem: All requested pages (%s) are already loaded in the context above.", context_, strings.Join(requestedNames, ", "))
		}
	}

	finalMsg := fmt.Sprintf(
		"Query: %s\n\nFull context accumulated:\n%s\n\n"+
			"You have now read all the relevant wiki pages. Synthesize a COMPREHENSIVE answer.\n"+
			"Your response MUST be a detailed Markdown document with:\n"+
			"- ## Headings to organize the answer\n"+
			"- Bullet lists, **bold**, `code blocks` for clarity\n"+
			"- Thorough explanations (several paragraphs minimum)\n"+
			"- Inline [source-id]/[[Page_Name]] references as citations\n"+
			"- DO NOT return a list of page names — write a proper synthesis.\n"+
			"Write your complete answer now:",
		query, context_,
	)
	result.TokensSent += len(finalMsg) / 4

	answer, err := client.Complete(ctx, synthesisSystemPrompt, finalMsg)
	if err != nil {
		return result, fmt.Errorf("AI final answer: %w", err)
	}
	result.Answer = strings.TrimSpace(answer)

	if isPageRefOnlyAnswer(result.Answer) {
		retryMsg := buildSynthesisRetryPrompt(query, context_)
		result.TokensSent += len(retryMsg) / 4
		retryAnswer, retryErr := client.Complete(ctx, synthesisSystemPrompt, retryMsg)
		if retryErr == nil && !isPageRefOnlyAnswer(strings.TrimSpace(retryAnswer)) {
			result.Answer = strings.TrimSpace(retryAnswer)
		}
	}

	return result, nil
}

func BM25SearchMulti(ctx context.Context, sources []WikiSource, query string, topNPerSource int) []BM25ResultWithSource {

	var allResults []BM25ResultWithSource
	for _, src := range sources {
		results := BM25Search(ctx, src.Dir, query, topNPerSource)
		for _, r := range results {
			allResults = append(allResults, BM25ResultWithSource{
				BM25Result:  r,
				SourceID:    src.ID,
				SourceLabel: src.Label,
			})
		}
	}

	return allResults
}

func bm25PreFilterMulti(ctx context.Context, sources []WikiSource, query string, topNPerSource int) string {
	allResults := BM25SearchMulti(ctx, sources, query, topNPerSource)
	if len(allResults) == 0 {
		return ""
	}

	var b strings.Builder
	b.WriteString("=== BM25 Relevant Pages (pre-filtered, multi-wiki) ===\n")
	_, _ = fmt.Fprintf(&b, "Query: %q — top results per wiki source:\n\n", query)

	for i, r := range allResults {
		_, _ = fmt.Fprintf(&b, "%d. [%s]/[[%s]]",
			i+1, r.SourceID, strings.TrimSuffix(r.Path, ".md"))
		if r.Title != "" {
			_, _ = fmt.Fprintf(&b, " — %s", r.Title)
		}
		_, _ = fmt.Fprintf(&b, " (score: %.3f, source: %s)\n", r.Score, r.SourceLabel)
	}

	return b.String()
}

func buildMultiSearchSystemPrompt(sources []WikiSource) string {
	var sourceList strings.Builder
	for _, s := range sources {
		_, _ = fmt.Fprintf(&sourceList, "- [%s]: %s\n", s.ID, s.Label)
	}

	return fmt.Sprintf(`You are a knowledge search agent operating across multiple Obsidian-compatible wikis.

AVAILABLE WIKI SOURCES:
%s
PROTOCOL:
- You will receive: a query and context from multiple wiki sources.
- Context pages are prefixed with [source-id] to indicate which wiki they belong to.
- If BM25 pre-filtered results are included, prioritize reading those pages first.
- To request more pages, reply with page names in the format: [source-id]/page-name (one per line).
  Example: [knowledge]/Architecture_Overview
- When you have enough context, reply with: DONE: <your complete answer>
- Request up to 5 pages per turn. Minimize token usage by being selective.

ANSWER REQUIREMENTS — CRITICAL:
- Your DONE answer MUST be a COMPREHENSIVE, DETAILED Markdown synthesis.
- Use ## headings, bullet lists, **bold**, code blocks, and tables as appropriate.
- Write at least several paragraphs explaining the topic thoroughly.
- Reference pages with [source-id]/[[page]] syntax as citations, NOT as the entire answer.
- NEVER reply with ONLY a list of page names or references — that is NOT an answer.
- Explain concepts, describe relationships, provide context and architectural insights.
- Do NOT hallucinate content — only reference what you have read.`, sourceList.String())
}

type multiPageRequest struct {
	sourceID string
	page     string
	dir      string
}

func parseMultiPageList(reply string, sources []WikiSource) []multiPageRequest {

	sourceMap := make(map[string]WikiSource, len(sources))
	for _, s := range sources {
		sourceMap[s.ID] = s
	}

	var requests []multiPageRequest
	for _, line := range strings.Split(reply, "\n") {
		line = strings.TrimSpace(line)
		line = strings.TrimPrefix(line, "- ")
		line = strings.TrimPrefix(line, "* ")

		for i := 1; i <= 9; i++ {
			line = strings.TrimPrefix(line, fmt.Sprintf("%d. ", i))
		}
		line = strings.TrimSuffix(line, ".md")

		if line == "" || strings.HasPrefix(line, "DONE") {
			continue
		}

		line = strings.TrimPrefix(line, "[")

		var sourceID, pageName string

		if idx := strings.Index(line, "]/"); idx > 0 {

			sourceID = line[:idx]
			pageName = line[idx+2:]
		} else if idx := strings.Index(line, "/"); idx > 0 {

			candidate := line[:idx]
			if _, ok := sourceMap[candidate]; ok {
				sourceID = candidate
				pageName = line[idx+1:]
			}
		}

		pageName = strings.TrimPrefix(pageName, "[[")
		pageName = strings.TrimSuffix(pageName, "]]")

		if sourceID != "" && pageName != "" {
			if src, ok := sourceMap[sourceID]; ok {
				requests = append(requests, multiPageRequest{
					sourceID: sourceID,
					page:     pageName,
					dir:      src.Dir,
				})
			}
		} else if pageName == "" {

			page := strings.TrimPrefix(line, "[[")
			page = strings.TrimSuffix(page, "]]")
			page = strings.TrimSuffix(page, ".md")
			if page == "" || strings.ContainsAny(page, ":/") {
				continue
			}
			for _, src := range sources {
				if content, _ := loadWikiPage(src.Dir, page); content != "" {
					requests = append(requests, multiPageRequest{
						sourceID: src.ID,
						page:     page,
						dir:      src.Dir,
					})
					break
				}
			}
		}
	}

	return requests
}

// wikiOverview renders a wiki's catalogue from its index, replacing the generated `index.md`.
//
// Titles and types, not bodies: it exists so the model can choose which pages to ask for, which
// is what the index page was for, and a body per page would spend the budget the choice is
// supposed to save.
func wikiOverview(ctx context.Context, wikiDir string) string {
	db, err := OpenWikiDB(ctx, wikiDir)
	if err != nil {
		return ""
	}
	defer func() { _ = db.Close() }()

	entries, err := db.Browse(ctx, BrowseFilter{ClusterID: -1, Limit: 500})
	if err != nil || len(entries) == 0 {
		return ""
	}

	var b strings.Builder
	for _, e := range entries {
		_, _ = fmt.Fprintf(&b, "- [[%s]]", e.Slug)
		if e.Title != "" && e.Title != e.Slug {
			_, _ = fmt.Fprintf(&b, " — %s", e.Title)
		}
		if e.DocType != "" {
			_, _ = fmt.Fprintf(&b, " (%s)", e.DocType)
		}
		b.WriteString("\n")
	}
	return b.String()
}

// loadWikiPageFromIndex fetches one requested page out of a wiki's index.
//
// It replaced loadWikiPage, which did os.ReadFile on `<dir>/<slug>.md`. The pages are not written
// any more, so a multi-wiki loop that read them found nothing and answered in one turn.
func loadWikiPageFromIndex(ctx context.Context, wikiDir, page string) (content, slug string) {
	res, err := ReadPageAt(ctx, wikiDir, page, textslice.Request{})
	if err != nil {
		return "", ""
	}
	return res.Source, res.Page
}
