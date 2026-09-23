package mcpstdio

import (
	"context"
	"encoding/json"
	"fmt"
	"sync"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/agentpolicy"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/agent"
	"github.com/graphit-labs/graphit-code/internal/toon"
	"github.com/graphit-labs/graphit-code/internal/version"
)

var dreamMemoryTools = map[string]bool{
	"graphit_ast_query": true, "graphit_ast_schema": true, "graphit_ast_fts_schema": true,
	"graphit_ast_fts_query": true, "graphit_ast_list": true, "graphit_ast_source": true, "graphit_ast_search": true,
	"graphit_knowledge_search": true, "graphit_knowledge_schema": true, "graphit_knowledge_query": true,
	"graphit_knowledge_lint": true, "graphit_knowledge_list": true,
	"graphit_wiki_search": true, "graphit_wiki_browse": true, "graphit_wiki_log": true,
	"graphit_wiki_xrefs": true, "graphit_wiki_source": true,
	"graphit_task_schema": true, "graphit_task_query": true, "graphit_task_get": true,
	"graphit_task_export": true, "graphit_task_list": true, "graphit_task_search": true,
	"graphit_task_session_get": true, "graphit_task_session_list": true, "graphit_task_session_search": true,
	"graphit_memory_insert": true, "graphit_memory_update": true, "graphit_memory_delete": true,
	"graphit_memory_list": true, "graphit_memory_search": true, "graphit_memory_source": true,
	"graphit_memory_mandatory": true, "graphit_memory_important": true,
	"graphit_memory_promote": true, "graphit_memory_demote": true,
	"graphit_memory_schema": true, "graphit_memory_query": true,
	"graphit_hub_list": true, "graphit_hub_search": true, "graphit_hub_show": true,
	"graphit_hub_content": true, "graphit_hub_projects": true, "graphit_hub_type-path": true,
	"graphit_references_query": true,
}

var serverProfiles sync.Map

func addTool[In, Out any](server *mcp.Server, tool *mcp.Tool, handler mcp.ToolHandlerFor[In, Out]) {
	profile, _ := serverProfiles.Load(server)
	if profile == agentpolicy.ProfileDreamMemory && !dreamMemoryTools[tool.Name] {
		return
	}
	if profile != nil && profile != "" && profile != agentpolicy.ProfileDreamMemory {
		return
	}
	mcp.AddTool(server, tool, handler)
}

func requireDreamMemoryPrecondition(req *mcp.CallToolRequest, revision *int, contentHash string) error {
	if requestCapabilityProfile(req) != agentpolicy.ProfileDreamMemory {
		return nil
	}
	if revision == nil && contentHash == "" {
		return fmt.Errorf("dream memory mutations require expected_revision or expected_content_hash")
	}
	return nil
}

func requestCapabilityProfile(req *mcp.CallToolRequest) string {
	if req != nil && req.Extra != nil {
		if profile := req.Extra.Header.Get(agentpolicy.ProfileHeader); profile != "" {
			return profile
		}
	}
	return agentpolicy.DetectProfile()
}

func NewServer() *mcp.Server {
	return NewServerForProfile(agentpolicy.DetectProfile())
}

func NewServerForProfile(profile string) *mcp.Server {
	server := mcp.NewServer(
		&mcp.Implementation{
			Name:    brand.MCPServerName("code-stdio"),
			Version: version.Version,
		},
		&mcp.ServerOptions{},
	)
	serverProfiles.Store(server, profile)
	defer serverProfiles.Delete(server)

	registerLifecycleTools(server)
	registerASTTools(server)
	registerKnowledgeTools(server)
	registerMemoryTools(server)
	registerTaskTools(server)
	registerReferenceTools(server)
	registerHubTools(server)
	registerWikiTools(server)
	registerDreamTools(server)
	registerDaemonTools(server)
	registerClusterTools(server)

	return server
}

func safeTool[T any](
	handler func(ctx context.Context, req *mcp.CallToolRequest, input T) (*mcp.CallToolResult, any, error),
) func(ctx context.Context, req *mcp.CallToolRequest, input T) (*mcp.CallToolResult, any, error) {
	return func(ctx context.Context, req *mcp.CallToolRequest, input T) (result *mcp.CallToolResult, session any, err error) {
		defer func() {
			if r := recover(); r != nil {
				err = fmt.Errorf("internal error (panic): %v", r)
				result = nil
				session = nil
			}
		}()
		return handler(ctx, req, input)
	}
}

func textResult(text string) (*mcp.CallToolResult, any, error) {
	return &mcp.CallToolResult{
		Content: []mcp.Content{&mcp.TextContent{Text: text + agent.SysReminder}},
	}, nil, nil
}

func errResult(err error) (*mcp.CallToolResult, any, error) {
	return nil, nil, err
}

func jsonResult(v any) (*mcp.CallToolResult, any, error) {
	data, err := json.MarshalIndent(v, "", "  ")
	if err != nil {
		return errResult(fmt.Errorf("marshal result: %w", err))
	}
	content := []mcp.Content{
		&mcp.TextContent{Text: string(data)},
	}
	if agent.SysReminder != "" {
		content = append(content, &mcp.TextContent{Text: agent.SysReminder})
	}
	return &mcp.CallToolResult{
		Content: content,
	}, nil, nil
}

func toonResult(v any) (*mcp.CallToolResult, any, error) {
	if s, ok := v.(string); ok {
		return textResult(s)
	}
	return textResult(toon.FormatAny(v))
}

func noticeResult(notice string, v any, useToon bool) (*mcp.CallToolResult, any, error) {
	if useToon {
		return textResult(notice + "\n" + toon.FormatAny(v))
	}
	data, err := json.MarshalIndent(v, "", "  ")
	if err != nil {
		return errResult(fmt.Errorf("marshal result: %w", err))
	}
	return textResult(notice + "\n" + string(data))
}

func wantPreview(v *bool) bool {
	return v != nil && *v
}

func aiOpt(v *bool) bool {
	return v == nil || *v
}
