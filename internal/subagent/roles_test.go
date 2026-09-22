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
				for _, want := range []string{"then finish your turn", "Do not run a waiting loop", "follow-up/resume mechanism when supported", "may later send you another instruction", "Reuse is optional", "no keep-alive, reuse or explicit dismissal is required", "never create, claim or close a coordination session"} {
					if !strings.Contains(content, want) {
						t.Errorf("%s/%s lost lifecycle guidance %q", agentName, role.Name, want)
					}
				}
				for _, want := range []string{"Subagent default: create none unless", "explicitly assigns bounded work to a delegated role", "authorizes and requires only that role and work", "already in that role works directly, not recursively", "Task/session claims, revisions, completion and lifecycle stay with the coordinator"} {
					if !strings.Contains(content, want) {
						t.Errorf("%s/%s lost bounded delegation guidance %q", agentName, role.Name, want)
					}
				}
				for _, forbidden := range []string{"Stay alive", "Never end yourself", "waits forever", "must reuse the same delegate", "should reuse the same delegate"} {
					if strings.Contains(content, forbidden) {
						t.Errorf("%s/%s requires a waiting turn: %q", agentName, role.Name, forbidden)
					}
				}
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

func TestStaticRoleContextNeverSerializesMandatoryMemory(t *testing.T) {
	const runtimeOnly = "runtime-only-mandatory-memory-sentinel"
	context := staticRoleContext(sessionhook.Context{
		Mandatory:       runtimeOnly,
		MandatoryLoaded: true,
	})
	body := sessionhook.RoleProtocol(sessionhook.RoleScout, context)

	if strings.Contains(body, runtimeOnly) {
		t.Fatalf("static role persisted runtime mandatory memory: %s", body)
	}
	if strings.Contains(body, "Standing context already read from the authoritative memory table") {
		t.Fatalf("static role claims a runtime memory snapshot was already loaded: %s", body)
	}
	if !strings.Contains(body, "call `graphit_memory_mandatory` once per available scope before acting") {
		t.Fatalf("static role lost its runtime mandatory-memory instruction: %s", body)
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
