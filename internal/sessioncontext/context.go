// Package sessioncontext assembles the project-specific context injected at
// agent session boundaries. Keeping this outside the CLI and MCP packages makes
// the native hooks and remote mandate tool consume one authoritative builder.
package sessioncontext

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/hub"
	ideadapter "github.com/graphit-labs/graphit-code/internal/hub/adapters/ide"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/sessionhook"
)

// FindProjectRoot walks upward from start until it finds the Graphit lockfile.
// Git metadata is deliberately irrelevant: Graphit projects do not require Git.
func FindProjectRoot(start string) string {
	if start == "" {
		return ""
	}
	dir, err := filepath.Abs(start)
	if err != nil {
		return ""
	}
	if info, err := os.Stat(dir); err == nil && !info.IsDir() {
		dir = filepath.Dir(dir)
	}
	dir = filepath.Clean(dir)
	for {
		if _, err := os.Stat(filepath.Join(dir, brand.LockFileName())); err == nil {
			return dir
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			return ""
		}
		dir = parent
	}
}

// Build reads the current project state and returns the context used by native
// lifecycle hooks. includeMandatory is false only at repeated boundaries where
// the initial bootstrap has already happened.
func Build(projectDir string, includeMandatory bool) sessionhook.Context {
	if projectDir == "" {
		return sessionhook.Context{}
	}
	projectCfg := loadProjectConfig(projectDir)
	context := sessionhook.Context{
		Instructions:   loadInstructionContext(projectDir, projectCfg),
		MemoryDisabled: config.IsModuleDisabled("memory", nil, projectCfg),
	}
	if includeMandatory && !context.MemoryDisabled {
		context.Mandatory, context.MandatoryLoaded = loadMandatoryContext(projectDir)
	}
	return context
}

// Mandates returns the project-independent module router that used to be
// materialized in agent instruction files. It resolves global overrides and
// otherwise uses the framework defaults; project config, mandatory memory, and
// installed Hub rules are intentionally excluded.
func Mandates() string {
	return loadMandateContext("", nil)
}

func loadProjectConfig(projectDir string) config.ConfigMap {
	lf, err := hub.LoadLockfile(filepath.Join(projectDir, brand.LockFileName()))
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
	rules, err := hub.InstalledRuleContext(projectDir)
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
	triggers := map[string]string{}
	for _, module := range []struct {
		name    string
		tag     string
		content func() string
	}{
		{name: "memory", tag: "mem_rule", content: func() string {
			return brand.ResolveModuleRuleIn(projectDir, "memory", memory.MandateTrigger())
		}},
		{name: "ast", tag: "ast_rule", content: func() string {
			return brand.ResolveModuleRuleIn(projectDir, "ast", ast.MandateTrigger())
		}},
		{name: "hub", tag: "hub_rule", content: func() string {
			return brand.ResolveModuleRuleIn(projectDir, "hub", hub.MandateTrigger())
		}},
		{name: "knowledge", tag: "doc_rule", content: func() string {
			return brand.ResolveModuleRuleIn(projectDir, "knowledge", knowledge.MandateTrigger())
		}},
	} {
		if !config.IsModuleDisabled(module.name, nil, projectCfg) {
			triggers[module.tag] = module.content()
		}
	}

	return ideadapter.MandateContext(triggers)
}

func loadMandatoryContext(projectDir string) (string, bool) {
	return loadMandatoryContextWith(projectDir, memory.ListMandatoryMemoriesForProject)
}

func loadMandatoryContextWith(projectDir string, list func(string, string) ([]memory.MandatoryEntry, error)) (string, bool) {
	var sections []string
	for _, scope := range []string{"project", "user"} {
		entries, err := list(projectDir, scope)
		if err != nil {
			return "", false
		}
		for _, entry := range entries {
			sections = append(sections, fmt.Sprintf("### %s memory: %s\n%s", scope, entry.Title, entry.Content))
		}
	}
	return strings.Join(sections, "\n\n"), true
}
