package prep

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/livesearch"
)

type fakeInstaller struct {
	calls []installCall
	fail  map[string]error
	place func(projectDir string, call installCall) error
}

type installCall struct {
	entryID string
	ide     string
	artType hub.ArtifactType
	dir     string
}

func (f *fakeInstaller) Install(_ context.Context, entryID, _, ide string, entryType hub.ArtifactType, _, projectDir string) (*hub.InstallResult, error) {
	call := installCall{entryID: entryID, ide: ide, artType: entryType, dir: projectDir}
	f.calls = append(f.calls, call)
	id, version := entryID, "1.0.0"
	if parts := strings.SplitN(entryID, "@", 2); len(parts) == 2 {
		id, version = parts[0], parts[1]
	}
	if err, ok := f.fail[id]; ok {
		return nil, err
	}
	if f.place != nil {
		if err := f.place(projectDir, call); err != nil {
			return nil, err
		}
	}
	return &hub.InstallResult{EntryID: id, Name: id, Version: version, ArtType: entryType}, nil
}

func withInstaller(t *testing.T, inst artifactInstaller, openErr error) *fakeInstaller {
	t.Helper()
	previous := newInstaller
	t.Cleanup(func() { newInstaller = previous })
	newInstaller = func(context.Context) (artifactInstaller, error) {
		if openErr != nil {
			return nil, openErr
		}
		return inst, nil
	}
	if f, ok := inst.(*fakeInstaller); ok {
		return f
	}
	return nil
}

func prepareWith(t *testing.T, ide string, artifacts []livesearch.Artifact) (*livesearch.Session, []string, error) {
	t.Helper()
	mgr := livesearch.NewManager(t.TempDir(), nil, nil)
	t.Cleanup(mgr.CloseAll)
	s, err := mgr.Create(livesearch.Options{IDE: ide, Artifacts: artifacts})
	if err != nil {
		t.Fatalf("Create: %v", err)
	}
	var progress []string
	prepErr := Prepare(context.Background(), s, func(msg string) { progress = append(progress, msg) })
	return s, progress, prepErr
}

func TestArtifactsOfAnyTypeAreInstalledIntoTheWorkspace(t *testing.T) {
	isolateHome(t)
	fake := withInstaller(t, &fakeInstaller{}, nil)

	chosen := []livesearch.Artifact{
		{ID: "some-wiki", Type: string(hub.TypeKnowledge)},
		{ID: "some-graph", Type: string(hub.TypeAST), Version: "2.1.0"},
		{ID: "some-rule", Type: string(hub.TypeRule)},
		{ID: "some-skill", Type: string(hub.TypeSkill)},
		{ID: "some-mcp", Type: string(hub.TypeMCP)},
	}
	s, progress, err := prepareWith(t, "claude", chosen)
	if err != nil {
		t.Fatalf("Prepare: %v (progress: %v)", err, progress)
	}

	if len(fake.calls) != len(chosen) {
		t.Fatalf("installed %d artifacts, want %d: %+v", len(fake.calls), len(chosen), fake.calls)
	}
	for i, call := range fake.calls {
		if call.dir != s.WorkspaceDir() {
			t.Fatalf("artifact %d was installed into %q, want the workspace %q", i, call.dir, s.WorkspaceDir())
		}
		if call.ide != "claude" {
			t.Fatalf("artifact %d was installed for ide %q, want claude", i, call.ide)
		}
	}
	var sawPinned bool
	for _, call := range fake.calls {
		if call.entryID == "some-graph@2.1.0" {
			sawPinned = true
		}
	}
	if !sawPinned {
		t.Fatalf("the pinned version was dropped: %+v", fake.calls)
	}
}

func TestEachInstalledArtifactIsReported(t *testing.T) {
	isolateHome(t)
	withInstaller(t, &fakeInstaller{}, nil)

	_, progress, err := prepareWith(t, "claude", []livesearch.Artifact{
		{ID: "some-wiki", Type: string(hub.TypeKnowledge)},
	})
	if err != nil {
		t.Fatalf("Prepare: %v", err)
	}
	joined := strings.ToLower(strings.Join(progress, " | "))
	if !strings.Contains(joined, "fetching 1 artifact") {
		t.Fatalf("the fetch was not announced: %v", progress)
	}
	if !strings.Contains(joined, "installed knowledge some-wiki") {
		t.Fatalf("the install was not reported: %v", progress)
	}
}

func TestOneFailedArtifactDoesNotLoseTheOthers(t *testing.T) {
	isolateHome(t)
	withInstaller(t, &fakeInstaller{
		fail: map[string]error{"broken": errors.New("no such version")},
	}, nil)

	_, progress, err := prepareWith(t, "claude", []livesearch.Artifact{
		{ID: "broken", Type: string(hub.TypeKnowledge)},
		{ID: "fine", Type: string(hub.TypeKnowledge)},
	})
	if err != nil {
		t.Fatalf("one bad artifact failed the whole preparation: %v", err)
	}
	joined := strings.Join(progress, " | ")
	if !strings.Contains(joined, "broken") || !strings.Contains(joined, "no such version") {
		t.Fatalf("the failure was not reported: %v", progress)
	}
	if !strings.Contains(joined, "installed knowledge fine") {
		t.Fatalf("the good artifact was not installed: %v", progress)
	}
}

func TestASessionWithNoUsableArtifactFailsInsteadOfSearchingNothing(t *testing.T) {
	isolateHome(t)
	withInstaller(t, &fakeInstaller{
		fail: map[string]error{"gone": errors.New("not in the registry")},
	}, nil)

	_, _, err := prepareWith(t, "claude", []livesearch.Artifact{{ID: "gone", Type: string(hub.TypeKnowledge)}})
	if err == nil {
		t.Fatal("preparation succeeded with nothing installed")
	}
	if !strings.Contains(err.Error(), "none of the selected artifacts") {
		t.Fatalf("unhelpful error: %v", err)
	}
}

func TestAnUnreachableHubFailsPreparationLoudly(t *testing.T) {
	isolateHome(t)
	withInstaller(t, nil, errors.New("the hub registry is not available"))

	_, _, err := prepareWith(t, "claude", []livesearch.Artifact{{ID: "any", Type: string(hub.TypeKnowledge)}})
	if err == nil {
		t.Fatal("preparation succeeded without the hub")
	}
	if !strings.Contains(err.Error(), "not available") {
		t.Fatalf("unhelpful error: %v", err)
	}
}

func TestASessionWithNoArtifactsNeverOpensTheHub(t *testing.T) {
	isolateHome(t)
	withInstaller(t, nil, errors.New("the hub must not be opened"))

	_, _, err := prepareWith(t, "claude", nil)
	if err != nil {
		t.Fatalf("Prepare: %v", err)
	}
}

func TestAnMCPArtifactsServersReachTheProjectConfigAndThePermissions(t *testing.T) {
	isolateHome(t)

	artifactDir := t.TempDir()
	extra := map[string]any{
		"weather-mcp": map[string]any{"command": "weather-server", "args": []string{"--stdio"}},
	}
	data, err := json.Marshal(extra)
	if err != nil {
		t.Fatalf("setup: %v", err)
	}
	if err := os.WriteFile(filepath.Join(artifactDir, "mcp.json"), data, 0o600); err != nil {
		t.Fatalf("setup: %v", err)
	}

	withInstaller(t, &fakeInstaller{place: func(projectDir string, call installCall) error {
		lockPath := filepath.Join(projectDir, brand.LockFileName())
		lf, err := hub.LoadLockfile(lockPath)
		if err != nil || lf == nil {
			return fmt.Errorf("the lockfile must exist before an artifact is installed: %w", err)
		}
		if lf.Artifacts[call.artType] == nil {
			lf.Artifacts[call.artType] = map[string]*hub.LockfileArtifactMeta{}
		}
		lf.Artifacts[call.artType][call.entryID] = &hub.LockfileArtifactMeta{
			Version:    "1.0.0",
			Origin:     "hub",
			LinkSource: artifactDir,
		}
		return hub.SaveLockfile(lockPath, lf)
	}}, nil)

	s, progress, err := prepareWith(t, "claude", []livesearch.Artifact{
		{ID: "weather", Type: string(hub.TypeMCP)},
	})
	if err != nil {
		t.Fatalf("Prepare: %v (progress: %v)", err, progress)
	}

	mcpRaw, err := os.ReadFile(filepath.Join(s.WorkspaceDir(), ".mcp.json"))
	if err != nil {
		t.Fatalf("reading the MCP config: %v", err)
	}
	var conf struct {
		MCPServers map[string]any `json:"mcpServers"`
	}
	if err := json.Unmarshal(mcpRaw, &conf); err != nil {
		t.Fatalf("decoding the MCP config: %v", err)
	}
	if _, ok := conf.MCPServers["weather-mcp"]; !ok {
		var names []string
		for k := range conf.MCPServers {
			names = append(names, k)
		}
		sort.Strings(names)
		t.Fatalf("the artifact's server is missing from the project config: %v", names)
	}
	if _, ok := conf.MCPServers[brand.MCPServerName("code-stdio")]; !ok {
		t.Fatal("the graphit server was dropped when the artifact's was added")
	}

	permRaw, err := os.ReadFile(filepath.Join(s.WorkspaceDir(), ".claude", "settings.local.json"))
	if err != nil {
		t.Fatalf("reading the permissions: %v", err)
	}
	var settings struct {
		Permissions struct {
			Allow []string `json:"allow"`
		} `json:"permissions"`
	}
	if err := json.Unmarshal(permRaw, &settings); err != nil {
		t.Fatalf("decoding the permissions: %v", err)
	}
	for _, want := range []string{"mcp__weather-mcp", "mcp__" + brand.MCPServerName("code-stdio")} {
		var found bool
		for _, a := range settings.Permissions.Allow {
			if a == want {
				found = true
			}
		}
		if !found {
			t.Fatalf("%s is not allowed: %v", want, settings.Permissions.Allow)
		}
	}
}

func TestInstalledArtifactsReadsTheProjectsOwnLockfile(t *testing.T) {
	isolateHome(t)
	ws := t.TempDir()

	if got, err := hub.InstalledArtifacts("claude", ws); err != nil || len(got) != 0 {
		t.Fatalf("a project with no lockfile reported %v, %v — want an empty map and no error", got, err)
	}

	lf := &hub.Lockfile{
		Project: hub.ProjectIdentity{ID: "PROJ", Name: "n"},
		Artifacts: map[hub.ArtifactType]map[string]*hub.LockfileArtifactMeta{
			hub.TypeSkill: {"a-skill": {Version: "1.2.3", LinkSource: "/somewhere"}},
		},
	}
	if err := hub.SaveLockfile(filepath.Join(ws, brand.LockFileName()), lf); err != nil {
		t.Fatalf("setup: %v", err)
	}

	got, err := hub.InstalledArtifacts("claude", ws)
	if err != nil {
		t.Fatalf("InstalledArtifacts: %v", err)
	}
	entry, ok := got["a-skill"]
	if !ok {
		t.Fatalf("the skill is missing: %v", got)
	}
	if entry["type"] != string(hub.TypeSkill) || entry["version"] != "1.2.3" {
		t.Fatalf("unexpected entry: %v", entry)
	}
	if entry["project_id"] != "PROJ" {
		t.Fatalf("the entry carries project_id %q, want PROJ", entry["project_id"])
	}
}

func TestCountNounReadsProperly(t *testing.T) {
	if got := countNoun(1, "artifact"); got != "1 artifact" {
		t.Fatalf("countNoun(1) = %q", got)
	}
	if got := countNoun(3, "artifact"); got != "3 artifacts" {
		t.Fatalf("countNoun(3) = %q", got)
	}
}

func TestDescribeArtifactOmitsWhatWasNotGiven(t *testing.T) {
	cases := []struct {
		in   livesearch.Artifact
		want string
	}{
		{livesearch.Artifact{ID: "acme"}, "acme"},
		{livesearch.Artifact{ID: "acme", Version: "1.0"}, "acme@1.0"},
		{livesearch.Artifact{ID: "acme", Type: "knowledge"}, "acme (knowledge)"},
		{livesearch.Artifact{ID: "acme", Type: "ast", Version: "2.1"}, "acme@2.1 (ast)"},
	}
	for _, c := range cases {
		if got := describeArtifact(c.in); got != c.want {
			t.Fatalf("describeArtifact(%+v) = %q, want %q", c.in, got, c.want)
		}
	}
}
