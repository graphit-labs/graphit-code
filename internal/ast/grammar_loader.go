package ast

import (
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"strings"
	"sync"

	"github.com/graphit-labs/graphit-code/internal/ast/wasmts"
	"github.com/graphit-labs/graphit-code/internal/version"
)

var (
	tsEngine     *wasmts.Engine
	tsEngineOnce sync.Once
	tsEngineErr  error
	pluginMu     sync.Mutex

	// Map of grammarName -> *sync.Pool
	grammarPools sync.Map
)

func initTSEngine() (*wasmts.Engine, error) {
	tsEngineOnce.Do(func() {
		home, _ := os.UserHomeDir()
		var cacheDir string
		if home != "" {
			cacheDir = filepath.Join(home, ".cache", "graphit", "wasmtime")
		}
		tsEngine, tsEngineErr = wasmts.NewEngine(cacheDir)
		if tsEngineErr != nil {
			slog.Error("failed to initialize Wasmtime TS engine", "error", tsEngineErr)
		}
	})
	return tsEngine, tsEngineErr
}

func getTSLanguage(grammarName string, projectDir string) (*wasmts.Language, func(), error) {
	poolVal, _ := grammarPools.LoadOrStore(grammarName, &sync.Pool{})
	pool := poolVal.(*sync.Pool)

	if val := pool.Get(); val != nil {
		lang := val.(*wasmts.Language)
		cleanup := func() {
			pool.Put(lang)
		}
		return lang, cleanup, nil
	}

	// Instantiate new
	lang, err := instantiateTSLanguage(grammarName, projectDir)
	if err != nil {
		return nil, nil, err
	}

	cleanup := func() {
		pool.Put(lang)
	}
	return lang, cleanup, nil
}

func instantiateTSLanguage(grammarName string, projectDir string) (*wasmts.Language, error) {
	engine, err := initTSEngine()
	if err != nil {
		return nil, fmt.Errorf("init Wasmtime engine: %w", err)
	}

	pluginMu.Lock()
	hasCompiled := engine.HasCompiledModule(grammarName)
	pluginMu.Unlock()

	var mod *wasmts.Module
	if !hasCompiled {
		pluginMu.Lock()
		// Double check
		hasCompiled = engine.HasCompiledModule(grammarName)
		if !hasCompiled {
			wasmPath := findGrammarWASM(grammarName, projectDir)
			if wasmPath == "" {
				pluginMu.Unlock()
				return nil, fmt.Errorf("no grammar found for %q (searched as %s.wasm)", grammarName, grammarName)
			}

			wasmBytes, err := os.ReadFile(wasmPath)
			if err != nil {
				pluginMu.Unlock()
				return nil, fmt.Errorf("read TS grammar %q: %w", wasmPath, err)
			}

			slog.Debug("loading TS grammar (WASM)",
				"grammar", grammarName,
				"path", wasmPath,
				"size", len(wasmBytes))

			var errLoad error
			mod, errLoad = engine.LoadModule(grammarName, wasmBytes)
			if errLoad != nil {
				pluginMu.Unlock()
				return nil, fmt.Errorf("load TS module for %q: %w", grammarName, errLoad)
			}
		} else {
			var errInst error
			mod, errInst = engine.InstantiateModule(grammarName)
			if errInst != nil {
				pluginMu.Unlock()
				return nil, fmt.Errorf("instantiate TS module for %q: %w", grammarName, errInst)
			}
		}
		pluginMu.Unlock()
	} else {
		var errInst error
		mod, errInst = engine.InstantiateModule(grammarName)
		if errInst != nil {
			return nil, fmt.Errorf("instantiate TS module for %q: %w", grammarName, errInst)
		}
	}

	langName := strings.TrimPrefix(grammarName, "tree-sitter-")
	lang, err := mod.LoadLanguage(langName)
	if err != nil {
		return nil, fmt.Errorf("load TS language %q: %w", langName, err)
	}

	return lang, nil
}

func findGrammarWASM(grammarName string, projectDir string) string {
	fileName := grammarName + ".wasm"
	searchDirs := grammarSearchDirs(projectDir)

	for _, dir := range searchDirs {
		path := filepath.Join(dir, fileName)
		if _, err := os.Stat(path); err == nil {
			return path
		}
	}
	return ""
}

func grammarSearchDirs(projectDir string) []string {
	var dirs []string

	if projectDir != "" {
		dirs = append(dirs, filepath.Join(projectDir, ".graphit", "ast", "grammars"))
		dirs = append(dirs, filepath.Join(projectDir, "internal", "ast", "grammars"))
	}

	home, _ := os.UserHomeDir()
	if home != "" {
		dirs = append(dirs, filepath.Join(home, ".graphit", "ast", "grammars"))
		dirs = append(dirs, filepath.Join(home, ".graphit", "runtime", version.Version, "ast", "grammars"))
	}

	return dirs
}

func CloseParserPlugin() {
	pluginMu.Lock()
	defer pluginMu.Unlock()
	if tsEngine != nil {
		tsEngine.Close()
		tsEngine = nil
	}
}
