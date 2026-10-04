// Package storework runs the Task and Memory store maintenance the daemon schedules:
// `task maintain`, `memory maintain`, `memory embed` and `memory index`. The core CLI and
// the lightweight graphit-store binary both run it from here, so the daemon's frequent
// maintenance children start without the parsers, the graph database or the Hub
// registry the core carries.
package storework

import (
	"context"
	"fmt"
	"path/filepath"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/output"
	"github.com/graphit-labs/graphit-code/internal/projectlock"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/sysutil"
	graphtask "github.com/graphit-labs/graphit-code/internal/task"
)

func projectID(projectDir string) (string, error) {
	lf, err := projectlock.Load(filepath.Join(projectDir, brand.LockFileName()))
	if err != nil || lf == nil || lf.Project.ID == "" {
		return "", fmt.Errorf("project not initialised — run '%s init' first", brand.BinName())
	}
	return lf.Project.ID, nil
}

// MemoryTableURI resolves the authoritative memory table of the project, or of the
// user scope.
func MemoryTableURI(projectDir string, userScope bool) (string, error) {
	if userScope {
		userID, err := memory.UserScopeID()
		if err != nil {
			return "", err
		}
		if uri := memory.TableURIFor("user", userID); uri != "" {
			return uri, nil
		}
		return "", fmt.Errorf("user memory table is not available")
	}
	id, err := projectID(projectDir)
	if err != nil {
		return "", err
	}
	if uri := memory.TableURIFor("project", id); uri != "" {
		return uri, nil
	}
	return "", fmt.Errorf("project memory table is not available")
}

func scopeLabel(userScope bool) string {
	if userScope {
		return "user"
	}
	return "project"
}

// MaintainMemory compacts the memory table and prunes old versions.
func MaintainMemory(ctx context.Context, projectDir string, userScope bool) error {
	uri, err := MemoryTableURI(projectDir, userScope)
	if err != nil {
		return err
	}
	p := output.NewPrinter("")
	task := p.StartTask("Maintaining %s memory table...", scopeLabel(userScope))
	if err := memory.MaintainTable(ctx, uri); err != nil {
		task.Fail("Memory maintenance: %v", err)
		return err
	}
	task.Done("%s memory table maintained", scopeLabel(userScope))
	return nil
}

// EmbedMemory fills the vectors of memories that are new or changed.
func EmbedMemory(ctx context.Context, projectDir string, userScope bool) error {
	uri, err := MemoryTableURI(projectDir, userScope)
	if err != nil {
		return err
	}
	client, err := ai.NewEmbeddingClientFromConfig()
	if err != nil {
		return fmt.Errorf("embedding client: %w", err)
	}
	release, err := sysutil.AcquireHeavy(ctx)
	if err != nil {
		return err
	}
	defer release()

	p := output.NewPrinter("")
	task := p.StartTask("Embedding %s memories...", scopeLabel(userScope))
	n, err := memory.NewEmbedder(client, memory.DefaultEmbedConfig()).RunCycle(ctx, uri)
	if err != nil {
		task.Fail("Memory embedding: %v", err)
		return err
	}
	task.Done("%d %s memories embedded", n, scopeLabel(userScope))
	return nil
}

// IndexMemory refreshes the indexes of the authoritative memory table. reset is kept
// for compatibility and refreshes the same indexes in place.
func IndexMemory(ctx context.Context, projectDir string, userScope, reset bool) error {
	p := output.NewPrinter("")
	scope, scopeID := memory.MemoryScopeProject, ""
	if userScope {
		userID, err := memory.UserScopeID()
		if err != nil {
			return err
		}
		scope, scopeID = memory.MemoryScopeUser, userID
	} else {
		if _, err := store.EnsureProjectID(projectDir); err != nil {
			return err
		}
		id, err := projectID(projectDir)
		if err != nil {
			return err
		}
		scopeID = id
	}
	ms, _ := memory.NewMemoryStore()
	svc := memory.NewMemoryService(scope, scopeID, ms).WithContext(context.Background())
	_ = svc.EnsureInitialised()
	defer func() { _ = svc.Close() }()

	if reset {
		p.Step("The authoritative table has no derived index to clear; refreshing its indexes in place.")
	}
	task := p.StartTask("Refreshing memory indexes at %s...", svc.TableURI())
	if err := svc.IndexMemories(ctx); err != nil {
		task.Fail("Memory indexing failed: %v", err)
		return err
	}
	task.Done("Authoritative memory indexes are current")
	return nil
}

// MaintainTasks compacts the project's task and session tables and refreshes their
// indexes.
func MaintainTasks(ctx context.Context, projectDir string) error {
	service, err := graphtask.Open(projectDir)
	if err != nil {
		return err
	}
	p := output.NewPrinter("")
	task := p.StartTask("Maintaining task tables...")
	if err := service.Maintain(ctx); err != nil {
		task.Fail("Task maintenance: %v", err)
		return err
	}
	task.Done("Task tables maintained")
	return nil
}
