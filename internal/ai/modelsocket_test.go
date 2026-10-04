package ai

import (
	"context"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"
)

func TestModelSocketPathKeepsTheGlobalDirectoryWhenItFits(t *testing.T) {
	global := filepath.Join(string(filepath.Separator)+"home", "u", ".graphit")
	for task, name := range map[ModelTask]string{ModelTaskEmbedding: "embed.sock", ModelTaskRerank: "rerank.sock"} {
		got := modelSocketPathIn(global, "/run/user/1000", 1000, modelSocketName(task))
		if want := filepath.Join(global, "daemon", name); got != want {
			t.Errorf("%s: %s, want %s", task, got, want)
		}
	}
}

// A global directory long enough to break the listen moves the socket to a short,
// per-user directory every process derives the same way.
func TestModelSocketPathMovesALongGlobalDirectoryToAShortOne(t *testing.T) {
	long := filepath.Join(string(filepath.Separator)+"tmp", strings.Repeat("deep-directory/", 14), "global")
	runtimeDir := filepath.Join(string(filepath.Separator)+"run", "user", "1000")
	embed := modelSocketPathIn(long, runtimeDir, 1000, "embed.sock")
	rerank := modelSocketPathIn(long, runtimeDir, 1000, "rerank.sock")
	for _, p := range []string{embed, rerank} {
		if len(p) > maxSocketPathLen || !strings.HasPrefix(p, runtimeDir+string(filepath.Separator)) {
			t.Fatalf("fallback %s (%d bytes) is not a short path under %s", p, len(p), runtimeDir)
		}
	}
	if filepath.Dir(embed) != filepath.Dir(rerank) || embed == rerank {
		t.Fatalf("both tasks share one directory with distinct sockets: %s %s", embed, rerank)
	}
	if again := modelSocketPathIn(long, runtimeDir, 1000, "embed.sock"); again != embed {
		t.Fatalf("not deterministic: %s then %s", embed, again)
	}
	if other := modelSocketPathIn(long+"2", runtimeDir, 1000, "embed.sock"); other == embed {
		t.Fatal("two global directories share one socket")
	}
	if other := modelSocketPathIn(long, runtimeDir, 1001, "embed.sock"); other == embed {
		t.Fatal("two users share one socket")
	}
}

// The server and its clients agree on the fallback, and the server listens and answers
// there even though the global directory itself is far too long for a socket.
func TestModelServerServesFromALongGlobalDirectory(t *testing.T) {
	base := t.TempDir()
	long := filepath.Join(base, strings.Repeat("a-rather-long-directory-name/", 5))
	t.Setenv(globalDirEnv(), long)
	runtimeDir, err := os.MkdirTemp("", "rt")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = os.RemoveAll(runtimeDir) })
	t.Setenv("XDG_RUNTIME_DIR", runtimeDir)
	t.Setenv("TMPDIR", runtimeDir)
	t.Setenv("TMP", runtimeDir)
	t.Setenv("TEMP", runtimeDir)

	sock := ModelSocketPath(ModelTaskRerank)
	if strings.HasPrefix(sock, long) || len(sock) > maxSocketPathLen {
		t.Fatalf("socket %s did not move out of the long global directory", sock)
	}
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan error, 1)
	go func() { done <- serveModelSocket(ctx, sock, rerankHandler(fixedScorer{})) }()
	t.Cleanup(func() {
		cancel()
		if err := <-done; err != nil {
			t.Errorf("model server: %v", err)
		}
	})
	if !dialModelServer(ModelTaskRerank, 5*time.Second) {
		t.Fatal("model server never accepted connections at the fallback socket")
	}
	scores, err := newProxyScorer().Score(context.Background(), "q", []string{"a", "abc"})
	if err != nil || len(scores) != 2 || scores[1] != 3 {
		t.Fatalf("scores = %v, %v", scores, err)
	}
	if runtime.GOOS != "windows" {
		info, err := os.Stat(filepath.Dir(sock))
		if err != nil || info.Mode().Perm() != 0o700 {
			t.Fatalf("fallback directory mode = %v, %v; want 0700", info.Mode().Perm(), err)
		}
	}
}

// An over-long path is reported as such instead of as an opaque listen failure.
func TestModelServerReportsASocketPathTooLongToListenOn(t *testing.T) {
	sock := filepath.Join(t.TempDir(), strings.Repeat("x", maxSocketPathLen), "embed.sock")
	err := serveModelSocket(context.Background(), sock, embeddingHandler(fixedEmbedder{}))
	if err == nil || !strings.Contains(err.Error(), "operating system accepts") {
		t.Fatalf("err = %v", err)
	}
}
