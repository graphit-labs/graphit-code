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
	client, _ := ai.NewClientFromConfig()
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
	pages, err := listWikiPages(absDir)
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

	// Try SQLite FTS5 first (faster, richer results)
	if db, err := wiki.OpenWikiDB(r.Context(), wikiDir); err == nil {
		defer db.Close()
		ftsResults, err := db.Search(r.Context(), query, 30)
		if err == nil && len(ftsResults) > 0 {
			var results []SearchResult
			for _, r := range ftsResults {
				results = append(results, SearchResult{
					Path:    r.Slug + ".md",
					Title:   r.Title,
					Snippet: r.Snippet,
					Score:   int(r.Score * 100),
				})
			}
			writeJSON(w, results)
			return
		}
	}

	bm25Results := wiki.BM25Search(r.Context(), wikiDir, query, 30)
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

// discoverModules lists the wikis the UI can browse for one project: its own
// documentation wiki, its two memory scopes, and every context it has imported.
//
// Every wiki is global and keyed by identity, so each directory is RESOLVED through
// internal/store — nothing here walks the project. It used to probe four places that
// no longer hold a wiki: `<project>/.graphit/knowledge/project` and
// `<project>/.graphit/memory/{project,user}`, which were the per-project replicas the
// storage centralization removed, and `<global>/knowledge` / `<global>/memory`, which
// moved under `<global>/wiki/`. All four missed at once, so the knowledge context list
// came back empty and the memory entries vanished from the sidebar while every wiki
// behind them was intact — a resolution bug that reads exactly like data loss.
//
// Which documentation contexts a project may read is a per-project record, not a
// directory listing: they are claims in its lockfile, resolved per origin, because a
// listing of the global wiki root would report every context anybody on this machine
// ever installed. Memory contexts are the deliberate exception — one is a branch of the
// shared memory repository, so the worktree set is the record and there is no second
// one to consult.
func discoverModules(projectDir string) []WikiModule {
	if projectDir == "" {
		projectDir, _ = os.Getwd()
	}
	idNames := loadProjectIDNames()

	var modules []WikiModule
	add := func(id, label, dir, contextName string, requirePages bool) {
		if dir == "" {
			// No identity to key the store by: a project with no lockfile has no
			// project-scoped memory, and an ephemeral session has no wiki of its own.
			return
		}
		resolved := resolveDir(dir)
		if info, err := os.Stat(resolved); err != nil || !info.IsDir() {
			return
		}
		pages := countMarkdownFiles(resolved)
		if requirePages && pages == 0 {
			return
		}
		_, hasLog := os.Stat(filepath.Join(resolved, "log.md"))
		modules = append(modules, WikiModule{
			ID:      id,
			Label:   label,
			Path:    resolved,
			Context: contextName,
			Pages:   pages,
			HasLog:  hasLog == nil,
		})
	}

	// The project's own three wikis. Existence of the directory is enough — a wiki
	// compiled but empty is a real state the explorer should open, and hiding it
	// reads as "this module is gone" rather than "nothing is indexed yet".
	add("knowledge", projectDisplayName(projectDir), knowledge.WikiDirFor(projectDir), "project", false)
	add("memory-project", "Memory (project)",
		memory.WikiDirFor(projectDir, string(memory.MemoryScopeProject)), "project", false)
	add("memory-user", "Memory (user)",
		memory.WikiDirFor(projectDir, string(memory.MemoryScopeUser)), "user", false)

	// Imported documentation sets, resolved per origin: a Hub artifact is
	// version-keyed, a link points at a sibling project's own wiki.
	for _, name := range store.ContextNames(projectDir, store.KindKnowledge) {
		add("knowledge/"+name, contextLabel(name, idNames),
			store.KnowledgeContextDirIn(projectDir, name), name, true)
	}

	// Imported memory contexts, whose scope and id are the same name.
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

// contextLabel prefers a readable project name over the ULID a Hub context published
// by a project is keyed by.
func contextLabel(name string, idNames map[string]string) string {
	if readable, ok := idNames[name]; ok && readable != "" {
		return readable
	}
	return name
}

var reH1 = regexp.MustCompile(`(?m)^#\s+(.+)$`)
var reFMConfidence = regexp.MustCompile(`(?m)^confidence:\s*([0-9.]+)`)

// Frontmatter readers for the shape OKF specifies.
//
// `tags` is a block sequence and provenance is `sources` (plural) whose entries carry a
// REQUIRED `resource` (§5.1). The explorer used to read `tags: [a, b]` and a singular
// `source:` — the pre-OKF shapes — which is why every page here showed no tags and no
// source once the generator moved. Those readers are gone rather than kept alongside:
// the wiki is a compiled artifact, regenerated from its sources, so there is no old page
// left to read.
var (
	reFMTags     = regexp.MustCompile(`(?m)^tags:\s*$((?:\n[ \t]*-[ \t]*.+)+)`)
	reFMType     = regexp.MustCompile(`(?m)^type:\s*(.+)$`)
	reFMSources  = regexp.MustCompile(`(?m)^sources:\s*$((?:\n[ \t]*-[ \t]*.+|\n[ \t]{2,}\w[\w.-]*:.+)+)`)
	reFMListItem = regexp.MustCompile(`(?m)^[ \t]*-[ \t]*(.+)$`)
	reFMResource = regexp.MustCompile(`(?m)^[ \t]*-?[ \t]*resource:\s*(.+)$`)
)

// frontmatterBlock returns the leading YAML block, so a `type:` or `tags:` line inside the
// page BODY — a code sample, a quoted example — cannot be mistaken for metadata.
func frontmatterBlock(content string) string {
	trimmed := strings.TrimLeft(content, "\ufeff \t\r\n")
	if !strings.HasPrefix(trimmed, "---") {
		return ""
	}
	rest := trimmed[3:]
	if i := strings.Index(rest, "\n"); i >= 0 {
		rest = rest[i+1:]
	} else {
		return ""
	}
	if end := strings.Index(rest, "\n---"); end >= 0 {
		return rest[:end]
	}
	return rest
}

func parseFMTags(fm string) []string {
	tags := make([]string, 0)
	if m := reFMTags.FindStringSubmatch(fm); m != nil {
		for _, item := range reFMListItem.FindAllStringSubmatch(m[1], -1) {
			if tag := unquoteFMScalar(item[1]); tag != "" {
				tags = append(tags, tag)
			}
		}
	}
	return tags
}

// unquoteFMScalar undoes the quoting the generator applies to free-text values so that the
// frontmatter block always parses as YAML.
func unquoteFMScalar(raw string) string {
	v := strings.TrimSpace(raw)
	if len(v) >= 2 && v[0] == '"' && v[len(v)-1] == '"' {
		if unquoted, err := strconv.Unquote(v); err == nil {
			return unquoted
		}
	}
	return strings.Trim(v, `"'`)
}

// parseFMSource returns the page's first provenance path, which is what the explorer shows.
func parseFMSource(fm string) string {
	m := reFMSources.FindStringSubmatch(fm)
	if m == nil {
		return ""
	}
	if r := reFMResource.FindStringSubmatch(m[1]); r != nil {
		return unquoteFMScalar(r[1])
	}
	return ""
}

func listWikiPages(wikiDir string) ([]WikiPageMeta, error) {
	var pages []WikiPageMeta
	err := filepath.WalkDir(wikiDir, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return nil
		}
		if d.IsDir() {
			// Skip cache shard directories — not wiki content.
			if d.Name() == "shards" {
				return filepath.SkipDir
			}
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
	return pages, err
}

func extractPageMeta(relPath, content string) WikiPageMeta {
	name := filepath.Base(relPath)
	nameNoExt := strings.TrimSuffix(name, ".md")

	fm := frontmatterBlock(content)

	// The reserved filenames win over frontmatter: OKF §3.1 gives `index.md` and `log.md`
	// their meaning by NAME, and §8 says an index carries no frontmatter to read anyway.
	// For everything else the frontmatter `type` is the answer, because it is the one field
	// OKF requires (§4.1) and the filename prefixes below are this project's own convention,
	// which an imported bundle has no reason to follow.
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
	default:
		if m := reFMType.FindStringSubmatch(fm); m != nil {
			if t := unquoteFMScalar(m[1]); t != "" {
				pageType = t
			}
		}
	}

	title := nameNoExt
	if m := reH1.FindStringSubmatch(content); m != nil {
		title = strings.TrimSpace(m[1])
	}

	tags := parseFMTags(fm)

	links := wiki.FindWikiLinks(content)

	words := len(strings.Fields(content))

	var confidence float64
	if m := reFMConfidence.FindStringSubmatch(content); m != nil {
		if v, err := strconv.ParseFloat(m[1], 64); err == nil {
			confidence = v
		}
	}

	source := parseFMSource(fm)

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

	bm25Results := wiki.BM25Search(r.Context(), body.Dir, body.Query, 15)

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
		return true // same-origin requests have no Origin header
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
