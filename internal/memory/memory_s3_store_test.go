package memory

import (
	"os"
	"path/filepath"
	"sort"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/testsupport"
)

// newFakeBackedMemoryStore wires a memory store to an in-memory bucket, with its raw directory
// root inside the test's temp dir.
//
// It cannot be used from a parallel test: the Hub configuration is injected through environment
// variables, and memory shares the Hub bucket.
func newFakeBackedMemoryStore(t *testing.T) (*MemoryStore, *testsupport.FakeS3) {
	t.Helper()

	fake, endpoint := testsupport.StartFakeS3(t, "graphit-hub")
	t.Setenv("GRAPHIT_HUB_BUCKET", "graphit-hub")
	t.Setenv("GRAPHIT_HUB_REGION", "us-east-1")
	t.Setenv("GRAPHIT_HUB_ENDPOINT", endpoint)
	t.Setenv("GRAPHIT_HUB_PREFIX", "")

	st, err := NewMemoryStore()
	if err != nil {
		t.Fatalf("NewMemoryStore: %v", err)
	}
	if !st.Configured() {
		t.Fatal("store reports itself unconfigured with a bucket set")
	}
	st.rawBase = filepath.Join(t.TempDir(), "memory-raw")
	return st, fake
}

// An unset bucket is local-only mode, which every caller must keep working in — it is what an
// unset memory.repo used to mean, and it is the mode the framework runs in until setup names a
// bucket.
func TestNewMemoryStoreWithoutABucketIsLocalOnlyNotAnError(t *testing.T) {
	t.Setenv("GRAPHIT_HUB_BUCKET", "")

	st, err := NewMemoryStore()
	if err != nil {
		t.Fatalf("NewMemoryStore without a bucket returned an error: %v", err)
	}
	if st.Configured() {
		t.Fatal("store reports itself configured with no bucket")
	}
}

// THE ACCEPTANCE CRITERION OF T6: a memory written locally reaches the bucket, under the prefix
// the branch named.
func TestMemoryPublishUploadsUnderTheScopePrefix(t *testing.T) {
	st, fake := newFakeBackedMemoryStore(t)

	const branch = "memory/project/proj-1"
	w, err := st.OpenScopeLocal(branch)
	if err != nil {
		t.Fatalf("open scope: %v", err)
	}
	if err := w.WriteFile("01ABC.md", []byte("# a memory")); err != nil {
		t.Fatalf("write: %v", err)
	}
	if err := w.WriteFile(filepath.Join("sub", "01DEF.md"), []byte("# nested")); err != nil {
		t.Fatalf("write nested: %v", err)
	}
	if err := w.Publish("adding two memories"); err != nil {
		t.Fatalf("Publish: %v", err)
	}
	WaitForPendingPushes()

	got := fake.Keys()
	sort.Strings(got)
	want := []string{
		"memory/project/proj-1/01ABC.md",
		"memory/project/proj-1/sub/01DEF.md",
	}
	if strings.Join(got, ",") != strings.Join(want, ",") {
		t.Errorf("uploaded keys = %v, want %v", got, want)
	}
	if data, ok := fake.Object("memory/project/proj-1/01ABC.md"); !ok || string(data) != "# a memory" {
		t.Errorf("object content = %q (present=%v)", data, ok)
	}
}

// A removal has to reach the bucket too. It cannot be inferred from the directory afterwards —
// the file is already gone — so the store records it when RemoveFile happens.
func TestMemoryPublishDeletesRemovedMemories(t *testing.T) {
	st, fake := newFakeBackedMemoryStore(t)

	const branch = "memory/user/abc123"
	w, err := st.OpenScopeLocal(branch)
	if err != nil {
		t.Fatalf("open scope: %v", err)
	}
	for _, name := range []string{"keep.md", "drop.md"} {
		if err := w.WriteFile(name, []byte("# "+name)); err != nil {
			t.Fatal(err)
		}
	}
	if err := w.Publish("two memories"); err != nil {
		t.Fatal(err)
	}
	WaitForPendingPushes()

	if err := w.RemoveFile("drop.md"); err != nil {
		t.Fatalf("RemoveFile: %v", err)
	}
	if err := w.Publish("removing one"); err != nil {
		t.Fatal(err)
	}
	WaitForPendingPushes()

	if _, ok := fake.Object("memory/user/abc123/drop.md"); ok {
		t.Error("the removed memory is still in the bucket")
	}
	if _, ok := fake.Object("memory/user/abc123/keep.md"); !ok {
		t.Error("the kept memory disappeared from the bucket")
	}
}

// Pull MERGES: it must not delete a memory written locally and not yet uploaded.
//
// This is the opposite of the Hub registry, which mirrors on purpose. Here the local directory is
// the truth, so mirroring would throw away the newest memory in the system.
func TestMemoryPullMergesAndKeepsUnpublishedMemories(t *testing.T) {
	st, fake := newFakeBackedMemoryStore(t)

	const branch = "memory/project/proj-2"
	fake.Put("memory/project/proj-2/remote.md", []byte("# from another machine"))

	w, err := st.OpenScopeLocal(branch)
	if err != nil {
		t.Fatalf("open scope: %v", err)
	}
	if err := w.WriteFile("local-only.md", []byte("# not uploaded yet")); err != nil {
		t.Fatal(err)
	}

	if err := w.Pull(); err != nil {
		t.Fatalf("Pull: %v", err)
	}

	if _, err := os.Stat(filepath.Join(w.Dir(), "local-only.md")); err != nil {
		t.Errorf("Pull deleted an unpublished memory: %v", err)
	}
	data, err := os.ReadFile(filepath.Join(w.Dir(), "remote.md"))
	if err != nil || string(data) != "# from another machine" {
		t.Errorf("Pull did not bring the remote memory down: %q %v", data, err)
	}
}

// A scope nobody has published yet is a normal state, not an error.
func TestMemoryPullOnAnEmptyScopeIsNotAnError(t *testing.T) {
	st, _ := newFakeBackedMemoryStore(t)

	w, err := st.OpenScopeLocal("memory/project/never-published")
	if err != nil {
		t.Fatalf("open scope: %v", err)
	}
	if err := w.Pull(); err != nil {
		t.Errorf("Pull on an empty scope: %v", err)
	}
}

// The branch-to-prefix translation is the identity, so the layout the git branches described is
// preserved exactly. A leading `memory/` must not be doubled.
func TestRemotePrefixMatchesTheBranchLayout(t *testing.T) {
	for _, c := range []struct{ branch, want string }{
		{"memory/project/proj-1", "memory/project/proj-1"},
		{"memory/user/abc123", "memory/user/abc123"},
		{"memory/mycontext/mycontext", "memory/mycontext/mycontext"},
		{"project/proj-1", "memory/project/proj-1"},
	} {
		if got := remotePrefix(c.branch); got != c.want {
			t.Errorf("remotePrefix(%q) = %q, want %q", c.branch, got, c.want)
		}
	}
}

// Prune reclaims local disk and MUST NOT delete the remote prefix: another machine may still be
// using the scope, and `git branch -D` was local too.
func TestMemoryPruneLeavesTheRemoteAlone(t *testing.T) {
	st, fake := newFakeBackedMemoryStore(t)
	t.Setenv("HOME", t.TempDir()) // isolate from the developer's global lock file

	const branch = "memory/project/proj-4"
	w, err := st.OpenScopeLocal(branch)
	if err != nil {
		t.Fatalf("open scope: %v", err)
	}
	if err := w.WriteFile("m.md", []byte("# m")); err != nil {
		t.Fatal(err)
	}
	if err := w.Publish("one memory"); err != nil {
		t.Fatal(err)
	}
	WaitForPendingPushes()

	if err := w.Prune(); err != nil {
		t.Fatalf("Prune: %v", err)
	}
	if _, err := os.Stat(w.Dir()); !os.IsNotExist(err) {
		t.Errorf("expected the raw directory to be gone, got %v", err)
	}
	if _, ok := fake.Object("memory/project/proj-4/m.md"); !ok {
		t.Error("Prune deleted the remote prefix — another machine's memories would be gone")
	}
}
