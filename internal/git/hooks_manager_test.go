package git

import (
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func hookTestRepo(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	cmd := exec.Command("git", "-C", dir, "init", "-q")
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("git init: %v: %s", err, out)
	}
	return dir
}

func fakeGitVersion(t *testing.T, version string) string {
	t.Helper()
	if runtime.GOOS == "windows" {
		t.Skip("shell test")
	}
	realGit, err := exec.LookPath("git")
	if err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(t.TempDir(), "git")
	script := "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'git version " + version + "'; else exec " + realGit + " \"$@\"; fi\n"
	if err := os.WriteFile(path, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	return path
}

func TestHookManagerModernRegistrationAndRemoval(t *testing.T) {
	dir := hookTestRepo(t)
	hm := NewHookManager(dir)
	hm.gitBinary = fakeGitVersion(t, "2.54.0")
	if err := hm.Install(false); err != nil {
		t.Fatal(err)
	}
	if err := hm.Install(false); err != nil {
		t.Fatalf("reinstall: %v", err)
	}
	event, err := hm.gitConfigGet(configuredHook() + ".event")
	if err != nil || event != preCommitEvent {
		t.Fatalf("event = %q, %v", event, err)
	}
	command, err := hm.gitConfigGet(configuredHook() + ".command")
	if err != nil || command != hookCommand() {
		t.Fatalf("command = %q, %v", command, err)
	}
	if _, err := os.Stat(filepath.Join(hm.hooksDir, preCommitEvent)); !os.IsNotExist(err) {
		t.Fatalf("modern path wrote legacy hook: %v", err)
	}
	if err := hm.Remove(); err != nil {
		t.Fatal(err)
	}
	if _, err := hm.gitConfigGet(configuredHook() + ".event"); err == nil {
		t.Fatal("event remains after remove")
	}
}

func TestHookManagerLegacyPreservesShellAndWarnsForNonShell(t *testing.T) {
	dir := hookTestRepo(t)
	hm := NewHookManager(dir)
	hm.gitBinary = fakeGitVersion(t, "2.53.0")
	if err := hm.gitConfig("--replace-all", "core.hooksPath", "third-party-hooks"); err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(hm.hooksDir, preCommitEvent)
	original := "#!/bin/sh\necho third-party\n"
	if err := os.WriteFile(path, []byte(original), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := hm.Install(false); err != nil {
		t.Fatal(err)
	}
	if err := hm.Install(false); err != nil {
		t.Fatal(err)
	}
	data, _ := os.ReadFile(path)
	if !strings.Contains(string(data), "echo third-party") || strings.Count(string(data), hookBlockMarker()) != 2 {
		t.Fatalf("shell content lost or duplicate block: %s", data)
	}
	if runtime.GOOS != "windows" {
		info, err := os.Stat(path)
		if err != nil || info.Mode()&0o111 == 0 {
			t.Fatalf("hook is not executable: %v", err)
		}
	}
	if err := hm.Remove(); err != nil {
		t.Fatal(err)
	}
	data, _ = os.ReadFile(path)
	if string(data) != original {
		t.Fatalf("third-party content not preserved byte for byte: %q", data)
	}
	if err := os.WriteFile(path, []byte("#!/usr/bin/env python3\nprint('other')\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	err := hm.Install(false)
	if err == nil || !strings.Contains(err.Error(), "non-shell or unsupported-shell shebang") || !strings.Contains(err.Error(), "Manual integration:") || !strings.Contains(err.Error(), "_git-hook pre-commit") {
		t.Fatalf("missing manual warning: %v", err)
	}
}

func TestHookManagerRemovalDoesNotRewriteThirdPartyOnlyFile(t *testing.T) {
	dir := hookTestRepo(t)
	hm := NewHookManager(dir)
	hm.gitBinary = fakeGitVersion(t, "2.53.0")
	path := filepath.Join(hm.hooksDir, preCommitEvent)
	original := []byte("#!/bin/sh\n\n\nexit 0\n\n")
	if err := os.WriteFile(path, original, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := hm.Remove(); err != nil {
		t.Fatal(err)
	}
	data, err := os.ReadFile(path)
	if err != nil || string(data) != string(original) {
		t.Fatalf("third-party hook changed: %q, %v", data, err)
	}
}

func TestLegacyHookPrecedesThirdPartyEarlyExit(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("shell test")
	}
	dir := hookTestRepo(t)
	hm := NewHookManager(dir)
	hm.gitBinary = fakeGitVersion(t, "2.53.0")
	path := filepath.Join(hm.hooksDir, preCommitEvent)
	if err := os.WriteFile(path, []byte("#!/bin/sh\nexit 0\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := hm.Install(false); err != nil {
		t.Fatal(err)
	}
	data, err := os.ReadFile(path)
	if err != nil || strings.Index(string(data), hookBlockMarker()) > strings.Index(string(data), "exit 0") {
		t.Fatalf("Graphit block follows early exit: %s, %v", data, err)
	}
	binDir := t.TempDir()
	fakeBrand := filepath.Join(binDir, binPath())
	if err := os.WriteFile(fakeBrand, []byte("#!/bin/sh\necho graphit-ran >&2\nexit 7\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	cmd := exec.Command("sh", path)
	cmd.Dir = dir
	cmd.Env = append(os.Environ(), "PATH="+filepath.Dir(hm.gitBinary)+":"+binDir+":"+os.Getenv("PATH"))
	output, err := cmd.CombinedOutput()
	exitErr, ok := err.(*exec.ExitError)
	if !ok || exitErr.ExitCode() != 7 || !strings.Contains(string(output), "graphit-ran") {
		t.Fatalf("Graphit was skipped or failure ignored: %v, %s", err, output)
	}
	if err := hm.Remove(); err != nil {
		t.Fatal(err)
	}
	restored, err := os.ReadFile(path)
	if err != nil || string(restored) != "#!/bin/sh\nexit 0\n" {
		t.Fatalf("third-party early exit not restored: %q, %v", restored, err)
	}
}

func TestConfiguredHookUsesBrand(t *testing.T) {
	original := brand.Brand
	brand.Brand = "otherbrand"
	defer func() { brand.Brand = original }()
	if got := configuredHook(); got != "hook.otherbrand-pre-commit" {
		t.Fatalf("configured hook = %q", got)
	}
}

func TestLegacyHookSkipsWhenModernOrRegistered(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("shell test")
	}
	dir := hookTestRepo(t)
	hm := NewHookManager(dir)
	hm.gitBinary = fakeGitVersion(t, "2.53.0")
	path := filepath.Join(hm.hooksDir, preCommitEvent)
	if err := hm.Install(false); err != nil {
		t.Fatal(err)
	}
	binDir := t.TempDir()
	log := filepath.Join(t.TempDir(), "runs")
	fakeBrand := filepath.Join(binDir, binPath())
	if err := os.WriteFile(fakeBrand, []byte("#!/bin/sh\necho run >> \"$GRAPHIT_HOOK_TEST_LOG\"\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	run := func(gitBin string) {
		t.Helper()
		env := append(os.Environ(), "PATH="+filepath.Dir(gitBin)+":"+binDir+":"+os.Getenv("PATH"), "GRAPHIT_HOOK_TEST_LOG="+log)
		cmd := exec.Command("sh", path)
		cmd.Dir = dir
		cmd.Env = env
		if out, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("hook failed: %v: %s", err, out)
		}
	}
	oldGit := fakeGitVersion(t, "2.53.0")
	newGit := fakeGitVersion(t, "2.54.0")
	run(oldGit)
	data, _ := os.ReadFile(log)
	if strings.Count(string(data), "run") != 1 {
		t.Fatalf("legacy hook did not run once: %s", data)
	}
	run(newGit)
	data, _ = os.ReadFile(log)
	if strings.Count(string(data), "run") != 1 {
		t.Fatalf("legacy hook ran under Git 2.54: %s", data)
	}
	if err := hm.gitConfig("--replace-all", configuredHook()+".event", preCommitEvent); err != nil {
		t.Fatal(err)
	}
	run(oldGit)
	data, _ = os.ReadFile(log)
	if strings.Count(string(data), "run") != 1 {
		t.Fatalf("legacy hook ran with registered hook: %s", data)
	}
}

func TestHookManagerMissingGitWarns(t *testing.T) {
	hm := NewHookManager(t.TempDir())
	hm.gitBinary = fakeGitVersion(t, "2.53.0")
	err := hm.Install(false)
	if err == nil || !strings.Contains(err.Error(), ".git/hooks/pre-commit") || !strings.Contains(err.Error(), "Manual integration:") {
		t.Fatalf("missing useful warning: %v", err)
	}
}

func TestLegacyHookUnreadableTargetWarns(t *testing.T) {
	dir := hookTestRepo(t)
	hm := NewHookManager(dir)
	hm.gitBinary = fakeGitVersion(t, "2.53.0")
	path := filepath.Join(hm.hooksDir, preCommitEvent)
	if err := os.Mkdir(path, 0o755); err != nil {
		t.Fatal(err)
	}
	err := hm.Install(false)
	if err == nil || !strings.Contains(err.Error(), "pre-commit hook not installed") || !strings.Contains(err.Error(), "Manual integration:") {
		t.Fatalf("missing write/read failure warning: %v", err)
	}
}

func TestModernRegistrationFailureWarnsWithManualCommands(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("shell test")
	}
	dir := hookTestRepo(t)
	fakeGit := filepath.Join(t.TempDir(), "git")
	if err := os.WriteFile(fakeGit, []byte("#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'git version 2.54.0'; else echo config-denied >&2; exit 1; fi\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	hm := NewHookManager(dir)
	hm.gitBinary = fakeGit
	err := hm.Install(false)
	if err == nil || !strings.Contains(err.Error(), "config-denied") || !strings.Contains(err.Error(), "hook.graphit-pre-commit.command") || !strings.Contains(err.Error(), "hook.graphit-pre-commit.event") {
		t.Fatalf("missing modern manual warning: %v", err)
	}
}

func TestHookManagerAdditionalEventsAndReconciliation(t *testing.T) {
	dir := hookTestRepo(t)
	hm := NewHookManager(dir)
	hm.gitBinary = fakeGitVersion(t, "2.54.0")
	if err := hm.Install(false, "pre-push", "commit-msg"); err != nil {
		t.Fatal(err)
	}
	for _, event := range []string{"pre-commit", "pre-push", "commit-msg"} {
		value, err := hm.gitConfigGet(configuredHookFor(event) + ".event")
		if err != nil || value != event {
			t.Fatalf("%s event = %q, %v", event, value, err)
		}
		command, err := hm.gitConfigGet(configuredHookFor(event) + ".command")
		if err != nil || !strings.Contains(command, "_git-hook "+event) || !strings.Contains(command, `"$@"`) {
			t.Fatalf("%s command = %q, %v", event, command, err)
		}
		if _, err := os.Stat(filepath.Join(hm.hooksDir, event)); !os.IsNotExist(err) {
			t.Fatalf("modern installation wrote %s legacy hook: %v", event, err)
		}
	}
	if err := hm.Install(false, "commit-msg"); err != nil {
		t.Fatal(err)
	}
	if _, err := hm.gitConfigGet(configuredHookFor("pre-push") + ".event"); err == nil {
		t.Fatal("stale pre-push registration remains")
	}
	if err := hm.Remove(); err != nil {
		t.Fatal(err)
	}
	if _, err := hm.gitConfigGet(configuredHookFor("commit-msg") + ".event"); err == nil {
		t.Fatal("commit-msg registration remains after removal")
	}
}

func TestModernManualInstructionsPreserveHookArguments(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("POSIX shell instructions")
	}
	dir := filepath.Join(t.TempDir(), "quoted ' $repo")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	if output, err := exec.Command("git", "-C", dir, "init", "-q").CombinedOutput(); err != nil {
		t.Fatalf("git init: %v: %s", err, output)
	}
	hm := NewHookManager(dir)
	command := exec.Command("sh", "-c", hm.ManualInstructionsFor("pre-push", true))
	if output, err := command.CombinedOutput(); err != nil {
		t.Fatalf("manual integration failed: %v: %s", err, output)
	}
	stored, err := hm.gitConfigGet(configuredHookFor("pre-push") + ".command")
	if err != nil || stored != hookCommandFor("pre-push") {
		t.Fatalf("manual command = %q, %v", stored, err)
	}
}

func TestLegacyAdditionalEventPreservesThirdPartyExecutable(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("shell test")
	}
	dir := hookTestRepo(t)
	hm := NewHookManager(dir)
	hm.gitBinary = fakeGitVersion(t, "2.53.0")
	path := filepath.Join(hm.hooksDir, "pre-push")
	original := "#!/bin/sh\necho third-party\n"
	if err := os.WriteFile(path, []byte(original), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := hm.Install(false, "pre-push"); err != nil {
		t.Fatal(err)
	}
	data, err := os.ReadFile(path)
	if err != nil || !strings.Contains(string(data), "_git-hook pre-push \"$@\"") {
		t.Fatalf("pre-push block missing arguments: %q, %v", data, err)
	}
	if err := hm.Remove(); err != nil {
		t.Fatal(err)
	}
	data, err = os.ReadFile(path)
	if err != nil || string(data) != original {
		t.Fatalf("third-party pre-push changed: %q, %v", data, err)
	}
	info, err := os.Stat(path)
	if err != nil || info.Mode()&0o111 == 0 {
		t.Fatalf("third-party hook lost executable bit: %v", err)
	}
}

func TestLegacyAdditionalEventRunsThroughGitWithArgumentsAndStdin(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("POSIX shell integration test")
	}
	gitVersion, err := exec.Command("git", "--version").Output()
	if err != nil || !strings.HasPrefix(string(gitVersion), "git version 2.53.") {
		t.Skip("requires the Git 2.53 fallback runtime")
	}
	dir := hookTestRepo(t)
	hm := NewHookManager(dir)
	if err := hm.Install(false, "pre-push"); err != nil {
		t.Fatal(err)
	}
	binDir := t.TempDir()
	fakeBrand := filepath.Join(binDir, binPath())
	if err := os.WriteFile(fakeBrand, []byte("#!/bin/sh\nprintf 'args:%s:%s:%s\\n' \"$1\" \"$2\" \"$3\"; cat\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	input := filepath.Join(t.TempDir(), "refs")
	if err := os.WriteFile(input, []byte("ref-old ref-new\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	cmd := exec.Command("git", "-C", dir, "hook", "run", "--to-stdin="+input, "pre-push", "--", "origin", "ssh://example/repo")
	cmd.Env = append(os.Environ(), "PATH="+binDir+":"+os.Getenv("PATH"))
	output, err := cmd.CombinedOutput()
	if err != nil || !strings.Contains(string(output), "args:_git-hook:pre-push:origin") || !strings.Contains(string(output), "ref-old ref-new") {
		t.Fatalf("Git 2.53 pre-push result: %v, %s", err, output)
	}
}
