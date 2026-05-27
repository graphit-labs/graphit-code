package hub

import (
	"context"
	"fmt"

	"github.com/graphit-labs/graphit-code/internal/paths"
)

func OnInit(ctx context.Context, registry *RegistryManager, ide string) error {
	svc := NewHubService(registry)
	pp := paths.GetPaths(ide, false)

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
		tracker := NewEventTracker(registry.GitStore())
		tracker.TrackEvent("project.init", lf.Project.ID, nil, map[string]string{"ide": ide})
	}

	return nil
}

func OnUpdate(ctx context.Context, registry *RegistryManager, ide string) error {
	svc := NewHubService(registry)
	pp := paths.GetPaths(ide, false)

	if registry.IsReady() {
		lf, _ := LoadLockfile(pp.LockFilePath)
		if lf != nil {
			tracker := NewEventTracker(registry.GitStore())
			tracker.TrackEvent("project.update", lf.Project.ID, nil, map[string]string{"ide": ide})
		}
	}

	_ = svc.UpdateAll(ctx, ide, "")

	lf, _ := LoadLockfile(pp.LockFilePath)
	if lf != nil {
		_ = syncIDEAdapter(ide, pp, lf)
	}

	return nil
}

func OnRemove(ctx context.Context, registry *RegistryManager, ide string) error {
	svc := NewHubService(registry)
	pp := paths.GetPaths(ide, false)

	if registry.IsReady() {
		lf, _ := LoadLockfile(pp.LockFilePath)
		projectID := ""
		if lf != nil {
			projectID = lf.Project.ID
		}
		tracker := NewEventTracker(registry.GitStore())
		tracker.TrackEvent("project.remove", projectID, nil, map[string]string{"ide": ide})
	}

	remaining, _ := RemoveIDE(pp.LockFilePath, ide)

	if len(remaining) == 0 {
		_ = svc.UninstallAll(ctx, ide, "")
	}

	return nil
}

func SyncIDEAdapter(ide string, lf *Lockfile) error {
	pp := paths.GetPaths(ide, false)
	return syncIDEAdapter(ide, pp, lf)
}

func syncIDEAdapter(ide string, pp *paths.ProjectPaths, lf *Lockfile) error {
	adapter, err := getIDEAdapter(ide)
	if err != nil || adapter == nil {
		return err
	}

	flat := make(map[string]map[string]string)
	for artType, typeMap := range lf.Artifacts {
		for artID, meta := range typeMap {
			artPath := resolveArtifactPath(meta, artType, artID, pp)
			flat[artID] = map[string]string{
				"type":    string(artType),
				"path":    artPath,
				"version": meta.Version,
				"hash":    meta.Hash,
			}
		}
	}

	return adapter.Sync(flat, pp, lf.Project.ID)
}
