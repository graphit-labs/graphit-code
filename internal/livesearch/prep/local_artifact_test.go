//go:build lancedb

package prep

import (
	"context"
	"errors"
	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/livesearch"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/wiki"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

type localSourceRunner struct{ result chan error }

func (f *localSourceRunner) Complete(context.Context, string, string) (string, error) {
	return "", errors.New("unused")
}
func (f *localSourceRunner) SupportsStructuredStream() bool { return true }
func (f *localSourceRunner) CompleteStream(ctx context.Context, req ai.StreamRequest, emit ai.EventFunc) (*ai.StreamResult, error) {
	names := knowledge.InstalledContextsIn(req.WorkDir)
	if len(names) != 1 {
		err := errors.New("selected wiki not visible to runner")
		f.result <- err
		return nil, err
	}
	db, err := wiki.OpenWikiDB(ctx, knowledge.ReadDirIn(req.WorkDir, names[0]))
	if err != nil {
		f.result <- err
		return nil, err
	}
	defer db.Close()
	page, err := db.Chunk(ctx, "contract")
	if err == nil && page.Body != "Durable engineering knowledge" {
		err = errors.New("wrong source body")
	}
	if err == nil {
		paths, _ := filepath.Glob(filepath.Join(req.WorkDir, ".claude", "skills", "*", "support.txt"))
		if len(paths) != 1 {
			err = errors.New("skill resources were not materialized for runner")
		}
		if len(paths) == 1 {
			script, statErr := os.Stat(filepath.Join(filepath.Dir(paths[0]), "check.sh"))
			if statErr != nil || script.Mode().Perm()&0111 == 0 {
				err = errors.New("skill executable permissions were not preserved")
			}
		}
	}
	f.result <- err
	return &ai.StreamResult{Text: "Read selected project sources", Structured: true}, err
}

func TestProjectArtifactsPrepareWithoutHubAndReachRunner(t *testing.T) {
	isolateHome(t)
	ctx := context.Background()
	dir := t.TempDir()
	wikiDir, err := store.EnsureKnowledgeProjectDir(dir)
	if err != nil {
		t.Fatal(err)
	}
	id := store.ProjectID(dir)
	if err := wiki.SyncDB(ctx, wikiDir, []wiki.WikiChunk{{Slug: "contract", Title: "Contract", Body: "Durable engineering knowledge"}}, nil, nil); err != nil {
		t.Fatal(err)
	}
	skill := filepath.Join(dir, ".claude", "skills", "review")
	if err := os.MkdirAll(skill, 0755); err != nil {
		t.Fatal(err)
	}
	for name, body := range map[string]string{"SKILL.md": "---\nname: review\ndescription: Read support\n---\nRead support.txt", "support.txt": "source resource"} {
		if err := os.WriteFile(filepath.Join(skill, name), []byte(body), 0644); err != nil {
			t.Fatal(err)
		}
	}
	if err := os.WriteFile(filepath.Join(skill, "check.sh"), []byte("#!/bin/sh\nexit 0\n"), 0755); err != nil {
		t.Fatal(err)
	}
	manager, _ := hub.NewGlobalLockManager()
	if err := manager.RegisterProject(id, dir); err != nil {
		t.Fatal(err)
	}
	before, _ := os.ReadFile(filepath.Join(dir, brand.LockFileName()))
	catalog, err := ProjectCatalog("claude")
	if err != nil {
		t.Fatal(err)
	}
	refs := []livesearch.Artifact{}
	for _, item := range catalog {
		if item.Type == "knowledge" || item.Type == "skill" {
			refs = append(refs, item.Artifact)
		}
	}
	if len(refs) != 2 {
		t.Fatalf("catalog missing local sources: %+v", catalog)
	}
	withInstaller(t, nil, errors.New("Hub must not open for local sources"))
	runner := &localSourceRunner{result: make(chan error, 1)}
	m := livesearch.NewManager(t.TempDir(), runner, Prepare)
	defer m.CloseAll()
	s, err := m.Create(livesearch.Options{Agent: "claude", Artifacts: refs, Prompt: "inspect selected sources"})
	if err != nil {
		t.Fatal(err)
	}
	select {
	case err := <-runner.result:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(10 * time.Second):
		t.Fatal("runner did not execute")
	}
	waitReady(t, s)
	after, _ := os.ReadFile(filepath.Join(dir, brand.LockFileName()))
	if string(before) != string(after) {
		t.Fatal("source lock changed")
	}
	data, _ := os.ReadFile(filepath.Join(skill, "support.txt"))
	if string(data) != "source resource" {
		t.Fatal("source changed")
	}
	installed, _ := hub.InstalledArtifacts("claude", s.WorkspaceDir())
	for _, entry := range installed {
		if entry["type"] == "skill" && !strings.HasPrefix(entry["path"], s.WorkspaceDir()+string(os.PathSeparator)) {
			t.Fatal("skill still points at mutable source")
		}
	}
	forged := refs[0]
	forged.InstanceID = "arbitrary-path"
	if _, err := resolveProjectArtifact("claude", forged); err == nil {
		t.Fatal("forged instance accepted")
	}
	if _, err := resolveProjectArtifact("codex", refs[1]); err == nil && refs[1].Type == "skill" {
		t.Fatal("other agent's unlisted skill accepted")
	}
}

func TestLocalPackageRejectsSymlinkResources(t *testing.T) {
	root := t.TempDir()
	target := filepath.Join(t.TempDir(), "private")
	if err := os.WriteFile(target, []byte("secret"), 0600); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(target, filepath.Join(root, "escape")); err != nil {
		t.Skip(err)
	}
	if err := copyLocalPackage(context.Background(), root, t.TempDir(), "skill", false); err == nil {
		t.Fatal("symlink accepted")
	}
}

func TestLocalSkillAliasPreservesMetadataAndBody(t *testing.T) {
	item := ProjectArtifact{Artifact: livesearch.Artifact{ID: strings.Repeat("a", 64), Type: "skill", ProjectID: "p", InstanceID: "i", ProjectKind: "file"}}
	name := projectArtifactAlias(item)
	if len(name) > 64 {
		t.Fatalf("skill name too long: %s", name)
	}
	dir := t.TempDir()
	original := "---\nname: old\ndescription: Review evidence\nallowed-tools: Read\n---\n\nRead `support.txt`.\n"
	if err := os.WriteFile(filepath.Join(dir, "SKILL.md"), []byte(original), 0644); err != nil {
		t.Fatal(err)
	}
	if err := renameLocalSkill(dir, name); err != nil {
		t.Fatal(err)
	}
	data, _ := os.ReadFile(filepath.Join(dir, "SKILL.md"))
	text := string(data)
	for _, want := range []string{"name: " + name, "allowed-tools: Read", "description: Review evidence", "Read `support.txt`."} {
		if !strings.Contains(text, want) {
			t.Fatalf("lost %s: %s", want, text)
		}
	}
}

func TestConflictingPublishedVersionsFailBeforeInstall(t *testing.T) {
	isolateHome(t)
	dir := t.TempDir()
	id, err := store.EnsureProjectID(dir)
	if err != nil {
		t.Fatal(err)
	}
	lf, err := hub.LoadLockfile(filepath.Join(dir, brand.LockFileName()))
	if err != nil {
		t.Fatal(err)
	}
	lf.Artifacts = map[hub.ArtifactType]map[string]*hub.LockfileArtifactMeta{hub.TypeKnowledge: {"published": {Origin: "hub", Version: "1", ProjectID: "01ARZ3NDEKTSV4RRFFQ69G5FA0"}}}
	if err := hub.SaveLockfile(filepath.Join(dir, brand.LockFileName()), lf); err != nil {
		t.Fatal(err)
	}
	manager, _ := hub.NewGlobalLockManager()
	if err := manager.RegisterProject(id, dir); err != nil {
		t.Fatal(err)
	}
	entries, err := ProjectCatalog("claude")
	if err != nil || len(entries) != 1 {
		t.Fatalf("catalog: %+v %v", entries, err)
	}
	if !entries[0].BelongsTo(dir) || entries[0].BelongsTo(t.TempDir()) {
		t.Fatal("active project filter lost")
	}
	err = validateSelectedVersions("claude", []livesearch.Artifact{entries[0].Artifact, {ID: "published", Type: "knowledge", Version: "2", Source: "hub"}})
	if err == nil || !strings.Contains(err.Error(), "choose one version") {
		t.Fatalf("conflict not rejected: %v", err)
	}
}
