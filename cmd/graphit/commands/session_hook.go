package commands

import (
	"encoding/json"
	"fmt"
	"io"
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
	"github.com/spf13/cobra"
)

func newSessionHookCmd() *cobra.Command {
	var format string
	var projectDir string

	cmd := &cobra.Command{
		Use:    "_session-hook",
		Short:  "Render internal IDE lifecycle hook output",
		Hidden: true,
		Args:   cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			input, err := readSessionHookInput(cmd.InOrStdin(), format)
			if err != nil {
				return fmt.Errorf("reading session hook input: %w", err)
			}
			projectDir = resolveSessionHookProjectDir(projectDir, input)
			context := sessionhook.Context{}
			if projectDir != "" {
				projectCfg := loadProjectConfigFromDir(projectDir)
				context.Instructions = loadHookInstructionContext(projectDir, projectCfg)
				context.MemoryDisabled = config.IsModuleDisabled("memory", nil, projectCfg)
				if !context.MemoryDisabled && hookInputNeedsMandatory(format, input) {
					context.Mandatory, context.MandatoryLoaded = loadMandatoryHookContext(projectDir)
				}
			}
			payload, err := sessionhook.RenderWithContext(format, input, context)
			if err != nil {
				return err
			}
			if len(payload) == 0 {
				return nil
			}
			_, err = fmt.Fprintln(cmd.OutOrStdout(), string(payload))
			return err
		},
	}
	cmd.Flags().StringVar(&format, "format", "", "session hook output format")
	cmd.Flags().StringVar(&projectDir, "project-dir", "", "starting directory for Graphit project discovery")
	_ = cmd.MarkFlagRequired("format")
	return cmd
}

func resolveSessionHookProjectDir(explicit string, input []byte) string {
	type hookInput struct {
		CWD            string   `json:"cwd"`
		WorkspaceRoots []string `json:"workspace_roots"`
		WorkspacePaths []string `json:"workspacePaths"`
	}

	candidates := make([]string, 0, 5)
	if explicit != "" {
		return findSessionHookProjectRoot(explicit)
	}

	var event hookInput
	if json.Unmarshal(input, &event) == nil {
		candidates = append(candidates, event.CWD)
		candidates = append(candidates, event.WorkspaceRoots...)
		candidates = append(candidates, event.WorkspacePaths...)
	}
	if wd, err := os.Getwd(); err == nil {
		candidates = append(candidates, wd)
	}

	for _, candidate := range candidates {
		if root := findSessionHookProjectRoot(candidate); root != "" {
			return root
		}
	}
	return ""
}

func findSessionHookProjectRoot(start string) string {
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

func hookInputNeedsMandatory(format string, input []byte) bool {
	switch strings.ToLower(format) {
	case sessionhook.FormatFirstInvocation:
		var event struct {
			InvocationNum *int `json:"invocationNum"`
		}
		return json.Unmarshal(input, &event) == nil && event.InvocationNum != nil && *event.InvocationNum == 0
	case sessionhook.FormatSessionStart, sessionhook.FormatAdditionalContext,
		sessionhook.FormatPlainContext, sessionhook.FormatSubagentStart,
		sessionhook.FormatCursorSubagentTask:
		return true
	default:
		return false
	}
}

func loadMandatoryHookContext(projectDir string) (string, bool) {
	return loadMandatoryHookContextWith(projectDir, memory.ListMandatoryMemoriesForProject)
}

func loadMandatoryHookContextWith(projectDir string, list func(string, string) ([]memory.MandatoryEntry, error)) (string, bool) {
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

func loadHookInstructionContext(projectDir string, projectCfg config.ConfigMap) string {
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

	sections := make([]string, 0, 3)
	if mandate := ideadapter.MandateContext(triggers); mandate != "" {
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

func readSessionHookInput(input io.Reader, format string) ([]byte, error) {
	if file, ok := input.(*os.File); ok {
		info, err := file.Stat()
		if err != nil {
			return nil, fmt.Errorf("inspecting session hook input: %w", err)
		}
		if info.Mode()&os.ModeCharDevice != 0 {
			if strings.EqualFold(format, sessionhook.FormatFirstInvocation) {
				return []byte(`{"invocationNum":0}`), nil
			}
			return nil, nil
		}
	}
	return io.ReadAll(input)
}
