package git

import (
	"errors"
	"fmt"
	"reflect"
	"testing"
)

type snapshotGit struct {
	outputs map[string]string
	errors  map[string]error
}

func (g snapshotGit) Run(_ string, _ ...string) error { return nil }
func (g snapshotGit) RunOutput(_ string, args ...string) (string, error) {
	key := fmt.Sprint(args)
	return g.outputs[key], g.errors[key]
}
func (g snapshotGit) RunSilent(_ string, _ ...string) string { return "" }
func (g snapshotGit) RunWithStdin(_ string, _ []byte, _ ...string) (string, error) {
	return "", nil
}
func (g snapshotGit) RunWithEnv(_ string, _ map[string]string, _ ...string) error { return nil }
func (g snapshotGit) RunOutputWithEnv(_ string, _ map[string]string, _ ...string) (string, error) {
	return "", nil
}
func (g snapshotGit) RunGlobal(_ ...string) error                 { return nil }
func (g snapshotGit) RunGlobalOutput(_ ...string) (string, error) { return "", nil }

func TestInspectSnapshotPreservesBranchPathAndAncestry(t *testing.T) {
	g := snapshotGit{outputs: map[string]string{
		"[rev-parse --show-toplevel]":                   "/repo\n",
		"[rev-parse HEAD]":                              "c3\n",
		"[symbolic-ref --quiet --short HEAD]":           "feature/api/v2\n",
		"[status --porcelain --untracked-files=normal]": " M service.go\n",
		"[rev-list HEAD]":                               "c3\nc2\nc1\n",
	}}

	got, err := inspectSnapshot(g, "/repo/subdir")
	if err != nil {
		t.Fatal(err)
	}
	if got.BranchVersion() != "branch/feature/api/v2" {
		t.Fatalf("branch version = %q", got.BranchVersion())
	}
	if !got.Dirty || !reflect.DeepEqual(got.Ancestors, []string{"c3", "c2", "c1"}) {
		t.Fatalf("snapshot = %#v", got)
	}
}

func TestInspectSnapshotRejectsNonRepository(t *testing.T) {
	g := snapshotGit{
		outputs: map[string]string{},
		errors:  map[string]error{"[rev-parse --show-toplevel]": fmt.Errorf("outside work tree")},
	}
	if _, err := inspectSnapshot(g, "/tmp/project"); !errors.Is(err, ErrNotRepository) {
		t.Fatalf("error = %v, want ErrNotRepository", err)
	}
}

func TestInspectSnapshotUsesConfiguredBaseBranchWhenDetached(t *testing.T) {
	t.Setenv("GRAPHIT_GIT_BASE_BRANCH", "refs/heads/main")
	g := snapshotGit{
		outputs: map[string]string{
			"[rev-parse --show-toplevel]":                   "/repo\n",
			"[rev-parse HEAD]":                              "tagged\n",
			"[status --porcelain --untracked-files=normal]": "",
			"[rev-list HEAD]":                               "tagged\nparent\n",
		},
		errors: map[string]error{"[symbolic-ref --quiet --short HEAD]": fmt.Errorf("detached")},
	}
	got, err := inspectSnapshot(g, "/repo")
	if err != nil {
		t.Fatal(err)
	}
	if !got.Detached || got.Branch != "main" {
		t.Fatalf("snapshot = %#v", got)
	}
}
