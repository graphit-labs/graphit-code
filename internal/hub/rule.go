package hub

import (
	"os"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/ide"
)

const hubBlockName = "HUB_DISCOVERY"

func HubRuleContent(installed []InstalledArtifactInfo) string {
	dotBrand := brand.DotDir()

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
		"| `knowledge` | Pre-indexed documentation wiki for a framework/library | Read the wiki at `" + dotBrand + "/knowledge/<id>/index.md` |",
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
		"- **Knowledge**: Read the wiki `" + dotBrand + "/knowledge/<id>/index.md` to understand",
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
		"**call the " + clusterProjectsRef + " tool (passing absolute `project_dir` parameter):**",
		"",
		"```",
		clusterProjects + "(project_dir: \"/path/to/project\")",
		"```",
		"",
		"This tool returns a JSON map containing all sibling projects that belong to the **same cluster**",
		"as the current project. Clusters are managed via " + clusterSetRef + ", " + clusterGetRef + ",",
		"and " + clusterUnsetRef + " MCP tools — projects sharing at least one identical cluster label",
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
		"  " + astQuery + "(project_dir: \"/path/to/other-project\", query: \"MATCH (f:Function) WHERE toLower(f.name) CONTAINS 'handler' RETURN f.name, f.path\", ai_optimized: true)",
		"  ```",
		"- **Read another project's knowledge wiki** — understand its architecture without grepping by using the `view_file` (or read file) tool on:",
		"  ```",
		"  /path/to/other-project/"+dotBrand+"/knowledge/project/index.md",
		"  ```",
		"- **Make cross-project changes** — if the user asks to modify code in another project,",
		"  use the path from the tool output to locate, read, and edit files there directly",
		"",
		"**Example workflow:** The user asks \"how does the auth service validate tokens?\".",
		"You call " + clusterProjectsRef + " to find the auth service project path,",
		"then call " + astQueryRef + " with `project_dir: \"/path/to/auth-service\"`, `query: \"MATCH (f:Function) WHERE toLower(f.name) CONTAINS 'validate' RETURN f.name, f.path, f.line_number\"`, and `ai_optimized: true` to locate the validation logic, and read the relevant source files.",
	)

	lines = append(lines,
		"",
		"## ⚠️ Rule",
		"",
		"Rely entirely on the official artifacts from the Hub rather than generic internet knowledge.",
		"When in doubt: call " + hubListRef + " → " + hubShowRef + " → " + hubInstallRef + ".",
	)

	return strings.Join(lines, "\n") + "\n"
}

var hubSkillName = brand.SkillDirName("hub")

func HubRouterContent(installed []InstalledArtifactInfo, globalRulesFile string) string {
	hubListRef := brand.MCPToolRef("hub", "list")
	hubShowRef := brand.MCPToolRef("hub", "show")
	hubInstallRef := brand.MCPToolRef("hub", "install")
	hubUpdateRef := brand.MCPToolRef("hub", "update")
	clusterProjectsRef := brand.MCPToolRef("cluster", "projects")
	astQueryRef := brand.MCPToolRef("ast", "query")

	lines := []string{
		"# 🔗 Hub Discovery",
		"",
		"> Centralized registry of knowledge, AST, rules, skills, commands, agents, MCPs, and powers.",
		"> **Detailed instructions are in the `" + hubSkillName + "` skill.**",
		"",
		"## Activation Triggers — You MUST read the `" + hubSkillName + "` skill when:",
		"",
		"- Working with a third-party library, framework, or API you haven't used in this session",
		"- Needing documentation or code examples for an external dependency",
		"- Looking for reusable rules, skills, commands, agents, or MCP servers",
		"- Setting up a new project or adding new dependencies",
		"- When " + astQueryRef + " returns no results for an external library (it might have a hub artifact)",
		"",
		"## 🔒 MANDATORY: Read Skill Before Acting",
		"",
		"**When ANY activation trigger above matches your current task, you MUST read the",
		"`" + hubSkillName + "` skill BEFORE executing your first Hub operation.**",
		"The skill contains artifact types, usage patterns, installation workflows,",
		"ecosystem project discovery, and post-install protocols.",
		"",
		"## Quick Reference (always active)",
		"",
		"- **Search**: call " + hubListRef + " tool",
		"- **Filter**: call " + hubListRef + " tool with `type` parameter",
		"- **Inspect**: call " + hubShowRef + " tool with `id` parameter",
		"- **Install**: call " + hubInstallRef + " tool (passing absolute `project_dir` parameter)",
		"- **Update**: call " + hubUpdateRef + " tool (passing absolute `project_dir` parameter)",
		"- **Ecosystem**: call " + clusterProjectsRef + " tool to find sibling projects",
		"",
		"## ⛔ Critical Rule",
		"",
		"**NEVER guess APIs or structures.** If uncertain about a framework or library,",
		"check the Hub first: call " + hubListRef + " → " + hubShowRef + " → " + hubInstallRef + ".",
		"",
		"## 🔗 Subagent Propagation",
		"",
		"When spawning subagents that work with external libraries, include in their prompt:",
		`"Before implementing integrations, check Hub for knowledge artifacts: call ` + hubListRef + ` → ` + hubInstallRef + ` (passing absolute ` + "`project_dir`" + `). Read the project's ` + "`" + globalRulesFile + "`" + ` before starting work."`,
	}
	return strings.Join(lines, "\n") + "\n"
}

func InstallRule(projectDir, ideName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}

	installed := LoadInstalledArtifacts()

	routerContent := brand.ResolveModuleRule("hub", HubRouterContent(installed, ide.GlobalRulesFile(ideName)))
	if err := ide.InjectManagedBlock(projectDir, ideName, hubBlockName, routerContent); err != nil {
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
	installed := LoadInstalledArtifacts()
	skillContent := brand.ResolveModuleSkill("hub", HubRuleContent(installed))
	frontmatter := "---\nname: " + hubSkillName + "\ndescription: Centralized registry of knowledge, AST, rules, skills, commands, agents, MCPs, and powers. Use when working with external libraries, APIs, or frameworks to find pre-built knowledge artifacts. Check the hub BEFORE implementing integrations with unfamiliar systems. Also use to install/update artifacts and discover reusable components.\n---\n\n"
	return ide.InstallManagedSkill(projectDir, ideName, hubSkillName, frontmatter+skillContent)
}

func RemoveRule(projectDir, ideName string) error {
	if projectDir == "" {
		var err error
		projectDir, err = os.Getwd()
		if err != nil {
			return err
		}
	}

	return ide.RemoveManagedBlock(projectDir, ideName, hubBlockName)
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
