package store

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

const ephemeralProjectID = "01ARZ3NDEKTSV4RRFFQ69G5FAV"

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
		{"marked", `{"project":{"id":"` + ephemeralProjectID + `","ephemeral":true}}`, true},
		{"marked false", `{"project":{"id":"` + ephemeralProjectID + `","ephemeral":false}}`, false},
		{"a normal project, which says nothing", `{"project":{"id":"` + ephemeralProjectID + `"}}`, false},
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
	dir := t.TempDir()
	writeLock(t, dir, `{"project":{"id":"`+ephemeralProjectID+`","ephemeral":true}}`)

	if ProjectStoreID(dir) != ephemeralProjectID {
		t.Errorf("ProjectStoreID = %q, want the lockfile id", ProjectStoreID(dir))
	}
	if ASTProjectDirByID(ephemeralProjectID) == "" || KnowledgeProjectDirByID(ephemeralProjectID) == "" {
		t.Error("the by-id helpers must still resolve, or nothing can reclaim residue")
	}
}
