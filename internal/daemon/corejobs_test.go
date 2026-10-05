package daemon

import (
	"bytes"
	"context"
	"os"
	"path/filepath"
	"reflect"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/auth"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/clirun"
	"github.com/graphit-labs/graphit-code/internal/projectversion"
	"github.com/graphit-labs/graphit-code/internal/store"
)

func TestDaemonCoreServicesIndexExplicitProject(t *testing.T) {
	requireParsers(t)
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "main.go"), []byte("package main\nfunc main() {}\n"), 0600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "README.md"), []byte("# Example\n\nLocal documentation.\n"), 0600); err != nil {
		t.Fatal(err)
	}
	before, _ := os.Getwd()
	if err := indexProjectAST(context.Background(), dir, nil, nil); err != nil {
		t.Fatal(err)
	}
	if err := indexProjectKnowledge(context.Background(), dir); err != nil {
		t.Fatal(err)
	}
	cfg := ast.LadybugConfigForContextIn(dir, "")
	if _, err := os.Stat(filepath.Join(cfg.IcebugDir, "schema.cypher")); err != nil {
		t.Fatalf("project graph: %v", err)
	}
	entries, err := os.ReadDir(store.KnowledgeProjectDir(dir))
	if err != nil || len(entries) == 0 {
		t.Fatalf("project wiki: %v (%d entries)", err, len(entries))
	}
	if after, _ := os.Getwd(); after != before {
		t.Fatal("job changed process CWD")
	}
	// An incremental deletion must remove the old parse entry without discovery.
	if err := os.Remove(filepath.Join(dir, "main.go")); err != nil {
		t.Fatal(err)
	}
	if err := indexProjectAST(context.Background(), dir, &astChangeSet{Removed: []string{"main.go"}}, nil); err != nil {
		t.Fatal(err)
	}
}

func TestDaemonCoreStoreProgressUsesSeparateSinks(t *testing.T) {
	for _, id := range []string{"01M00000000000000000000002", "01M00000000000000000000003"} {
		dir := t.TempDir()
		if err := os.WriteFile(filepath.Join(dir, brand.LockFileName()), []byte(`{"project":{"id":"`+id+`"}}`), 0600); err != nil {
			t.Fatal(err)
		}
		var output bytes.Buffer
		if err := (inProcessRunner{}).Run(context.Background(), clirun.Spec{Dir: dir, Args: []string{"memory", "index"}, Stderr: &output}); err != nil {
			t.Fatal(err)
		}
		if output.Len() == 0 {
			t.Fatal("store progress escaped job sink")
		}
	}
}

func TestDaemonCoreSupervisorStopJoinsJobBeforeResourcesClose(t *testing.T) {
	started, canceled, finish := make(chan struct{}), make(chan struct{}), make(chan struct{})
	mod := &fakeModule{name: "internal", startFn: func(ctx context.Context) error {
		close(started)
		<-ctx.Done()
		close(canceled)
		<-finish
		return ctx.Err()
	}}
	sup := newProjectSupervisor("fixture", t.TempDir(), []WatchModule{mod})
	closed := make(chan struct{})
	sup.AddCloser(closerFunc(func() error { close(closed); return nil }))
	go sup.Start(context.Background(), nil)
	select {
	case <-started:
	case <-time.After(time.Second):
		t.Fatal("job did not start")
	}
	done := make(chan struct{})
	go func() { sup.Stop(); close(done) }()
	select {
	case <-canceled:
	case <-time.After(time.Second):
		t.Fatal("job did not cancel")
	}
	select {
	case <-closed:
		t.Fatal("resources closed before job finished")
	case <-done:
		t.Fatal("Stop abandoned active job")
	case <-time.After(20 * time.Millisecond):
	}
	close(finish)
	select {
	case <-done:
	case <-time.After(time.Second):
		t.Fatal("Stop did not join")
	}
	select {
	case <-closed:
	default:
		t.Fatal("resources not closed")
	}
}

func TestDaemonCoreSupervisorStoppedBeforeStartDoesNoWork(t *testing.T) {
	sup := newProjectSupervisor("fixture", t.TempDir(), []WatchModule{&fakeModule{name: "internal", startFn: func(context.Context) error { t.Error("stopped job started"); return nil }}})
	sup.Stop()
	done := make(chan struct{})
	go func() { sup.Start(context.Background(), nil); close(done) }()
	select {
	case <-done:
	case <-time.After(time.Second):
		t.Fatal("stopped supervisor started")
	}
}

func TestDaemonCoreBrokerStoreRouteDoesNotLaunchNestedCore(t *testing.T) {
	ctx := auth.WithRequestSnapshot(context.Background(), auth.Snapshot{Provider: auth.Provider{Type: auth.ProviderBroker}})
	if reflect.TypeOf(jobRunner(ctx, []string{"memory", "maintain"})) != reflect.TypeOf(coreRunner) {
		t.Fatal("Broker store job would launch the thin binary's core fallback")
	}
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, brand.LockFileName()), []byte(`{"project":{"id":"01M00000000000000000000001"}}`), 0600); err != nil {
		t.Fatal(err)
	}
	// No valid Broker/grant is available: internal work must fail before mutation.
	if err := (inProcessRunner{}).Run(ctx, clirun.Spec{Dir: dir, Args: []string{"memory", "maintain"}}); err == nil {
		t.Fatal("Broker work bypassed authorization")
	}
}

func TestDaemonCoreDispatcherRunsIndexWithoutLaunchingCore(t *testing.T) {
	requireParsers(t)
	old := cliRunner
	forbidden := &recordingRunner{result: func(spec clirun.Spec) (string, error) {
		t.Errorf("unexpected subprocess: %v", spec.Args)
		return "", nil
	}}
	cliRunner = forbidden
	t.Cleanup(func() { cliRunner = old })
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "README.md"), []byte("# Sample\n"), 0600); err != nil {
		t.Fatal(err)
	}
	for _, args := range [][]string{{"ast", "index"}, {"knowledge", "index"}, {"hub", "hydrate", "--json"}} {
		if err := runProjectChild(context.Background(), dir, nil, args...); err != nil {
			t.Fatalf("%v: %v", args, err)
		}
	}
	if specs, _ := forbidden.recorded(); len(specs) != 0 {
		t.Fatal("core subprocess started")
	}
	if reflect.TypeOf(jobRunner(context.Background(), []string{"mcp", "--stdio"})) != reflect.TypeOf(forbidden) {
		t.Fatal("dedicated MCP route changed")
	}
}

func TestDaemonCoreDispatcherSyncKeepsDedicatedStoreJobs(t *testing.T) {
	requireParsers(t)
	t.Setenv("GRAPHIT_MODULES_EMBEDDING", "false")
	t.Setenv("GRAPHIT_MODULES_MEMORY", "false")
	t.Setenv("GRAPHIT_MODULES_HOOKS", "false")
	old := cliRunner
	storeRunner := &recordingRunner{}
	cliRunner = storeRunner
	t.Cleanup(func() { cliRunner = old })
	dir := t.TempDir()
	lock := `{"project":{"id":"01M00000000000000000000001"},"config":{"modules":{"embedding":false,"memory":false,"hooks":false}}}`
	if err := os.WriteFile(filepath.Join(dir, brand.LockFileName()), []byte(lock), 0600); err != nil {
		t.Fatal(err)
	}
	if err := runProjectChild(context.Background(), dir, nil, "sync"); err != nil {
		t.Fatal(err)
	}
	specs, _ := storeRunner.recorded()
	if len(specs) != 2 {
		t.Fatalf("store runs = %v", specs)
	}
	for _, s := range specs {
		if s.Args[0] != "memory" || s.Args[1] != "index" || s.Dir != dir {
			t.Fatalf("unexpected child: %v", s)
		}
		if !reflect.DeepEqual(s.Env, []string{projectversion.EnvVersion() + "=main"}) {
			t.Fatalf("lost version: %v", s.Env)
		}
	}
}

func TestDaemonCoreVersionIgnoresOtherRequestOverride(t *testing.T) {
	dir := t.TempDir()
	release, err := projectversion.Acquire(dir, "another-request")
	if err != nil {
		t.Fatal(err)
	}
	defer release()
	if !reflect.DeepEqual(versionEnv(dir), []string{projectversion.EnvVersion() + "=main"}) {
		t.Fatal("job used another request's scope")
	}
	if got := supervisionKey(ProjectInfo{ID: "p", Dir: dir}); got != "p@"+dir+"#main" {
		t.Fatalf("supervisor used request override: %q", got)
	}
}
