package daemon

import "context"

// SuperviseGlobal keeps a module that belongs to the machine rather than to a project running,
// with the same restart, backoff and panic handling a project's modules get.
//
// Global modules include the embedding server, whose ONNX session is shared across projects, and
// the optional unified UI. They need the same restart and panic handling as project modules.
//
// It blocks until ctx is cancelled or the module gives up, so callers run it in a goroutine.
func SuperviseGlobal(ctx context.Context, mod WatchModule, logf func(string, ...any)) {
	if mod == nil {
		return
	}
	if logf == nil {
		logf = func(string, ...any) {}
	}
	superviseModule(ctx, newModuleEntry(mod), logf)
}
