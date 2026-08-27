package store

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func writeLock(t *testing.T, dir, body string) {
	t.Helper()
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatalf("setup: %v", err)
	}
	if err := os.WriteFile(filepath.Join(dir, brand.LockFileName()), []byte(body), 0o600); err != nil {
		t.Fatalf("setup: %v", err)
	}
}

func TestAnEphemeralProjectIsOnlyTheOneThatSaysSo(t *testing.T) {
	cases := []struct {
		name string
		body string
		want bool
	}{
		{"marked", `{"project":{"id":"01ABC","ephemeral":true}}`, true},
		{"marked false", `{"project":{"id":"01ABC","ephemeral":false}}`, false},
		{"a normal project, which says nothing", `{"project":{"id":"01ABC"}}`, false},
		{"empty project block", `{"project":{}}`, false},
		{"an empty but valid lockfile", `{}`, false},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			dir := t.TempDir()
			writeLock(t, dir, c.body)
			if got := IsEphemeralProject(dir); got != c.want {
				t.Errorf("IsEphemeralProject = %v, want %v", got, c.want)
			}
		})
	}
}

func TestAnUnreadableProjectIsNotEphemeral(t *testing.T) {
	// The answer has to be "no" for anything it cannot read, because the flag GRANTS a
	// restriction: reading it wrong in the permissive direction leaves a session with
	// stores, and reading it wrong in the other direction would deny a real project
	// its own.
	if IsEphemeralProject("") {
		t.Error("an empty path must not be ephemeral")
	}
	if IsEphemeralProject(filepath.Join(t.TempDir(), "nothing-here")) {
		t.Error("a directory with no lockfile must not be ephemeral")
	}

	broken := t.TempDir()
	writeLock(t, broken, `{"project": {"ephemeral": tru`)
	if IsEphemeralProject(broken) {
		t.Error("an unparseable lockfile must not be ephemeral")
	}
}

func TestAnEphemeralProjectStillResolvesItsStorePaths(t *testing.T) {
	// The flag changes who CONSULTS these paths, not what they are. A path that
	// stopped resolving would break the reclaim, which needs to name what an older
	// version created.
	dir := t.TempDir()
	writeLock(t, dir, `{"project":{"id":"01SESSION","ephemeral":true}}`)

	if ProjectStoreID(dir) != "01SESSION" {
		t.Errorf("ProjectStoreID = %q, want the lockfile id", ProjectStoreID(dir))
	}
	if ASTProjectDirByID("01SESSION") == "" || KnowledgeProjectDirByID("01SESSION") == "" {
		t.Error("the by-id helpers must still resolve, or nothing can reclaim residue")
	}
}
