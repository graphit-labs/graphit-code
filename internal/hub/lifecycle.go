package hub

import (
	"context"
	"fmt"

	"github.com/graphit-labs/graphit-code/internal/output"
	"github.com/graphit-labs/graphit-code/internal/paths"
)

var lifecyclePrinter = output.NewPrinter("")

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

	lifecyclePrinter.Step("Creating project identity")
	if err := SaveLockfile(pp.LockFilePath, lf); err != nil {
		return fmt.Errorf("creating lockfile: %w", err)
	}

	if _, err := AddIDE(pp.LockFilePath, ide); err != nil {
		lifecyclePrinter.StepWarn("Could not register IDE %q: %v", ide, err)
	}

	lifecyclePrinter.Step("Provisioning baseline artifacts")
	if registry.IsReady() {
		baselines, err := registry.GetDefaultBaselines(ctx)
		if err != nil {
			lifecyclePrinter.StepWarn("Could not load baselines: %v", err)
		} else {
			for _, baseline := range baselines {
				entryID := baseline.ID
				if baseline.Version != "" && baseline.Version != "latest" {
					entryID = baseline.ID + "@" + baseline.Version
				}
				if _, err := svc.Install(ctx, entryID, "", ide, baseline.Type, "", ""); err != nil {
					lifecyclePrinter.StepWarn("Baseline %q: %v", baseline.ID, err)
				}
			}
		}
	}

	lifecyclePrinter.Step("Syncing IDE adapter (%s)", ide)
	if err := syncIDEAdapter(ide, pp, lf); err != nil {
		lifecyclePrinter.StepWarn("IDE adapter sync: %v", err)
	}

	if registry.IsReady() {
		tracker := NewEventTracker(registry.GitStore())
		tracker.TrackEvent("project.init", lf.Project.ID, nil, map[string]string{"ide": ide})
	}

	return nil
}

func OnUpdate(ctx context.Context, registry *RegistryManager, ide string) error {
	svc := NewHubService(registry)
	pp := paths.GetPaths(ide, false)
	p := output.NewPrinter("")

	if registry.IsReady() {
		lf, _ := LoadLockfile(pp.LockFilePath)
		if lf != nil {
			tracker := NewEventTracker(registry.GitStore())
			tracker.TrackEvent("project.update", lf.Project.ID, nil, map[string]string{"ide": ide})
		}
	}

	p.Step("Checking for artifact updates")

	results := svc.UpdateAll(ctx, ide, "")
	for artID, err := range results {
		if err != nil {
			p.StepWarn("Update failed for %q: %v", artID, err)
		}
	}

	lf, _ := LoadLockfile(pp.LockFilePath)
	if lf != nil {
		if err := syncIDEAdapter(ide, pp, lf); err != nil {
			p.StepWarn("IDE adapter sync: %v", err)
		}
	}

	return nil
}

func OnRemove(ctx context.Context, registry *RegistryManager, ide string) error {
	svc := NewHubService(registry)
	pp := paths.GetPaths(ide, false)

	p := output.NewPrinter("")

	if registry.IsReady() {
		lf, _ := LoadLockfile(pp.LockFilePath)
		projectID := ""
		if lf != nil {
			projectID = lf.Project.ID
		}
		tracker := NewEventTracker(registry.GitStore())
		tracker.TrackEvent("project.remove", projectID, nil, map[string]string{"ide": ide})
	}

	remaining, err := RemoveIDE(pp.LockFilePath, ide)
	if err != nil {
		p.StepWarn("Could not deregister IDE: %v", err)
	}

	if len(remaining) == 0 {

		p.Step("Removing all artifacts")
		if err := svc.UninstallAll(ctx, ide, ""); err != nil {
			p.StepWarn("Artifact cleanup: %v", err)
		}
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
