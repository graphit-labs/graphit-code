package hub

import (
	"context"
	"fmt"
	"path/filepath"

	"github.com/graphit-labs/graphit-code/internal/paths"
)

func OnInit(ctx context.Context, registry *RegistryManager, ide, projectDir string) error {
	svc := NewHubService(registry)
	pp := paths.GetPathsForProject(ide, projectDir)

	lf, err := LoadLockfile(pp.LockFilePath)
	if err != nil {
		return fmt.Errorf("reading lockfile: %w", err)
	}
	if lf == nil {
		lf = &Lockfile{Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta)}
	}

	if err := SaveLockfile(pp.LockFilePath, lf); err != nil {
		return fmt.Errorf("creating lockfile: %w", err)
	}

	// Register the project in the global lock. SaveLockfile guarantees the
	// project ID exists (it generates one when absent), so we reload and
	// register unconditionally. This must happen inside OnInit — not only in
	// the CLI command — so that any caller (MCP, daemon, etc.) also ensures
	// registration.
	if savedLf, loadErr := LoadLockfile(pp.LockFilePath); loadErr == nil && savedLf != nil && savedLf.Project.ID != "" {
		if mgr, mgrErr := NewGlobalLockManager(); mgrErr == nil {
			projDir := filepath.Dir(pp.LockFilePath)
			var regOpts []func(*InstanceEntry)
			if savedLf.Project.Name != "" {
				regOpts = append(regOpts, WithProjectName(savedLf.Project.Name))
			}
			if savedLf.Project.Description != "" {
				regOpts = append(regOpts, WithProjectDescription(savedLf.Project.Description))
			}
			_ = mgr.RegisterProject(savedLf.Project.ID, projDir, regOpts...)
		}
		if registry.IsReady() {
			if _, err := registry.UpsertProject(ctx, savedLf.Project.ID, savedLf.Project.Name, savedLf.Project.Description); err != nil {
				return fmt.Errorf("registering project in Hub: %w", err)
			}
		}
	}

	_, _ = AddIDE(pp.LockFilePath, ide)

	if registry.IsReady() {
		baselines, err := registry.GetDefaultBaselines(ctx)
		if err == nil {
			for _, baseline := range baselines {
				entryID := baseline.ID
				if baseline.Version != "" && baseline.Version != "latest" {
					entryID = baseline.ID + "@" + baseline.Version
				}
				_, _ = svc.Install(ctx, entryID, "", ide, baseline.Type, "", "")
			}
		}
	}

	_ = syncIDEAdapter(ide, pp, lf)

	if registry.IsReady() {
		tracker := NewEventTracker(registry.Store())
		tracker.TrackEvent(ctx, "project.init", lf.Project.ID, nil, map[string]string{"ide": ide})
	}

	return nil
}

func OnUpdate(ctx context.Context, registry *RegistryManager, ide, projectDir string) error {
	svc := NewHubService(registry)
	pp := paths.GetPathsForProject(ide, projectDir)

	if registry.IsReady() {
		lf, _ := LoadLockfile(pp.LockFilePath)
		if lf != nil {
			if _, err := registry.UpsertProject(ctx, lf.Project.ID, lf.Project.Name, lf.Project.Description); err != nil {
				return fmt.Errorf("updating project in Hub: %w", err)
			}
			tracker := NewEventTracker(registry.Store())
			tracker.TrackEvent(ctx, "project.update", lf.Project.ID, nil, map[string]string{"ide": ide})
		}
	}

	_ = svc.UpdateAll(ctx, ide, projectDir)

	lf, _ := LoadLockfile(pp.LockFilePath)
	if lf != nil {
		_ = syncIDEAdapter(ide, pp, lf)
	}

	return nil
}

func OnRemove(ctx context.Context, registry *RegistryManager, ide, projectDir string) error {
	svc := NewHubService(registry)
	pp := paths.GetPathsForProject(ide, projectDir)

	lf, _ := LoadLockfile(pp.LockFilePath)
	projectID := ""
	if lf != nil {
		projectID = lf.Project.ID
	}

	if registry.IsReady() {
		tracker := NewEventTracker(registry.Store())
		tracker.TrackEvent(ctx, "project.remove", projectID, nil, map[string]string{"ide": ide})
	}

	remaining, _ := RemoveIDE(pp.LockFilePath, ide)

	if adapter, err := getIDEAdapter(ide); err == nil && adapter != nil {
		flat := buildInstalledFlat(lf, pp, projectID)
		_ = adapter.Remove(pp, flat)
	}

	if len(remaining) == 0 {
		if projectID != "" {
			if mgr, err := NewGlobalLockManager(); err == nil {
				_ = mgr.UnregisterProject(projectID, pp.ActiveProjectDir)
			}
		}
		_ = svc.UninstallAll(ctx, ide, projectDir)
	}

	return nil
}

func SyncIDEAdapter(ide, projectDir string, lf *Lockfile) error {
	pp := paths.GetPathsForProject(ide, projectDir)
	return syncIDEAdapter(ide, pp, lf)
}

func syncIDEAdapter(ide string, pp *paths.ProjectPaths, lf *Lockfile) error {
	adapter, err := getIDEAdapter(ide)
	if err != nil || adapter == nil {
		return err
	}

	flat := buildInstalledFlat(lf, pp, lf.Project.ID)

	return adapter.Sync(flat, pp, lf.Project.ID)
}

// InstalledArtifacts returns a project's installed artifacts in the flat form the
// IDE adapters consume: artifact ID to type, path, version, hash and project ID.
//
// Exported for callers that need to know what a project has without driving an
// adapter — the live search reads it to decide which MCP servers its ephemeral
// project should declare. A project with no lockfile has no artifacts, which is an
// empty map rather than an error.
func InstalledArtifacts(ide, projectDir string) (map[string]map[string]string, error) {
	pp := paths.GetPathsForProject(ide, projectDir)
	lf, err := LoadLockfile(pp.LockFilePath)
	if err != nil {
		return nil, fmt.Errorf("reading lockfile: %w", err)
	}
	if lf == nil {
		return map[string]map[string]string{}, nil
	}
	return buildInstalledFlat(lf, pp, lf.Project.ID), nil
}

func buildInstalledFlat(lf *Lockfile, pp *paths.ProjectPaths, projectID string) map[string]map[string]string {
	flat := make(map[string]map[string]string)
	if lf == nil {
		return flat
	}
	for artType, typeMap := range lf.Artifacts {
		for artID, meta := range typeMap {
			artPath := resolveArtifactPath(meta, artType, artID, pp)
			flat[artID] = map[string]string{
				"type":       string(artType),
				"path":       artPath,
				"version":    meta.Version,
				"hash":       meta.Hash,
				"project_id": projectID,
			}
		}
	}

	return flat
}
