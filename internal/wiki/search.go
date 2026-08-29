package wiki

import (
	"context"
	"fmt"
	"log/slog"
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

	// Try WikiDB catalog first, fall back to index.md.
	var indexContent []byte
	if db, dbErr := OpenWikiDB(ctx, cfg.WikiDir); dbErr == nil {
		entries, browseErr := db.Browse(ctx, BrowseFilter{Limit: 100})
		db.Close()
		if browseErr == nil && len(entries) > 0 {
			var b strings.Builder
			b.WriteString("# Wiki Catalog\n\n")
			for _, e := range entries {
				fmt.Fprintf(&b, "- [[%s]] — %s\n", e.Slug, e.Summary)
			}
			indexContent = []byte(b.String())
		}
	}
	if len(indexContent) == 0 {
		indexPath := filepath.Join(cfg.WikiDir, "index.md")
		var err error
		indexContent, err = os.ReadFile(indexPath)
		if err != nil {
			return nil, fmt.Errorf("wiki not found at %s — run '%s index' first", cfg.WikiDir, cfg.ModuleTag)
		}
	}

	systemPrompt := buildSearchSystemPrompt(cfg.ModuleTag)
	result := &SearchResult{}

	context_ := fmt.Sprintf("=== index.md ===\n%s", string(indexContent))
	result.TokensSent += len(indexContent) / 4

	if cfg.UseBM25 {
		bm25Ctx := bm25PreFilter(ctx, cfg.WikiDir, query, cfg.BM25TopN)
		if bm25Ctx != "" {
			context_ += "\n\n" + bm25Ctx
			result.TokensSent += len(bm25Ctx) / 4
		}
	}

	loadedPages := make(map[string]bool)

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
		foundAny := false
		for _, page := range pages {
			content, resolvedSlug := loadWikiPage(cfg.WikiDir, page)
			if content != "" {
				foundAny = true
				if !loadedPages[resolvedSlug] {
					loadedPages[resolvedSlug] = true
					loaded = append(loaded, fmt.Sprintf("=== %s.md ===\n%s", resolvedSlug, content))
					result.TokensSent += len(content) / 4
				}
			}
		}

		if !foundAny {
			result.Answer = fmt.Sprintf("(no matching pages found for: %s)", strings.Join(pages, ", "))
			return result, nil
		}

		if len(loaded) > 0 {
			context_ = fmt.Sprintf("%s\n\n%s", context_, strings.Join(loaded, "\n\n"))
		} else {
			context_ = fmt.Sprintf("%s\n\nSystem: All requested pages (%s) are already loaded in the context above.", context_, strings.Join(pages, ", "))
		}
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

// searchCompiledWiki asks the compiled index, and says whether that index was in a
// position to answer at all.
//
// The second return value is the whole point, and it used to be missing. The markdown
// files are the source of truth and the SQLite index is a compiled cache of them — see
// OpenWikiDB, which gitignores the database as a derived artifact — so there are two
// genuinely different situations behind an empty result:
//
//   - the index holds chunks, so it is AUTHORITATIVE. Its empty answer is a real "no
//     matches", and re-asking the markdown would only manufacture hits that the index
//     deliberately did not rank.
//   - the index holds nothing, so it has not been compiled yet: a fresh project, or a
//     page written in the seconds before the daemon rebuilds. Only the markdown can
//     answer, and scanning it is why a memory is findable the moment it is written.
//
// Deciding this on `len(results) > 0` conflated the two, which is what made an empty or
// stale index invisible: every miss quietly came back from the other engine, so nothing
// ever surfaced that the index had no content.
func searchCompiledWiki(ctx context.Context, wikiDir, query string, topN int) (results []WikiSearchResult, authoritative bool) {
	db, err := OpenWikiDB(ctx, wikiDir)
	if err != nil {
		return nil, false
	}
	defer db.Close()

	if !db.HasContent(ctx) {
		return nil, false
	}
	results, err = db.Search(ctx, query, topN)
	if err != nil {
		return nil, false
	}
	return results, true
}

func bm25PreFilter(ctx context.Context, wikiDir, query string, topN int) string {
	// The compiled index answers when it has content and ranked something. It falling
	// through is not a clean miss — see BM25Search for why the scan stays underneath.
	if results, authoritative := searchCompiledWiki(ctx, wikiDir, query, topN); authoritative && len(results) > 0 {
		var b strings.Builder
		b.WriteString("=== FTS5 Relevant Pages (pre-filtered) ===\n")
		fmt.Fprintf(&b, "Query: %q — top %d by BM25+FTS5 relevance:\n\n", query, len(results))
		for i, r := range results {
			fmt.Fprintf(&b, "%d. [[%s]]", i+1, r.Slug)
			if r.Title != "" {
				fmt.Fprintf(&b, " — %s", r.Title)
			}
			fmt.Fprintf(&b, " (score: %.3f)\n", r.Score)
		}
		return b.String()
	}

	// Not compiled yet: rank the markdown, which is the source of truth. The heading
	// names the engine, so a reader can tell which one answered.
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
	_, _ = fmt.Fprintf(&b, "Query: %q — top %d by BM25 relevance:\n\n", query, len(results))

	for i, r := range results {
		_, _ = fmt.Fprintf(&b, "%d. [[%s]]", i+1, strings.TrimSuffix(r.Path, ".md"))
		if r.Title != "" {
			_, _ = fmt.Fprintf(&b, " — %s", r.Title)
		}
		_, _ = fmt.Fprintf(&b, " (score: %.3f)\n", r.Score)
	}

	return b.String()
}

// BM25Search searches a wiki, from the compiled index when there is one and from the
// markdown itself when there is not.
//
// Both paths rank by BM25 — the index through FTS5's bm25(), the scan through the Go
// implementation in bm25.go — so the name describes the ranking, not one engine. Which
// path answers is decided by searchCompiledWiki: see it for why an empty index and an
// empty ANSWER are not the same thing.
//
// An authoritative miss still falls through to the scan, and that is deliberate. The
// tempting change is to report it as a real miss, which reads cleaner and is wrong here:
// an index can hold content and still be BEHIND the files it indexes, and this project
// has already shipped that exact state — a stale pre-check left new pages unindexed while
// old ones were present, so every session ran without recall while believing the project
// simply had no memories. The scan is the net under that.
//
// What was missing was not the fallback but the signal, so the disagreement is now said
// out loud: an index that ranked nothing for a query the markdown answers is stale, and
// that is worth a warning naming the diagnostic instead of a silent second opinion.
func BM25Search(ctx context.Context, wikiDir, query string, topN int) []BM25Result {
	compiled, authoritative := searchCompiledWiki(ctx, wikiDir, query, topN)
	if authoritative && len(compiled) > 0 {
		return wikiFTSToB25Results(compiled)
	}

	scanned := scanMarkdownBM25(wikiDir, query, topN)
	if authoritative && len(scanned) > 0 {
		slog.Warn("wiki index is behind its markdown: it ranked nothing for a query the files answer",
			"wiki_dir", wikiDir, "hits_from_markdown", len(scanned),
			"diagnose", "SELECT count(*) FROM chunks in wiki.db, then reindex")
	}
	return scanned
}

// scanMarkdownBM25 ranks the markdown files directly, which is what answers before the
// wiki has ever been compiled — and, for a memory, within the seconds between writing it
// and the daemon rebuilding.
func scanMarkdownBM25(wikiDir, query string, topN int) []BM25Result {
	idx, err := NewBM25Index(wikiDir, DefaultBM25Config())
	if err != nil {
		return nil
	}
	results := idx.Search(query, topN)

	for i := range results {
		content, _ := loadWikiPage(wikiDir, strings.TrimSuffix(results[i].Path, ".md"))
		if content != "" {
			results[i].Snippet = extractSnippet(content, query)
		}
	}

	return results
}

func wikiFTSToB25Results(ftsResults []WikiSearchResult) []BM25Result {
	results := make([]BM25Result, 0, len(ftsResults))
	for _, r := range ftsResults {
		results = append(results, BM25Result{
			Path:    r.Slug + ".md",
			Title:   r.Title,
			DocType: r.DocType,
			Score:   r.Score,
			Snippet: r.Snippet,
		})
	}
	return results
}

// extractSnippet previews a markdown page for a query. It delegates to the same
// window builder the compiled index uses, so the two engines cannot disagree about
// what a preview is; the frontmatter goes first because it is metadata the reader
// did not ask to see.
func extractSnippet(content, query string) string {
	return snippetAround(StripFrontmatter(content), query, wikiSnippetWidth)
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

func loadWikiPage(wikiDir, page string) (string, string) {
	candidates := []string{
		filepath.Join(wikiDir, page+".md"),
		filepath.Join(wikiDir, SafeSlug(page)+".md"),
	}
	for _, p := range candidates {
		data, err := os.ReadFile(p)
		if err == nil {
			slug := strings.TrimSuffix(filepath.Base(p), ".md")
			return string(data), slug
		}
	}

	// Fallback to fuzzy matching using trigrams
	if bestMatch := findBestFuzzyMatch(wikiDir, page); bestMatch != "" {
		p := filepath.Join(wikiDir, bestMatch+".md")
		data, err := os.ReadFile(p)
		if err == nil {
			return string(data), bestMatch
		}
	}

	return "", ""
}

func findBestFuzzyMatch(wikiDir, targetPage string) string {
	targetClean := CleanForFuzzy(targetPage)
	if targetClean == "" {
		return ""
	}

	entries, err := os.ReadDir(wikiDir)
	if err != nil {
		return ""
	}

	bestMatch := ""
	bestScore := 0.0

	for _, entry := range entries {
		if entry.IsDir() || filepath.Ext(entry.Name()) != ".md" {
			continue
		}
		slug := strings.TrimSuffix(entry.Name(), ".md")
		if slug == "index" || slug == "log" {
			continue
		}

		score := TrigramSimilarity(targetClean, CleanForFuzzy(slug))
		if score > bestScore {
			bestScore = score
			bestMatch = slug
		}
	}

	if bestScore >= 0.65 {
		return bestMatch
	}
	return ""
}
