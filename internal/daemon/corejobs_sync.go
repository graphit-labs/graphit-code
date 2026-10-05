package daemon

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"sort"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/clirun"
	"github.com/graphit-labs/graphit-code/internal/config"
	gitstate "github.com/graphit-labs/graphit-code/internal/git"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/agent"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/lockfile"
	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/subagent"
	graphtask "github.com/graphit-labs/graphit-code/internal/task"
)

func syncProjectJobs(ctx context.Context, spec clirun.Spec, logger *slog.Logger) error {
	guard, err := lockfile.TryAcquire(brand.ProjectRuntimePath(spec.Dir, "sync.lock"))
	if errors.Is(err, lockfile.ErrLocked) {
		return nil
	}
	if err != nil {
		return err
	}
	defer guard.Release()
	cfg := loadProjectConfigFromDir(spec.Dir)
	step := func(args ...string) error {
		child := spec
		child.Args, child.Stdin = args, nil
		return jobRunner(ctx, args).Run(ctx, child)
	}
	warn := func(label string, err error) {
		if err != nil && ctx.Err() == nil {
			logger.Warn(label, "error", err)
		}
	}
	if err := step("hub", "hydrate"); err != nil {
		return err
	}
	if !config.IsModuleDisabled("ast", nil, cfg) {
		if err := step("ast", "index"); err != nil {
			return err
		}
	}
	if !config.IsModuleDisabled("knowledge", nil, cfg) {
		if err := step("knowledge", "index"); err != nil {
			return err
		}
	}
	// Own-bin work is never awaited while a parent version/heavy lease is held.
	for _, args := range [][]string{{"memory", "index"}, {"memory", "index", "--user"}} {
		warn("memory index", step(args...))
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	if st, err := hub.NewS3Store(ctx, nil, cfg); err == nil {
		warn("hub storage", st.EnsureReachable(ctx))
	} else {
		warn("hub storage", err)
	}
	lf, err := hub.LoadLockfile(filepath.Join(spec.Dir, brand.LockFileName()))
	if err != nil {
		return fmt.Errorf("read lockfile for agent sync: %w", err)
	}
	if lf != nil {
		if lf.Project.ID != "" && !store.IsEphemeralProject(spec.Dir) {
			if mgr, err := hub.NewGlobalLockManager(); err == nil {
				warn("register checkout", mgr.RegisterProject(lf.Project.ID, spec.Dir, hub.WithProjectName(lf.Project.Name), hub.WithProjectDescription(lf.Project.Description), hub.WithProjectCluster(lf.Project.Cluster)))
			}
		}
		for _, target := range hub.FilterSupportedAgents(lf.Agents) {
			warn("retired skill cleanup", agent.RemoveManagedSkill(spec.Dir, target, brand.SkillDirName("improvements")))
			for _, install := range []func(string, string) error{knowledge.InstallSkill, ast.InstallSkill, hub.InstallSkill, memory.InstallSkill, graphtask.InstallSkill, subagent.InstallRoles} {
				warn("agent skills", install(spec.Dir, target))
			}
			if err := hub.SyncAgentAdapter(target, spec.Dir, lf); err != nil {
				return fmt.Errorf("sync agent %s: %w", target, err)
			}
		}
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	hooks := gitstate.NewHookManager(spec.Dir)
	if config.IsModuleDisabled("hooks", nil, cfg) {
		warn("remove Git hooks", hooks.Remove())
	} else {
		var events []string
		if lf != nil {
			for event := range lf.Hooks {
				events = append(events, event)
			}
		}
		sort.Strings(events)
		warn("install Git hooks", hooks.Install(false, events...))
	}
	// Match sync's heavy phase without starting sync --heavy as a second core.
	stamp := brand.ProjectRuntimePath(spec.Dir, "sync.stamp")
	if err := os.MkdirAll(filepath.Dir(stamp), 0755); err == nil {
		warn("sync stamp", os.WriteFile(stamp, nil, 0644))
	}
	heavyGuard, err := lockfile.TryAcquire(brand.ProjectRuntimePath(spec.Dir, "sync-heavy.lock"))
	if errors.Is(err, lockfile.ErrLocked) {
		return nil
	}
	if err != nil {
		return err
	}
	defer heavyGuard.Release()
	if !config.IsModuleDisabled("embedding", nil, cfg) {
		warn("AST embedding", step("ast", "embed"))
		warn("wiki embedding", step("wiki", "embed"))
		if !config.IsModuleDisabled("memory", nil, cfg) {
			warn("memory embedding", step("memory", "embed"))
			warn("user memory embedding", step("memory", "embed", "--user"))
		}
	}
	if !config.IsModuleDisabled("memory", nil, cfg) {
		warn("memory maintenance", step("memory", "maintain"))
		warn("user memory maintenance", step("memory", "maintain", "--user"))
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	if registry, err := hub.NewRegistryManager(ctx); err == nil {
		warn("Hub reconciliation", hub.ReconcileManagedArtifactsFromDir(registry, spec.Dir))
	} else {
		warn("Hub registry", err)
	}
	warn("global language artifacts", hub.EnsureGlobalLanguageArtifacts(filepath.Join(spec.Dir, brand.LockFileName())))
	return ctx.Err()
}
