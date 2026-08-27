package mcpstdio

import (
	"context"
	"errors"
	"fmt"
	"path/filepath"
	"sort"
	"strings"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/daemon"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/textslice"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

type wikiSearchInput struct {
	Query       string   `json:"query" jsonschema:"Natural language question to search across multiple wikis"`
	Wikis       []string `json:"wikis,omitempty" jsonschema:"Wiki sources to search (project, memory, or project IDs from ecosystem)"`
	HubRefs     []string `json:"hub_refs,omitempty" jsonschema:"Hub knowledge artifact references to include (format: artifact-id@version)"`
	SessionID   string   `json:"session_id,omitempty" jsonschema:"Session ID to continue an existing conversation"`
	TopK        int      `json:"top_k,omitempty" jsonschema:"BM25 results per wiki source (0 = no limit)"`
	ProjectDir  string   `json:"project_dir" jsonschema:"Project directory (required)"`
	Mode        string   `json:"mode,omitempty" jsonschema:"Search mode: hybrid (default, combines BM25 + semantic via RRF), fts (BM25 only), semantic (vector only)"`
	AiOptimized *bool    `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type wikiBrowseInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	Wiki        string `json:"wiki,omitempty" jsonschema:"Wiki scope: project, memory (default: project)"`
	Context     string `json:"context,omitempty" jsonschema:"Named imported context, or a memory scope other than project"`
	DocType     string `json:"doc_type,omitempty" jsonschema:"Filter by document type (e.g., specification, architecture, decision)"`
	Limit       int    `json:"limit,omitempty" jsonschema:"Max results (default: 100)"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type wikiLogInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	Wiki        string `json:"wiki,omitempty" jsonschema:"Wiki scope: project, memory (default: project)"`
	Context     string `json:"context,omitempty" jsonschema:"Named imported context, or a memory scope other than project"`
	Limit       int    `json:"limit,omitempty" jsonschema:"Max log entries (default: 10)"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type wikiXRefsInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	Query       string `json:"query" jsonschema:"Entity slug or name to find cross-references for"`
	Wiki        string `json:"wiki,omitempty" jsonschema:"Wiki scope: project, memory (default: project)"`
	Context     string `json:"context,omitempty" jsonschema:"Named imported context, or a memory scope other than project"`
	Depth       int    `json:"depth,omitempty" jsonschema:"Depth of graph traversal (default: 1, max: 3)"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type wikiEmbedInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Wiki       string `json:"wiki,omitempty" jsonschema:"Wiki scope: project (default) or memory"`
}

type wikiSourceInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory — may be another project in the ecosystem (required)"`
	Path        string `json:"path" jsonschema:"Page to read: the slug returned by search/browse/xrefs, that slug with .md, or a path relative to the wiki directory (required)"`
	Wiki        string `json:"wiki,omitempty" jsonschema:"Wiki scope: project (default) or memory"`
	Context     string `json:"context,omitempty" jsonschema:"Named imported knowledge context where the page resides"`
	Head        int    `json:"head,omitempty" jsonschema:"Show only the first N lines"`
	Tail        int    `json:"tail,omitempty" jsonschema:"Show only the last N lines"`
	StartLine   int    `json:"start_line,omitempty" jsonschema:"Start line number (1-indexed)"`
	EndLine     int    `json:"end_line,omitempty" jsonschema:"End line number (1-indexed, inclusive)"`
	Pattern     string `json:"pattern,omitempty" jsonschema:"Search for a pattern (literal text or regex if regex=true)"`
	IsRegex     bool   `json:"regex,omitempty" jsonschema:"Treat pattern as a regular expression"`
	Before      int    `json:"before,omitempty" jsonschema:"Number of context lines before each pattern match"`
	After       int    `json:"after,omitempty" jsonschema:"Number of context lines after each pattern match"`
	LineNumbers bool   `json:"line_numbers,omitempty" jsonschema:"Include line numbers in the output (default: false)"`
}

// resolveWikiScopeDirContext is resolveWikiScopeDir for an imported context.
//
// A context is addressed differently by the two wikis, which is why the name is a
// single parameter rather than two: for knowledge it names an imported documentation
// set, and for memory it names the SCOPE — an imported memory context, or "project"
// when unspecified.
func resolveWikiScopeDirContext(projectDir, wikiScope, contextName string) (string, error) {
	switch wikiScope {
	case "", "project", "knowledge":
		return resolveWikiDir("knowledge", projectDir, contextName), nil
	case "memory":
		if contextName == "" {
			contextName = "project"
		}
		return resolveWikiDir("memory", projectDir, contextName), nil
	default:
		return "", fmt.Errorf("unknown wiki scope %q — use 'project' or 'memory'", wikiScope)
	}
}

// openWikiForRead opens a wiki index for querying and refuses one with no content.
//
// OpenWikiDB CREATES what it opens, so "opened fine" says nothing about whether
// anything was ever indexed: a fresh project, or a page written in the seconds
// before the daemon recompiles, yields a perfectly healthy EMPTY index whose every
// answer is "no results" for a reason that has nothing to do with the query. The
// markdown-backed search paths were taught this distinction after it cost a whole
// session of silent no-recall; these tools had not been.
func openWikiForRead(ctx context.Context, projectDir, wikiScope string) (*wiki.WikiDB, error) {
	return openWikiForReadContext(ctx, projectDir, wikiScope, "")
}

func openWikiForReadContext(ctx context.Context, projectDir, wikiScope, contextName string) (*wiki.WikiDB, error) {
	// A PUBLISHED CONTEXT IS READ WHERE IT LIVES. Nothing was downloaded for it, so there is no
	// local directory to resolve — the engine queries the objects on S3 directly. Tried first,
	// because a stale local copy from before the switch would otherwise win silently.
	if db, mounted, err := openMountedWiki(ctx, projectDir, wikiScope, contextName); mounted {
		return db, err
	}

	wikiDir, err := resolveWikiScopeDirContext(projectDir, wikiScope, contextName)
	if err != nil {
		return nil, err
	}
	db, err := wiki.OpenWikiDB(ctx, wikiDir)
	if err != nil {
		return nil, fmt.Errorf("opening wiki db: %w", err)
	}
	if !db.HasContent(ctx) {
		_ = db.Close()
		return nil, fmt.Errorf("the %q wiki at %s has no indexed content yet — "+
			"run graphit_knowledge_index (or graphit_sync) and retry; "+
			"this is an empty index, not an empty answer", wikiScope, wikiDir)
	}
	return db, nil
}

// openMountedWiki opens a published knowledge context from object storage.
//
// The middle return says whether this context IS mounted, which is separate from whether opening
// it worked: false means "not a published context, use the local path", and true with an error
// means "it is published and the mount failed", which must surface rather than fall back to a
// local directory that does not exist.
//
// Memory is never mounted: it is read-and-write and multi-writer, so it carries its source and is
// compiled locally. Only knowledge is frozen by a version.
func openMountedWiki(ctx context.Context, projectDir, wikiScope, contextName string) (*wiki.WikiDB, bool, error) {
	switch wikiScope {
	case "", "project", "knowledge":
	default:
		return nil, false, nil
	}
	if contextName == "" {
		return nil, false, nil
	}
	st, err := hub.NewS3Store(ctx, nil, loadProjectConfig(projectDir))
	if err != nil {
		return nil, false, nil
	}
	mount, ok := st.MountedWikiFor(projectDir, contextName)
	if !ok {
		return nil, false, nil
	}
	db, err := wiki.OpenWikiDBAt(ctx, mount.Config)
	if err != nil {
		return nil, true, fmt.Errorf("opening the published wiki %s@%s at %s: %w",
			mount.ArtifactID, mount.Version, mount.Config.URI, err)
	}
	return db, true, nil
}

func registerWikiTools(server *mcp.Server) {
	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("wiki", "search"),
		Description: "Search across multiple wiki sources using BM25 full-text and optional semantic search.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input wikiSearchInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		topK := input.TopK
		if topK <= 0 {
			topK = 15
		}

		mode := input.Mode
		if mode == "" {
			mode = "hybrid"
		}

		wikis := input.Wikis
		if len(wikis) == 0 {
			wikis = []string{"project"}
		}

		switch mode {
		case "fts":
			var allResults []wiki.WikiSearchResult
			var skipped []string
			for _, scope := range wikis {
				wikiDB, err := openWikiForRead(ctx, projectDir, scope)
				if err != nil {
					skipped = append(skipped, err.Error())
					continue
				}
				results, err := wikiDB.Search(ctx, input.Query, topK)
				_ = wikiDB.Close()
				if err == nil {
					allResults = append(allResults, results...)
				}
			}
			if len(allResults) == 0 && len(skipped) > 0 {
				return errResult(fmt.Errorf("%s", strings.Join(skipped, "; ")))
			}
			sort.Slice(allResults, func(i, j int) bool {
				return allResults[i].Score > allResults[j].Score
			})
			if len(allResults) > topK {
				allResults = allResults[:topK]
			}
			if aiOpt(input.AiOptimized) {
				return textResult(wiki.FormatSearchResultsTOON(allResults))
			}
			return jsonResult(allResults)

		case "semantic":
			embClient, err := ai.NewEmbeddingClientFromConfig()
			if err != nil {
				return errResult(err)
			}
			queryVec, err := embClient.Embed(ctx, input.Query)
			if err != nil {
				return errResult(fmt.Errorf("embed query: %w", err))
			}
			var allResults []wiki.WikiSearchResult
			var skipped []string
			for _, scope := range wikis {
				wikiDB, err := openWikiForRead(ctx, projectDir, scope)
				if err != nil {
					skipped = append(skipped, err.Error())
					continue
				}
				results, err := wikiDB.SemanticSearch(ctx, queryVec, topK)
				_ = wikiDB.Close()
				if err == nil {
					allResults = append(allResults, results...)
				}
			}
			if len(allResults) == 0 && len(skipped) > 0 {
				return errResult(fmt.Errorf("%s", strings.Join(skipped, "; ")))
			}
			sort.Slice(allResults, func(i, j int) bool {
				return allResults[i].Score > allResults[j].Score
			})
			if len(allResults) > topK {
				allResults = allResults[:topK]
			}
			if aiOpt(input.AiOptimized) {
				return textResult(wiki.FormatSearchResultsTOON(allResults))
			}
			return jsonResult(allResults)

		default: // hybrid
			// Try to get embedding client; fall back to FTS-only if unavailable.
			//
			// The fallback is deliberate, but it used to be SILENT: with no vectors,
			// "hybrid" answered from the lexical index and said so nowhere, so a
			// broken embedder read as a working hybrid search for as long as nobody
			// checked. The reason is reported alongside the results instead.
			var queryVec []float32
			var degraded string
			if embClient, embErr := ai.NewEmbeddingClientFromConfig(); embErr != nil {
				degraded = fmt.Sprintf("embedding client unavailable (%v)", embErr)
			} else if vec, err := embClient.Embed(ctx, input.Query); err != nil {
				degraded = fmt.Sprintf("query could not be embedded (%v)", err)
			} else {
				queryVec = vec
			}

			var allResults []wiki.WikiSearchResult
			var skipped []string
			for _, scope := range wikis {
				wikiDB, err := openWikiForRead(ctx, projectDir, scope)
				if err != nil {
					skipped = append(skipped, err.Error())
					continue
				}
				var results []wiki.WikiSearchResult
				if queryVec != nil {
					results, err = wikiDB.HybridSearch(ctx, input.Query, queryVec, topK)
				} else {
					results, err = wikiDB.Search(ctx, input.Query, topK)
				}
				_ = wikiDB.Close()
				if err == nil {
					allResults = append(allResults, results...)
				}
			}
			if len(allResults) == 0 && len(skipped) > 0 {
				return errResult(fmt.Errorf("%s", strings.Join(skipped, "; ")))
			}

			// Sort merged results by score descending and trim.
			sort.Slice(allResults, func(i, j int) bool {
				return allResults[i].Score > allResults[j].Score
			})
			if len(allResults) > topK {
				allResults = allResults[:topK]
			}
			if aiOpt(input.AiOptimized) {
				out := wiki.FormatSearchResultsTOON(allResults)
				if degraded != "" {
					out += fmt.Sprintf("\n\nNOTE: hybrid degraded to full-text only — %s. "+
						"Semantic ranking contributed nothing to these results.\n", degraded)
				}
				return textResult(out)
			}
			if degraded != "" {
				return jsonResult(map[string]any{"results": allResults, "degraded": degraded})
			}
			return jsonResult(allResults)
		}
	}))

	// New WikiDB tools

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("wiki", "browse"),
		Description: "Browse wiki documents in a structured format. Lists chunks/documents from the WikiDB with optional filtering by type. Pass context to browse an imported knowledge context or another memory scope.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input wikiBrowseInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		db, err := openWikiForReadContext(ctx, projectDir, input.Wiki, input.Context)
		if err != nil {
			return errResult(err)
		}
		defer func() { _ = db.Close() }()

		limit := input.Limit
		if limit <= 0 {
			limit = 100
		}

		filter := wiki.BrowseFilter{
			DocType:   input.DocType,
			ClusterID: -1,
			Limit:     limit,
		}

		entries, err := db.Browse(ctx, filter)
		if err != nil {
			return errResult(fmt.Errorf("browsing wiki: %w", err))
		}

		if len(entries) == 0 {
			return textResult("No wiki documents found.")
		}

		if aiOpt(input.AiOptimized) {
			return textResult(wiki.FormatBrowseResultsTOON(entries))
		}

		var b strings.Builder
		_, _ = fmt.Fprintf(&b, "Found %d document(s):\n\n", len(entries))
		for i, e := range entries {
			typeLabel := e.DocType
			if typeLabel == "" {
				typeLabel = "doc"
			}
			_, _ = fmt.Fprintf(&b, "%d. **%s** `%s` (confidence: %.1f)\n", i+1, e.Title, typeLabel, e.Confidence)
			if e.Breadcrumb != "" {
				_, _ = fmt.Fprintf(&b, "   📍 %s\n", e.Breadcrumb)
			}
			if e.Summary != "" {
				summary := e.Summary
				if len(summary) > 150 {
					summary = summary[:150] + "…"
				}
				_, _ = fmt.Fprintf(&b, "   > %s\n", summary)
			}
		}
		return textResult(b.String())
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("wiki", "log"),
		Description: "Show wiki sync history. Returns a timeline of sync operations showing what was added, updated, and deleted. Pass context for an imported knowledge context or another memory scope.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input wikiLogInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		db, err := openWikiForReadContext(ctx, projectDir, input.Wiki, input.Context)
		if err != nil {
			return errResult(err)
		}
		defer func() { _ = db.Close() }()

		limit := input.Limit
		if limit <= 0 {
			limit = 10
		}

		entries, err := db.QuerySyncLog(ctx, limit)
		if err != nil {
			return errResult(fmt.Errorf("querying sync log: %w", err))
		}

		if len(entries) == 0 {
			return textResult("No sync history found.")
		}

		if aiOpt(input.AiOptimized) {
			return textResult(wiki.FormatSyncLogTOON(entries))
		}

		var b strings.Builder
		_, _ = fmt.Fprintf(&b, "Sync log (%d entries):\n\n", len(entries))
		for _, e := range entries {
			_, _ = fmt.Fprintf(&b, "**#%d** — %s\n", e.ID, e.Timestamp)
			_, _ = fmt.Fprintf(&b, "  docs: %d | written: %d", e.TotalDocs, e.ArticlesWritten)
			if e.BacklinksAdded > 0 {
				_, _ = fmt.Fprintf(&b, " | backlinks: %d", e.BacklinksAdded)
			}
			b.WriteString("\n")
			if len(e.Added) > 0 {
				_, _ = fmt.Fprintf(&b, "  ＋ %s\n", strings.Join(e.Added, ", "))
			}
			if len(e.Updated) > 0 {
				_, _ = fmt.Fprintf(&b, "  ⟳ %s\n", strings.Join(e.Updated, ", "))
			}
			if len(e.Deleted) > 0 {
				_, _ = fmt.Fprintf(&b, "  − %s\n", strings.Join(e.Deleted, ", "))
			}
			b.WriteString("\n")
		}
		return textResult(b.String())
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("wiki", "xrefs"),
		Description: "Show cross-references for a wiki entity. Returns inbound and outbound references with configurable graph traversal depth. Pass context for an imported knowledge context or another memory scope.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input wikiXRefsInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		if input.Query == "" {
			return errResult(fmt.Errorf("query is required"))
		}

		db, err := openWikiForReadContext(ctx, projectDir, input.Wiki, input.Context)
		if err != nil {
			return errResult(err)
		}
		defer func() { _ = db.Close() }()

		depth := input.Depth
		if depth < 1 {
			depth = 1
		}
		if depth > 3 {
			depth = 3
		}

		refs, err := db.FindXRefs(ctx, input.Query, depth)
		if err != nil {
			return errResult(fmt.Errorf("finding xrefs: %w", err))
		}

		if len(refs) == 0 {
			return textResult(fmt.Sprintf("No cross-references found for %q.", input.Query))
		}

		if aiOpt(input.AiOptimized) {
			return textResult(wiki.FormatXRefResultsTOON(input.Query, depth, refs))
		}

		var outbound, inbound []wiki.XRefResult
		for _, r := range refs {
			if r.Direction == "outbound" {
				outbound = append(outbound, r)
			} else {
				inbound = append(inbound, r)
			}
		}

		var b strings.Builder
		_, _ = fmt.Fprintf(&b, "Cross-references for **%s** (depth %d):\n\n", input.Query, depth)

		if len(outbound) > 0 {
			_, _ = fmt.Fprintf(&b, "**Outbound** (%d):\n", len(outbound))
			for _, r := range outbound {
				_, _ = fmt.Fprintf(&b, "  → %s (`%s`) [%s]\n", r.Title, r.Slug, r.RefType)
			}
			b.WriteString("\n")
		}

		if len(inbound) > 0 {
			_, _ = fmt.Fprintf(&b, "**Inbound** (%d):\n", len(inbound))
			for _, r := range inbound {
				_, _ = fmt.Fprintf(&b, "  ← %s (`%s`) [%s]\n", r.Title, r.Slug, r.RefType)
			}
		}

		return textResult(b.String())
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("wiki", "embed"),
		Description: "Generate or update vector embeddings for wiki document chunks. Embeddings enable semantic and hybrid search.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input wikiEmbedInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		embClient, err := ai.NewEmbeddingClientFromConfig()
		if err != nil {
			return errResult(err)
		}
		embedder := wiki.NewWikiEmbedder(embClient, wiki.DefaultWikiEmbedConfig())

		// The same targets the daemon embeds, so a manual run and the background loop
		// cannot disagree about which copy of a wiki is the one worth embedding.
		targets := daemon.WikiEmbedTargets(projectDir, nil)
		if input.Wiki != "" {
			filtered := targets[:0]
			for _, t := range targets {
				if wikiScopeMatchesTarget(input.Wiki, t.Dir) {
					filtered = append(filtered, t)
				}
			}
			targets = filtered
		}
		if len(targets) == 0 {
			return errResult(fmt.Errorf("no wiki to embed for scope %q", input.Wiki))
		}

		total := 0
		for _, target := range targets {
			count, err := embedder.RunCycle(ctx, target.Dir)
			if err != nil {
				return errResult(fmt.Errorf("embedding %s: %w", target.Dir, err))
			}
			total += count
		}

		return textResult(fmt.Sprintf("%d wiki chunks embedded.", total))
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name: brand.MCPToolName("wiki", "source"),
		Description: "Read the content of a wiki page, with head/tail, line ranges and pattern search with context — the same slicing as the code-source tool. " +
			"This is the ONLY way to read a page: wikis are stored once, in the global directory, so there is no page file inside the project to open. " +
			"It takes the project as a parameter, so it also reads pages belonging to any other project in the ecosystem.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input wikiSourceInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		module := "knowledge"
		contextName := input.Context
		switch input.Wiki {
		case "", "project", "knowledge":
		case "memory":
			module = "memory"
			// The memory wiki is addressed by scope, not by imported context.
			if contextName == "" {
				contextName = "project"
			}
		default:
			return errResult(fmt.Errorf("unknown wiki scope %q — use 'project' or 'memory'", input.Wiki))
		}

		slice := textslice.Request{
			Head:        input.Head,
			Tail:        input.Tail,
			StartLine:   input.StartLine,
			EndLine:     input.EndLine,
			Pattern:     input.Pattern,
			IsRegex:     input.IsRegex,
			Before:      input.Before,
			After:       input.After,
			LineNumbers: input.LineNumbers,
		}

		// A PUBLISHED CONTEXT HAS NO PAGE FILES. Nothing was downloaded, so the text comes out of
		// the index — the same text, because the wiki compiles one chunk per document, so
		// `chunks.body` is the page rather than a slice of it.
		if module == "knowledge" {
			if db, mounted, mErr := openMountedWiki(ctx, projectDir, "knowledge", contextName); mounted {
				if mErr != nil {
					return errResult(mErr)
				}
				defer func() { _ = db.Close() }()
				result, rErr := wiki.ReadPageFrom(ctx, db, input.Path, slice)
				if rErr != nil {
					if pages := wiki.ListPagesFrom(ctx, db); errors.Is(rErr, wiki.ErrPageNotFound) && len(pages) > 0 {
						shown := pages
						suffix := ""
						if len(shown) > 40 {
							shown = shown[:40]
							suffix = fmt.Sprintf("\n… and %d more", len(pages)-40)
						}
						return errResult(fmt.Errorf("%w\n\nPages in this wiki:\n%s%s",
							rErr, strings.Join(shown, "\n"), suffix))
					}
					return errResult(rErr)
				}
				if result.Source == "" && len(result.Matches) == 0 {
					return textResult(fmt.Sprintf("No matches found for pattern %q in wiki page %s",
						input.Pattern, result.Page))
				}
				return textResult(result.Source)
			}
		}

		wikiDir := resolveWikiDir(module, projectDir, contextName)
		result, err := wiki.ReadPage(wikiDir, input.Path, textslice.Request{
			Head:        input.Head,
			Tail:        input.Tail,
			StartLine:   input.StartLine,
			EndLine:     input.EndLine,
			Pattern:     input.Pattern,
			IsRegex:     input.IsRegex,
			Before:      input.Before,
			After:       input.After,
			LineNumbers: input.LineNumbers,
		})
		if err != nil {
			// A wrong slug is the common mistake, so name what is actually there
			// rather than leaving the agent to guess or fall back to a file read.
			// A refused reference keeps its own reason instead.
			if pages := wiki.ListPages(wikiDir); errors.Is(err, wiki.ErrPageNotFound) && len(pages) > 0 {
				sort.Strings(pages)
				shown := pages
				suffix := ""
				if len(shown) > 40 {
					shown = shown[:40]
					suffix = fmt.Sprintf("\n… and %d more", len(pages)-40)
				}
				return errResult(fmt.Errorf("%w\n\nPages in this wiki:\n%s%s",
					err, strings.Join(shown, "\n"), suffix))
			}
			return errResult(err)
		}

		if result.Source == "" && len(result.Matches) == 0 {
			return textResult(fmt.Sprintf("No matches found for pattern %q in wiki page %s", input.Pattern, result.Page))
		}
		return textResult(result.Source)
	}))
}

// wikiScopeMatchesTarget maps a user-facing scope name onto one of the embed targets.
func wikiScopeMatchesTarget(scope, dir string) bool {
	slashed := filepath.ToSlash(dir)
	switch scope {
	case "project", "knowledge":
		return strings.Contains(slashed, "/knowledge/")
	case "memory":
		return strings.Contains(slashed, "/memory/")
	default:
		return false
	}
}
