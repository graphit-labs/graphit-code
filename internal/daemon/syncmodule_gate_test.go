package daemon

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"reflect"
	"sort"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/clirun"
	"github.com/graphit-labs/graphit-code/internal/fswatch"
	"github.com/graphit-labs/graphit-code/internal/store"
)

// A batch with nothing to reindex must not start any child.
func TestHandleBatchStartsNothingWhenThereIsNoWork(t *testing.T) {
	runner := &recordingRunner{}
	useRunner(t, runner)
	projectDir := t.TempDir()
	mod := NewSyncModule(projectDir, store.ASTProjectDir(projectDir))
	mod.handleBatch(context.Background(), fswatch.Batch{}, ast.NewAstIgnoreChecker(projectDir), nil)
	if specs, _ := runner.recorded(); len(specs) != 0 {
		t.Fatalf("an empty batch started %d child(ren): %+v", len(specs), specs)
	}
}

// A parked or shutting-down supervisor must not reindex on the way out.
func TestHandleBatchStartsNothingOnceCancelled(t *testing.T) {
	runner := &recordingRunner{}
	useRunner(t, runner)
	projectDir := t.TempDir()
	mod := NewSyncModule(projectDir, store.ASTProjectDir(projectDir))
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	mod.handleBatch(ctx, fswatch.Batch{Rescan: true}, ast.NewAstIgnoreChecker(projectDir), nil)
	if specs, _ := runner.recorded(); len(specs) != 0 {
		t.Fatalf("a cancelled batch started children: %+v", specs)
	}
}

func TestHandleBatchRescanRunsAFullProjectSync(t *testing.T) {
	runner := &recordingRunner{}
	useRunner(t, runner)
	projectDir := t.TempDir()
	mod := NewSyncModule(projectDir, store.ASTProjectDir(projectDir))
	mod.handleBatch(context.Background(), fswatch.Batch{Rescan: true}, ast.NewAstIgnoreChecker(projectDir), nil)
	specs, _ := runner.recorded()
	if len(specs) != 1 || !reflect.DeepEqual(specs[0].Args, []string{"sync"}) || specs[0].Dir != projectDir {
		t.Fatalf("rescan children = %+v, want one `sync` in the project", specs)
	}
}

func withHubStorage(t *testing.T, present bool) {
	t.Helper()
	old := hubStorageMayExist
	hubStorageMayExist = func(string) bool { return present }
	t.Cleanup(func() { hubStorageMayExist = old })
}

func TestHandleBatchIndexesExactlyTheChangedFilesThroughTheCLI(t *testing.T) {
	withHubStorage(t, true)
	runner := &recordingRunner{result: func(spec clirun.Spec) (string, error) {
		if spec.Args[0] == "hub" {
			return `{"ast_base_changed":false,"knowledge_base_changed":false}`, nil
		}
		return "", nil
	}}
	useRunner(t, runner)
	projectDir := t.TempDir()
	changed := filepath.Join(projectDir, "main.go")
	if err := os.WriteFile(changed, []byte("package main\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	mod := NewSyncModule(projectDir, store.ASTProjectDir(projectDir))
	batch := fswatch.Batch{Changed: []string{changed}, Removed: []string{filepath.Join(projectDir, "gone.go")}}
	mod.handleBatch(context.Background(), batch, ast.NewAstIgnoreChecker(projectDir), nil)

	specs, stdins := runner.recorded()
	if len(specs) != 2 {
		t.Fatalf("children = %+v, want hydrate then ast index", specs)
	}
	for _, spec := range specs {
		if spec.Dir != projectDir {
			t.Errorf("child %v ran in %q, want the project", spec.Args, spec.Dir)
		}
	}
	if !reflect.DeepEqual(specs[0].Args, []string{"hub", "hydrate", "--json"}) {
		t.Errorf("first child = %v, want hub hydrate --json", specs[0].Args)
	}
	if !reflect.DeepEqual(specs[1].Args, []string{"ast", "index", "--changes-from", "-"}) {
		t.Errorf("second child = %v, want ast index --changes-from -", specs[1].Args)
	}
	var set struct{ Changed, Removed []string }
	if err := json.Unmarshal([]byte(stdins[1]), &set); err != nil {
		t.Fatalf("change set %q: %v", stdins[1], err)
	}
	sort.Strings(set.Changed)
	if !reflect.DeepEqual(set.Changed, []string{"main.go"}) || !reflect.DeepEqual(set.Removed, []string{"gone.go"}) {
		t.Fatalf("change set = %+v, want project-relative main.go changed and gone.go removed", set)
	}
}

func TestHandleBatchReconcilesTheProjectWhenThePublishedBaseChanged(t *testing.T) {
	withHubStorage(t, true)
	runner := &recordingRunner{result: func(spec clirun.Spec) (string, error) {
		if spec.Args[0] == "hub" {
			return `{"ast_base_changed":true,"knowledge_base_changed":true}`, nil
		}
		return "", nil
	}}
	useRunner(t, runner)
	projectDir := t.TempDir()
	changed := filepath.Join(projectDir, "main.go")
	if err := os.WriteFile(changed, []byte("package main\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	mod := NewSyncModule(projectDir, store.ASTProjectDir(projectDir))
	mod.handleBatch(context.Background(), fswatch.Batch{Changed: []string{changed}}, ast.NewAstIgnoreChecker(projectDir), nil)
	specs, _ := runner.recorded()
	var got [][]string
	for _, s := range specs {
		got = append(got, s.Args)
	}
	want := [][]string{{"hub", "hydrate", "--json"}, {"ast", "index"}, {"knowledge", "index"}}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("children = %v, want %v", got, want)
	}
}

// A project without Hub storage has nothing to hydrate: the batch goes straight to the
// index, and a finished index wakes the AST embedder.
func TestHandleBatchSkipsHydrationWithoutHubAndWakesTheEmbedder(t *testing.T) {
	withHubStorage(t, false)
	runner := &recordingRunner{}
	useRunner(t, runner)
	projectDir := t.TempDir()
	changed := filepath.Join(projectDir, "main.go")
	if err := os.WriteFile(changed, []byte("package main\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	triggers := NewProjectTriggers()
	mod := NewSyncModule(projectDir, store.ASTProjectDir(projectDir)).WithTriggers(triggers)
	mod.handleBatch(context.Background(), fswatch.Batch{Changed: []string{changed}}, ast.NewAstIgnoreChecker(projectDir), nil)

	specs, _ := runner.recorded()
	if len(specs) != 1 || !reflect.DeepEqual(specs[0].Args, []string{"ast", "index", "--changes-from", "-"}) {
		t.Fatalf("children = %+v, want only ast index (no hub hydrate)", specs)
	}
	select {
	case <-triggers.ASTIndexed.c():
	default:
		t.Fatal("a successful AST index did not wake the embedder")
	}
	select {
	case <-triggers.KnowledgeIndexed.c():
		t.Fatal("the wiki embedder woke although Knowledge was not indexed")
	default:
	}
}

func TestHubStorageNeedsAProjectIdentity(t *testing.T) {
	if hubStorageMayExist(t.TempDir()) {
		t.Fatal("a directory without a project identity cannot have a published base")
	}
}
