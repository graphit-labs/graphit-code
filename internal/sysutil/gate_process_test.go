package sysutil

import (
	"context"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

const (
	gateHelperEnv    = "SYSUTIL_GATE_HELPER"
	gateHelperGlobal = "SYSUTIL_GATE_GLOBAL"
)

// runGateHelper is the second process: it takes the heavy slot, reports it on stdout and
// holds it until its stdin closes.
func runGateHelper() int {
	_ = os.Setenv("GRAPHIT_GLOBAL_DIR", os.Getenv(gateHelperGlobal))
	release, err := AcquireHeavy(context.Background())
	if err != nil {
		return 1
	}
	_, _ = os.Stdout.WriteString("held\n")
	buf := make([]byte, 1)
	_, _ = os.Stdin.Read(buf)
	release()
	return 0
}

// Indexing and embedding run as separate CLI processes, so the slot must hold across
// processes, not just goroutines: with one slot, this process waits for the other.
func TestAcquireHeavySerializesAcrossProcesses(t *testing.T) {
	global := t.TempDir()
	t.Setenv("GRAPHIT_GLOBAL_DIR", global)
	t.Setenv(heavySlotsEnv, "1")
	resetHeavyGate()
	t.Cleanup(resetHeavyGate)

	exe, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	holder := exec.Command(exe)
	holder.Env = append(os.Environ(), gateHelperEnv+"=1", gateHelperGlobal+"="+global, heavySlotsEnv+"=1")
	stdin, err := holder.StdinPipe()
	if err != nil {
		t.Fatal(err)
	}
	stdout, err := holder.StdoutPipe()
	if err != nil {
		t.Fatal(err)
	}
	if err := holder.Start(); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = holder.Process.Kill(); _ = holder.Wait() })

	line := make([]byte, 5)
	if _, err := stdout.Read(line); err != nil || !strings.HasPrefix(string(line), "held") {
		t.Fatalf("holder did not take the slot: %q %v", line, err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 1500*time.Millisecond)
	defer cancel()
	if release, err := AcquireHeavy(ctx); err == nil {
		release()
		t.Fatal("acquired the only slot while another process held it")
	}

	_ = stdin.Close()
	ctx2, cancel2 := context.WithTimeout(context.Background(), 15*time.Second)
	defer cancel2()
	release, err := AcquireHeavy(ctx2)
	if err != nil {
		t.Fatalf("slot never freed after the holder released it: %v", err)
	}
	release()

	if _, err := os.Stat(filepath.Join(global, "locks", "heavy-slot-0.lock")); err != nil {
		t.Fatalf("slot lock file missing: %v", err)
	}
}
