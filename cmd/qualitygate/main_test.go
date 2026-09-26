package main

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestConfiguredHooksInvokeGate(t *testing.T) {
	data, err := os.ReadFile(filepath.Join("..", "..", "graphit.lock.json"))
	if err != nil {
		t.Fatal(err)
	}
	var lock struct {
		Hooks map[string][]string `json:"hooks"`
	}
	if err := json.Unmarshal(data, &lock); err != nil {
		t.Fatal(err)
	}
	for event, command := range map[string]string{
		"pre-commit": "go run ./cmd/qualitygate pre-commit",
		"pre-push":   "go run ./cmd/qualitygate pre-push",
	} {
		if got := lock.Hooks[event]; len(got) != 1 || got[0] != command {
			t.Errorf("hooks.%s = %q, want %q", event, got, command)
		}
	}
}

func TestFastChecksBlockCommitAndSlowChecksBlockPush(t *testing.T) {
	workflows := []string{".github/workflows/ci.yml"}
	commit := preCommitSteps(workflows)
	push := prePushSteps()
	contains := func(steps []step, argument string) bool {
		for _, s := range steps {
			if strings.Contains(strings.Join(s.args, " "), argument) {
				return true
			}
		}
		return false
	}
	if !contains(commit, "actionlint@v1.7.7") || contains(push, "actionlint@v1.7.7") {
		t.Fatal("short workflow validation must block the commit")
	}
	if !contains(push, "govulncheck@v1.7.0") || contains(commit, "govulncheck@v1.7.0") {
		t.Fatal("long vulnerability analysis belongs to the pre-push gate")
	}
	if !contains(commit, workflows[0]) {
		t.Fatal("pre-commit actionlint did not receive the workflow path")
	}
}

func TestFormattingFailureBlocksGate(t *testing.T) {
	path := filepath.Join(t.TempDir(), "bad.go")
	if err := os.WriteFile(path, []byte("package main\nfunc f( ){ }\n"), 0600); err != nil {
		t.Fatal(err)
	}
	var out, errOut bytes.Buffer
	if err := checkFormatting([]string{path}, &out, &errOut); err == nil {
		t.Fatal("unformatted Go source passed")
	}
	if !strings.Contains(out.String(), "bad.go") {
		t.Fatalf("missing formatting output: %s", out.String())
	}
}

func TestMissingToolBlocksGate(t *testing.T) {
	var out, errOut bytes.Buffer
	err := runSteps([]step{{name: "missing checker", command: "graphit-qualitygate-tool-that-does-not-exist"}}, &out, &errOut)
	if err == nil || !strings.Contains(err.Error(), "unavailable on PATH") {
		t.Fatalf("missing tool error = %v", err)
	}
}

func TestFailedStepPreventsFollowingStep(t *testing.T) {
	var out, errOut bytes.Buffer
	// go's invalid subcommand fails on every supported platform.
	err := runSteps([]step{
		{name: "broken", command: "go", args: []string{"qualitygate-invalid-subcommand"}},
		{name: "should not run", command: "go", args: []string{"version"}},
	}, &out, &errOut)
	if err == nil || strings.Contains(out.String(), "should not run") {
		t.Fatalf("failed command did not stop the gate: %v, output %q", err, out.String())
	}
	if !strings.Contains(errOut.String(), "unknown command") {
		t.Fatalf("stderr was not forwarded: %q", errOut.String())
	}
}
