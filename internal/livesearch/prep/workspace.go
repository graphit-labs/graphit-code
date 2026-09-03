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
	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/livesearch"
	"github.com/graphit-labs/graphit-code/internal/memory"
)

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

	progress("installing the framework's skills")
	installGuidance(ws, ideName, progress)
	if err := ctx.Err(); err != nil {
		return err
	}

	if err := installArtifacts(ctx, ws, ideName, meta.Artifacts, progress); err != nil {
		return err
	}

	if err := prepareIndexes(ctx, ws, progress); err != nil {
		return err
	}

	progress("giving the agent access to the graphit tools")
	configureTools(ws, ideName, progress)

	return ctx.Err()
}

type artifactInstaller interface {
	Install(ctx context.Context, entryID, alias, ide string, entryType hub.ArtifactType, parentID, projectDir string) (*hub.InstallResult, error)
}

var newInstaller = func(ctx context.Context) (artifactInstaller, error) {
	registry, err := hub.NewRegistryManager(ctx)
	if err != nil {
		return nil, fmt.Errorf("opening the hub registry: %w", err)
	}
	if !registry.IsReady() {
		return nil, errors.New("the hub registry is not available, so the selected artifacts cannot be installed")
	}
	return hub.NewUntrackedHubService(registry), nil
}

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

// guidanceModule is one of the framework modules whose skill must remain a
// physical, host-discoverable artifact.
type guidanceModule struct {
	name  string
	skill func(projectDir, ide string) error
}

func guidanceModules() []guidanceModule {
	return []guidanceModule{
		{"knowledge", knowledge.InstallSkill},
		{"ast", ast.InstallSkill},
		{"hub", hub.InstallSkill},
		{"memory", memory.InstallSkill},
	}
}

func installGuidance(ws, ideName string, progress func(string)) {
	for _, m := range guidanceModules() {
		if config.IsModuleDisabled(m.name, nil, nil) {
			continue
		}
		if err := m.skill(ws, ideName); err != nil {
			progress(fmt.Sprintf("the %s skill could not be installed: %v", m.name, err))
		}
	}
}

func mcpServers(ws, ideName string) map[string]any {
	installed, err := hub.InstalledArtifacts(ideName, ws)
	if err != nil {
		installed = nil
	}
	return ide.DesiredMCPServers(installed)
}

var localPermissionsFile = map[string]string{
	"claude": filepath.Join(".claude", "settings.local.json"),
}

func configureTools(ws, ideName string, progress func(string)) {
	lf, err := hub.LoadLockfile(filepath.Join(ws, brand.LockFileName()))
	if err != nil || lf == nil {
		progress(fmt.Sprintf("the project lockfile could not be read while configuring hooks and MCP: %v", err))
	} else if err := hub.SyncIDEAdapter(ideName, ws, lf); err != nil {
		progress(fmt.Sprintf("the IDE hooks and MCP configuration could not be written: %v", err))
	}

	permRel, ok := localPermissionsFile[ideName]
	if !ok {
		return
	}
	if err := writeToolPermissions(filepath.Join(ws, permRel), ideName, mcpServers(ws, ideName)); err != nil {
		progress(fmt.Sprintf("the tool permissions could not be written: %v", err))
	}
}

func writeToolPermissions(path, ideName string, servers map[string]any) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return fmt.Errorf("creating the settings directory: %w", err)
	}
	allow := make([]string, 0, len(servers)+3)
	for name := range servers {
		allow = append(allow, "mcp__"+name)
	}
	sort.Strings(allow)
	if ideName == "claude" {
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
