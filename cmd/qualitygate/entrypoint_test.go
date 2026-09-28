package main

import (
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
)

func TestDockerEntrypointSetupHooks(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("the Linux container entrypoint requires POSIX sh")
	}
	if _, err := exec.LookPath("sh"); err != nil {
		t.Skip("POSIX sh is unavailable")
	}

	root := t.TempDir()
	global := filepath.Join(root, "global")
	hooksDir := filepath.Join(root, "entrypoint-hooks")
	bin := filepath.Join(root, "bin")
	for _, dir := range []string{global, hooksDir, bin} {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			t.Fatal(err)
		}
	}
	log := filepath.Join(root, "events")
	writeEntrypointFixture(t, filepath.Join(bin, "graphit"), "#!/bin/sh\nprintf '%s\\n' \"$1\" >> \"$GRAPHIT_TEST_LOG\"\nif [ \"$1\" = setup ]; then touch \"$GRAPHIT_GLOBAL_DIR/config.json\"; fi\n", 0o755)
	writeEntrypointFixture(t, filepath.Join(bin, "custom"), "#!/bin/sh\nprintf 'custom\\n' >> \"$GRAPHIT_TEST_LOG\"\n", 0o755)
	// The global volume may contain similarly named paths, but they are not hooks.
	globalDecoy := filepath.Join(global, "pre-setup.d")
	if err := os.Mkdir(globalDecoy, 0o755); err != nil {
		t.Fatal(err)
	}
	writeEntrypointFixture(t, filepath.Join(globalDecoy, "00-ignored.sh"), "#!/bin/sh\nexit 99\n", 0o755)
	for _, hook := range []string{"pre-setup.d", "setup.d", "post-setup.d"} {
		dir := filepath.Join(hooksDir, hook)
		if err := os.Mkdir(dir, 0o755); err != nil {
			t.Fatal(err)
		}
		writeEntrypointFixture(t, filepath.Join(dir, "20-second.sh"), "#!/bin/sh\nprintf '"+hook+"-second\\n' >> \"$GRAPHIT_TEST_LOG\"\n", 0o755)
		writeEntrypointFixture(t, filepath.Join(dir, "10-first.sh"), "#!/bin/sh\nprintf '"+hook+"-first\\n' >> \"$GRAPHIT_TEST_LOG\"\n", 0o755)
		writeEntrypointFixture(t, filepath.Join(dir, "30-disabled.sh"), "#!/bin/sh\nexit 99\n", 0o644)
	}

	hookRoot := hooksDir
	run := func(args ...string) error {
		t.Helper()
		cmd := exec.Command("sh", append([]string{filepath.Join("..", "..", "scripts", "graphit-entrypoint.sh")}, args...)...)
		cmd.Env = append(os.Environ(),
			"PATH="+bin+string(os.PathListSeparator)+os.Getenv("PATH"),
			"GRAPHIT_GLOBAL_DIR="+global,
			"GRAPHIT_ENTRYPOINT_HOOKS_DIR="+hookRoot,
			"GRAPHIT_TEST_LOG="+log,
			"GRAPHIT_HUB_EVENTS_ANONYMIZE=false", "GRAPHIT_AGENT=", "GRAPHIT_CLI=",
		)
		if output, err := cmd.CombinedOutput(); err != nil {
			return &entrypointError{err: err, output: string(output)}
		}
		return nil
	}
	readEvents := func() string {
		t.Helper()
		data, err := os.ReadFile(log)
		if err != nil {
			t.Fatal(err)
		}
		return string(data)
	}

	if err := run(); err != nil {
		t.Fatal(err)
	}
	wantFirst := "pre-setup.d-first\npre-setup.d-second\nsetup\nsetup.d-first\nsetup.d-second\npost-setup.d-first\npost-setup.d-second\ndaemon\n"
	if got := readEvents(); got != wantFirst {
		t.Fatalf("first start events:\n%s\nwant:\n%s", got, wantFirst)
	}
	if err := os.WriteFile(log, nil, 0o644); err != nil {
		t.Fatal(err)
	}
	if err := run("custom"); err != nil {
		t.Fatal(err)
	}
	wantRestart := "pre-setup.d-first\npre-setup.d-second\npost-setup.d-first\npost-setup.d-second\ncustom\n"
	if got := readEvents(); got != wantRestart {
		t.Fatalf("restart events:\n%s\nwant:\n%s", got, wantRestart)
	}

	if err := os.RemoveAll(filepath.Join(hooksDir, "pre-setup.d")); err != nil {
		t.Fatal(err)
	}
	if err := os.RemoveAll(filepath.Join(hooksDir, "post-setup.d")); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(log, nil, 0o644); err != nil {
		t.Fatal(err)
	}
	if err := run("custom"); err != nil {
		t.Fatal(err)
	}
	if got := readEvents(); got != "custom\n" {
		t.Fatalf("missing hook directories: %q", got)
	}

	failing := filepath.Join(hooksDir, "pre-setup.d")
	if err := os.Mkdir(failing, 0o755); err != nil {
		t.Fatal(err)
	}
	writeEntrypointFixture(t, filepath.Join(failing, "10-fail.sh"), "#!/bin/sh\nprintf 'failed\\n' >> \"$GRAPHIT_TEST_LOG\"\nexit 7\n", 0o755)
	if err := os.WriteFile(log, nil, 0o644); err != nil {
		t.Fatal(err)
	}
	err := run()
	if err == nil || !strings.Contains(err.Error(), "Script failed (7):") {
		t.Fatalf("failed hook should abort startup with diagnostic, got %v", err)
	}
	if got := readEvents(); got != "failed\n" {
		t.Fatalf("startup continued after failed hook: %q", got)
	}

	for _, invalid := range []string{"relative/hooks", "/"} {
		hookRoot = invalid
		err := run("custom")
		if err == nil || !strings.Contains(err.Error(), "GRAPHIT_ENTRYPOINT_HOOKS_DIR must") {
			t.Fatalf("invalid hook directory %q should fail, got %v", invalid, err)
		}
		if got := readEvents(); got != "failed\n" {
			t.Fatalf("startup continued with invalid hook directory: %q", got)
		}
	}
}

type entrypointError struct {
	err    error
	output string
}

func (e *entrypointError) Error() string { return e.err.Error() + ": " + e.output }

func writeEntrypointFixture(t *testing.T, path, content string, mode os.FileMode) {
	t.Helper()
	if err := os.WriteFile(path, []byte(content), mode); err != nil {
		t.Fatal(err)
	}
}
