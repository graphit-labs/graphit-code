// Package prep builds the ephemeral project a live search session runs inside.
//
// It is a package of its own rather than part of livesearch because of what it has
// to import: the AST indexer, the knowledge pipeline, the Hub. Those pull in the
// generated ANTLR parsers, which take minutes to link, and the session runtime has
// no need of any of it — it takes a PrepareFunc and calls it. Keeping the seam means
// the runtime's tests stay seconds rather than minutes, which is the difference
// between running them on every change and not running them.
package prep

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/ide"
	"github.com/graphit-labs/graphit-code/internal/improvements"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/livesearch"
	"github.com/graphit-labs/graphit-code/internal/memory"
)

// The ephemeral project is a real project that nothing knows about.
//
// It gets a lockfile, an identity, the framework's rules and skills, and an MCP
// server — everything an agent CLI looks for when it starts, because the agent
// discovers all of it from its working directory and would otherwise find an empty
// folder. What it deliberately does not get is a place in the ecosystem: no entry in
// the global lock, no registration, no telemetry event. It exists for one search and
// is deleted with the session.
//
// This is why hub.OnInit is not called even though it does most of these steps.
// OnInit tracks: it writes a project.init event into the Hub's git store, and the
// baseline installs it performs register themselves in the global lock. A throwaway
// project appearing in a user's permanent records is exactly what "anonymous" has to
// rule out. The steps OnInit performs that DO belong here are performed here.

// Prepare is the livesearch.PrepareFunc that builds the workspace.
func Prepare(ctx context.Context, s *livesearch.Session, progress func(string)) error {
	ws := s.WorkspaceDir()
	meta := s.Meta()
	ideName := canonicalIDE(meta.IDE)

	if err := os.MkdirAll(ws, 0o755); err != nil {
		return fmt.Errorf("creating the workspace: %w", err)
	}

	progress("creating the ephemeral project")
	if err := writeLockfile(ws, ideName, s.ID()); err != nil {
		return err
	}
	if err := ctx.Err(); err != nil {
		return err
	}

	progress("installing the framework's rules and skills")
	installGuidance(ws, ideName, progress)
	if err := ctx.Err(); err != nil {
		return err
	}

	if err := installArtifacts(ctx, ws, ideName, meta.Artifacts, progress); err != nil {
		return err
	}

	// The work a daemon would have done for a real project, done once, inline.
	if err := prepareIndexes(ctx, ws, progress); err != nil {
		return err
	}

	// After the artifacts, not before: an MCP artifact contributes servers of its
	// own, and a configuration written first would describe the project as it was
	// a moment ago.
	progress("giving the agent access to the graphit tools")
	configureTools(ws, ideName, progress)

	return ctx.Err()
}

// artifactInstaller is the part of the Hub service this package uses.
//
// Narrowed to one method so that tests can supply one, which matters here more than
// usual: the real implementation opens the Hub registry, and opening the registry
// clones a git repository over the network. A test that exercised the real thing
// would be a test that fails on a train.
type artifactInstaller interface {
	Install(ctx context.Context, entryID, alias, ide string, entryType hub.ArtifactType, parentID, projectDir string) (*hub.InstallResult, error)
}

// newInstaller opens the Hub. It is a variable so tests can replace it.
var newInstaller = func(ctx context.Context) (artifactInstaller, error) {
	registry, err := hub.NewRegistryManager(ctx)
	if err != nil {
		return nil, fmt.Errorf("opening the hub registry: %w", err)
	}
	if !registry.IsReady() {
		// Nothing can be resolved without the registry. Failing here is the point:
		// a session that quietly prepared an empty workspace would answer "there is
		// nothing about that in these sources", which reads like a fact about the
		// sources rather than a fact about the download.
		return nil, errors.New("the hub registry is not available, so the selected artifacts cannot be installed")
	}
	return hub.NewUntrackedHubService(registry), nil
}

// installArtifacts installs the Hub artifacts chosen for this session.
//
// Any type is allowed, and the type decides where it lands: a knowledge artifact is
// copied into the project, a code graph goes to a shared versioned store that the
// lockfile entry points at, a rule or skill is copied into the IDE's directories, a
// language artifact registers a grammar. None of that is reimplemented here — it is
// what hub.Install does, and doing it differently for the live search is how the
// live search would come to disagree with the rest of the framework.
func installArtifacts(ctx context.Context, ws, ideName string, artifacts []livesearch.Artifact, progress func(string)) error {
	if len(artifacts) == 0 {
		return nil
	}

	progress(fmt.Sprintf("fetching %s from the hub", countNoun(len(artifacts), "artifact")))
	svc, err := newInstaller(ctx)
	if err != nil {
		return err
	}

	var installed int
	for _, a := range artifacts {
		if err := ctx.Err(); err != nil {
			return err
		}
		entryID := a.ID
		if a.Version != "" {
			entryID = a.ID + "@" + a.Version
		}
		res, err := svc.Install(ctx, entryID, "", ideName, hub.ArtifactType(a.Type), "", ws)
		if err != nil {
			// One artifact failing is reported and the rest still installed: a
			// search over three of four selections is worth more than no search,
			// and the report says which one is missing so an empty answer is not
			// mistaken for an empty source.
			progress(fmt.Sprintf("%s could not be installed: %v", describeArtifact(a), err))
			continue
		}
		installed++
		progress(fmt.Sprintf("installed %s %s at version %s", res.ArtType, res.EntryID, res.Version))
	}

	if installed == 0 {
		return errors.New("none of the selected artifacts could be installed")
	}
	return nil
}

// describeArtifact names an artifact for a person reading progress.
//
// The type is omitted when it was not given rather than shown as empty parentheses:
// the type is optional precisely because the registry can resolve it, so an absent
// one is a normal choice and not a missing value.
func describeArtifact(a livesearch.Artifact) string {
	name := a.ID
	if a.Version != "" {
		name += "@" + a.Version
	}
	if a.Type == "" {
		return name
	}
	return fmt.Sprintf("%s (%s)", name, a.Type)
}

func countNoun(n int, noun string) string {
	if n == 1 {
		return "1 " + noun
	}
	return fmt.Sprintf("%d %ss", n, noun)
}

// writeLockfile creates the project identity.
//
// The identity is written explicitly rather than left to SaveLockfile, for two
// reasons. It must not be empty: reconcileMCPFile treats the project ID as a claim
// on each MCP server and deletes any server nothing claims, so an empty ID would
// have the entry written and then removed in the same pass. And SaveLockfile would
// otherwise call resolveProjectIdentity, which runs `git remote get-url origin` in
// the directory — a command that walks UP the tree, so on a machine where the home
// directory is itself a git repository the throwaway project would be named after
// the user's dotfiles.
//
// The session ID is the project ID because one session is one project. Nothing else
// will ever refer to it.
//
// Ephemeral is the field that keeps the rest of the framework from treating this like
// somewhere to keep things. Having a lockfile with an ID is what earns a project its
// stores, and this one has a lockfile for an unrelated reason, so the distinction has
// to be written down where every resolver can read it.
//
// It MERGES rather than replaces. The lockfile is now the only record of what a project
// has installed, so overwriting it wholesale would drop the artifact entries — and this
// runs before the installs only by convention, which is not a property worth depending
// on.
func writeLockfile(ws, ideName, sessionID string) error {
	path := filepath.Join(ws, brand.LockFileName())
	lf, err := hub.LoadLockfile(path)
	if err != nil {
		return fmt.Errorf("reading the project lockfile: %w", err)
	}
	if lf == nil {
		lf = &hub.Lockfile{}
	}
	lf.Project = hub.ProjectIdentity{
		ID:          sessionID,
		Name:        "live-search-" + strings.ToLower(sessionID),
		Description: "Ephemeral workspace for a single live search session. Not registered in the ecosystem.",
		Ephemeral:   true,
	}
	lf.IDEs = []string{ideName}
	if lf.Artifacts == nil {
		lf.Artifacts = map[hub.ArtifactType]map[string]*hub.LockfileArtifactMeta{}
	}
	if err := hub.SaveLockfile(path, lf); err != nil {
		return fmt.Errorf("creating the project lockfile: %w", err)
	}
	return nil
}

// guidanceModule is one of the framework's five modules, in the form the installers
// take.
type guidanceModule struct {
	name  string
	rule  func(projectDir, ide string) error
	skill func(projectDir, ide string) error
}

func guidanceModules() []guidanceModule {
	return []guidanceModule{
		{"knowledge", knowledge.InstallRule, knowledge.InstallSkill},
		{"ast", ast.InstallRule, ast.InstallSkill},
		{"hub", hub.InstallRule, hub.InstallSkill},
		{"memory", memory.InstallRule, memory.InstallSkill},
		{"improvements", improvements.InstallRule, improvements.InstallSkill},
	}
}

// installGuidance writes the module rules and skills into the workspace, and the
// mandate with them — InstallRule upserts it.
//
// This is the part that makes injecting instructions into the prompt unnecessary:
// the agent reads them from the working directory, as files, the same way it would
// in a real project, and can re-read them as it works.
//
// A module that fails to install is reported and skipped rather than fatal. Four
// skills still make a working search, and a session refused because one rule file
// could not be written would be a worse outcome than a session that says so. The
// report goes through progress, so it lands in the event log and in front of the
// user rather than in a swallowed error.
func installGuidance(ws, ideName string, progress func(string)) {
	for _, m := range guidanceModules() {
		// The user's global configuration decides which modules exist. Installing a
		// rule the user turned off would reintroduce it for this search only.
		if config.IsModuleDisabled(m.name, nil, nil) {
			continue
		}
		if err := m.rule(ws, ideName); err != nil {
			progress(fmt.Sprintf("the %s rule could not be installed: %v", m.name, err))
		}
		if err := m.skill(ws, ideName); err != nil {
			progress(fmt.Sprintf("the %s skill could not be installed: %v", m.name, err))
		}
	}
}

// mcpServers is what this project should declare: the graphit server, plus anything
// an installed MCP artifact brought with it.
//
// The set comes from the ide package rather than being assembled here, so that this
// writer and the adapters cannot come to disagree — in particular about the
// artifact-contributed servers, which are easy to forget and impossible to notice
// missing until a tool the user chose is simply absent.
func mcpServers(ws, ideName string) map[string]any {
	installed, err := hub.InstalledArtifacts(ideName, ws)
	if err != nil {
		installed = nil
	}
	return ide.DesiredMCPServers(installed)
}

// localMCPFile is where an IDE reads MCP servers that belong to a project.
//
// The adapters cannot answer this: every MCPFilePath they declare is under the home
// directory, and writing the ephemeral project there would be wrong twice. It would
// add a throwaway project's claim to the user's real configuration, and
// reconcileMCPFile is an unlocked read-modify-write — so two sessions created in the
// same moment can drop each other's claims, and a claim lost is a server deleted
// from under the user's own project.
//
// Only the IDEs whose project-level convention is known are listed. An IDE that is
// missing is reported rather than guessed at, because a config written to the wrong
// path is indistinguishable from one that was never written, and costs an hour to
// find. A session without local MCP still works: the wikis and the memory are
// markdown in the workspace, which the agent can read with its own tools. Only the
// code graphs strictly need the MCP server, since a graph database is not a file you
// can read.
var localMCPFile = map[string]string{
	"claude": ".mcp.json",
	"cursor": filepath.Join(".cursor", "mcp.json"),
	"kiro":   filepath.Join(".kiro", "settings", "mcp.json"),
}

// localPermissionsFile is where an IDE reads which tools may be used without asking.
//
// Same rule as localMCPFile: only what is known. An agent told not to wait for
// approval still waits for approval if its own configuration says to ask, so this
// file is what makes the autonomous preamble true rather than aspirational.
var localPermissionsFile = map[string]string{
	"claude": filepath.Join(".claude", "settings.local.json"),
}

func configureTools(ws, ideName string, progress func(string)) {
	rel, ok := localMCPFile[ideName]
	if !ok {
		progress(fmt.Sprintf(
			"no project-level MCP configuration is known for %s, so the agent will use whatever it already has; "+
				"documentation still reads as files, code graphs need the graphit MCP server", ideName))
	} else if err := writeMCPConfig(filepath.Join(ws, rel), mcpServers(ws, ideName)); err != nil {
		progress(fmt.Sprintf("the MCP configuration could not be written: %v", err))
	}

	permRel, ok := localPermissionsFile[ideName]
	if !ok {
		return
	}
	if err := writeToolPermissions(filepath.Join(ws, permRel), ideName, mcpServers(ws, ideName)); err != nil {
		progress(fmt.Sprintf("the tool permissions could not be written: %v", err))
	}
}

func writeMCPConfig(path string, servers map[string]any) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return fmt.Errorf("creating the MCP configuration directory: %w", err)
	}
	// No managed-keys bookkeeping and no merge: this file is created with the
	// project and deleted with it, so there is no other owner to reconcile with and
	// nothing to reference-count. That bookkeeping exists for shared, long-lived
	// configuration, which this is the opposite of.
	body := map[string]any{"mcpServers": servers}
	data, err := json.MarshalIndent(body, "", "  ")
	if err != nil {
		return fmt.Errorf("encoding the MCP configuration: %w", err)
	}
	if err := os.WriteFile(path, append(data, '\n'), 0o600); err != nil {
		return fmt.Errorf("writing the MCP configuration: %w", err)
	}
	return nil
}

// writeToolPermissions allows every MCP server this project declares, plus the
// agent's own reading tools.
//
// The allowance is written at server level — "every tool from this server" — rather
// than as a list of tool names. The graphit server alone has upwards of seventy tools
// and gains more with each release; a list would be wrong by omission the first time
// one was added, and the failure is an agent that stops to ask permission in a run
// where nobody is watching.
//
// Every declared server is allowed, not just graphit's: a user who chose an MCP
// artifact chose its tools, and leaving them to prompt would make the choice useless.
func writeToolPermissions(path, ideName string, servers map[string]any) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return fmt.Errorf("creating the settings directory: %w", err)
	}
	allow := make([]string, 0, len(servers)+3)
	for name := range servers {
		allow = append(allow, "mcp__"+name)
	}
	sort.Strings(allow) // a stable file is a diffable file
	if ideName == "claude" {
		// Reading is the other half of searching: the wikis and the memory are
		// files, and an agent that may query the graph but not open a page can only
		// half answer.
		allow = append(allow, "Read", "Grep", "Glob")
	}
	body := map[string]any{"permissions": map[string]any{"allow": allow}}
	data, err := json.MarshalIndent(body, "", "  ")
	if err != nil {
		return fmt.Errorf("encoding the tool permissions: %w", err)
	}
	if err := os.WriteFile(path, append(data, '\n'), 0o600); err != nil {
		return fmt.Errorf("writing the tool permissions: %w", err)
	}
	return nil
}

// canonicalIDE resolves the aliases the rest of the framework accepts, so that a
// session created with "claude-code" is set up the same as one created with
// "claude".
func canonicalIDE(name string) string {
	switch strings.ToLower(strings.TrimSpace(name)) {
	case "claude", "claude-code":
		return "claude"
	case "gemini", "gemini-code":
		return "gemini"
	default:
		return strings.ToLower(strings.TrimSpace(name))
	}
}

// ErrUnsupportedIDE reports an IDE the framework has no adapter for.
var ErrUnsupportedIDE = errors.New("unsupported IDE")

// ValidateIDE reports whether a session can be prepared for this IDE, so that the
// caller can refuse at creation rather than fail halfway through preparation.
func ValidateIDE(name string) error {
	want := canonicalIDE(name)
	for _, supported := range ide.SupportedIDEs() {
		if supported == want {
			return nil
		}
	}
	return fmt.Errorf("%w: %q (supported: %s)", ErrUnsupportedIDE, name, strings.Join(ide.SupportedIDEs(), ", "))
}
