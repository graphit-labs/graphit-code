package mcpstdio

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/store"
)

// Every tool that addresses a project must also let the caller address a version of
// it, or an agent working in one worktree cannot reach another version's stores.
func TestEveryProjectToolAcceptsAProjectVersion(t *testing.T) {
	session := testMCPClient(t)
	listed, err := session.ListTools(context.Background(), nil)
	if err != nil {
		t.Fatal(err)
	}
	projectTools := 0
	for _, tool := range listed.Tools {
		encoded, err := json.Marshal(tool.InputSchema)
		if err != nil {
			t.Fatal(err)
		}
		var schema struct {
			Properties map[string]json.RawMessage `json:"properties"`
			Required   []string                   `json:"required"`
		}
		if err := json.Unmarshal(encoded, &schema); err != nil {
			t.Fatal(err)
		}
		if _, ok := schema.Properties["project_dir"]; !ok {
			continue
		}
		projectTools++
		if _, ok := schema.Properties["project_version"]; !ok {
			t.Errorf("%s accepts project_dir but not project_version", tool.Name)
		}
		for _, required := range schema.Required {
			if required == "project_version" {
				t.Errorf("%s requires project_version; it must default to the checkout's version", tool.Name)
			}
		}
	}
	if projectTools == 0 {
		t.Fatal("no tool exposes project_dir; the schema inspection is broken")
	}
}

type versionProbeInput struct {
	ProjectDir     string `json:"project_dir"`
	ProjectVersion string `json:"project_version,omitempty"`
}

// The handler of a call that names project_version resolves that version's stores;
// one that omits it resolves the checkout's.
func TestToolCallsResolveStoresOfTheRequestedVersion(t *testing.T) {
	isolateHome(t)
	t.Setenv(brand.EnvVar("PROJECT_VERSION"), "")
	project := realProject(t)

	var seen string
	probe := withProjectVersion(mcp.ToolHandlerFor[versionProbeInput, any](
		func(_ context.Context, _ *mcp.CallToolRequest, in versionProbeInput) (*mcp.CallToolResult, any, error) {
			seen = store.ASTProjectDir(in.ProjectDir)
			return nil, nil, nil
		}))

	root := store.ASTProjectRoot(project)
	if _, _, err := probe(context.Background(), nil, versionProbeInput{ProjectDir: project, ProjectVersion: "feature/x"}); err != nil {
		t.Fatal(err)
	}
	if want := store.VersionStoreDir(root, "feature/x"); seen != want {
		t.Fatalf("explicit version resolved %q, want %q", seen, want)
	}
	if _, _, err := probe(context.Background(), nil, versionProbeInput{ProjectDir: project}); err != nil {
		t.Fatal(err)
	}
	if want := store.VersionStoreDir(root, "main"); seen != want {
		t.Fatalf("omitted version resolved %q, want the checkout's %q", seen, want)
	}
	if got := store.ASTProjectDir(project); got != filepath.Join(root, "versions", "main") {
		t.Fatalf("the explicit version outlived its call: %q", got)
	}
	if _, _, err := probe(context.Background(), nil, versionProbeInput{ProjectVersion: "feature/x"}); err == nil {
		t.Fatal("a version without a project must be rejected")
	}
	if _, err := os.Stat(root); err == nil {
		t.Fatal("resolving a path must not create a store")
	}
}
