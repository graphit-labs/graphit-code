package commands

import (
	"bytes"
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func writeHookLockfile(t *testing.T, dir, hooks string) {
	t.Helper()
	data := `{"project":{"id":"test"},"artifacts":{},"hooks":` + hooks + `}`
	if err := os.WriteFile(filepath.Join(dir, brand.LockFileName()), []byte(data), 0o644); err != nil {
		t.Fatal(err)
	}
}

func TestRunConfiguredGitHookRunsAllCommandsAndForwardsOutput(t *testing.T) {
	dir := t.TempDir()
	failure := "echo failed 1>&2; exit 3"
	if runtime.GOOS == "windows" {
		failure = "echo failed 1>&2 && exit /B 3"
	}
	writeHookLockfile(t, dir, `{"pre-commit":["echo first","`+failure+`","echo third"]}`)
	var stdout, stderr bytes.Buffer
	err := runConfiguredGitHook(context.Background(), dir, "pre-commit", strings.NewReader(""), &stdout, &stderr)
	if err == nil || !strings.Contains(err.Error(), "command 2 failed") {
		t.Fatalf("expected second command failure, got %v", err)
	}
	if !strings.Contains(stdout.String(), "first") || !strings.Contains(stdout.String(), "third") {
		t.Fatalf("stdout did not show first and later commands: %q", stdout.String())
	}
	if strings.Index(stdout.String(), "first") > strings.Index(stdout.String(), "third") {
		t.Fatalf("commands ran out of order: %q", stdout.String())
	}
	if !strings.Contains(stderr.String(), "failed") {
		t.Fatalf("stderr was not forwarded: %q", stderr.String())
	}
}

func TestRunConfiguredGitHookResolvesExternalAndProjectRelativeExecutable(t *testing.T) {
	dir := t.TempDir()
	name, content, command := "project-check", "#!/bin/sh\necho project-check-ran\n", "./project-check"
	if runtime.GOOS == "windows" {
		name, content, command = "project-check.cmd", "@echo off\r\necho project-check-ran\r\n", `.\project-check.cmd`
	}
	if err := os.WriteFile(filepath.Join(dir, name), []byte(content), 0o755); err != nil {
		t.Fatal(err)
	}
	hooks, err := json.Marshal(map[string][]string{"pre-commit": {command, "git --version"}})
	if err != nil {
		t.Fatal(err)
	}
	writeHookLockfile(t, dir, string(hooks))
	var stdout, stderr bytes.Buffer
	if err := runConfiguredGitHook(context.Background(), dir, "pre-commit", strings.NewReader(""), &stdout, &stderr); err != nil {
		t.Fatalf("run project-relative and PATH commands: %v; stderr=%s", err, &stderr)
	}
	for _, want := range []string{"project-check-ran", "git version"} {
		if !strings.Contains(stdout.String(), want) {
			t.Fatalf("missing %q in stdout %q", want, stdout.String())
		}
	}
}

func TestRunConfiguredGitHookRejectsInvalidCommandsAndLockfile(t *testing.T) {
	dir := t.TempDir()
	var stdout, stderr bytes.Buffer
	run := func() error {
		return runConfiguredGitHook(context.Background(), dir, "pre-commit", strings.NewReader(""), &stdout, &stderr)
	}
	if err := run(); err == nil || !strings.Contains(err.Error(), "not found") {
		t.Fatalf("missing lockfile error = %v", err)
	}
	writeHookLockfile(t, dir, `{"pre-commit":["", "echo after-empty"]}`)
	if err := run(); err == nil || !strings.Contains(err.Error(), "command 1 is empty") {
		t.Fatalf("empty command error = %v", err)
	}
	if !strings.Contains(stdout.String(), "after-empty") {
		t.Fatalf("later command was not run: %q", stdout.String())
	}
	writeHookLockfile(t, dir, `{"pre-commit":[]}`)
	if err := run(); err != nil {
		t.Fatalf("empty command list should pass: %v", err)
	}
	writeHookLockfile(t, dir, `{}`)
	if err := run(); err != nil {
		t.Fatalf("absent command list should pass: %v", err)
	}
	writeHookLockfile(t, dir, `{"pre-commit":null}`)
	if err := run(); err == nil {
		t.Fatal("malformed lockfile should fail")
	}
}

func TestRunConfiguredGitHookPassesArgumentsAndRepeatsStdin(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("POSIX shell positional argument test")
	}
	dir := t.TempDir()
	writeHookLockfile(t, dir, `{"pre-push":["printf 'first:%s:%s:%s\\n' \"$1\" \"$2\" \"$(cat)\"","printf 'second:%s:%s\\n' \"$GRAPHIT_HOOK_ARG_1\" \"$(cat)\""]}`)
	var stdout, stderr bytes.Buffer
	err := runConfiguredGitHook(context.Background(), dir, "pre-push", strings.NewReader("refs/heads/main abc refs/heads/main def\n"), &stdout, &stderr, "origin", "ssh://example/repo")
	if err != nil {
		t.Fatal(err)
	}
	for _, part := range []string{"first:origin:ssh://example/repo:refs/heads/main abc refs/heads/main def", "second:origin:refs/heads/main abc refs/heads/main def"} {
		if !strings.Contains(stdout.String(), part) {
			t.Fatalf("missing %q in %q", part, stdout.String())
		}
	}
	if stderr.Len() != 0 {
		t.Fatalf("unexpected stderr: %s", &stderr)
	}
}

func TestRunConfiguredGitHookRejectsMultipleProtocolCommands(t *testing.T) {
	dir := t.TempDir()
	writeHookLockfile(t, dir, `{"proc-receive":["echo first","echo second"]}`)
	var stdout, stderr bytes.Buffer
	err := runConfiguredGitHook(context.Background(), dir, "proc-receive", strings.NewReader(""), &stdout, &stderr)
	if err == nil || !strings.Contains(err.Error(), "requires exactly one command") {
		t.Fatalf("protocol error = %v", err)
	}
}

func TestRunConfiguredGitHookDoesNotPreReadStdinWithoutGitPayload(t *testing.T) {
	dir := t.TempDir()
	writeHookLockfile(t, dir, `{"pre-commit":["echo ready"]}`)
	var stdout, stderr bytes.Buffer
	reader, writer, err := os.Pipe()
	if err != nil {
		t.Fatal(err)
	}
	defer reader.Close()
	defer writer.Close()
	done := make(chan error, 1)
	go func() {
		done <- runConfiguredGitHook(context.Background(), dir, "pre-commit", reader, &stdout, &stderr)
	}()
	select {
	case err := <-done:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(3 * time.Second):
		writer.Close()
		<-done
		t.Fatal("pre-commit waited for EOF on an open stdin")
	}
	if !strings.Contains(stdout.String(), "ready") {
		t.Fatalf("command did not run: %q", stdout.String())
	}
}
