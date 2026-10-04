// Package sessioncontext assembles the project-specific context injected at
// agent session boundaries. Keeping this outside the CLI and MCP packages makes
// the native hooks and remote mandate tool consume one authoritative builder.
//
// It reads only the project lockfile and installed files, never a store, so the
// lightweight hook binary renders hook events with it; the store-backed half of an
// event (Task lifecycle, mandatory memory) is supplied by the caller (see Render).
package sessioncontext

import (
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	ideadapter "github.com/graphit-labs/graphit-code/internal/hub/adapters/agent"
	"github.com/graphit-labs/graphit-code/internal/hubartifact"
	"github.com/graphit-labs/graphit-code/internal/mandates"
	"github.com/graphit-labs/graphit-code/internal/projectlock"
	"github.com/graphit-labs/graphit-code/internal/sessionhook"
)

// FindProjectRoot walks upward from start until it finds the Graphit lockfile.
func FindProjectRoot(start string) string { return sessionhook.FindProjectRoot(start) }

// Build reads the current project state and returns the context used by native
// lifecycle hooks: module switches and instructions. Mandatory memory is store
// state; Render adds it.
func Build(projectDir string) sessionhook.Context {
	if projectDir == "" {
		return sessionhook.Context{}
	}
	projectCfg := loadProjectConfig(projectDir)
	context := moduleContext(projectCfg)
	context.Instructions = loadInstructionContext(projectDir, projectCfg)
	return context
}

// BuildForRole returns the static context installed for a delegated role. The
// router carries only the modules the role uses, so a performer is never handed
// rules for tools its role will not touch. Mandatory memory is runtime state and
// must never be serialized into the installed role document.
func BuildForRole(projectDir string, role sessionhook.Role) sessionhook.Context {
	if projectDir == "" {
		return sessionhook.Context{}
	}
	projectCfg := loadProjectConfig(projectDir)
	context := moduleContext(projectCfg)
	context.Instructions = loadMandateContextForRole(projectDir, projectCfg, role)
	return context
}

// ModuleContext reads only enabled-module switches at recurring hook boundaries.
func ModuleContext(projectDir string) sessionhook.Context {
	if projectDir == "" {
		return sessionhook.Context{}
	}
	return moduleContext(loadProjectConfig(projectDir))
}

func moduleContext(projectCfg config.ConfigMap) sessionhook.Context {
	return sessionhook.Context{
		MemoryDisabled: config.IsModuleDisabled("memory", nil, projectCfg),
		TaskDisabled:   config.IsModuleDisabled("task", nil, projectCfg),
	}
}

func Mandates() string {
	return loadMandateContext("", nil)
}

func loadProjectConfig(projectDir string) config.ConfigMap {
	lf, err := projectlock.Load(filepath.Join(projectDir, brand.LockFileName()))
	if err != nil || lf == nil {
		return nil
	}
	return lf.Config
}

func loadInstructionContext(projectDir string, projectCfg config.ConfigMap) string {
	sections := make([]string, 0, 3)
	if mandate := loadMandateContext(projectDir, projectCfg); mandate != "" {
		sections = append(sections, mandate)
	}
	rules, err := hubartifact.InstalledRuleContext(projectDir)
	if rules != "" {
		sections = append(sections, rules)
	}
	if err != nil {
		sections = append(sections,
			"Graphit could not load every installed Hub rule ("+err.Error()+"). Before rule-dependent work, use `graphit_hub_content` when available; otherwise continue with the agent's native capabilities.")
	}
	return strings.Join(sections, "\n\n")
}

func loadMandateContext(projectDir string, projectCfg config.ConfigMap) string {
	return loadMandateContextForRole(projectDir, projectCfg, sessionhook.Role{})
}

// loadMandateContextForRole filters the router by role. An empty role selects
// every enabled module, which is what a coordinator needs.
func loadMandateContextForRole(projectDir string, projectCfg config.ConfigMap, role sessionhook.Role) string {
	triggers := map[string]string{}
	for _, module := range []struct {
		name    string
		tag     string
		content func() string
	}{
		{name: "task", tag: "task_rule", content: func() string {
			// A performer gets the worker rule: the coordinator's is written around
			// owning a session, and handing that to a delegate produces duplicates.
			trigger := mandates.Task()
			if role.Name != "" {
				trigger = mandates.TaskWorker()
			}
			return brand.ResolveModuleRuleIn(projectDir, "task", trigger)
		}},
		{name: "memory", tag: "mem_rule", content: func() string {
			return brand.ResolveModuleRuleIn(projectDir, "memory", mandates.Memory())
		}},
		{name: "ast", tag: "ast_rule", content: func() string {
			return brand.ResolveModuleRuleIn(projectDir, "ast", mandates.AST())
		}},
		{name: "hub", tag: "hub_rule", content: func() string {
			return brand.ResolveModuleRuleIn(projectDir, "hub", mandates.Hub())
		}},
		{name: "knowledge", tag: "doc_rule", content: func() string {
			return brand.ResolveModuleRuleIn(projectDir, "knowledge", mandates.Knowledge())
		}},
	} {
		if config.IsModuleDisabled(module.name, nil, projectCfg) || !role.Uses(module.name) {
			continue
		}
		triggers[module.tag] = module.content()
	}

	return ideadapter.MandateContext(triggers)
}
