package main

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"runtime"
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
	if len(commit) == 0 || !strings.Contains(strings.Join(commit[0].args, " "), "TestSCIPFailureFallsBackToTreeSitter") {
		t.Fatal("SCIP fallback regression must block commits")
	}
	push := prePushSteps("linux")
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
	if len(push) != 2 || push[0].command != "make" || !contains(push[:1], "test-full") || push[0].env[0] != "GRAPHIT_HEAVY_TEST_ISOLATED=0" {
		t.Fatalf("Linux pre-push must run the isolated CI suite first: %+v", push)
	}
	for _, goos := range []string{"darwin", "windows"} {
		steps := prePushSteps(goos)
		if len(steps) != 2 || steps[0].command != "go" || !contains(steps[:1], "./internal/daemonctl") || !contains(steps[:1], "-race") {
			t.Errorf("%s pre-push lacks platform semantics checks: %+v", goos, steps)
		}
	}
	if !contains(commit, "TestSyncOutput.*") || !contains(commit, "TestSyncChildren.*") || !contains(commit, "./cmd/store") {
		t.Fatal("sync output and store maintenance regressions must block the commit")
	}
	if !contains(commit, workflows[0]) {
		t.Fatal("pre-commit actionlint did not receive the workflow path")
	}
	for _, pkg := range []string{"./internal/relations", "./internal/wiki", "./internal/knowledge", "./internal/task", "./internal/memory"} {
		if !contains(commit, pkg) {
			t.Errorf("pre-commit is missing LanceDB relation regression package %s", pkg)
		}
	}
}

func TestPrePushFullSuiteFailureBlocksLaterChecks(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("Unix fake executable fixture")
	}
	root := t.TempDir()
	fakeMake := filepath.Join(root, "make")
	if err := os.WriteFile(fakeMake, []byte("#!/bin/sh\nprintf 'isolation=%s\\n' \"$GRAPHIT_HEAVY_TEST_ISOLATED\"\nexit 42\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	t.Setenv("PATH", root+string(os.PathListSeparator)+os.Getenv("PATH"))
	t.Setenv("GRAPHIT_HEAVY_TEST_ISOLATED", "1")
	var out, errOut bytes.Buffer
	err := runSteps(prePushSteps("linux"), &out, &errOut)
	if err == nil || !strings.Contains(out.String(), "isolation=0") || strings.Contains(out.String(), "Go vulnerability analysis") {
		t.Fatalf("failed full suite did not block push checks: err=%v stdout=%q stderr=%q", err, out.String(), errOut.String())
	}
}

func TestFallbackRegressionFailureBlocksCommit(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("Unix fake executable fixture")
	}
	root := t.TempDir()
	if err := os.WriteFile(filepath.Join(root, "go"), []byte("#!/bin/sh\nexit 42\n"), 0o700); err != nil {
		t.Fatal(err)
	}
	t.Setenv("PATH", root+string(os.PathListSeparator)+os.Getenv("PATH"))
	var out, errOut bytes.Buffer
	err := runSteps(preCommitSteps(nil), &out, &errOut)
	if err == nil || !strings.Contains(err.Error(), "AST fallback and cancellation regressions") || strings.Contains(out.String(), "Go unit tests") {
		t.Fatalf("failed fallback regression did not block commit checks: err=%v stdout=%q", err, out.String())
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

func TestThinBinaryLinkingAStoreBlocksCommit(t *testing.T) {
	limits := map[string][]string{"./cmd/hook": {modulePath + "internal/hub"}}
	deps := map[string][]string{"./cmd/hook": {"fmt", modulePath + "internal/hubaccess", modulePath + "internal/hub/s3"}}
	list := func(pkg string) ([]string, error) { return deps[pkg], nil }
	var out bytes.Buffer
	err := checkThinBinaries(limits, list, &out)
	if err == nil || !strings.Contains(err.Error(), "internal/hub/s3") || strings.Contains(err.Error(), "hubaccess") {
		t.Fatalf("a store package in a thin binary must block the commit, and only it: %v", err)
	}
	deps["./cmd/hook"] = []string{"fmt", modulePath + "internal/hubaccess"}
	if err := checkThinBinaries(limits, list, &out); err != nil {
		t.Fatalf("a light dependency was rejected: %v", err)
	}
}

func TestEveryThinBinaryIsGuarded(t *testing.T) {
	for _, pkg := range []string{"./cmd/hook", "./cmd/githook", "./cmd/tray", "./cmd/model", "./cmd/store"} {
		if len(thinBinaryLimits[pkg]) == 0 {
			t.Errorf("%s has no dependency limits", pkg)
		}
	}
}

func TestLightSubpackageOfAHeavyPathIsAllowed(t *testing.T) {
	limits := map[string][]string{"./cmd/hook": {modulePath + "internal/hub"}}
	list := func(string) ([]string, error) { return []string{modulePath + "internal/hub/adapters/agent"}, nil }
	if err := checkThinBinaries(limits, list, &bytes.Buffer{}); err != nil {
		t.Fatalf("the agent adapter links no store: %v", err)
	}
}
