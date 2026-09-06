package uiserver

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/chat"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/projectlock"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

type WikiModule struct {
	ID      string `json:"id"`
	Label   string `json:"label"`
	Path    string `json:"path"`
	Context string `json:"context"`
	Pages   int    `json:"pages"`
	HasLog  bool   `json:"hasLog"`
}

type WikiPageMeta struct {
	Path       string   `json:"path"`
	Title      string   `json:"title"`
	Type       string   `json:"type"`
	WordCount  int      `json:"wordCount"`
	Links      []string `json:"links"`
	Tags       []string `json:"tags"`
	Confidence float64  `json:"confidence"`
	Source     string   `json:"source"`
}

type WikiPageContent struct {
	WikiPageMeta
	Content string `json:"content"`
}

type SearchResult struct {
	Path    string `json:"path"`
	Title   string `json:"title"`
	Snippet string `json:"snippet"`
	Score   int    `json:"score"`
}

type AISearchResult struct {
	Path      string `json:"path"`
	Title     string `json:"title"`
	Relevance string `json:"relevance"`
	Score     int    `json:"score"`
}

type AISearchResponse struct {
	Answer    string           `json:"answer"`
	Results   []AISearchResult `json:"results"`
	SessionID string           `json:"session_id,omitempty"`
	Error     string           `json:"error,omitempty"`
}

type WikiHandler struct {
	aiClient ai.Client
	hubSvc   *hub.HubService
}

func NewWikiHandler(hubSvc *hub.HubService) *WikiHandler {
	var client ai.Client
	if config.AgentFeaturesEnabled(nil, nil) {
		client, _ = ai.NewClientFromConfig()
	}
	return &WikiHandler{aiClient: client, hubSvc: hubSvc}
}

func (h *WikiHandler) RegisterAPIRoutes(mux *http.ServeMux) {
	mux.HandleFunc("/api/wiki/modules", corsJSON(h.handleModules))
	mux.HandleFunc("/api/wiki/pages", corsJSON(h.handlePages))
	mux.HandleFunc("/api/wiki/page", corsJSON(h.handlePage))
	mux.HandleFunc("/api/wiki/search", corsJSON(h.handleSearch))
	mux.HandleFunc("POST /api/wiki/ai-search", corsJSON(h.handleAISearch))

}

func (h *WikiHandler) handleModules(w http.ResponseWriter, r *http.Request) {
	projectDir := r.URL.Query().Get("project_dir")
	var modules []WikiModule
	modules = append(modules, discoverModules(projectDir)...)
	sort.Slice(modules, func(i, j int) bool { return modules[i].ID < modules[j].ID })
	writeJSON(w, modules)
}

func (h *WikiHandler) handlePages(w http.ResponseWriter, r *http.Request) {
	wikiDir := r.URL.Query().Get("dir")
	if wikiDir == "" {
		http.Error(w, "dir required", http.StatusBadRequest)
		return
	}

	wikiDir = resolveDir(wikiDir)
	absDir, err := filepath.Abs(filepath.Clean(wikiDir))
	if err != nil {
		http.Error(w, "invalid dir", http.StatusBadRequest)
		return
	}
	info, err := os.Stat(absDir)
	if err != nil || !info.IsDir() {
		http.Error(w, "dir not found or not a directory", http.StatusBadRequest)
		return
	}
	pages, err := listWikiPages(r.Context(), absDir)
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
	writeJSON(w, pages)
}

func (h *WikiHandler) handlePage(w http.ResponseWriter, r *http.Request) {
	wikiDir := r.URL.Query().Get("dir")
	pagePath := r.URL.Query().Get("path")
	if wikiDir == "" || pagePath == "" {
		http.Error(w, "dir and path required", http.StatusBadRequest)
		return
	}

	absWiki, err := filepath.Abs(resolveDir(wikiDir))
	if err != nil {
		http.Error(w, "invalid dir", http.StatusBadRequest)
		return
	}

	db, err := wiki.OpenWikiDB(r.Context(), absWiki)
	if err != nil {
		http.Error(w, "wiki index not found", http.StatusNotFound)
		return
	}
	defer db.Close()

	slug := strings.TrimSuffix(strings.TrimPrefix(filepath.ToSlash(pagePath), "/"), ".md")
	chunk, err := db.Chunk(r.Context(), slug)
	if err != nil {
		http.Error(w, "page not found", http.StatusNotFound)
		return
	}
	links, _ := db.AllXRefs(r.Context())

	writeJSON(w, WikiPageContent{
		WikiPageMeta: chunkPageMeta(*chunk, links[chunk.Slug]),
		Content:      chunk.Body,
	})
}

func (h *WikiHandler) handleSearch(w http.ResponseWriter, r *http.Request) {
	wikiDir := r.URL.Query().Get("dir")
	query := r.URL.Query().Get("q")
	if wikiDir == "" || query == "" {
		writeJSON(w, []SearchResult{})
		return
	}

	results := []SearchResult{}
	bm25Results := wiki.BM25Search(r.Context(), wikiDir, query, 30)
	for _, br := range bm25Results {
		results = append(results, SearchResult{
			Path:    br.Path,
			Title:   br.Title,
			Snippet: br.Snippet,
			Score:   int(br.Score * 100),
		})
	}
	writeJSON(w, results)
}

func discoverModules(projectDir string) []WikiModule {
	if projectDir == "" {
		projectDir, _ = os.Getwd()
	}
	idNames := loadProjectIDNames()

	var modules []WikiModule
	add := func(id, label, dir, contextName string, requirePages bool) {
		if dir == "" {
			return
		}
		resolved := resolveDir(dir)
		if info, err := os.Stat(resolved); err != nil || !info.IsDir() {
			return
		}
		pages, hasLog := indexedModuleStats(resolved)
		if requirePages && pages == 0 {
			return
		}
		modules = append(modules, WikiModule{
			ID:      id,
			Label:   label,
			Path:    resolved,
			Context: contextName,
			Pages:   pages,
			HasLog:  hasLog,
		})
	}

	add("knowledge", projectDisplayName(projectDir), knowledge.WikiDirFor(projectDir), "project", false)
	add("memory-project", "Memory (project)",
		memory.WikiDirFor(projectDir, string(memory.MemoryScopeProject)), "project", false)
	add("memory-user", "Memory (user)",
		memory.WikiDirFor(projectDir, string(memory.MemoryScopeUser)), "user", false)

	for _, name := range store.ContextNames(projectDir, store.KindKnowledge) {
		add("knowledge/"+name, contextLabel(name, idNames),
			store.KnowledgeContextDirIn(projectDir, name), name, true)
	}

	for _, name := range memory.AllContextDirs() {
		add("memory/"+name, contextLabel(name, idNames),
			memory.MemoryWikiGlobalDir(name, name), name, true)
	}

	return modules
}

// projectDisplayName is the project's name from its lockfile, falling back to the
// directory name for a project that was never initialised.
func projectDisplayName(projectDir string) string {
	if lf, err := projectlock.Load(filepath.Join(projectDir, brand.LockFileName())); err == nil &&
		lf != nil && lf.Project.Name != "" {
		return lf.Project.Name
	}
	return filepath.Base(projectDir)
}

func contextLabel(name string, idNames map[string]string) string {
	if readable, ok := idNames[name]; ok && readable != "" {
		return readable
	}
	return name
}

func listWikiPages(ctx context.Context, wikiDir string) ([]WikiPageMeta, error) {
	db, err := wiki.OpenWikiDB(ctx, wikiDir)
	if err != nil {
		return nil, err
	}
	defer db.Close()

	chunks, err := db.Chunks(ctx)
	if err != nil {
		return nil, err
	}
	links, err := db.AllXRefs(ctx)
	if err != nil {
		return nil, err
	}

	pages := make([]WikiPageMeta, 0, len(chunks))
	for _, c := range chunks {
		pages = append(pages, chunkPageMeta(c, links[c.Slug]))
	}
	sort.Slice(pages, func(i, j int) bool {
		rank := func(p WikiPageMeta) int {
			switch p.Type {
			case "community":
				return 0
			default:
				return 1
			}
		}
		ri, rj := rank(pages[i]), rank(pages[j])
		if ri != rj {
			return ri < rj
		}
		return pages[i].Path < pages[j].Path
	})
	return pages, nil
}

func chunkPageMeta(c wiki.WikiChunk, outbound []string) WikiPageMeta {
	pageType := c.DocType
	if pageType == "" {
		pageType = "entity"
	}
	title := c.Title
	if title == "" {
		title = c.Slug
	}
	tags := append([]string(nil), c.Tags...)
	if outbound == nil {
		outbound = []string{}
	}
	return WikiPageMeta{
		Path:       c.Slug + ".md",
		Title:      title,
		Type:       pageType,
		WordCount:  c.WordCount,
		Links:      outbound,
		Tags:       tags,
		Confidence: c.Confidence,
		Source:     c.Source,
	}
}

func resolveDir(dir string) string {
	if resolved, err := filepath.EvalSymlinks(dir); err == nil {
		return resolved
	}
	return dir
}

func indexedModuleStats(wikiDir string) (pages int, hasLog bool) {
	ctx := context.Background()
	db, err := wiki.OpenWikiDB(ctx, wikiDir)
	if err != nil {
		return 0, false
	}
	defer db.Close()
	chunks, _, _, logEntries, err := db.Stats(ctx)
	if err != nil {
		return 0, false
	}
	return chunks, logEntries > 0
}

func (h *WikiHandler) handleAISearch(w http.ResponseWriter, r *http.Request) {
	var body struct {
		Dir   string `json:"dir"`
		Query string `json:"query"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil || body.Query == "" || body.Dir == "" {
		writeJSON(w, AISearchResponse{Error: "dir and query are required"})
		return
	}

	if h.aiClient == nil {
		writeJSON(w, AISearchResponse{
			Error: "AI CLI not configured. Run:\n" +
				"  " + brand.BinName() + " config --global ai.cli <gemini|claude|opencode|codex|cursor-agent>\n" +
				"Then make sure the CLI is installed and authenticated on your system.",
		})
		return
	}

	bm25Results := wiki.BM25Search(r.Context(), body.Dir, body.Query, 15)

	db, err := wiki.OpenWikiDB(r.Context(), body.Dir)
	if err != nil {
		writeJSON(w, AISearchResponse{Error: "wiki index not found: " + err.Error()})
		return
	}
	defer db.Close()
	chunks, err := db.Chunks(r.Context())
	if err != nil {
		writeJSON(w, AISearchResponse{Error: "failed to list pages: " + err.Error()})
		return
	}

	pages := make([]WikiPageMeta, 0, len(chunks))
	var catalog strings.Builder
	catalog.WriteString("=== Wiki Page Catalog ===\n")
	for _, c := range chunks {
		pg := chunkPageMeta(c, nil)
		pages = append(pages, pg)

		excerpt := strings.TrimSpace(c.Body)
		if len(excerpt) > 300 {
			excerpt = excerpt[:300] + "…"
		}
		fmt.Fprintf(&catalog, "\n--- Page: %s (type: %s, path: %s) ---\n%s\n",
			pg.Title, pg.Type, pg.Path, excerpt)
	}

	if len(bm25Results) > 0 {
		catalog.WriteString("\n=== BM25 Pre-Search Results (keyword matches) ===\n")
		for _, br := range bm25Results {
			fmt.Fprintf(&catalog, "- %s (path: %s, score: %.2f)\n  Snippet: %s\n",
				br.Title, br.Path, br.Score, br.Snippet)
		}
	}

	systemPrompt := fmt.Sprintf(`You are a senior software architect answering questions about a project using its knowledge wiki.

You have access to a knowledge wiki with %d pages containing architecture docs, ADRs, feature specs, and entity pages.

Your PRIMARY goal is to provide a **comprehensive, detailed answer** in rich Markdown format.
Your secondary goal is to identify the most relevant wiki pages.

IMPORTANT: Respond with valid JSON only. The "answer" field MUST contain rich Markdown.
Response format:
{
  "answer": "# Detailed Answer\n\nYour rich markdown answer here...\n\n## Section\n\n- bullet points\n- **bold** emphasis\n- etc.",
  "results": [
    {"path": "page-file.md", "title": "Page Title", "relevance": "One-line reason", "score": 85}
  ]
}

Answer Rules:
- Write the answer as if you are explaining to a developer who needs to understand the topic deeply
- Use Markdown headings (##, ###), bullet lists, bold, code blocks, and tables when appropriate
- If the question asks for details, provide thorough coverage: explain concepts, list components, describe relationships
- Reference wiki pages inline using [[Page_Title]] wikilink syntax when mentioning documented concepts
- The answer length should match the question complexity: simple questions get focused answers, detailed questions get comprehensive multi-section responses
- Always include relevant context: why decisions were made, what alternatives were considered, how components interact

Result Rules:
- Only include pages that exist in the catalog
- Use exact "path" values from the catalog
- Limit to the 8 most relevant pages
- Each "relevance" should be a single concise sentence

Wiki Content:
%s`, len(pages), catalog.String())

	userPrompt := fmt.Sprintf("Find relevant wiki pages and answer: %s", body.Query)

	ctx := context.Background()
	response, err := h.aiClient.Complete(ctx, systemPrompt, userPrompt)
	if err != nil {
		writeJSON(w, AISearchResponse{Error: "AI query failed: " + err.Error()})
		return
	}

	response = strings.TrimSpace(response)

	if strings.HasPrefix(response, "```") {
		lines := strings.Split(response, "\n")
		if len(lines) > 2 {
			response = strings.Join(lines[1:len(lines)-1], "\n")
		}
	}

	var aiResp AISearchResponse
	if err := json.Unmarshal([]byte(response), &aiResp); err != nil {

		aiResp = AISearchResponse{
			Answer: response,
		}
	}

	pagesByPath := map[string]WikiPageMeta{}
	for _, pg := range pages {
		pagesByPath[pg.Path] = pg
	}
	var validated []AISearchResult
	for _, r := range aiResp.Results {
		if _, exists := pagesByPath[r.Path]; exists {
			if r.Title == "" {
				r.Title = pagesByPath[r.Path].Title
			}
			validated = append(validated, r)
		}
	}
	aiResp.Results = validated

	wd, _ := os.Getwd()
	session := chat.NewSession(wd, []chat.Source{
		{ID: "wiki", Label: "Wiki", Dir: body.Dir},
	}, body.Query)
	_ = session.Append(chat.ChatMessage{
		Role:    "user",
		Content: body.Query,
	})
	_ = session.Append(chat.ChatMessage{
		Role:    "assistant",
		Content: aiResp.Answer,
	})
	aiResp.SessionID = session.ID

	writeJSON(w, aiResp)
}

func corsJSON(h http.HandlerFunc) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		origin := r.Header.Get("Origin")
		if isAllowedOrigin(origin) {
			w.Header().Set("Access-Control-Allow-Origin", origin)
		}
		w.Header().Set("Content-Type", "application/json")
		w.Header().Set("X-Content-Type-Options", "nosniff")
		w.Header().Set("X-Frame-Options", "DENY")
		if r.Method == http.MethodOptions {
			w.WriteHeader(http.StatusNoContent)
			return
		}
		h(w, r)
	}
}

func isAllowedOrigin(origin string) bool {
	if origin == "" {
		return true
	}
	return strings.HasPrefix(origin, "http://localhost:") ||
		strings.HasPrefix(origin, "http://127.0.0.1:") ||
		strings.HasPrefix(origin, "http://[::1]:") ||
		origin == "http://localhost" ||
		origin == "http://127.0.0.1" ||
		origin == "http://[::1]"
}

func writeJSON(w http.ResponseWriter, v any) {
	_ = json.NewEncoder(w).Encode(v)
}

func loadProjectIDNames() map[string]string {
	return map[string]string{}
}
