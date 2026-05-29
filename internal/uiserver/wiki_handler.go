package uiserver

import (
	"context"
	"encoding/json"
	"fmt"
	"io/fs"
	"net/http"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strconv"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/chat"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/wiki"
	"github.com/graphit-labs/graphit-code/internal/wikisvc"
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
	client, _ := ai.NewClientFromConfig()
	return &WikiHandler{aiClient: client, hubSvc: hubSvc}
}

func (h *WikiHandler) RegisterAPIRoutes(mux *http.ServeMux) {
	mux.HandleFunc("/api/wiki/modules", corsJSON(h.handleModules))
	mux.HandleFunc("/api/wiki/pages", corsJSON(h.handlePages))
	mux.HandleFunc("/api/wiki/page", corsJSON(h.handlePage))
	mux.HandleFunc("/api/wiki/search", corsJSON(h.handleSearch))
	mux.HandleFunc("POST /api/wiki/ai-search", corsJSON(h.handleAISearch))

	mux.HandleFunc("POST /api/wiki/multi-search", corsJSON(h.handleMultiSearch))
	mux.HandleFunc("POST /api/wiki/multi-keyword-search", corsJSON(h.handleMultiKeywordSearch))
	mux.HandleFunc("POST /api/wiki/chat", corsJSON(h.handleChat))
	mux.HandleFunc("/api/wiki/sessions", corsJSON(h.handleSessions))
	mux.HandleFunc("GET /api/wiki/sessions/messages", corsJSON(h.handleSessionMessages))
	mux.HandleFunc("/api/wiki/hub-knowledge", corsJSON(h.handleHubKnowledge))
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
	pages, err := listWikiPages(wikiDir)
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
	absPage := filepath.Join(absWiki, filepath.Clean("/"+pagePath))
	if !strings.HasPrefix(absPage, absWiki) {
		http.Error(w, "path traversal denied", http.StatusForbidden)
		return
	}

	data, err := os.ReadFile(absPage)
	if err != nil {
		http.Error(w, "page not found", http.StatusNotFound)
		return
	}
	content := string(data)
	meta := extractPageMeta(filepath.Base(absPage), content)
	writeJSON(w, WikiPageContent{
		WikiPageMeta: meta,
		Content:      content,
	})
}

func (h *WikiHandler) handleSearch(w http.ResponseWriter, r *http.Request) {
	wikiDir := r.URL.Query().Get("dir")
	query := r.URL.Query().Get("q")
	if wikiDir == "" || query == "" {
		writeJSON(w, []SearchResult{})
		return
	}

	bm25Results := wiki.BM25Search(wikiDir, query, 30)
	if len(bm25Results) > 0 {
		var results []SearchResult
		for _, br := range bm25Results {
			results = append(results, SearchResult{
				Path:    br.Path,
				Title:   br.Title,
				Snippet: br.Snippet,
				Score:   int(br.Score * 100),
			})
		}
		writeJSON(w, results)
		return
	}

	queryLower := strings.ToLower(query)
	pages, err := listWikiPages(wikiDir)
	if err != nil {
		writeJSON(w, []SearchResult{})
		return
	}

	var results []SearchResult
	for _, pg := range pages {
		absPage := filepath.Join(wikiDir, pg.Path)
		data, err := os.ReadFile(absPage)
		if err != nil {
			continue
		}
		content := strings.ToLower(string(data))
		if !strings.Contains(content, queryLower) {
			continue
		}
		idx := strings.Index(content, queryLower)
		start := max(0, idx-60)
		end := min(len(content), idx+120)
		snippet := "…" + strings.TrimSpace(string(data)[start:end]) + "…"
		score := strings.Count(content, queryLower)
		results = append(results, SearchResult{
			Path:    pg.Path,
			Title:   pg.Title,
			Snippet: snippet,
			Score:   score,
		})
	}
	sort.Slice(results, func(i, j int) bool { return results[i].Score > results[j].Score })
	if len(results) > 30 {
		results = results[:30]
	}
	writeJSON(w, results)
}

func discoverModules(projectDir string) []WikiModule {
	var dotBrand string
	if projectDir != "" {
		dotBrand = filepath.Join(projectDir, brand.DotDir())
	} else {
		dotBrand = brand.DotDir()
	}
	globalBase := brand.GlobalDir()

	projectName := ""
	lockPath := filepath.Join(dotBrand, "..", brand.LockFileName())
	if data, err := os.ReadFile(lockPath); err == nil {
		var lockData map[string]any
		if json.Unmarshal(data, &lockData) == nil {
			if proj, ok := lockData["project"].(map[string]any); ok {
				if name, ok := proj["name"].(string); ok && name != "" {
					projectName = name
				}
			}
		}
	}

	projectIDNames := loadProjectIDNames()
	if projectName == "" {
		if projectDir != "" {
			projectName = filepath.Base(projectDir)
		} else if cwd, err := os.Getwd(); err == nil {
			projectName = filepath.Base(cwd)
		}
	}

	type candidate struct {
		id      string
		label   string
		relDir  string
		context string
	}

	locals := []candidate{
		{"knowledge", projectName, filepath.Join(dotBrand, "knowledge", "project"), "project"},
		{"memory-project", "Memory (project)", filepath.Join(dotBrand, "memory", "project"), "project"},
		{"memory-user", "Memory (user)", filepath.Join(dotBrand, "memory", "user"), "user"},
	}

	var modules []WikiModule
	for _, c := range locals {
		dir := c.relDir
		if _, err := os.Stat(dir); err != nil {
			dir = filepath.Join(c.relDir, "wiki")
		}
		if _, err := os.Stat(dir); err == nil {

			resolved := resolveDir(dir)
			n := countMarkdownFiles(resolved)
			_, hasLog := os.Stat(filepath.Join(resolved, "log.md"))
			modules = append(modules, WikiModule{
				ID:      c.id,
				Label:   c.label,
				Path:    dir,
				Context: c.context,
				Pages:   n,
				HasLog:  hasLog == nil,
			})
		}
	}

	discoverContexts(&modules, filepath.Join(dotBrand, "knowledge"), "knowledge", "Knowledge", projectIDNames)
	discoverContexts(&modules, filepath.Join(dotBrand, "memory"), "memory", "Memory", projectIDNames)

	if globalBase != "" {
		discoverContexts(&modules, filepath.Join(globalBase, "knowledge"), "knowledge", "Knowledge", projectIDNames)
		discoverContexts(&modules, filepath.Join(globalBase, "memory"), "memory", "Memory", projectIDNames)
	}

	return modules
}

func discoverContexts(out *[]WikiModule, base, moduleID, label string, idNames map[string]string) {
	scanDir(out, base, base, moduleID, label, 1, idNames)
}

func scanDir(out *[]WikiModule, base, current, moduleID, label string, depth int, idNames map[string]string) {
	entries, err := os.ReadDir(current)
	if err != nil {
		return
	}
	for _, e := range entries {
		isDir := e.IsDir()
		if !isDir {
			info, err := os.Stat(filepath.Join(current, e.Name()))
			if err == nil && info.IsDir() {
				isDir = true
			}
		}
		if !isDir {
			continue
		}

		name := e.Name()
		if depth == 1 && (name == "project" || name == "user" || name == "export" || strings.HasPrefix(name, ".")) {
			continue
		}

		fullPath := filepath.Join(current, name)

		if resolved, err := filepath.EvalSymlinks(fullPath); err == nil {
			fullPath = resolved
		}
		wikiDir := filepath.Join(fullPath, "wiki")
		if info, err := os.Stat(wikiDir); err != nil || !info.IsDir() {
			wikiDir = fullPath
		}

		mdCount := countMarkdownFiles(wikiDir)
		if mdCount > 0 {
			contextName := name
			if depth > 1 {

				origFull := filepath.Join(current, name)
				if rel, err := filepath.Rel(base, origFull); err == nil {
					contextName = filepath.ToSlash(rel)
				}
			}

			displayName := contextName
			if readable, ok := idNames[contextName]; ok {
				displayName = readable
			}

			_, hasLog := os.Stat(filepath.Join(wikiDir, "log.md"))

			exists := false
			for _, m := range *out {
				if m.ID == moduleID+"/"+contextName {
					exists = true
					break
				}
			}
			if !exists {
				*out = append(*out, WikiModule{
					ID:      moduleID + "/" + contextName,
					Label:   displayName,
					Path:    wikiDir,
					Context: contextName,
					Pages:   mdCount,
					HasLog:  hasLog == nil,
				})
			}
		} else if depth < 2 {
			scanDir(out, base, fullPath, moduleID, label, depth+1, idNames)
		}
	}
}

var reH1 = regexp.MustCompile(`(?m)^#\s+(.+)$`)
var reFMTags = regexp.MustCompile(`(?m)^tags:\s*\[([^\]]+)\]`)
var reFMConfidence = regexp.MustCompile(`(?m)^confidence:\s*([0-9.]+)`)
var reFMSource = regexp.MustCompile(`(?m)^source:\s*(.+)$`)

func listWikiPages(wikiDir string) ([]WikiPageMeta, error) {
	var pages []WikiPageMeta
	err := filepath.WalkDir(wikiDir, func(path string, d fs.DirEntry, err error) error {
		if err != nil || d.IsDir() {
			return nil
		}
		if filepath.Ext(path) != ".md" {
			return nil
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return nil
		}
		rel, _ := filepath.Rel(wikiDir, path)
		meta := extractPageMeta(rel, string(data))
		pages = append(pages, meta)
		return nil
	})
	sort.Slice(pages, func(i, j int) bool {

		rank := func(p WikiPageMeta) int {
			switch p.Type {
			case "index":
				return 0
			case "log":
				return 1
			case "community":
				return 2
			default:
				return 3
			}
		}
		ri, rj := rank(pages[i]), rank(pages[j])
		if ri != rj {
			return ri < rj
		}
		return pages[i].Path < pages[j].Path
	})
	return pages, err
}

func extractPageMeta(relPath, content string) WikiPageMeta {
	name := filepath.Base(relPath)
	nameNoExt := strings.TrimSuffix(name, ".md")

	pageType := "entity"
	switch {
	case name == "index.md":
		pageType = "index"
	case name == "log.md":
		pageType = "log"
	case strings.HasPrefix(nameNoExt, "community-"):
		pageType = "community"
	case strings.HasPrefix(nameNoExt, "god-node-"):
		pageType = "god-node"
	}

	title := nameNoExt
	if m := reH1.FindStringSubmatch(content); m != nil {
		title = strings.TrimSpace(m[1])
	}

	tags := make([]string, 0)
	if m := reFMTags.FindStringSubmatch(content); m != nil {
		for _, t := range strings.Split(m[1], ",") {
			if tag := strings.TrimSpace(t); tag != "" {
				tags = append(tags, tag)
			}
		}
	}

	links := wiki.FindWikiLinks(content)

	words := len(strings.Fields(content))

	var confidence float64
	if m := reFMConfidence.FindStringSubmatch(content); m != nil {
		if v, err := strconv.ParseFloat(m[1], 64); err == nil {
			confidence = v
		}
	}

	var source string
	if m := reFMSource.FindStringSubmatch(content); m != nil {
		source = strings.TrimSpace(m[1])
	}

	return WikiPageMeta{
		Path:       relPath,
		Title:      title,
		Type:       pageType,
		WordCount:  words,
		Links:      links,
		Tags:       tags,
		Confidence: confidence,
		Source:     source,
	}
}

func resolveDir(dir string) string {
	if resolved, err := filepath.EvalSymlinks(dir); err == nil {
		return resolved
	}
	return dir
}

func countMarkdownFiles(dir string) int {
	count := 0
	_ = filepath.WalkDir(dir, func(_ string, d fs.DirEntry, _ error) error {
		if !d.IsDir() && filepath.Ext(d.Name()) == ".md" {
			count++
		}
		return nil
	})
	return count
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

	bm25Results := wiki.BM25Search(body.Dir, body.Query, 15)

	pages, err := listWikiPages(body.Dir)
	if err != nil {
		writeJSON(w, AISearchResponse{Error: "failed to list pages: " + err.Error()})
		return
	}

	var catalog strings.Builder
	catalog.WriteString("=== Wiki Page Catalog ===\n")
	for _, pg := range pages {
		absPage := filepath.Join(body.Dir, pg.Path)
		data, err := os.ReadFile(absPage)
		if err != nil {
			continue
		}
		content := string(data)

		if strings.HasPrefix(content, "---") {
			if idx := strings.Index(content[3:], "---"); idx > 0 {
				content = content[idx+6:]
			}
		}
		content = strings.TrimSpace(content)

		if len(content) > 300 {
			content = content[:300] + "…"
		}
		fmt.Fprintf(&catalog, "\n--- Page: %s (type: %s, path: %s) ---\n%s\n",
			pg.Title, pg.Type, pg.Path, content)
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
	session := chat.NewSession(wd, []chat.WikiSource{
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
		w.Header().Set("Access-Control-Allow-Origin", "*")
		w.Header().Set("Content-Type", "application/json")
		if r.Method == http.MethodOptions {
			w.WriteHeader(http.StatusNoContent)
			return
		}
		h(w, r)
	}
}

func writeJSON(w http.ResponseWriter, v any) {
	_ = json.NewEncoder(w).Encode(v)
}

type MultiSearchRequest struct {
	Query    string       `json:"query"`
	WikiDirs []WikiDirRef `json:"wiki_dirs"`
	HubRefs  []HubKnowRef `json:"hub_refs"`
}

type WikiDirRef struct {
	ID    string `json:"id"`
	Label string `json:"label"`
	Dir   string `json:"dir"`
}

type HubKnowRef struct {
	ID      string `json:"id"`
	Version string `json:"version"`
}

type MultiSearchResponse struct {
	Answer         string   `json:"answer"`
	SessionID      string   `json:"session_id"`
	Turns          int      `json:"turns"`
	Tokens         int      `json:"tokens"`
	PagesConsulted []string `json:"pages_consulted,omitempty"`
	Error          string   `json:"error,omitempty"`
}

func (h *WikiHandler) handleMultiSearch(w http.ResponseWriter, r *http.Request) {
	var body MultiSearchRequest
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil || body.Query == "" {
		writeJSON(w, MultiSearchResponse{Error: "query is required"})
		return
	}

	if h.aiClient == nil {
		writeJSON(w, MultiSearchResponse{
			Error: "AI CLI not configured. Run:\n" +
				"  " + brand.BinName() + " config --global ai.cli <gemini|claude|opencode|codex|cursor-agent>\n" +
				"Then make sure the CLI is installed and authenticated on your system.",
		})
		return
	}

	var sources []wiki.WikiSource
	for _, wd := range body.WikiDirs {
		dir := resolveDir(wd.Dir)
		if _, err := os.Stat(dir); err != nil {
			continue
		}
		sources = append(sources, wiki.WikiSource{
			ID:    wd.ID,
			Label: wd.Label,
			Dir:   dir,
		})
	}

	if h.hubSvc != nil {
		for _, ref := range body.HubRefs {
			artifactRef := ref.ID
			if ref.Version != "" {
				artifactRef += "@" + ref.Version
			}
			wikiDir, err := h.hubSvc.EnsureKnowledgeAvailable(r.Context(), artifactRef)
			if err != nil {
				writeJSON(w, MultiSearchResponse{Error: "hub knowledge error: " + err.Error()})
				return
			}
			sources = append(sources, wiki.WikiSource{
				ID:    "hub/" + ref.ID,
				Label: ref.ID,
				Dir:   wikiDir,
			})
		}
	}

	if len(sources) == 0 {
		writeJSON(w, MultiSearchResponse{Error: "no valid wiki sources found"})
		return
	}

	ctx := r.Context()
	result, err := wiki.SearchMultiWiki(ctx, h.aiClient, body.Query, wiki.MultiWikiSearchConfig{
		Sources:           sources,
		UseBM25:           true,
		BM25TopNPerSource: 5,
	})
	if err != nil {
		writeJSON(w, MultiSearchResponse{Error: "search failed: " + err.Error()})
		return
	}

	wd, _ := os.Getwd()
	chatSources := make([]chat.WikiSource, len(sources))
	for i, s := range sources {
		chatSources[i] = chat.WikiSource{ID: s.ID, Label: s.Label, Dir: s.Dir}
	}
	session := chat.NewSession(wd, chatSources, body.Query)

	_ = session.Append(chat.ChatMessage{
		Role:    "user",
		Content: body.Query,
	})
	_ = session.Append(chat.ChatMessage{
		Role:    "assistant",
		Content: result.Answer,
	})

	wikiLinkRe := regexp.MustCompile(`\[\[([^\]]+)\]\]`)
	matches := wikiLinkRe.FindAllStringSubmatch(result.Answer, -1)
	var pagesConsulted []string
	seen := map[string]bool{}
	for _, m := range matches {
		if len(m) > 1 && !seen[m[1]] {
			seen[m[1]] = true
			pagesConsulted = append(pagesConsulted, m[1])
		}
	}

	writeJSON(w, MultiSearchResponse{
		Answer:         result.Answer,
		SessionID:      session.ID,
		Turns:          result.Turns,
		Tokens:         result.TokensSent,
		PagesConsulted: pagesConsulted,
	})
}

type ChatRequest struct {
	SessionID string `json:"session_id"`
	Message   string `json:"message"`
}

type ChatResponse struct {
	Answer    string `json:"answer"`
	SessionID string `json:"session_id"`
	Error     string `json:"error,omitempty"`
}

func (h *WikiHandler) handleChat(w http.ResponseWriter, r *http.Request) {
	var body ChatRequest
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil || body.SessionID == "" || body.Message == "" {
		writeJSON(w, ChatResponse{Error: "session_id and message are required"})
		return
	}

	if h.aiClient == nil {
		writeJSON(w, ChatResponse{Error: "AI CLI not configured"})
		return
	}

	wikiSvc := wikisvc.NewWikiService("")
	response, err := wikiSvc.ContinueChat(r.Context(), body.SessionID, body.Message)
	if err != nil {
		writeJSON(w, ChatResponse{Error: err.Error()})
		return
	}

	writeJSON(w, ChatResponse{
		Answer:    response,
		SessionID: body.SessionID,
	})
}

type SessionListItem struct {
	ID           string            `json:"id"`
	Title        string            `json:"title"`
	CreatedAt    string            `json:"created_at"`
	UpdatedAt    string            `json:"updated_at"`
	MessageCount int               `json:"message_count"`
	WikiSources  []chat.WikiSource `json:"wiki_sources"`
}

func (h *WikiHandler) handleSessions(w http.ResponseWriter, r *http.Request) {
	switch r.Method {
	case http.MethodGet:
		projectDir := r.URL.Query().Get("project_dir")
		if projectDir == "" {
			projectDir, _ = os.Getwd()
		}
		wikiSvc := wikisvc.NewWikiService(projectDir)
		sessions, err := wikiSvc.ListSessions()
		if err != nil {
			writeJSON(w, []SessionListItem{})
			return
		}
		items := make([]SessionListItem, len(sessions))
		for i, s := range sessions {
			items[i] = SessionListItem{
				ID:           s.ID,
				Title:        s.Title,
				CreatedAt:    s.CreatedAt.Format("2006-01-02T15:04:05Z"),
				UpdatedAt:    s.UpdatedAt.Format("2006-01-02T15:04:05Z"),
				MessageCount: s.MessageCount,
				WikiSources:  s.WikiSources,
			}
		}
		writeJSON(w, items)

	case http.MethodDelete:
		sessionID := r.URL.Query().Get("id")
		if sessionID == "" {
			http.Error(w, "id required", http.StatusBadRequest)
			return
		}
		wikiSvc := wikisvc.NewWikiService("")
		if err := wikiSvc.DeleteSession(sessionID); err != nil {
			http.Error(w, err.Error(), http.StatusNotFound)
			return
		}
		writeJSON(w, map[string]string{"status": "deleted"})

	default:
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
	}
}

type HubKnowledgeItem struct {
	ID          string   `json:"id"`
	Name        string   `json:"name"`
	Description string   `json:"description"`
	Version     string   `json:"version"`
	Versions    []string `json:"versions"`
	Installed   bool     `json:"installed"`
}

func (h *WikiHandler) handleHubKnowledge(w http.ResponseWriter, r *http.Request) {
	if h.hubSvc == nil {
		writeJSON(w, []HubKnowledgeItem{})
		return
	}

	entries := h.hubSvc.ListEntries(hub.TypeKnowledge)
	items := make([]HubKnowledgeItem, 0, len(entries))

	for _, e := range entries {

		globalDir := filepath.Join(brand.GlobalDir(), "wiki", "knowledge", e.ProjectID)
		installed := false
		if _, err := os.Stat(globalDir); err == nil {
			installed = true
		}

		versions := make([]string, len(e.Versions))
		for i, v := range e.Versions {
			versions[len(e.Versions)-1-i] = v
		}

		items = append(items, HubKnowledgeItem{
			ID:          e.ID,
			Name:        e.Name,
			Description: e.Description,
			Version:     e.Latest,
			Versions:    versions,
			Installed:   installed,
		})
	}

	writeJSON(w, items)
}

type MultiKeywordResult struct {
	SourceID    string  `json:"source_id"`
	SourceLabel string  `json:"source_label"`
	Path        string  `json:"path"`
	Title       string  `json:"title"`
	Snippet     string  `json:"snippet"`
	Score       float64 `json:"score"`
}

func (h *WikiHandler) handleMultiKeywordSearch(w http.ResponseWriter, r *http.Request) {
	var body MultiSearchRequest
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil || body.Query == "" {
		writeJSON(w, []MultiKeywordResult{})
		return
	}

	type source struct {
		ID    string
		Label string
		Dir   string
	}
	var sources []source
	for _, wd := range body.WikiDirs {
		dir := resolveDir(wd.Dir)
		if _, err := os.Stat(dir); err != nil {
			continue
		}
		sources = append(sources, source{ID: wd.ID, Label: wd.Label, Dir: dir})
	}

	if h.hubSvc != nil {
		for _, ref := range body.HubRefs {
			artifactRef := ref.ID
			if ref.Version != "" {
				artifactRef += "@" + ref.Version
			}
			wikiDir, err := h.hubSvc.EnsureKnowledgeAvailable(r.Context(), artifactRef)
			if err != nil {
				continue
			}
			sources = append(sources, source{ID: "hub/" + ref.ID, Label: ref.ID, Dir: wikiDir})
		}
	}

	var results []MultiKeywordResult
	for _, src := range sources {
		bm25Results := wiki.BM25Search(src.Dir, body.Query, 10)
		for _, br := range bm25Results {
			results = append(results, MultiKeywordResult{
				SourceID:    src.ID,
				SourceLabel: src.Label,
				Path:        br.Path,
				Title:       br.Title,
				Snippet:     br.Snippet,
				Score:       br.Score,
			})
		}
	}

	sort.Slice(results, func(i, j int) bool { return results[i].Score > results[j].Score })

	writeJSON(w, results)
}

func (h *WikiHandler) handleSessionMessages(w http.ResponseWriter, r *http.Request) {
	sessionID := r.URL.Query().Get("id")
	if sessionID == "" {
		http.Error(w, "id required", http.StatusBadRequest)
		return
	}

	session, err := chat.LoadSession(sessionID)
	if err != nil {
		http.Error(w, "session not found: "+err.Error(), http.StatusNotFound)
		return
	}

	messages, err := session.LoadHistory()
	if err != nil {
		http.Error(w, "failed to load history: "+err.Error(), http.StatusInternalServerError)
		return
	}

	if messages == nil {
		messages = []chat.ChatMessage{}
	}

	writeJSON(w, messages)
}

func loadProjectIDNames() map[string]string {
	names := map[string]string{}
	registryPath := filepath.Join(brand.GlobalDir(), "hub.registry.json")
	data, err := os.ReadFile(registryPath)
	if err != nil {
		return names
	}
	var cache struct {
		Projects map[string]struct {
			Name string `json:"name"`
		} `json:"projects"`
	}
	if json.Unmarshal(data, &cache) == nil {
		for id, proj := range cache.Projects {
			if proj.Name != "" {
				names[id] = proj.Name
			}
		}
	}
	return names
}
