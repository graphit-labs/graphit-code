package commands

import (
	"os"

	"github.com/graphit-labs/graphit-code/internal/backlog"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub"
	ideAdapter "github.com/graphit-labs/graphit-code/internal/hub/adapters/ide"
	"github.com/spf13/cobra"
)

func completionIDEs() func(*cobra.Command, []string, string) ([]string, cobra.ShellCompDirective) {
	return func(_ *cobra.Command, _ []string, _ string) ([]string, cobra.ShellCompDirective) {
		return ideAdapter.SupportedIDEs(), cobra.ShellCompDirectiveNoFileComp
	}
}

func registerIDEFlagCompletion(cmd *cobra.Command) {
	_ = cmd.RegisterFlagCompletionFunc("ide", completionIDEs())
}

var allArtifactTypes = []string{
	"rule", "agent", "skill", "command", "mcp", "power",
	"knowledge", "ast", "workflow",
}

func completionArtifactTypes() func(*cobra.Command, []string, string) ([]string, cobra.ShellCompDirective) {
	return func(_ *cobra.Command, _ []string, _ string) ([]string, cobra.ShellCompDirective) {
		return allArtifactTypes, cobra.ShellCompDirectiveNoFileComp
	}
}

func registerArtifactTypeFlagCompletion(cmd *cobra.Command) {
	_ = cmd.RegisterFlagCompletionFunc("type", completionArtifactTypes())
}

var allMemoryTypes = []string{
	"convention", "correction", "decision", "tension", "fact", "skill",
}

func completionMemoryTypes() func(*cobra.Command, []string, string) ([]string, cobra.ShellCompDirective) {
	return func(_ *cobra.Command, _ []string, _ string) ([]string, cobra.ShellCompDirective) {
		return allMemoryTypes, cobra.ShellCompDirectiveNoFileComp
	}
}

func registerMemoryTypeFlagCompletion(cmd *cobra.Command) {
	_ = cmd.RegisterFlagCompletionFunc("type", completionMemoryTypes())
}

func completionInstalledArtifactIDs() func(*cobra.Command, []string, string) ([]string, cobra.ShellCompDirective) {
	return func(_ *cobra.Command, _ []string, _ string) ([]string, cobra.ShellCompDirective) {
		lf, err := hub.LoadLockfile(lockfilePath())
		if err != nil || lf == nil {
			return nil, cobra.ShellCompDirectiveNoFileComp
		}
		seen := make(map[string]struct{})
		for _, byID := range lf.Artifacts {
			for id := range byID {
				seen[id] = struct{}{}
			}
		}
		ids := make([]string, 0, len(seen))
		for id := range seen {
			ids = append(ids, id)
		}
		return ids, cobra.ShellCompDirectiveNoFileComp
	}
}

func completionBacklogSlugs() func(*cobra.Command, []string, string) ([]string, cobra.ShellCompDirective) {
	return func(_ *cobra.Command, _ []string, _ string) ([]string, cobra.ShellCompDirective) {
		projectDir, err := os.Getwd()
		if err != nil {
			return nil, cobra.ShellCompDirectiveNoFileComp
		}
		items, err := backlog.List(projectDir)
		if err != nil {
			return nil, cobra.ShellCompDirectiveNoFileComp
		}
		slugs := make([]string, 0, len(items))
		for _, item := range items {
			if !item.Done {
				slugs = append(slugs, item.Slug)
			}
		}
		return slugs, cobra.ShellCompDirectiveNoFileComp
	}
}

func completionASTContexts() func(*cobra.Command, []string, string) ([]string, cobra.ShellCompDirective) {
	return func(_ *cobra.Command, _ []string, _ string) ([]string, cobra.ShellCompDirective) {
		projectDir, err := os.Getwd()
		if err != nil {
			return nil, cobra.ShellCompDirectiveNoFileComp
		}
		contextDir := projectDir + "/" + brand.DotDir() + "/ast"
		entries, err := os.ReadDir(contextDir)
		if err != nil {
			return nil, cobra.ShellCompDirectiveNoFileComp
		}
		names := make([]string, 0)
		for _, e := range entries {
			if e.IsDir() && e.Name() != "project" {
				names = append(names, e.Name())
			}
		}
		return names, cobra.ShellCompDirectiveNoFileComp
	}
}
