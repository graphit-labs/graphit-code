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

// grammarEngine is the singleton wazero engine for tree-sitter WASM modules.
// Initialized once at startup by initWASMEngine().
var (
	grammarEngine     *wasmts.Engine
	grammarEngineOnce sync.Once
	grammarEngineErr  error

	// loadedLanguages caches *wasmts.Language by language name.
	loadedLanguages sync.Map // map[string]*wasmts.Language
)

// initWASMEngine initializes the global wazero engine (once).
func initWASMEngine() (*wasmts.Engine, error) {
	grammarEngineOnce.Do(func() {
		cacheDir := filepath.Join(userCacheDir(), "graphit", "wasmts")
		grammarEngine, grammarEngineErr = wasmts.NewEngine(cacheDir)
		if grammarEngineErr != nil {
			slog.Error("failed to initialize WASM tree-sitter engine", "error", grammarEngineErr)
		}
	})
	return grammarEngine, grammarEngineErr
}

// userCacheDir returns a cache directory for wazero compilation cache.
// Works on Windows (%LocalAppData%), macOS (~/Library/Caches), Linux (~/.cache).
func userCacheDir() string {
	dir, err := os.UserCacheDir()
	if err != nil {
		return os.TempDir()
	}
	return dir
}

// getLanguage returns a loaded Language for the given name.
// Each grammar is an individual .wasm file containing the tree-sitter runtime
// plus one grammar. This makes every grammar plug-and-play: just drop a
// tree-sitter-<lang>.wasm file into any directory in the resolution chain.
//
// Resolution chain (highest priority first):
//  1. Project: <projectDir>/.graphit/ast/grammars/
//  2. User global: ~/.graphit/ast/grammars/
//  3. Runtime: ~/.graphit/runtime/<version>/ast/grammars/
func getLanguage(langName string, projectDir string) (*wasmts.Language, error) {
	// Fast path: already loaded
	if v, ok := loadedLanguages.Load(langName); ok {
		return v.(*wasmts.Language), nil
	}

	engine, err := initWASMEngine()
	if err != nil {
		return nil, fmt.Errorf("init WASM engine: %w", err)
	}

	// The grammar function name inside the .wasm may differ from the
	// language name used in YAML configs (e.g. "csharp" → "c_sharp")
	funcName := grammarFuncName(langName)

	// Find the .wasm file for this grammar
	wasmPath := findGrammarWASM(funcName, projectDir)
	if wasmPath == "" {
		return nil, fmt.Errorf("no .wasm grammar found for %q (searched as tree-sitter-%s.wasm)", langName, funcName)
	}

	wasmBytes, err := os.ReadFile(wasmPath)
	if err != nil {
		return nil, fmt.Errorf("read grammar %q: %w", wasmPath, err)
	}

	slog.Debug("loading grammar WASM",
		"language", langName,
		"function", funcName,
		"path", wasmPath,
		"size", len(wasmBytes))

	// Each grammar is its own WASM module (contains ts runtime + grammar)
	moduleName := "tree-sitter-" + funcName
	mod, err := engine.LoadModule(moduleName, wasmBytes)
	if err != nil {
		return nil, fmt.Errorf("load WASM module for %q: %w", langName, err)
	}

	lang, err := mod.LoadLanguage(funcName)
	if err != nil {
		return nil, fmt.Errorf("load language %q from module: %w", langName, err)
	}

	loadedLanguages.Store(langName, lang)
	return lang, nil
}

// findGrammarWASM searches the resolution chain for a WASM grammar file.
// Returns the full path or empty string if not found.
//
// File name convention: tree-sitter-<funcName>.wasm
// Example: tree-sitter-go.wasm, tree-sitter-c_sharp.wasm
func findGrammarWASM(funcName string, projectDir string) string {
	fileName := "tree-sitter-" + funcName + ".wasm"
	searchDirs := grammarSearchDirs(projectDir)

	for _, dir := range searchDirs {
		path := filepath.Join(dir, fileName)
		if _, err := os.Stat(path); err == nil {
			return path
		}
	}
	return ""
}

// grammarSearchDirs returns directories to search for .wasm grammars,
// ordered by priority (highest first).
// Uses filepath.Join for cross-platform path compatibility (Windows/macOS/Linux).
func grammarSearchDirs(projectDir string) []string {
	var dirs []string

	// 1. Project-level (highest priority)
	if projectDir != "" {
		dirs = append(dirs, filepath.Join(projectDir, ".graphit", "ast", "grammars"))
	}

	// 2. User global
	home, _ := os.UserHomeDir()
	if home != "" {
		dirs = append(dirs, filepath.Join(home, ".graphit", "ast", "grammars"))
	}

	// 3. Runtime (lowest priority — bundled with release)
	if home != "" {
		dirs = append(dirs, filepath.Join(home, ".graphit", "runtime", version.Version, "ast", "grammars"))
	}

	return dirs
}

// initBuiltinGrammars loads all .wasm grammar files found in the resolution chain
// and returns a map of available languages. Called during init().
func initBuiltinGrammars() map[string]*wasmts.Language {
	engine, err := initWASMEngine()
	if err != nil {
		slog.Warn("WASM engine init failed, tree-sitter parsing unavailable", "error", err)
		return nil
	}

	// Scan all grammar directories for .wasm files
	searchDirs := grammarSearchDirs("")
	result := make(map[string]*wasmts.Language)

	// Collect all .wasm files, respecting priority (first found wins)
	seen := make(map[string]bool)
	for _, dir := range searchDirs {
		entries, err := os.ReadDir(dir)
		if err != nil {
			continue
		}
		for _, entry := range entries {
			if entry.IsDir() || !strings.HasSuffix(entry.Name(), ".wasm") {
				continue
			}
			// Extract function name from filename: tree-sitter-<name>.wasm → <name>
			name := strings.TrimSuffix(entry.Name(), ".wasm")
			name = strings.TrimPrefix(name, "tree-sitter-")
			if name == "" || seen[name] {
				continue
			}
			seen[name] = true

			wasmPath := filepath.Join(dir, entry.Name())
			wasmBytes, err := os.ReadFile(wasmPath)
			if err != nil {
				slog.Warn("failed to read grammar", "path", wasmPath, "error", err)
				continue
			}

			moduleName := "tree-sitter-" + name
			mod, err := engine.LoadModule(moduleName, wasmBytes)
			if err != nil {
				slog.Warn("failed to load grammar module", "name", name, "error", err)
				continue
			}

			lang, err := mod.LoadLanguage(name)
			if err != nil {
				slog.Warn("failed to load language", "name", name, "error", err)
				continue
			}

			result[name] = lang
			loadedLanguages.Store(name, lang)
		}
	}

	if len(result) > 0 {
		names := make([]string, 0, len(result))
		for k := range result {
			names = append(names, k)
		}
		slog.Info("loaded tree-sitter WASM grammars",
			"count", len(result),
			"languages", strings.Join(names, ", "))
	} else {
		slog.Debug("no .wasm grammars found in search paths")
	}

	return result
}

// CloseGrammarEngine shuts down the WASM engine.
// Should be called at program exit.
func CloseGrammarEngine() {
	if grammarEngine != nil {
		grammarEngine.Close()
	}
}
