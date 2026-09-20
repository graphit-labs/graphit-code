package subagent

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/agent"
	"github.com/graphit-labs/graphit-code/internal/sessionhook"
)

// Every supported host must receive the roles from sync, without the Hub.
func TestInstallRolesWritesEveryRoleForEveryHost(t *testing.T) {
	for _, agentName := range agent.SupportedAgents() {
		t.Run(agentName, func(t *testing.T) {
			projectDir := t.TempDir()
			if err := InstallRoles(projectDir, agentName); err != nil {
				t.Fatal(err)
			}
			for _, role := range sessionhook.Roles() {
				path := installedRolePath(t, projectDir, agentName, role)
				if path == "" {
					t.Fatalf("%s did not install role %s", agentName, role.Name)
				}
				content := readFile(t, path)
				if !strings.Contains(content, "Delegated work contract:") {
					t.Errorf("%s/%s lost the delegated contract: %s", agentName, role.Name, content)
				}
				if strings.Contains(content, "graphit_task_session_create") {
					t.Errorf("%s/%s instructs session creation, which a performer must never do", agentName, role.Name)
				}
			}
		})
	}
}

// Rewriting identical files on every periodic sync wakes the file watcher for
// nothing, which is why the skill installer caches a content hash.
func TestInstallRolesIsIdempotent(t *testing.T) {
	projectDir := t.TempDir()
	if err := InstallRoles(projectDir, "claude"); err != nil {
		t.Fatal(err)
	}
	path := installedRolePath(t, projectDir, "claude", sessionhook.RoleScout)
	before, err := os.Stat(path)
	if err != nil {
		t.Fatal(err)
	}

	if err := InstallRoles(projectDir, "claude"); err != nil {
		t.Fatal(err)
	}
	after, err := os.Stat(path)
	if err != nil {
		t.Fatal(err)
	}
	if !before.ModTime().Equal(after.ModTime()) {
		t.Fatalf("an unchanged role was rewritten: %v -> %v", before.ModTime(), after.ModTime())
	}
}

// Removal must not be a blunt directory wipe: a host's agent directory is also
// where the user keeps their own agents.
func TestRemoveRolesLeavesUserAuthoredAgentsAlone(t *testing.T) {
	projectDir := t.TempDir()
	if err := InstallRoles(projectDir, "claude"); err != nil {
		t.Fatal(err)
	}
	agentDir := filepath.Dir(installedRolePath(t, projectDir, "claude", sessionhook.RoleScout))

	mine := filepath.Join(agentDir, "my-own-agent.md")
	if err := os.WriteFile(mine, []byte("---\nname: my-own-agent\n---\n\nmine\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	// A hand-written file that happens to share a managed name must also survive,
	// because it carries no managed marker.
	replaced := filepath.Join(agentDir, brand.Brand+"-scribe.md")
	if err := os.WriteFile(replaced, []byte("---\nname: graphit-scribe\n---\n\nrewritten by hand\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	if err := RemoveRoles(projectDir, "claude"); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(mine); err != nil {
		t.Errorf("removal deleted an agent the user wrote: %v", err)
	}
	if got := readFile(t, replaced); !strings.Contains(got, "rewritten by hand") {
		t.Errorf("removal discarded a hand-rewritten role: %s", got)
	}
	if _, err := os.Stat(installedRolePath(t, projectDir, "claude", sessionhook.RoleScout)); !os.IsNotExist(err) {
		t.Errorf("the managed scout role survived removal: %v", err)
	}
}

func TestProjectOverrideReplacesARoleBody(t *testing.T) {
	projectDir := t.TempDir()
	overrideDir := filepath.Join(projectDir, brand.DotDir(), "rules")
	if err := os.MkdirAll(overrideDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(overrideDir, "scout_agent.md"), []byte("project-specific scout instructions"), 0o644); err != nil {
		t.Fatal(err)
	}

	if err := InstallRoles(projectDir, "claude"); err != nil {
		t.Fatal(err)
	}
	content := readFile(t, installedRolePath(t, projectDir, "claude", sessionhook.RoleScout))
	if !strings.Contains(content, "project-specific scout instructions") {
		t.Fatalf("the project override was ignored: %s", content)
	}
}

// Only the scribe writes, and only into Task and Memory. The other two must be
// declared read-only wherever the host can express it.
func TestOnlyTheScribeIsWritable(t *testing.T) {
	projectDir := t.TempDir()
	if err := InstallRoles(projectDir, "cursor"); err != nil {
		t.Fatal(err)
	}
	for _, role := range sessionhook.Roles() {
		content := readFile(t, installedRolePath(t, projectDir, "cursor", role))
		readonly := strings.Contains(content, "readonly: true")
		if role.Name == sessionhook.RoleScribe.Name {
			if readonly {
				t.Errorf("the scribe writes and must not be marked readonly")
			}
			if !strings.Contains(content, "writes only to task and memory") {
				t.Errorf("the scribe must have its writes bounded: %s", content)
			}
			continue
		}
		if !readonly {
			t.Errorf("%s must be read-only: %s", role.Name, content)
		}
	}
}

func installedRolePath(t *testing.T, projectDir, agentName string, role sessionhook.Role) string {
	t.Helper()
	base := strings.TrimSuffix(role.RoleFileName(), ".md")
	for _, ext := range []string{".md", ".toml"} {
		matches, err := filepath.Glob(filepath.Join(projectDir, "*", "agents", base+ext))
		if err != nil {
			t.Fatal(err)
		}
		if len(matches) == 1 {
			return matches[0]
		}
	}
	return ""
}

func readFile(t *testing.T, path string) string {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	return string(data)
}
