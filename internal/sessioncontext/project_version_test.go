package sessioncontext

import (
	"context"
	"github.com/graphit-labs/graphit-code/internal/gitenv"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/projectversion"
	"github.com/graphit-labs/graphit-code/internal/sessionhook"
)

// A hook resolves the version of the checkout it fires in and both asks the store
// half for that version and tells the agent which version it is working in.
func TestHookEventsCarryTheCheckoutsProjectVersion(t *testing.T) {
	if _, err := exec.LookPath("git"); err != nil {
		t.Skip("git is not installed")
	}
	t.Setenv(projectversion.EnvVersion(), "")
	project := t.TempDir()
	initRepo := exec.Command("git", "-C", project, "init", "-q", "-b", "feature/z")
	initRepo.Env = gitenv.WithoutInvocationScope(os.Environ())
	if out, err := initRepo.CombinedOutput(); err != nil {
		t.Fatalf("git init: %v: %s", err, out)
	}
	if err := os.WriteFile(filepath.Join(project, brand.LockFileName()),
		[]byte(`{"project":{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV"}}`), 0o644); err != nil {
		t.Fatal(err)
	}

	var seen sessionhook.StoreRequest
	store := func(_ context.Context, req sessionhook.StoreRequest) (sessionhook.StoreResult, error) {
		seen = req
		return sessionhook.StoreResult{MandatoryLoaded: true}, nil
	}
	out, err := Render(context.Background(), project, sessionhook.FormatSessionStart, []byte(`{"session_id":"s1"}`), store)
	if err != nil {
		t.Fatal(err)
	}
	if seen.Version != "feature/z" {
		t.Fatalf("store request version = %q, want feature/z", seen.Version)
	}
	if !strings.Contains(string(out), "Project version `feature/z`") {
		t.Fatalf("bootstrap does not name the project version:\n%s", out)
	}
}
