package commands

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestReadASTChangeSetFromStdin(t *testing.T) {
	set, err := readASTChangeSet("-", strings.NewReader(`{"changed":["a.go"],"removed":["pkg/b.go"]}`))
	if err != nil {
		t.Fatalf("readASTChangeSet: %v", err)
	}
	if len(set.Changed) != 1 || set.Changed[0] != "a.go" || len(set.Removed) != 1 || set.Removed[0] != "pkg/b.go" {
		t.Fatalf("change set = %+v", set)
	}
}

func TestReadASTChangeSetFromFile(t *testing.T) {
	path := filepath.Join(t.TempDir(), "changes.json")
	if err := os.WriteFile(path, []byte(`{"removed":["gone.go"]}`), 0o600); err != nil {
		t.Fatal(err)
	}
	set, err := readASTChangeSet(path, nil)
	if err != nil || len(set.Removed) != 1 {
		t.Fatalf("readASTChangeSet(file) = %+v, %v", set, err)
	}
}

// A child runs with the project as its working directory; a path that leaves the
// project would let a change set reach into another one.
func TestReadASTChangeSetRejectsPathsOutsideTheProject(t *testing.T) {
	abs, _ := filepath.Abs(filepath.Join(t.TempDir(), "x.go"))
	for _, body := range []string{
		`{"changed":["../other/x.go"]}`,
		`{"removed":[` + mustJSON(t, abs) + `]}`,
	} {
		if _, err := readASTChangeSet("-", strings.NewReader(body)); err == nil {
			t.Errorf("accepted %s", body)
		}
	}
}

func TestHubHydrateJSONReportsUnchangedBaseWithoutHub(t *testing.T) {
	t.Setenv("GRAPHIT_GLOBAL_DIR", t.TempDir())
	t.Chdir(t.TempDir())
	cmd := newHubHydrateCmd()
	var out bytes.Buffer
	cmd.SetOut(&out)
	cmd.SetArgs([]string{"--json"})
	if err := cmd.Execute(); err != nil {
		t.Fatalf("hub hydrate --json: %v", err)
	}
	var report hydrationReport
	if err := json.Unmarshal(out.Bytes(), &report); err != nil {
		t.Fatalf("output %q is not a hydration report: %v", out.String(), err)
	}
	if report.ASTBaseChanged || report.KnowledgeBaseChanged {
		t.Fatalf("report = %+v, want nothing changed without a Hub", report)
	}
}

func mustJSON(t *testing.T, v any) string {
	t.Helper()
	data, err := json.Marshal(v)
	if err != nil {
		t.Fatal(err)
	}
	return string(data)
}
