package hub

import (
	"os"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/ide"
	paths_pkg "github.com/graphit-labs/graphit-code/internal/paths"
)

func HubRuleContent(installed []InstalledArtifactInfo) string {

	astQueryRef := brand.MCPToolRef("ast", "query")
	astQuery := brand.MCPToolName("ast", "query")
	hubListRef := brand.MCPToolRef("hub", "list")
	hubList := brand.MCPToolName("hub", "list")
	hubShowRef := brand.MCPToolRef("hub", "show")
	hubShow := brand.MCPToolName("hub", "show")
	hubInstallRef := brand.MCPToolRef("hub", "install")
	hubInstall := brand.MCPToolName("hub", "install")
	hubUpdateRef := brand.MCPToolRef("hub", "update")
	hubUpdate := brand.MCPToolName("hub", "update")
	hubLink := brand.MCPToolName("hub", "link")
	hubLinkRef := brand.MCPToolRef("hub", "link")
	hubUnlink := brand.MCPToolName("hub", "unlink")
	hubUnlinkRef := brand.MCPToolRef("hub", "unlink")
	clusterProjectsRef := brand.MCPToolRef("cluster", "projects")
	clusterProjects := brand.MCPToolName("cluster", "projects")
	clusterSetRef := brand.MCPToolRef("cluster", "set")
	clusterGetRef := brand.MCPToolRef("cluster", "get")
	clusterUnsetRef := brand.MCPToolRef("cluster", "unset")

	lines := []string{
		"# Hub Discovery Rule",
		"",
		"## Objective",
		"",
		"The Hub is a centralized registry of shareable artifacts that enrich your development",
		"environment with pre-built knowledge bases, code analysis contexts, reusable rules,",
		"skills, commands, agents, MCP integrations, and power bundles.",
		"",
		"You MUST leverage the Hub BEFORE assuming or hallucinating how any framework,",
		"library, or domain concept works.",
		"",
		"## Artifact Types",
		"",
		"The Hub provides these artifact types — each serves a different purpose:",
		"",
		"| Type | What it provides | After installation |",
		"|---|---|---|",
		"| `knowledge` | Pre-indexed documentation wiki for a framework/library | Search via " + brand.MCPToolRef("knowledge", "search") + " or " + brand.MCPToolRef("wiki", "search") + " |",
		"| `ast` | Pre-indexed code graph of a framework's source code | Query via " + astQueryRef + " tool (passing absolute `project_dir` and setting `context` parameter to the artifact ID) |",
		"| `rule` | Coding conventions, style guides, governance rules | Auto-injected into IDE rules file |",
		"| `skill` | Detailed methodology for specific tasks (e.g. testing, migration) | Available as an on-demand skill |",
		"| `command` | Reusable CLI workflows/commands | Available in IDE's commands directory |",
		"| `agent` | Pre-configured agent personas with specific expertise | Available in IDE's agents directory |",
		"| `mcp` | MCP server configurations for external tool integrations | Auto-configured in IDE's MCP settings |",
		"| `power` | Curated bundle combining multiple artifacts as a cohesive package | Installs all bundled artifacts at once |",
		"",
		"## How to use the Hub",
		"",
		"If you encounter a framework, module, or domain concept you are not fully certain",
		"about, DO NOT guess its API or structure. Check if it is available in the Hub.",
		"",
		"### 1. Discovery",
		"To see all available artifacts or filter by type, call the " + hubListRef + " tool:",
		"```",
		hubList + "(type: \"<knowledge|ast|rule|skill|command|agent|mcp|power>\")",
		"```",
		"",
		"### 2. Inspection",
		"To see the details, tags, and description of a specific artifact, call the " + hubShowRef + " tool:",
		"```",
		hubShow + "(id: \"<artifact-id>\")",
		"```",
		"",
		"### 3. Installation",
		"To download and install the artifact into the current project, call the " + hubInstallRef + " tool (passing absolute `project_dir`):",
		"```",
		hubInstall + "(project_dir: \"/path/to/project\", id: \"<artifact-id>\", ide: \"<ide>\", alias: \"<alias>\")",
		"```",
		"",
		"### 4. Updates",
		"To keep all installed artifacts up to date, call the " + hubUpdateRef + " tool (passing absolute `project_dir`):",
		"```",
		hubUpdate + "(project_dir: \"/path/to/project\")",
		"```",
		"",
		"### 5. Link & Unlink (Local Development)",
		"To link or unlink local development artifacts into the current project, call " + hubLinkRef + " or " + hubUnlinkRef + " (passing absolute `project_dir`):",
		"```",
		hubLink + "(project_dir: \"/path/to/project\", name: \"<name>\", source_path: \"/path/to/source\", type: \"<type>\")",
		hubUnlink + "(project_dir: \"/path/to/project\", name: \"<name>\", type: \"<type>\")",
		"```",
		"",
		"## Using Installed Artifacts",
		"",
		"Once installed, artifacts enhance your capabilities automatically:",
		"",
		"- **Knowledge**: Search the wiki via " + brand.MCPToolRef("knowledge", "search") + " or " + brand.MCPToolRef("wiki", "search") + " to understand",
		"  a framework's API, architecture, and patterns — never guess.",
		"- **AST**: Query the code graph of the installed context using the " + astQueryRef + " tool (passing absolute `project_dir` and setting `context` parameter to the installed artifact ID).",
		"- **Rules**: Automatically injected — follow the conventions they define.",
		"- **Skills**: Read the skill when the task matches its domain. Skills appear",
		"  in the IDE's skills directory.",
		"- **Commands**: Execute pre-built workflows from the IDE's commands directory.",
		"- **Agents**: Delegate specialized tasks to agent personas with domain expertise.",
		"- **MCPs**: External tool integrations are auto-configured — use them as available tools.",
		"- **Powers**: All bundled artifacts are installed — use each by its individual type.",
	}

	lines = append(lines,
		"",
		"## Installed Artifacts",
		"",
		"To check installed artifacts, call the "+hubListRef+" tool (passing absolute `project_dir` parameter).",
		"Use "+hubShowRef+" to inspect details of any artifact.",
	)

	lines = append(lines,
		"",
		"## 🌐 Ecosystem Project Discovery",
		"",
		"**When you need to find other projects in the work ecosystem** (e.g., to understand",
		"cross-project dependencies, shared libraries, related services, or sibling projects),",
		"**call the "+clusterProjectsRef+" tool (passing absolute `project_dir` parameter):**",
		"",
		"```",
		clusterProjects+"(project_dir: \"/path/to/project\")",
		"```",
		"",
		"This tool returns a JSON map containing all sibling projects that belong to the **same cluster**",
		"as the current project. Clusters are managed via "+clusterSetRef+", "+clusterGetRef+",",
		"and "+clusterUnsetRef+" MCP tools — projects sharing at least one identical cluster label",
		"are grouped together. Projects without any labels form their own default group.",
		"",
		"Each sibling project entry includes:",
		"",
		"| Field | Description |",
		"|---|---|",
		"| `dir` | Absolute path to the project root directory |",
		"| `name` | Human-readable project name |",
		"| `description` | Project description |",
		"| `cluster` | Cluster labels (key→value map) |",
		"| `registeredAt` | When the project was registered |",
		"",
		"**With the project paths from this tool you can:**",
		"",
		"- **Discover and navigate** — find sibling project directories and read their source or docs",
		"- **Query code in another project** — run AST query against a sibling (always pass its absolute path in the `project_dir` parameter):",
		"  ```",
		"  "+astQuery+"(project_dir: \"/path/to/other-project\", query: \"MATCH (f:Function) WHERE toLower(f.name) CONTAINS 'handler' RETURN f.name, f.path\")",
		"  ```",
		"- **Read another project's knowledge wiki** — understand its architecture without grepping by calling "+brand.MCPToolRef("wiki", "search")+" with the other project's `project_dir`",
		"- **Make cross-project changes** — if the user asks to modify code in another project,",
		"  use the path from the tool output to locate, read, and edit files there directly",
		"",
		"**Example workflow:** The user asks \"how does the auth service validate tokens?\".",
		"You call "+clusterProjectsRef+" to find the auth service project path,",
		"then call "+astQueryRef+" with `project_dir: \"/path/to/auth-service\"` and `query: \"MATCH (f:Function) WHERE toLower(f.name) CONTAINS 'validate' RETURN f.name, f.path, f.line_number\"` to locate the validation logic, and read the relevant source files.",
	)

	lines = append(lines,
		"",
		"## ⚠️ Rule",
		"",
		"Rely entirely on the official artifacts from the Hub rather than generic internet knowledge.",
		"When in doubt: call "+hubListRef+" → "+hubShowRef+" → "+hubInstallRef+".",
	)

	return strings.Join(lines, "\n") + "\n"
}

var hubSkillName = brand.SkillDirName("hub")

func MandateTrigger() string {
	hubListRef := brand.MCPToolRef("hub", "list")
	hubShowRef := brand.MCPToolRef("hub", "show")
	hubInstallRef := brand.MCPToolRef("hub", "install")
	hubUpdateRef := brand.MCPToolRef("hub", "update")
	clusterRef := brand.MCPToolRef("cluster", "projects")
	astQueryRef := brand.MCPToolRef("ast", "query")

	return `
# Hub Discovery

> Centralized registry of knowledge, AST, rules, skills, commands, agents, MCPs, powers and languages.
> **Detailed instructions are in the ` + "`" + brand.SkillDirName("hub") + "`" + ` skill.**

## Activation Triggers:

- Working with a third-party library, framework, or API you haven't used in this session
- Needing documentation or code examples for an external dependency
- Looking for reusable rules, skills, commands, agents, or MCP servers
- Setting up a new project or adding new dependencies
- When ` + astQueryRef + ` returns no results for an external library (it might have a hub artifact)

## Quick Reference (always active)

- **Search**: call ` + hubListRef + ` tool
- **Filter**: call ` + hubListRef + ` tool with ` + "`type`" + ` parameter
- **Inspect**: call ` + hubShowRef + ` tool with ` + "`id`" + ` parameter
- **Install**: call ` + hubInstallRef + ` tool (passing absolute ` + "`project_dir`" + ` parameter)
- **Update**: call ` + hubUpdateRef + ` tool (passing absolute ` + "`project_dir`" + ` parameter)
- **Ecosystem**: call ` + clusterRef + ` tool to find sibling projects — query their AST/wiki using their project_dir

## Critical Rule

**NEVER guess APIs or structures.** If uncertain about a framework or library,
check the Hub first: call ` + hubListRef + ` → ` + hubShowRef + ` → ` + hubInstallRef + `.
After installing a knowledge artifact, search its wiki via MCP BEFORE coding.
`
}

func InstallRule(projectDir, ideName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}

	if err := ide.UpsertMandateTrigger(projectDir, ideName, "hub_rule", MandateTrigger()); err != nil {
		return err
	}

	return InstallSkill(projectDir, ideName)
}

func InstallSkill(projectDir, ideName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}

	// Fast-path: if the skill hash file is newer than the project lockfile,
	// the installed artifact list hasn't changed — skip the expensive
	// LoadInstalledArtifacts() → NewRegistryManager() chain.
	if skipHubSkillGeneration(projectDir, ideName) {
		return nil
	}

	installed := LoadInstalledArtifacts()
	skillContent := brand.ResolveModuleSkill("hub", HubRuleContent(installed))
	frontmatter := "---\nname: " + hubSkillName + "\ndescription: Centralized registry of knowledge, AST, rules, skills, commands, agents, MCPs, and powers. Use when working with external libraries, APIs, or frameworks to find pre-built knowledge artifacts. Check the hub BEFORE implementing integrations with unfamiliar systems. Also use to install/update artifacts and discover reusable components.\n---\n\n"
	return ide.InstallManagedSkill(projectDir, ideName, hubSkillName, frontmatter+skillContent)
}

// skipHubSkillGeneration returns true if the hub skill hash file exists and
// is newer than the project lockfile. This means installed artifacts haven't
// changed since the last skill generation, so we can skip the expensive
// LoadInstalledArtifacts → NewRegistryManager call chain.
func skipHubSkillGeneration(projectDir, ideName string) bool {
	hashFile := ide.ManagedSkillHashCachePath(projectDir, ideName, hubSkillName)
	if hashFile == "" {
		return false
	}
	hashInfo, err := os.Stat(hashFile)
	if err != nil {
		return false // no hash file yet
	}
	pp := paths_pkg.GetPaths(projectDir, false)
	lockInfo, err := os.Stat(pp.LockFilePath)
	if err != nil {
		return false // no lockfile
	}
	// If hash file is newer than lockfile, installed artifacts haven't changed.
	return hashInfo.ModTime().After(lockInfo.ModTime())
}


func RemoveRule(projectDir, ideName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}

	return ide.RemoveMandateTrigger(projectDir, ideName, "hub_rule")
}

func RemoveSkill(projectDir, ideName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}
	return ide.RemoveManagedSkill(projectDir, ideName, hubSkillName)
}
