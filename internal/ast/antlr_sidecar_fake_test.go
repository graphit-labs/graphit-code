package ast

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"
)

// The sidecar's three existing tests all skip unless ANTLR_SIDECAR_BIN points at
// a real grammar binary, so on an ordinary run the driver has no coverage at all
// — including its failure handling, which is where its defects are.
//
// This builds a stand-in that speaks the same length-prefixed protocol and can be
// told to misbehave in the specific ways a real sidecar can: die mid-session,
// announce a response far larger than it sends, or stop answering. None of that
// needs ANTLR, so it runs everywhere.

// fakeSidecarSrc is a complete sidecar whose behaviour is chosen by
// FAKE_SIDECAR_MODE. It implements the client side of the protocol in
// antlr_sidecar.go: request [4-byte LE length][grammar\0][source], response
// [4-byte LE length][1-byte status][JSON].
const fakeSidecarSrc = `package main

import (
	"encoding/binary"
	"io"
	"os"
	"time"
)

func main() {
	mode := os.Getenv("FAKE_SIDECAR_MODE")
	served := 0
	for {
		var n uint32
		if err := binary.Read(os.Stdin, binary.LittleEndian, &n); err != nil {
			return
		}
		buf := make([]byte, n)
		if _, err := io.ReadFull(os.Stdin, buf); err != nil {
			return
		}
		served++

		switch mode {
		case "die_after_first":
			if served > 1 {
				os.Exit(1)
			}
		case "hang":
			time.Sleep(10 * time.Minute)
		case "huge_length":
			// Announce 3 GB, send nothing. A client that trusts the header
			// allocates 3 GB before discovering the stream is short.
			_ = binary.Write(os.Stdout, binary.LittleEndian, uint32(3<<30))
			continue
		}

		body := []byte("\x00" + ` + "`" + `{"rule":"root","start":[1,0],"end":[1,1]}` + "`" + `)
		_ = binary.Write(os.Stdout, binary.LittleEndian, uint32(len(body)))
		_, _ = os.Stdout.Write(body)
	}
}
`

// buildFakeSidecar compiles the stand-in once per test run.
var (
	fakeSidecarOnce sync.Once
	fakeSidecarPath string
	fakeSidecarErr  error
)

func buildFakeSidecar(t *testing.T) string {
	t.Helper()
	fakeSidecarOnce.Do(func() {
		dir, err := os.MkdirTemp("", "fake-sidecar-*")
		if err != nil {
			fakeSidecarErr = err
			return
		}
		if err := os.WriteFile(filepath.Join(dir, "main.go"), []byte(fakeSidecarSrc), 0o644); err != nil {
			fakeSidecarErr = err
			return
		}
		if err := os.WriteFile(filepath.Join(dir, "go.mod"),
			[]byte("module fakesidecar\n\ngo 1.21\n"), 0o644); err != nil {
			fakeSidecarErr = err
			return
		}
		bin := filepath.Join(dir, "fake-sidecar")
		cmd := exec.Command("go", "build", "-o", bin, ".")
		cmd.Dir = dir
		if out, err := cmd.CombinedOutput(); err != nil {
			fakeSidecarErr = err
			t.Logf("build output: %s", out)
			return
		}
		fakeSidecarPath = bin
	})
	if fakeSidecarErr != nil {
		t.Skipf("cannot build the stand-in sidecar: %v", fakeSidecarErr)
	}
	return fakeSidecarPath
}

func withFakeMode(t *testing.T, mode string) {
	t.Helper()
	old, had := os.LookupEnv("FAKE_SIDECAR_MODE")
	if err := os.Setenv("FAKE_SIDECAR_MODE", mode); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		if had {
			_ = os.Setenv("FAKE_SIDECAR_MODE", old)
		} else {
			_ = os.Unsetenv("FAKE_SIDECAR_MODE")
		}
	})
}

// A healthy sidecar answers repeatedly through one pooled process.
func TestSidecarFakeHappyPath(t *testing.T) {
	bin := buildFakeSidecar(t)
	withFakeMode(t, "")

	d := NewSidecarDriver(bin, "plsql", 2)
	defer d.Close()

	for i := 0; i < 5; i++ {
		tree, err := d.Parse([]byte("SELECT 1"))
		if err != nil {
			t.Fatalf("request %d: %v", i, err)
		}
		if tree == nil || tree.Rule != "root" {
			t.Fatalf("request %d: unexpected tree %+v", i, tree)
		}
	}
}

// A sidecar that dies must not poison the pool.
//
// The driver restarts a process whose call failed, but on the paths where the
// restart itself is not attempted — or fails — it pushed the process it had just
// closed back into the pool, with a comment saying this avoided a deadlock. It
// avoided the deadlock by handing a dead process to the next caller, so one crash
// removed a pool slot permanently.
func TestSidecarDeadProcessIsNotReturnedToThePool(t *testing.T) {
	bin := buildFakeSidecar(t)
	withFakeMode(t, "die_after_first")

	d := NewSidecarDriver(bin, "plsql", 1)
	defer d.Close()

	if _, err := d.Parse([]byte("SELECT 1")); err != nil {
		t.Fatalf("first request should succeed: %v", err)
	}

	// The process exits during this one; the driver restarts and retries, and
	// the replacement also dies on its second request. Whatever the outcome, the
	// pool must not end up holding a corpse.
	_, _ = d.Parse([]byte("SELECT 2"))

	// Every later request must still be served by a live process.
	for i := 0; i < 3; i++ {
		done := make(chan error, 1)
		go func() {
			_, err := d.Parse([]byte("SELECT 3"))
			done <- err
		}()
		select {
		case err := <-done:
			if err != nil && strings.Contains(err.Error(), "file already closed") {
				t.Fatalf("request %d drew a closed process from the pool: %v", i, err)
			}
		case <-time.After(20 * time.Second):
			t.Fatalf("request %d hung — the pool is holding a process that never answers", i)
		}
	}
}

// A response header that announces more than could possibly be a parse tree must
// be rejected, not allocated.
func TestSidecarRejectsAbsurdResponseLength(t *testing.T) {
	bin := buildFakeSidecar(t)
	withFakeMode(t, "huge_length")

	d := NewSidecarDriver(bin, "plsql", 1)
	defer d.Close()

	done := make(chan error, 1)
	go func() {
		_, err := d.Parse([]byte("SELECT 1"))
		done <- err
	}()

	select {
	case err := <-done:
		if err == nil {
			t.Fatal("a 3 GB response header was accepted")
		}
		if !strings.Contains(err.Error(), "too large") {
			t.Errorf("error should name the oversized frame, got: %v", err)
		}
	case <-time.After(30 * time.Second):
		t.Fatal("the driver is still trying to read a 3 GB frame")
	}
}
