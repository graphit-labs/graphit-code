// Package subagent ships the delegated Graphit roles as a native surface,
// installed and refreshed by sync exactly like the module skills. They are not
// Hub artifacts: the roles are part of the tool, not optional catalogue content.
package subagent

import (
	"os"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/agent"
	"github.com/graphit-labs/graphit-code/internal/sessioncontext"
	"github.com/graphit-labs/graphit-code/internal/sessionhook"
)

// InstallRoles writes every delegated role into the host's agent directory.
func InstallRoles(projectDir, agentName string) error {
	projectDir = resolveProjectDir(projectDir)
	for _, role := range sessionhook.Roles() {
		if err := agent.InstallManagedAgent(projectDir, agentName, descriptorFor(projectDir, role)); err != nil {
			return err
		}
	}
	return nil
}

// RemoveRoles deletes the roles this tool installed, leaving any agent the user
// authored in the same directory untouched.
func RemoveRoles(projectDir, agentName string) error {
	projectDir = resolveProjectDir(projectDir)
	for _, role := range sessionhook.Roles() {
		if err := agent.RemoveManagedAgent(projectDir, agentName, roleFileBase(role)); err != nil {
			return err
		}
	}
	return nil
}

func resolveProjectDir(projectDir string) string {
	if projectDir != "" {
		return projectDir
	}
	wd, err := os.Getwd()
	if err != nil {
		return ""
	}
	return wd
}

func roleFileBase(role sessionhook.Role) string {
	return strings.TrimSuffix(role.RoleFileName(), ".md")
}

func descriptorFor(projectDir string, role sessionhook.Role) agent.AgentDescriptor {
	return agent.AgentDescriptor{
		Name:        roleFileBase(role),
		Description: role.Summary,
		ReadOnly:    len(role.Writes) == 0,
		AllowsShell: role.Name == sessionhook.RoleTracker.Name,
		Body:        brand.ResolveRoleBodyIn(projectDir, role.Name, RoleBody(projectDir, role)),
	}
}

// RoleBody is the document whoever performs the role works from. It is the
// host's system prompt when the role runs as a subagent, and the file the main
// agent reads when its host has no subagent to delegate to, so it is addressed
// to the performer either way.
func RoleBody(projectDir string, role sessionhook.Role) string {
	context := sessioncontext.BuildForRole(projectDir, role)
	sections := []string{
		sessionhook.RoleProtocol(role, context),
		"",
		"# How to work",
		"",
		roleGuidance(role),
	}
	return strings.Join(sections, "\n")
}

func roleGuidance(role sessionhook.Role) string {
	switch role.Name {
	case sessionhook.RoleScout.Name:
		return strings.Join([]string{
			"You are asked a question. Answer it from what is already recorded, and stop as soon as the evidence answers it.",
			"",
			"- Search before reading. A search answers with identifiers; read only the ones you selected.",
			"- Prefer narrow queries and small result caps. Widen only for a concrete gap you can name.",
			"- Report where each claim came from. An answer the coordinator cannot reopen is worth little, because it has to trust you instead of checking.",
			"- Say plainly when the records do not answer the question. An honest gap is useful; a plausible guess is not.",
		}, "\n")
	case sessionhook.RoleTracker.Name:
		return strings.Join([]string{
			"You are given changed files and the requirements they serve. Report what else those changes touch.",
			"",
			"- Find the dependents of what changed, the tests that cover it, and the documentation that describes it.",
			"- Name each divergence you find: a statement that is now false, a test that no longer matches, a caller that was missed.",
			"- Point at things precisely, as `file:line` or a page, so each one can be opened.",
			"- Do not decide whether the delivery is good enough. That needs the requirement context the coordinator holds; report the surface and let it judge.",
		}, "\n")
	case sessionhook.RoleScribe.Name:
		return strings.Join([]string{
			"You are given content that is already decided. Record it faithfully, in the right structure.",
			"",
			"- Read the module skill before writing, so the record has the fields and relationships that make it usable later.",
			"- Write what you were given. You are not the author: do not improve, summarise away, or invent the parts that are missing.",
			"- When what you were given is incomplete or contradictory, record what is sound and say what is missing instead of filling the gap.",
			"- Report the ids you wrote, so the coordinator can read them back.",
		}, "\n")
	}
	return ""
}
