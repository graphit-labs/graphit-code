// Command graphit-model serves one local model — embedding or rerank — over its socket.
//
// It is the model server the daemon supervises, built apart from the core so the process
// holding a model links only the model runtime: no LanceDB, graph engine or parsers. The
// launcher runs it for `graphit model ...`; the core keeps the same command as fallback.
package main

import (
	"context"
	"fmt"
	"os"
	"os/signal"
	"runtime"
	"syscall"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/clirun"
	"github.com/graphit-labs/graphit-code/internal/sysutil"
)

func main() {
	if err := run(os.Args[1:]); err != nil {
		fmt.Fprintf(os.Stderr, "%v\n", err)
		os.Exit(1)
	}
}

func run(args []string) error {
	if len(args) != 2 || args[0] != "serve" {
		return fmt.Errorf("usage: model serve <embedding|rerank>")
	}
	task := ai.ModelTask(args[1])
	if task != ai.ModelTaskEmbedding && task != ai.ModelTaskRerank {
		return fmt.Errorf("unknown model %q: want embedding or rerank", args[1])
	}
	if clirun.IsBackground() {
		runtime.GOMAXPROCS(sysutil.CPUBudget())
		_ = sysutil.LowerPriority()
	}

	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	clirun.WatchParent(stop)

	if err := ai.ValidateConfiguredLocalONNXDevices(task); err != nil {
		return err
	}
	if task == ai.ModelTaskEmbedding {
		go func() { _ = ai.PrefetchConfiguredLocalModels(ctx) }()
	}
	ai.LogModelServerStart(task)
	return ai.ServeModel(ctx, task, ai.ModelSocketPath(task))
}
