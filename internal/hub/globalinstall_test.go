package hub

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/store"
)

func registryWith(entries ...*Entry) *RegistryManager {
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	for _, e := range entries {
		if m.entries[e.Type] == nil {
			m.entries[e.Type] = make(map[string]*Entry)
		}
		m.entries[e.Type][e.ID] = e
	}
	return m
}

func globalInstallHome(t *testing.T) string {
	t.Helper()
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
	return home
}

func readGlobalLock(t *testing.T) map[string]*GlobalArtifact {
	t.Helper()
	mgr, err := NewGlobalLockManager()
	if err != nil {
		t.Fatalf("global lock: %v", err)
	}
	lock, err := mgr.Load()
	if err != nil {
		t.Fatalf("global lock: %v", err)
	}
	return lock.Artifacts
}

func serviceWithGlobalLock(t *testing.T, reg *RegistryManager) *HubService {
	t.Helper()
	mgr, err := NewGlobalLockManager()
	if err != nil {
		t.Fatalf("global lock: %v", err)
	}
	return &HubService{registry: reg, lockMgr: mgr}
}

// The install must record membership in the global lock and touch NO project — including
// the one the process happens to be sitting in, which is the trap: paths.GetPathsForProject
// with both arguments empty walks up from the working directory.
func TestAProjectlessInstallRecordsGloballyAndTouchesNoProject(t *testing.T) {
	globalInstallHome(t)

	bystander := t.TempDir()
	t.Chdir(bystander)

	svc := serviceWithGlobalLock(t, registryWith(&Entry{
		ID:     "demo-rule",
		Name:   "Demo Rule",
		Type:   TypeRule,
		Latest: "1.0.0",
		Hashes: map[string]string{"1.0.0": "deadbeef"},
	}))

	res, err := svc.Install(context.Background(), "demo-rule@1.0.0", "", "", TypeRule, "", "")
	if err != nil {
		t.Fatalf("global install failed: %v", err)
	}
	if res.Version != "1.0.0" {
		t.Errorf("version = %q, want 1.0.0", res.Version)
	}

	arts := readGlobalLock(t)
	art := arts["rule/demo-rule@1.0.0"]
	if art == nil {
		t.Fatalf("the global lock has no entry for the install; it holds %v", keysOfArtifacts(arts))
	}
	if _, ok := art.Projects[store.GlobalOwnerKey]; !ok {
		t.Errorf("owners = %v, want the reserved %q owner", keysOfProjects(art.Projects), store.GlobalOwnerKey)
	}
	if got := art.Projects[store.GlobalOwnerKey].ProjectDir; got != "" {
		t.Errorf("owner dir = %q, want empty — a global owner has no project directory", got)
	}

	if _, err := os.Stat(filepath.Join(bystander, brand.LockFileName())); !os.IsNotExist(err) {
		t.Error("a lockfile was created in the working directory: the install bound itself to a project it was not given")
	}
}

// The publishing project is half of a store's address. A project-scoped install has it in
// its own lockfile; a project-less one has only this entry, so the field must be written.
func TestAProjectlessInstallRecordsThePublishingProject(t *testing.T) {
	globalInstallHome(t)
	t.Chdir(t.TempDir())

	svc := serviceWithGlobalLock(t, registryWith(&Entry{
		ID:        "demo-ast",
		Name:      "Demo AST",
		Type:      TypeAST,
		Latest:    "2.1.0",
		ProjectID: testProjectOne,
	}))

	if _, err := svc.Install(context.Background(), "demo-ast@2.1.0", "", "", TypeAST, "", ""); err != nil {
		t.Fatalf("global install failed: %v", err)
	}

	art := readGlobalLock(t)["ast/demo-ast@2.1.0"]
	if art == nil {
		t.Fatal("no global lock entry")
	}
	if art.ProjectID != testProjectOne {
		t.Errorf("projectId = %q, want the publishing project — without it the store cannot be addressed", art.ProjectID)
	}

	rec, ok := store.LookupContext("", store.KindAST, "demo-ast@2.1.0")
	if !ok {
		t.Fatal("the install is not resolvable as a global context")
	}
	if want := store.ASTHubDir(testProjectOne, "2.1.0"); store.ASTContextDirIn("", "demo-ast@2.1.0") != want {
		t.Errorf("store = %q, want %q", store.ASTContextDirIn("", "demo-ast@2.1.0"), want)
	}
	if rec.Name != testProjectOne {
		t.Errorf("context name = %q, want the publishing project", rec.Name)
	}
}

// A project-scoped install must keep writing the project's lockfile, and must NOT claim the
// reserved global owner: that is what would let any project-less caller read it.
func TestAProjectScopedInstallIsStillClaimedByTheProject(t *testing.T) {
	globalInstallHome(t)

	projectDir := t.TempDir()
	if err := SaveLockfile(filepath.Join(projectDir, brand.LockFileName()), &Lockfile{
		Project: ProjectIdentity{ID: "01PROJECT", Name: "Proj"},
	}); err != nil {
		t.Fatalf("setup: %v", err)
	}

	svc := serviceWithGlobalLock(t, registryWith(&Entry{
		ID: "demo-rule", Name: "Demo Rule", Type: TypeRule, Latest: "1.0.0",
	}))

	if _, err := svc.Install(context.Background(), "demo-rule@1.0.0", "", "claude", TypeRule, "", projectDir); err != nil {
		t.Fatalf("install failed: %v", err)
	}
	if _, err := os.Stat(filepath.Join(projectDir, ".claude", "rules", "demo-rule.md")); !os.IsNotExist(err) {
		t.Fatalf("project install materialized a rule instead of leaving it for the hook: %v", err)
	}

	lf, err := LoadLockfile(filepath.Join(projectDir, brand.LockFileName()))
	if err != nil || lf == nil {
		t.Fatalf("reading the project lockfile: %v", err)
	}
	if lf.Artifacts[TypeRule]["demo-rule"] == nil {
		t.Error("the project's lockfile does not claim the artifact")
	}

	art := readGlobalLock(t)["rule/demo-rule@1.0.0"]
	if art == nil {
		t.Fatal("no global lock entry")
	}
	if _, ok := art.Projects[store.GlobalOwnerKey]; ok {
		t.Error("a project-scoped install claimed the reserved global owner: every project-less caller could now read it")
	}
	if _, ok := art.Projects["01PROJECT"]; !ok {
		t.Errorf("owners = %v, want the project", keysOfProjects(art.Projects))
	}
	if _, ok := store.LookupContext("", store.KindAST, "demo-rule"); ok {
		t.Error("a project-scoped install resolved in the global scope")
	}
}

func TestUninstallGlobalDropsTheGlobalClaim(t *testing.T) {
	globalInstallHome(t)
	t.Chdir(t.TempDir())

	reg := registryWith(&Entry{ID: "demo-rule", Name: "Demo Rule", Type: TypeRule, Latest: "1.0.0"})
	svc := serviceWithGlobalLock(t, reg)

	if _, err := svc.Install(context.Background(), "demo-rule@1.0.0", "", "", TypeRule, "", ""); err != nil {
		t.Fatalf("install failed: %v", err)
	}
	if readGlobalLock(t)["rule/demo-rule@1.0.0"] == nil {
		t.Fatal("setup: the install was not recorded")
	}

	if err := svc.Uninstall(context.Background(), "demo-rule", TypeRule, true, "", ""); err != nil {
		t.Fatalf("global uninstall failed: %v", err)
	}
	if art := readGlobalLock(t)["rule/demo-rule@1.0.0"]; art != nil {
		t.Errorf("the entry survived the uninstall with owners %v", keysOfProjects(art.Projects))
	}
}

func TestUninstallGlobalRefusesWhatWasNeverInstalledGlobally(t *testing.T) {
	globalInstallHome(t)
	t.Chdir(t.TempDir())

	svc := serviceWithGlobalLock(t, registryWith())
	err := svc.Uninstall(context.Background(), "demo-rule", TypeRule, true, "", "")
	if err == nil {
		t.Fatal("expected a refusal")
	}
}

// ValidateProjectDirs prunes owners whose project directory is gone. A global owner has no
// directory at all, and joining "" with the lockfile name yields a RELATIVE path — so the
// unguarded check deleted exactly the entries that are nobody's to validate.
func TestValidateProjectDirsKeepsOwnersThatHaveNoDirectory(t *testing.T) {
	globalInstallHome(t)
	t.Chdir(t.TempDir())

	mgr, err := NewGlobalLockManager()
	if err != nil {
		t.Fatalf("setup: %v", err)
	}
	if _, err := mgr.RegisterInstall(InstallRecord{
		ID: "demo-rule", Version: "1.0.0", Type: TypeRule, Owner: store.GlobalOwnerKey,
	}); err != nil {
		t.Fatalf("setup: %v", err)
	}
	if _, err := mgr.RegisterInstall(InstallRecord{
		ID: "gone-rule", Version: "1.0.0", Type: TypeRule,
		Owner: "01GONE", OwnerDir: filepath.Join(t.TempDir(), "deleted"),
	}); err != nil {
		t.Fatalf("setup: %v", err)
	}

	if _, err := mgr.ValidateProjectDirs(); err != nil {
		t.Fatalf("validate: %v", err)
	}

	arts := readGlobalLock(t)
	if arts["rule/demo-rule@1.0.0"] == nil {
		t.Error("the global install was pruned; it has no project directory to be stale")
	}
	if arts["rule/gone-rule@1.0.0"] != nil {
		t.Error("an install whose project directory is gone should have been pruned")
	}
}

// The global lock is written by this package and read by internal/store, which cannot import
// it. The two shapes therefore have to agree, and this is the test that notices when a field
// is renamed on one side only.
func TestTheGlobalLockShapeIsWhatTheStoreReaderExpects(t *testing.T) {
	globalInstallHome(t)
	t.Chdir(t.TempDir())

	svc := serviceWithGlobalLock(t, registryWith(&Entry{
		ID: "demo-kb", Name: "Demo KB", Type: TypeKnowledge, Latest: "1.0.0", ProjectID: testProjectOne,
	}))
	if _, err := svc.Install(context.Background(), "demo-kb@1.0.0", "", "", TypeKnowledge, "", ""); err != nil {
		t.Fatalf("install failed: %v", err)
	}

	raw, err := os.ReadFile(filepath.Join(brand.GlobalDir(), GlobalHubLockFile))
	if err != nil {
		t.Fatalf("reading the global lock: %v", err)
	}
	var probe struct {
		Artifacts map[string]struct {
			ID        string                     `json:"id"`
			Version   string                     `json:"version"`
			Type      string                     `json:"type"`
			ProjectID string                     `json:"projectId"`
			Projects  map[string]json.RawMessage `json:"projects"`
		} `json:"artifacts"`
	}
	if err := json.Unmarshal(raw, &probe); err != nil {
		t.Fatalf("the global lock is not readable with the field names internal/store uses: %v", err)
	}
	entry, ok := probe.Artifacts["knowledge/demo-kb@1.0.0"]
	if !ok {
		t.Fatal("the artifact key is not the one the reader expects")
	}
	if entry.ID != "demo-kb" || entry.Version != "1.0.0" || entry.Type != "knowledge" || entry.ProjectID != "01PUB" {
		t.Errorf("decoded entry = %+v; one of the field names the store reader relies on has drifted", entry)
	}
	if _, ok := entry.Projects[store.GlobalOwnerKey]; !ok {
		t.Error("the reserved owner key is not where the store reader looks for it")
	}
}

func keysOfArtifacts(m map[string]*GlobalArtifact) []string {
	out := make([]string, 0, len(m))
	for k := range m {
		out = append(out, k)
	}
	return out
}

func keysOfProjects(m map[string]*ProjectInstall) []string {
	out := make([]string, 0, len(m))
	for k := range m {
		out = append(out, k)
	}
	return out
}
