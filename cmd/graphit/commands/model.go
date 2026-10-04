package commands

import (
	"fmt"
	"os"
	"os/signal"
	"syscall"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/clirun"
	"github.com/spf13/cobra"
)

func newModelCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:    "model",
		Short:  "Local model server processes",
		Hidden: true,
	}
	cmd.AddCommand(newModelServeCmd())
	return cmd
}

func newModelServeCmd() *cobra.Command {
	return &cobra.Command{
		Use:       "serve <embedding|rerank>",
		Short:     "Serve one local model over its socket",
		ValidArgs: []string{string(ai.ModelTaskEmbedding), string(ai.ModelTaskRerank)},
		Long: `Serve the configured embedding or rerank model on its socket under the
global ` + brand.DotDir() + `/daemon directory until stopped.

The daemon runs one of these processes per model, so each model's CPU and memory
belong to a process of its own. The model loads on the first request.`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			task := ai.ModelTask(args[0])
			if task != ai.ModelTaskEmbedding && task != ai.ModelTaskRerank {
				return fmt.Errorf("unknown model %q: want embedding or rerank", args[0])
			}
			// Take shutdown over from main's immediate exit, as the daemon does, so the
			// socket is removed and in-flight requests finish.
			signal.Reset(os.Interrupt, syscall.SIGTERM)
			ctx, stop := signal.NotifyContext(commandContext(cmd), os.Interrupt, syscall.SIGTERM)
			defer stop()
			clirun.WatchParent(stop)

			if err := ai.ValidateConfiguredLocalONNXDevices(task); err != nil {
				return err
			}
			if task == ai.ModelTaskEmbedding {
				// Fetch configured bundles ahead of the first request; the download
				// belongs to the process that will hold the model.
				go func() { _ = ai.PrefetchConfiguredLocalModels(ctx) }()
			}
			ai.LogModelServerStart(task)
			return ai.ServeModel(ctx, task, ai.ModelSocketPath(task))
		},
	}
}
