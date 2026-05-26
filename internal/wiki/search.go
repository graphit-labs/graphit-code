package wiki

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

type AIClient interface {
	Complete(ctx context.Context, systemPrompt, userPrompt string) (string, error)
}

type SearchConfig struct {
	WikiDir string

	ModuleTag string

	MaxTurns int

	BM25TopN int

	UseBM25 bool
}

type SearchResult struct {
	Answer     string
	Turns      int
	TokensSent int
}

func SearchWiki(ctx context.Context, client AIClient, query string, cfg SearchConfig) (*SearchResult, error) {
	if cfg.MaxTurns <= 0 {
		cfg.MaxTurns = 6
	}

	indexPath := filepath.Join(cfg.WikiDir, "index.md")
	indexContent, err := os.ReadFile(indexPath)
	if err != nil {
		return nil, fmt.Errorf("wiki not found at %s — run '%s index' first", cfg.WikiDir, cfg.ModuleTag)
	}

	systemPrompt := buildSearchSystemPrompt(cfg.ModuleTag)
	result := &SearchResult{}

	context_ := fmt.Sprintf("=== index.md ===\n%s", string(indexContent))
	result.TokensSent += len(indexContent) / 4

	if cfg.UseBM25 {
		bm25Ctx := bm25PreFilter(cfg.WikiDir, query, cfg.BM25TopN)
		if bm25Ctx != "" {
			context_ += "\n\n" + bm25Ctx
			result.TokensSent += len(bm25Ctx) / 4
		}
	}

	for turn := 0; turn < cfg.MaxTurns; turn++ {
		result.Turns = turn + 1

		userMsg := fmt.Sprintf(
			"Query: %s\n\nAvailable context:\n%s\n\nReply with EITHER:\n"+
				"- A list of page names to read (one per line, no .md extension, no path prefix), OR\n"+
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

		pages := parsePageList(reply)
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
		for _, page := range pages {
			content := loadWikiPage(cfg.WikiDir, page)
			if content != "" {
				loaded = append(loaded, fmt.Sprintf("=== %s.md ===\n%s", page, content))
				result.TokensSent += len(content) / 4
			}
		}

		if len(loaded) == 0 {

			result.Answer = fmt.Sprintf("(no matching pages found for: %s)", strings.Join(pages, ", "))
			return result, nil
		}

		context_ = fmt.Sprintf("%s\n\n%s", context_, strings.Join(loaded, "\n\n"))
	}

	finalMsg := fmt.Sprintf(
		"Query: %s\n\nFull context accumulated:\n%s\n\n"+
			"You have now read all the relevant wiki pages. Synthesize a COMPREHENSIVE answer.\n"+
			"Your response MUST be a detailed Markdown document with:\n"+
			"- ## Headings to organize the answer\n"+
			"- Bullet lists, **bold**, `code blocks` for clarity\n"+
			"- Thorough explanations (several paragraphs minimum)\n"+
			"- Inline [[Page_Name]] references as citations\n"+
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

func bm25PreFilter(wikiDir, query string, topN int) string {
	idx, err := NewBM25Index(wikiDir, DefaultBM25Config())
	if err != nil || idx.totalDocs == 0 {
		return ""
	}

	results := idx.Search(query, topN)
	if len(results) == 0 {
		return ""
	}

	var b strings.Builder
	b.WriteString("=== BM25 Relevant Pages (pre-filtered) ===\n")
	b.WriteString(fmt.Sprintf("Query: %q — top %d by BM25 relevance:\n\n", query, len(results)))

	for i, r := range results {
		b.WriteString(fmt.Sprintf("%d. [[%s]]", i+1, strings.TrimSuffix(r.Path, ".md")))
		if r.Title != "" {
			b.WriteString(fmt.Sprintf(" — %s", r.Title))
		}
		b.WriteString(fmt.Sprintf(" (score: %.3f)\n", r.Score))
	}

	return b.String()
}

func BM25Search(wikiDir, query string, topN int) []BM25Result {
	idx, err := NewBM25Index(wikiDir, DefaultBM25Config())
	if err != nil {
		return nil
	}
	results := idx.Search(query, topN)

	for i := range results {
		content := loadWikiPage(wikiDir, strings.TrimSuffix(results[i].Path, ".md"))
		if content != "" {
			results[i].Snippet = extractSnippet(content, query)
		}
	}

	return results
}

func extractSnippet(content, query string) string {
	lower := strings.ToLower(content)
	queryLower := strings.ToLower(query)

	terms := strings.Fields(queryLower)
	bestIdx := -1
	for _, term := range terms {
		idx := strings.Index(lower, term)
		if idx >= 0 {
			bestIdx = idx
			break
		}
	}

	if bestIdx < 0 {

		body := stripYAMLFrontmatter(content)
		if len(body) > 150 {
			return body[:150] + "…"
		}
		return body
	}

	start := bestIdx - 60
	if start < 0 {
		start = 0
	}
	end := bestIdx + 120
	if end > len(content) {
		end = len(content)
	}

	snippet := strings.TrimSpace(content[start:end])
	if start > 0 {
		snippet = "…" + snippet
	}
	if end < len(content) {
		snippet += "…"
	}
	return snippet
}

func buildSearchSystemPrompt(moduleTag string) string {
	return fmt.Sprintf(`You are a %s knowledge search agent operating on an Obsidian-compatible wiki.

PROTOCOL:
- You will receive: a query and the current context (wiki pages you have read so far).
- If BM25 pre-filtered results are included, prioritize reading those pages first.
- If you need more pages to answer the query, reply ONLY with the page names (one per line, no .md extension).
- When you have enough context, reply with: DONE: <your complete answer>
- Page names are listed in index.md. Request up to 5 pages per turn.
- Minimize token usage by being selective about which pages you request.

ANSWER REQUIREMENTS — CRITICAL:
- Your DONE answer MUST be a COMPREHENSIVE, DETAILED Markdown synthesis.
- Use ## headings, bullet lists, **bold**, code blocks, and tables as appropriate.
- Write at least several paragraphs explaining the topic thoroughly.
- Reference wiki pages inline using [[Page_Name]] syntax as citations.
- NEVER reply with ONLY a list of page names or references — that is NOT an answer.
- Explain concepts, describe relationships, provide context and architectural insights.
- Do NOT hallucinate content — only reference what you have read.`, moduleTag)
}

func parsePageList(reply string) []string {
	var pages []string
	for _, line := range strings.Split(reply, "\n") {
		line = strings.TrimSpace(line)
		line = strings.TrimPrefix(line, "- ")
		line = strings.TrimPrefix(line, "* ")
		line = strings.TrimPrefix(line, "1. ")
		line = strings.TrimPrefix(line, "2. ")
		line = strings.TrimPrefix(line, "3. ")
		line = strings.TrimPrefix(line, "4. ")
		line = strings.TrimPrefix(line, "5. ")
		line = strings.TrimSuffix(line, ".md")
		if line == "" || strings.HasPrefix(line, "DONE") || strings.ContainsAny(line, ":/") {
			continue
		}

		line = strings.TrimPrefix(line, "[[")
		line = strings.TrimSuffix(line, "]]")
		if line != "" {
			pages = append(pages, line)
		}
	}
	return pages
}

func loadWikiPage(wikiDir, page string) string {
	candidates := []string{
		filepath.Join(wikiDir, page+".md"),
		filepath.Join(wikiDir, SafeFilename(page)+".md"),
	}
	for _, p := range candidates {
		data, err := os.ReadFile(p)
		if err == nil {
			return string(data)
		}
	}
	return ""
}
