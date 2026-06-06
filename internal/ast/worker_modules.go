package ast

import (
	"fmt"

	"github.com/graphit-labs/graphit-code/internal/ast/wasmts"
)

// WorkerModules holds per-worker WASM module instances, created lazily.
// NOT thread-safe — one WorkerModules per goroutine.
type WorkerModules struct {
	engine    *wasmts.Engine
	modules   map[string]*wasmts.Module   // module name → worker-local instance
	languages map[string]*wasmts.Language  // lang func name → worker-local Language
}

func NewWorkerModules(engine *wasmts.Engine) *WorkerModules {
	return &WorkerModules{
		engine:    engine,
		modules:   make(map[string]*wasmts.Module),
		languages: make(map[string]*wasmts.Language),
	}
}

func (wm *WorkerModules) GetLanguage(grammarName string) (*wasmts.Language, error) {
	if wm == nil || wm.engine == nil {
		return nil, fmt.Errorf("worker modules not initialized")
	}

	if lang, ok := wm.languages[grammarName]; ok {
		return lang, nil
	}

	mod, err := wm.engine.InstantiateModule(grammarName)
	if err != nil {
		return nil, fmt.Errorf("worker instantiate %q: %w", grammarName, err)
	}

	langs := mod.ListAvailableLanguages()
	if len(langs) == 0 {
		mod.CloseModule() //nolint:errcheck
		return nil, fmt.Errorf("no language export found in worker module %q", grammarName)
	}

	lang, err := mod.LoadLanguage(langs[0])
	if err != nil {
		mod.CloseModule() //nolint:errcheck
		return nil, fmt.Errorf("worker load language from %q: %w", grammarName, err)
	}

	wm.modules[grammarName] = mod
	wm.languages[grammarName] = lang
	return lang, nil
}

func (wm *WorkerModules) Close() {
	if wm == nil {
		return
	}
	for _, mod := range wm.modules {
		mod.CloseModule() //nolint:errcheck
	}
	wm.modules = nil
	wm.languages = nil
}
