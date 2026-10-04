// Command graphit-githook runs the commands a project configures for a Git hook event.
//
// Git runs `graphit _git-hook <event> [args]` on every commit, checkout, merge or push;
// the launcher runs this binary for it, so a hook costs a small process that reads the
// lockfile and starts the configured commands instead of a full core process.
package main

import (
	"context"
	"fmt"
	"os"

	"github.com/graphit-labs/graphit-code/internal/githookrun"
	"github.com/graphit-labs/graphit-code/internal/output"
)

func main() {
	if err := run(context.Background(), os.Args[1:]); err != nil {
		output.Fatal("%s", err)
	}
}

func run(ctx context.Context, args []string) error {
	if len(args) < 1 {
		return fmt.Errorf("requires at least 1 arg(s), only received 0")
	}
	projectDir, err := os.Getwd()
	if err != nil {
		return fmt.Errorf("git hook: determine project directory: %w", err)
	}
	return githookrun.Run(ctx, projectDir, args[0], os.Stdin, os.Stdout, os.Stderr, args[1:]...)
}
