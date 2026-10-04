package commands

import (
	"context"
	"errors"
	"reflect"
	"sort"
	"strings"
	"sync"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/clirun"
	"github.com/graphit-labs/graphit-code/internal/output"
)

type syncChildRecorder struct {
	mu    sync.Mutex
	specs []clirun.Spec
	fail  map[string]error
}

func (r *syncChildRecorder) Run(_ context.Context, spec clirun.Spec) error {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.specs = append(r.specs, spec)
	return r.fail[strings.Join(spec.Args, " ")]
}

func (r *syncChildRecorder) commands() []string {
	r.mu.Lock()
	defer r.mu.Unlock()
	var out []string
	for _, s := range r.specs {
		out = append(out, strings.Join(s.Args, " "))
	}
	return out
}

func useSyncChildren(t *testing.T, fail map[string]error) *syncChildRecorder {
	t.Helper()
	rec := &syncChildRecorder{fail: fail}
	old := syncChildRunner
	syncChildRunner = rec
	t.Cleanup(func() { syncChildRunner = old })
	return rec
}

func TestSyncChildrenPhaseOneRunsEveryIndexAsACLIProcessInTheProject(t *testing.T) {
	t.Setenv("GRAPHIT_GLOBAL_DIR", t.TempDir())
	wd := t.TempDir()
	t.Chdir(wd)
	rec := useSyncChildren(t, nil)

	if err := runSyncPhase1(context.Background(), wd, nil, output.NewPrinter("")); err != nil {
		t.Fatalf("runSyncPhase1: %v", err)
	}
	got := rec.commands()
	sort.Strings(got)
	want := []string{"ast index", "hub hydrate", "knowledge index", "memory index", "memory index --user"}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("children = %v, want %v", got, want)
	}
	for _, spec := range rec.specs {
		if spec.Dir != wd || !spec.Inherit {
			t.Errorf("child %v: dir=%q inherit=%v, want the project and shared streams", spec.Args, spec.Dir, spec.Inherit)
		}
	}
}

func TestSyncChildrenFailedASTIndexFailsTheSync(t *testing.T) {
	t.Setenv("GRAPHIT_GLOBAL_DIR", t.TempDir())
	wd := t.TempDir()
	t.Chdir(wd)
	useSyncChildren(t, map[string]error{"ast index": errors.New("exit 1")})
	if err := runSyncPhase1(context.Background(), wd, nil, output.NewPrinter("")); err == nil {
		t.Fatal("sync succeeded although its AST index child failed")
	}
}

func TestSyncChildrenHeavyPhaseDelegatesEmbeddingAndMaintenance(t *testing.T) {
	t.Setenv("GRAPHIT_GLOBAL_DIR", t.TempDir())
	wd := t.TempDir()
	rec := useSyncChildren(t, nil)
	runSyncHeavyTasks(context.Background(), wd, nil)
	got := rec.commands()
	want := []string{"ast embed", "wiki embed", "memory embed", "memory embed --user", "memory maintain", "memory maintain --user"}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("heavy children = %v, want %v", got, want)
	}
}

func TestSyncChildrenHeavyPhaseUnderTheDaemonWaitsForItsChild(t *testing.T) {
	t.Setenv(clirun.SubprocessEnv(), "1")
	wd := t.TempDir()
	rec := useSyncChildren(t, nil)
	spawnBackgroundSync(wd, "")
	if got := rec.commands(); !reflect.DeepEqual(got, []string{"sync --heavy"}) {
		t.Fatalf("children = %v, want an attached sync --heavy", got)
	}
}
