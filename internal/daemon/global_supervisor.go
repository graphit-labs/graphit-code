package daemon

import "context"

// SuperviseGlobal keeps a module that belongs to the machine rather than to a project running,
// with the same restart, backoff and panic handling a project's modules get.
//
// The two that need it are the memory watcher and the embedding server, and neither can be a
// project module: the memory store is global — one root holding every scope, a project's own and
// the user's — and the embedding server is one ONNX session shared by every process on the
// machine. Both used to be started as `go func() { _ = mod.Start(ctx) }()`, which discards the
// error, never restarts, and logs nothing. A watcher that died stopped recompiling memory
// silently; an embedding server that died sent every CLI back to loading its own copy of a
// 138 MB model, also silently.
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
