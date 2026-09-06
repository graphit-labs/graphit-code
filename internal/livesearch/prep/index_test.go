package prep

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/livesearch"
	"github.com/graphit-labs/graphit-code/internal/store"
)

func seedKnowledgeArtifact(t *testing.T, ws, id string, pages map[string]string) {
	t.Helper()
	dir := filepath.Join(store.KnowledgeContextDir(id), "docs")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatalf("seeding %s: %v", id, err)
	}
	for name, body := range pages {
		if err := os.WriteFile(filepath.Join(dir, name), []byte(body), 0o600); err != nil {
			t.Fatalf("seeding %s/%s: %v", id, name, err)
		}
	}
	if err := store.AddContext(ws, store.KindKnowledge, store.ContextRecord{Name: id}); err != nil {
		t.Fatalf("registering %s: %v", id, err)
	}
}

func newBareSession(t *testing.T, ide string) *livesearch.Session {
	t.Helper()
	mgr := livesearch.NewManager(t.TempDir(), nil, nil)
	t.Cleanup(mgr.CloseAll)
	s, err := mgr.Create(livesearch.Options{IDE: ide})
	if err != nil {
		t.Fatalf("Create: %v", err)
	}
	if err := writeLockfile(s.WorkspaceDir(), ide, s.ID()); err != nil {
		t.Fatalf("writeLockfile: %v", err)
	}
	return s
}

func TestTheSessionReadsEachSetWhereItAlreadySits(t *testing.T) {
	isolateHome(t)
	s := newBareSession(t, "claude")
	ws := s.WorkspaceDir()

	seedKnowledgeArtifact(t, ws, "acme-docs", map[string]string{
		"overview.md": "# Overview\n\nAcme ships widgets to three continents.\n",
	})

	var progress []string
	if err := prepareIndexes(context.Background(), ws, func(m string) { progress = append(progress, m) }); err != nil {
		t.Fatalf("prepareIndexes: %v", err)
	}

	if got, want := knowledge.ReadDirIn(ws, "acme-docs"), store.KnowledgeContextDir("acme-docs"); got != want {
		t.Errorf("the set resolves to %q, want its context store %q", got, want)
	}
	if got := knowledge.ReadDirIn(ws, ""); got != "" {
		t.Errorf("an unqualified read resolved to %q; a session has no documentation wiki of its own", got)
	}
	if _, err := os.Stat(store.KnowledgeProjectDir(ws)); err == nil {
		t.Errorf("preparation compiled a wiki for the session")
	}
	if !strings.Contains(strings.Join(progress, " | "), "acme-docs") {
		t.Errorf("preparation never named the set it can search: %v", progress)
	}
}

func TestEveryChosenDocumentationSetIsReachable(t *testing.T) {
	isolateHome(t)
	s := newBareSession(t, "claude")
	ws := s.WorkspaceDir()

	seedKnowledgeArtifact(t, ws, "alpha-docs", map[string]string{
		"alpha.md": "# Alpha\n\nAlpha handles authentication with rotating tokens.\n",
	})
	seedKnowledgeArtifact(t, ws, "beta-docs", map[string]string{
		"beta.md": "# Beta\n\nBeta handles billing and invoices.\n",
	})

	var progress []string
	if err := prepareIndexes(context.Background(), ws, func(m string) { progress = append(progress, m) }); err != nil {
		t.Fatalf("prepareIndexes: %v", err)
	}

	for _, name := range []string{"alpha-docs", "beta-docs"} {
		if got := knowledge.ReadDirIn(ws, name); got != store.KnowledgeContextDir(name) {
			t.Errorf("%s is not reachable: got %q", name, got)
		}
	}
	if !strings.Contains(strings.Join(progress, " | "), "2 documentation sets") {
		t.Fatalf("both sets should have been announced: %v", progress)
	}
}

func TestASetNobodySelectedIsNotClaimedBySession(t *testing.T) {
	isolateHome(t)
	s := newBareSession(t, "claude")
	ws := s.WorkspaceDir()

	seedKnowledgeArtifact(t, ws, "chosen-docs", map[string]string{
		"chosen.md": "# Chosen\n\nThis one was selected for the session.\n",
	})

	stranger := filepath.Join(store.KnowledgeContextDir("someone-elses-docs"), "docs")
	if err := os.MkdirAll(stranger, 0o755); err != nil {
		t.Fatalf("setup: %v", err)
	}
	if err := os.WriteFile(filepath.Join(stranger, "private.md"),
		[]byte("# Private\n\nAnother project installed this.\n"), 0o600); err != nil {
		t.Fatalf("setup: %v", err)
	}

	if err := prepareIndexes(context.Background(), ws, func(string) {}); err != nil {
		t.Fatalf("prepareIndexes: %v", err)
	}

	installed := knowledge.InstalledContextsIn(ws)
	if len(installed) != 1 || installed[0] != "chosen-docs" {
		t.Fatalf("the session should claim exactly the selected set, got %v", installed)
	}
}

func TestNoDocumentationMeansNoWikiAndNoComplaint(t *testing.T) {
	isolateHome(t)
	s := newBareSession(t, "claude")
	ws := s.WorkspaceDir()

	var progress []string
	if err := prepareIndexes(context.Background(), ws, func(m string) { progress = append(progress, m) }); err != nil {
		t.Fatalf("prepareIndexes: %v", err)
	}
	if _, err := os.Stat(store.KnowledgeProjectDir(ws)); !os.IsNotExist(err) {
		t.Fatalf("a wiki directory was created with nothing to put in it: %v", err)
	}
	joined := strings.Join(progress, " | ")
	if strings.Contains(joined, "compiling") {
		t.Fatalf("compilation was announced with nothing to compile: %v", progress)
	}
}

func TestCodeGraphsAreReportedByNameOnceAddressable(t *testing.T) {
	isolateHome(t)
	s := newBareSession(t, "claude")
	ws := s.WorkspaceDir()

	const (
		artifactID = "acme-graph"
		hubProject = "01HYPROJECT0000000000000"
		version    = "3.2.1"
	)
	lf := &hub.Lockfile{
		Project: hub.ProjectIdentity{ID: s.ID(), Name: "live"},
		Artifacts: map[hub.ArtifactType]map[string]*hub.LockfileArtifactMeta{
			hub.TypeAST: {artifactID: {Version: version, ProjectID: hubProject, Origin: "hub"}},
		},
	}
	if err := hub.SaveLockfile(filepath.Join(ws, brand.LockFileName()), lf); err != nil {
		t.Fatalf("setup: %v", err)
	}

	contextID := ast.HubContextID(hubProject)
	storeDir := ast.HubContextDir(contextID, version)
	if err := os.MkdirAll(storeDir, 0o755); err != nil {
		t.Fatalf("setup: %v", err)
	}
	if err := os.WriteFile(filepath.Join(storeDir, "schema.cypher"), []byte("// mount"), 0o644); err != nil {
		t.Fatalf("setup schema: %v", err)
	}

	var progress []string
	if err := prepareIndexes(context.Background(), ws, func(m string) { progress = append(progress, m) }); err != nil {
		t.Fatalf("prepareIndexes: %v", err)
	}

	joined := strings.Join(progress, " | ")
	if !strings.Contains(joined, contextID) {
		t.Fatalf("the graph context %q was not reported: %v", contextID, progress)
	}
	if !strings.Contains(joined, "1 code graph") {
		t.Fatalf("the graph count was not reported: %v", progress)
	}
}

func TestAGraphWhoseStoreWasNeverBuiltIsNotOffered(t *testing.T) {
	isolateHome(t)
	s := newBareSession(t, "claude")
	ws := s.WorkspaceDir()

	lf := &hub.Lockfile{
		Project: hub.ProjectIdentity{ID: s.ID(), Name: "live"},
		Artifacts: map[hub.ArtifactType]map[string]*hub.LockfileArtifactMeta{
			hub.TypeAST: {"ghost-graph": {Version: "1.0.0", ProjectID: "01HGHOST000000000000000", Origin: "hub"}},
		},
	}
	if err := hub.SaveLockfile(filepath.Join(ws, brand.LockFileName()), lf); err != nil {
		t.Fatalf("setup: %v", err)
	}

	var progress []string
	if err := prepareIndexes(context.Background(), ws, func(m string) { progress = append(progress, m) }); err != nil {
		t.Fatalf("prepareIndexes: %v", err)
	}
	if strings.Contains(strings.Join(progress, " | "), "code graph") {
		t.Fatalf("a graph with no store was offered: %v", progress)
	}
}

func TestUserMemoryIsReadInPlaceAndNotCopiedIntoTheWorkspace(t *testing.T) {
	isolateHome(t)
	s := newBareSession(t, "claude")
	ws := s.WorkspaceDir()

	prepareUserMemory(ws, func(string) {})

	entries, err := os.ReadDir(filepath.Join(ws, brand.DotDir()))
	if err != nil {
		return
	}
	for _, e := range entries {
		if e.Name() == "memory" {
			t.Fatalf("a memory directory was created inside the workspace at %s", filepath.Join(ws, brand.DotDir(), "memory"))
		}
	}
}

func TestUserMemoryDegradesWithAReasonRatherThanFailing(t *testing.T) {
	isolateHome(t)
	s := newBareSession(t, "claude")

	var progress []string
	prepareUserMemory(s.WorkspaceDir(), func(m string) { progress = append(progress, m) })

	if len(progress) == 0 {
		t.Fatal("the memory step said nothing at all")
	}
	joined := strings.ToLower(strings.Join(progress, " | "))
	if !strings.Contains(joined, "memory") {
		t.Fatalf("the memory step reported something unrelated: %v", progress)
	}
}

func TestPreparingASessionCompilesNothingOfItsOwn(t *testing.T) {
	isolateHome(t)
	withInstaller(t, &fakeInstaller{}, nil)

	mgr := livesearch.NewManager(t.TempDir(), nil, nil)
	t.Cleanup(mgr.CloseAll)
	s, err := mgr.Create(livesearch.Options{IDE: "claude"})
	if err != nil {
		t.Fatalf("Create: %v", err)
	}
	ws := s.WorkspaceDir()
	if err := writeLockfile(ws, "claude", s.ID()); err != nil {
		t.Fatalf("writeLockfile: %v", err)
	}
	seedKnowledgeArtifact(t, ws, "acme-docs", map[string]string{"a.md": "# A\n\nbody\n"})

	var progress []string
	if err := Prepare(context.Background(), s, func(m string) { progress = append(progress, m) }); err != nil {
		t.Fatalf("Prepare: %v (progress: %v)", err, progress)
	}

	for _, dir := range []string{
		store.KnowledgeProjectDirByID(s.ID()),
		store.ASTProjectDirByID(s.ID()),
		store.MemoryWikiDir("project", s.ID()),
		store.MemoryTableDir("project", s.ID()),
	} {
		if _, err := os.Stat(dir); err == nil {
			t.Errorf("the session acquired a store of its own at %s", dir)
		}
	}

	if !strings.Contains(strings.Join(progress, "\n"), "acme-docs") {
		t.Errorf("preparation never named the documentation set: %v", progress)
	}
}

func TestThePreparedWorkspaceIsSelfDescribing(t *testing.T) {
	isolateHome(t)
	withInstaller(t, &fakeInstaller{}, nil)

	mgr := livesearch.NewManager(t.TempDir(), nil, nil)
	t.Cleanup(mgr.CloseAll)
	s, err := mgr.Create(livesearch.Options{IDE: "claude"})
	if err != nil {
		t.Fatalf("Create: %v", err)
	}
	ws := s.WorkspaceDir()
	if err := writeLockfile(ws, "claude", s.ID()); err != nil {
		t.Fatalf("writeLockfile: %v", err)
	}
	seedKnowledgeArtifact(t, ws, "acme-docs", map[string]string{"a.md": "# A\n\nbody\n"})

	var progress []string
	if err := Prepare(context.Background(), s, func(m string) { progress = append(progress, m) }); err != nil {
		t.Fatalf("Prepare: %v (progress: %v)", err, progress)
	}

	for _, rel := range []string{
		brand.LockFileName(),
		".mcp.json",
		filepath.Join(".claude", "settings.json"),
		filepath.Join(".claude", "settings.local.json"),
	} {
		if _, err := os.Stat(filepath.Join(ws, rel)); err != nil {
			t.Fatalf("%s is missing from the prepared workspace: %v (progress: %v)", rel, err, progress)
		}
	}
	if _, err := os.Stat(store.KnowledgeProjectDir(ws)); err == nil {
		t.Fatalf("the session was given a documentation wiki of its own (progress: %v)", progress)
	}

	data, err := os.ReadFile(filepath.Join(ws, brand.LockFileName()))
	if err != nil {
		t.Fatalf("reading the lockfile: %v", err)
	}
	var lf hub.Lockfile
	if err := json.Unmarshal(data, &lf); err != nil {
		t.Fatalf("decoding the lockfile: %v", err)
	}
	if lf.Project.ID != s.ID() {
		t.Fatalf("the lockfile identity is %q, want the session ID %q", lf.Project.ID, s.ID())
	}
}
