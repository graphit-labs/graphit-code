package hub

import (
	"fmt"
	"os"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/ide"
)

const hubBlockName = "HUB_DISCOVERY"

func HubRuleContent(installed []InstalledArtifactInfo) string {
	binName := brand.BinName()
	dotBrand := brand.DotDir()

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
		"| `ast` | Pre-indexed code graph of a framework's source code | Query with `" + binName + " ast query \"...\" --context <id>` |",
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
		"To see all available artifacts or filter by type:",
		"```bash",
		binName + " hub list",
		binName + " hub list --type <knowledge|ast|rule|skill|command|agent|mcp|power>",
		"```",
		"",
		"### 2. Inspection",
		"To see the details, tags, and description of a specific artifact:",
		"```bash",
		binName + " hub show <artifact-id>",
		"```",
		"",
		"### 3. Installation",
		"To download and install the artifact into the current project:",
		"```bash",
		binName + " hub install <artifact-id> --ide <antigravity|gemini|claude|cursor|kiro|codex|opencode>",
		"```",
		"",
		"### 4. Updates",
		"To keep all installed artifacts up to date with the latest versions:",
		"```bash",
		binName + " hub update",
		"```",
		"",
		"## Using Installed Artifacts",
		"",
		"Once installed, artifacts enhance your capabilities automatically:",
		"",
		"- **Knowledge**: Read the wiki `" + dotBrand + "/knowledge/<id>/index.md` to understand",
		"  a framework's API, architecture, and patterns — never guess.",
		"- **AST**: Query the code graph to find functions, classes, and relationships",
		"  in the framework's source: `" + binName + " ast query \"...\" --context <id>`",
		"- **Rules**: Automatically injected — follow the conventions they define.",
		"- **Skills**: Read the skill when the task matches its domain. Skills appear",
		"  in the IDE's skills directory.",
		"- **Commands**: Execute pre-built workflows from the IDE's commands directory.",
		"- **Agents**: Delegate specialized tasks to agent personas with domain expertise.",
		"- **MCPs**: External tool integrations are auto-configured — use them as available tools.",
		"- **Powers**: All bundled artifacts are installed — use each by its individual type.",
	}

	lines = append(lines, "", "## Installed Artifacts", "")

	if len(installed) == 0 {
		lines = append(lines,
			"> No hub artifacts are currently installed in this project.",
			"",
			"Run `"+binName+" hub install <artifact-id> --ide <ide>` to install one.",
		)
	} else {
		lines = append(lines,
			"The following hub artifacts are installed in this project.",
			"**Use the paths and commands below — never guess their APIs.**",
			"",
			"| ID | Name | Type | Description |",
			"|---|---|---|---|",
		)
		for _, a := range installed {
			desc := a.Description
			if desc == "" {
				desc = "—"
			}
			name := a.Name
			if name == "" {
				name = a.ID
			}
			lines = append(lines,
				fmt.Sprintf("| `%s` | %s | %s | %s |", a.ID, name, a.Type, desc),
			)
		}
		lines = append(lines, "")

		byType := make(map[ArtifactType][]InstalledArtifactInfo)
		for _, a := range installed {
			byType[a.Type] = append(byType[a.Type], a)
		}

		if arts, ok := byType[TypeKnowledge]; ok {
			lines = append(lines, "### Knowledge Wiki Paths", "")
			for _, a := range arts {
				pathID := a.ProjectID
				if pathID == "" {
					pathID = a.ID
				}
				wikiPath := dotBrand + "/knowledge/" + pathID + "/index.md"
				lines = append(lines,
					"**"+a.Name+" (`"+a.ID+"`)** — "+a.Description,
					"  - Index: `"+wikiPath+"`",
					"",
				)
			}
		}

		if arts, ok := byType[TypeAST]; ok {
			lines = append(lines, "### AST Contexts", "")
			for _, a := range arts {
				contextID := a.ProjectID
				if contextID == "" {
					contextID = a.ID
				}
				lines = append(lines,
					"**"+a.Name+" (`"+a.ID+"`)** — "+a.Description,
					"  - Query: `"+binName+" ast query \"MATCH (n) RETURN n.name LIMIT 10\" --context "+contextID+"`",
					"",
				)
			}
		}

		if arts, ok := byType[TypeRule]; ok {
			lines = append(lines, "### Installed Rules", "")
			for _, a := range arts {
				lines = append(lines,
					"**"+a.Name+" (`"+a.ID+"`)** — "+a.Description,
					"  - Auto-injected into IDE rules. Follow the conventions defined.",
					"",
				)
			}
		}

		if arts, ok := byType[TypeSkill]; ok {
			lines = append(lines, "### Installed Skills", "")
			for _, a := range arts {
				lines = append(lines,
					"**"+a.Name+" (`"+a.ID+"`)** — "+a.Description,
					"  - Read this skill when its domain matches your current task.",
					"",
				)
			}
		}

		if arts, ok := byType[TypeCommand]; ok {
			lines = append(lines, "### Installed Commands", "")
			for _, a := range arts {
				lines = append(lines,
					"**"+a.Name+" (`"+a.ID+"`)** — "+a.Description,
					"  - Available in IDE commands directory.",
					"",
				)
			}
		}

		if arts, ok := byType[TypeAgent]; ok {
			lines = append(lines, "### Installed Agents", "")
			for _, a := range arts {
				lines = append(lines,
					"**"+a.Name+" (`"+a.ID+"`)** — "+a.Description,
					"  - Pre-configured agent persona with domain expertise.",
					"",
				)
			}
		}

		if arts, ok := byType[TypeMCP]; ok {
			lines = append(lines, "### Installed MCP Servers", "")
			for _, a := range arts {
				lines = append(lines,
					"**"+a.Name+" (`"+a.ID+"`)** — "+a.Description,
					"  - Auto-configured in IDE MCP settings. Available as external tools.",
					"",
				)
			}
		}
	}

	lines = append(lines,
		"",
		"## 🌐 Ecosystem Project Discovery",
		"",
		"**When you need to find other projects in the work ecosystem** (e.g., to understand",
		"cross-project dependencies, shared libraries, related services, or sibling projects),",
		"**consult the project lock file:**",
		"",
		"```",
		dotBrand+"/cluster.lock.json",
		"```",
		"",
		"This file is **automatically generated** during `"+binName+" sync` and contains only the",
		"sibling projects that belong to the **same cluster** as the current project.",
		"Clusters are managed via `"+binName+" cluster <key> <value>` — projects sharing at",
		"least one identical cluster label are grouped together. Projects without any labels",
		"form their own default group.",
		"",
		"Each sibling project entry includes:",
		"",
		"| Field | Description |",
		"|---|---|",
		"| `projects.<id>.dir` | Absolute path to the project root directory |",
		"| `projects.<id>.name` | Human-readable project name |",
		"| `projects.<id>.description` | Project description |",
		"| `projects.<id>.cluster` | Cluster labels (key→value map) |",
		"| `projects.<id>.registeredAt` | When the project was registered |",
		"",
		"**With the project paths from this file you can:**",
		"",
		"- **Discover and navigate** — find sibling project directories and read their source, docs, or lockfile",
		"- **Query code in another project** — run AST or full-text search against a sibling:",
		"  ```bash",
		"  cd /path/to/other-project && "+binName+" ast query \"MATCH (f:Function) WHERE toLower(f.name) CONTAINS 'handler' RETURN f.name, f.path\" --ai-optimized",
		"  ```",
		"- **Read another project's knowledge wiki** — understand its architecture without grepping:",
		"  ```bash",
		"  cat /path/to/other-project/"+dotBrand+"/knowledge/project/index.md",
		"  ```",
		"- **Make cross-project changes** — if the user asks to modify code in another project,",
		"  use the path from `cluster.lock.json` to locate, read, and edit files there directly",
		"",
		"**Example workflow:** The user asks \"how does the auth service validate tokens?\".",
		"You read `"+dotBrand+"/cluster.lock.json`, find the auth service project path,",
		"then run `cd /path/to/auth-service && "+binName+" ast query \"MATCH (f:Function) WHERE toLower(f.name) CONTAINS 'validate' RETURN f.name, f.path, f.line_number\" --ai-optimized`",
		"to locate the validation logic, and read the relevant source files.",
	)

	lines = append(lines,
		"",
		"## ⚠️ Rule",
		"",
		"Rely entirely on the official artifacts from the Hub rather than generic internet knowledge.",
		"When in doubt: `"+binName+" hub list` → `"+binName+" hub show <id>` → `"+binName+" hub install <id>`.",
	)

	return strings.Join(lines, "\n") + "\n"
}

var hubSkillName = brand.SkillDirName("hub")

func HubRouterContent(installed []InstalledArtifactInfo) string {
	binName := brand.BinName()
	dotBrand := brand.DotDir()
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
		"- When `" + binName + " ast query` returns no results for an external library (it might have a hub artifact)",
		"",
		"## 🔒 MANDATORY: Read Skill Before Acting",
		"",
		"**When ANY activation trigger above matches your current task, you MUST read the",
		"`" + hubSkillName + "` skill BEFORE executing your first Hub operation.**",
		"The Quick Reference below is a cheat sheet for agents who already read the skill —",
		"it is NOT a substitute. The skill contains artifact type details, usage patterns,",
		"installation workflows, and post-install protocols you must follow.",
		"",
		"## Quick Reference (always active)",
		"",
		"- **Search**: `" + binName + " hub list`",
		"- **Filter**: `" + binName + " hub list --type <knowledge|ast|rule|skill|command|agent|mcp|power>`",
		"- **Inspect**: `" + binName + " hub show <id>`",
		"- **Install**: `" + binName + " hub install <id> --ide <ide>`",
		"- **Update**: `" + binName + " hub update`",
		"",
		"## ⛔ Critical Rule",
		"",
		"**NEVER guess APIs or structures.** If uncertain about a framework or library,",
		"check the Hub first: `" + binName + " hub list` → `" + binName + " hub show <id>` → `" + binName + " hub install <id>`.",
		"",
		"## 🔗 Subagent Hub Access",
		"",
		"**When spawning subagents that work with external libraries, include in their prompt:**",
		`"Before implementing integrations with external libraries, check if knowledge artifacts exist: ` + "`" + binName + " hub list --type knowledge` → `" + binName + " hub install <id>`." + `"`,
		"",
		"## 🌐 Ecosystem Project Discovery",
		"",
		"**When you need to find other projects in the work ecosystem** (e.g., to understand",
		"cross-project dependencies, shared libraries, related services, or sibling projects),",
		"**consult the project lock file:**",
		"",
		"```",
		dotBrand + "/cluster.lock.json",
		"```",
		"",
		"This file is **automatically generated** during `" + binName + " sync` and contains only the",
		"sibling projects that belong to the **same cluster** as the current project.",
		"Clusters are managed via `" + binName + " cluster <key> <value>` — projects sharing at",
		"least one identical cluster label are grouped together. Projects without any labels",
		"form their own default group.",
		"",
		"Each sibling project entry includes:",
		"",
		"| Field | Description |",
		"|---|---|",
		"| `projects.<id>.dir` | Absolute path to the project root directory |",
		"| `projects.<id>.name` | Human-readable project name |",
		"| `projects.<id>.description` | Project description |",
		"| `projects.<id>.cluster` | Cluster labels (key→value map) |",
		"| `projects.<id>.registeredAt` | When the project was registered |",
		"",
		"**With the project paths from this file you can:**",
		"",
		"- **Discover and navigate** — find sibling project directories and read their source, docs, or lockfile",
		"- **Query code in another project** — run AST or full-text search against a sibling:",
		"  ```bash",
		"  cd /path/to/other-project && " + binName + " ast query \"MATCH (f:Function) WHERE toLower(f.name) CONTAINS 'handler' RETURN f.name, f.path\" --ai-optimized",
		"  ```",
		"- **Read another project's knowledge wiki** — understand its architecture without grepping:",
		"  ```bash",
		"  cat /path/to/other-project/" + dotBrand + "/knowledge/project/index.md",
		"  ```",
		"- **Make cross-project changes** — if the user asks to modify code in another project,",
		"  use the path from `cluster.lock.json` to locate, read, and edit files there directly",
		"",
		"**Example workflow:** The user asks \"how does the auth service validate tokens?\".",
		"You read `" + dotBrand + "/cluster.lock.json`, find the auth service project path,",
		"then run `cd /path/to/auth-service && " + binName + " ast query \"MATCH (f:Function) WHERE toLower(f.name) CONTAINS 'validate' RETURN f.name, f.path, f.line_number\" --ai-optimized`",
		"to locate the validation logic, and read the relevant source files.",
	}

	lines = append(lines, "", "## Installed Artifacts", "")
	if len(installed) == 0 {
		lines = append(lines,
			"> No hub artifacts are currently installed in this project.",
		)
	} else {
		lines = append(lines,
			"| ID | Type | Description |",
			"|---|---|---|",
		)
		for _, a := range installed {
			desc := a.Description
			if desc == "" {
				desc = "—"
			}
			lines = append(lines,
				fmt.Sprintf("| `%s` | %s | %s |", a.ID, a.Type, desc),
			)
		}
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

	routerContent := brand.ResolveModuleRule("hub", HubRouterContent(installed))
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
