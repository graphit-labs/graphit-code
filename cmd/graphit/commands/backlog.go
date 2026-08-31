package commands

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/graphit-labs/graphit-code/internal/backlog"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/output"
	"github.com/spf13/cobra"
)

func newBacklogCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "backlog",
		Short: "Manage the task backlog",
		Long: `Manage the task backlog — project tasks recorded for later work.

A backlog item records a project task that should survive the conversation or
session that identified it. Recording, listing, and removing tasks is independent
of Dream. Dream improves project knowledge and never consumes backlog items.

Items live as markdown files in ` + config.DefaultBacklogDir(nil, nil) + `/, so they are
versioned with the project and visible in review. Point backlog.dir
somewhere else to override that.

Subcommands:
  list  List every backlog item
  add   Add an item
  rm    Remove an item

Examples:
  ` + brand.BinName() + ` backlog list
  ` + brand.BinName() + ` backlog add "Refactor the auth module"
  ` + brand.BinName() + ` backlog add "Add error handling to API" --body "Focus on the /api/v2 endpoints"
  ` + brand.BinName() + ` backlog rm refactor-the-auth-module`,
	}

	cmd.AddCommand(
		newBacklogListCmd(),
		newBacklogAddCmd(),
		newBacklogRmCmd(),
	)

	return cmd
}

func newBacklogListCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "list",
		Short: "List the task backlog",
		Long: `List every item in the task backlog.

Each item is a markdown file in ` + config.DefaultBacklogDir(nil, nil) + `/ by default.
Dream may pick up pending items when that optional module is enabled.

Examples:
  ` + brand.BinName() + ` backlog list`,
		RunE: func(cmd *cobra.Command, args []string) error {
			return runBacklogList()
		},
	}
}

func runBacklogList() error {
	p := output.NewPrinter("")

	projectDir, err := os.Getwd()
	if err != nil {
		return fmt.Errorf("resolving project directory: %w", err)
	}

	items, err := backlog.List(projectDir)
	if err != nil {
		return fmt.Errorf("listing the backlog: %w", err)
	}

	if len(items) == 0 {
		p.Info("The task backlog is empty.")
		p.Step("Add an item with: %s backlog add \"Title of the item\"", brand.BinName())
		return nil
	}

	p.Header("Task Backlog")
	p.Info("%d registered task(s)", len(items))
	p.Blank()

	for _, item := range items {
		p.Step("%s", item.Title)
		p.Detail("Slug", item.Slug)
		p.Detail("Created", item.CreatedAt.Format("2006-01-02 15:04:05"))

		relPath := item.Path
		if rel, err := filepath.Rel(projectDir, item.Path); err == nil {
			relPath = rel
		}
		p.Detail("File", relPath)

	}
	p.Blank()

	return nil
}

func newBacklogAddCmd() *cobra.Command {
	var body string

	cmd := &cobra.Command{
		Use:   "add [title]",
		Short: "Add an item to the task backlog",
		Long: `Add an item to the task backlog.

The title becomes the filename (slugified). Use --body for the full brief.

Write the body for a reader with no conversation history: name the paths, the
symptom, what you already ruled out, and how to tell it worked.

Examples:
  ` + brand.BinName() + ` backlog add "Refactor the auth module"
  ` + brand.BinName() + ` backlog add "Fix API error handling" --body "Focus on /api/v2 endpoints, add proper HTTP status codes"`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return runBacklogAdd(args[0], body)
		},
	}

	cmd.Flags().StringVar(&body, "body", "", "The full brief for whoever picks the item up")
	return cmd
}

func runBacklogAdd(title, body string) error {
	p := output.NewPrinter("")

	projectDir, err := os.Getwd()
	if err != nil {
		return fmt.Errorf("resolving project directory: %w", err)
	}

	item, err := backlog.Add(projectDir, title, body)
	if err != nil {
		return fmt.Errorf("adding the backlog item: %w", err)
	}

	p.Success("Backlog item added: %s", item.Title)
	p.KeyValue("Slug", item.Slug)
	if rel, err := filepath.Rel(projectDir, item.Path); err == nil {
		p.KeyValue("File", rel)
	} else {
		p.KeyValue("File", item.Path)
	}
	p.Step("The task is recorded independently of Dream.")
	p.Step("Edit the file to add more details if needed.")

	return nil
}

func newBacklogRmCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "rm [slug]",
		Short: "Remove an item from the task backlog",
		Long: `Remove a backlog item by its slug (filename without extension).

Use '` + brand.BinName() + ` backlog list' to see available slugs.

Examples:
  ` + brand.BinName() + ` backlog rm refactor-the-auth-module`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return runBacklogRm(args[0])
		},
	}
	cmd.ValidArgsFunction = completionBacklogSlugs()
	return cmd
}

func runBacklogRm(slug string) error {
	p := output.NewPrinter("")

	projectDir, err := os.Getwd()
	if err != nil {
		return fmt.Errorf("resolving project directory: %w", err)
	}

	if err := backlog.Remove(projectDir, slug); err != nil {
		return fmt.Errorf("removing the backlog item: %w", err)
	}

	p.Success("Backlog item removed: %s", slug)
	return nil
}
