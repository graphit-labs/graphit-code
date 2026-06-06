package ast

import (
	"fmt"
	"log/slog"
	"os"
	"path/filepath"

	"sync"

	"github.com/graphit-labs/graphit-code/internal/ast/wasmantlr"
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

func GetEngine() *wasmts.Engine {
	engine, _ := initWASMEngine()
	return engine
}

// userCacheDir returns a cache directory for wazero compilation cache.
func userCacheDir() string {
	dir, err := os.UserCacheDir()
	if err != nil {
		return os.TempDir()
	}
	return dir
}

// getLanguage returns a loaded Language for the given grammar.
// grammarName is the YAML-declared name (e.g. "tree-sitter-c_sharp").
// Each grammar is an individual .wasm file: just drop a
// <grammar>.wasm file into any directory in the resolution chain.
//
// Resolution chain (highest priority first):
//  1. Project: <projectDir>/.graphit/ast/grammars/
//  2. User global: ~/.graphit/ast/grammars/
//  3. Runtime: ~/.graphit/runtime/<version>/ast/grammars/
func getLanguage(grammarName string, projectDir string) (*wasmts.Language, error) {
	if v, ok := loadedLanguages.Load(grammarName); ok {
		return v.(*wasmts.Language), nil
	}

	engine, err := initWASMEngine()
	if err != nil {
		return nil, fmt.Errorf("init WASM engine: %w", err)
	}

	wasmPath := findGrammarWASM(grammarName, projectDir)
	if wasmPath == "" {
		return nil, fmt.Errorf("no .wasm grammar found for %q (searched as %s.wasm)", grammarName, grammarName)
	}

	wasmBytes, err := os.ReadFile(wasmPath)
	if err != nil {
		return nil, fmt.Errorf("read grammar %q: %w", wasmPath, err)
	}

	slog.Debug("loading grammar WASM",
		"grammar", grammarName,
		"path", wasmPath,
		"size", len(wasmBytes))

	mod, err := engine.LoadModule(grammarName, wasmBytes)
	if err != nil {
		return nil, fmt.Errorf("load WASM module for %q: %w", grammarName, err)
	}

	langs := mod.ListAvailableLanguages()
	if len(langs) == 0 {
		return nil, fmt.Errorf("no language export found in module %q", grammarName)
	}

	lang, err := mod.LoadLanguage(langs[0])
	if err != nil {
		return nil, fmt.Errorf("load language from module %q: %w", grammarName, err)
	}

	loadedLanguages.Store(grammarName, lang)
	return lang, nil
}

// findGrammarWASM searches the resolution chain for a WASM grammar file.
// Returns the full path or empty string if not found.
// Works for any grammar type (tree-sitter, ANTLR, etc.).
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

// grammarSearchDirs returns directories to search for .wasm grammars,
// ordered by priority (highest first).
func grammarSearchDirs(projectDir string) []string {
	var dirs []string

	if projectDir != "" {
		dirs = append(dirs, filepath.Join(projectDir, ".graphit", "ast", "grammars"))
	}

	home, _ := os.UserHomeDir()
	if home != "" {
		dirs = append(dirs, filepath.Join(home, ".graphit", "ast", "grammars"))
	}

	if home != "" {
		dirs = append(dirs, filepath.Join(home, ".graphit", "runtime", version.Version, "ast", "grammars"))
	}

	return dirs
}

// ---------------------------------------------------------------------------
// ANTLR v4 Grammar Loading
// ---------------------------------------------------------------------------

var (
	antlrEngine     *wasmantlr.Engine
	antlrEngineOnce sync.Once
	antlrEngineErr  error

	loadedAntlrGrammars sync.Map // map[string]bool — name → compiled flag
)

func initAntlrEngine() (*wasmantlr.Engine, error) {
	antlrEngineOnce.Do(func() {
		cacheDir := filepath.Join(userCacheDir(), "graphit", "wasmantlr")
		antlrEngine, antlrEngineErr = wasmantlr.NewEngine(cacheDir)
		if antlrEngineErr != nil {
			slog.Error("failed to initialize ANTLR WASM engine", "error", antlrEngineErr)
		}
	})
	return antlrEngine, antlrEngineErr
}

// GetAntlrEngine returns the global ANTLR WASM engine singleton.
func GetAntlrEngine() *wasmantlr.Engine {
	engine, _ := initAntlrEngine()
	return engine
}

// getAntlrModule ensures an ANTLR grammar is loaded and returns the engine.
// grammarName is the YAML-declared name (e.g. "antlr-plsql").
// Prefers a native binary (fast subprocess) over WASM (portable fallback).
func getAntlrModule(grammarName string, projectDir string) (*wasmantlr.Engine, error) {
	if _, ok := loadedAntlrGrammars.Load(grammarName); ok {
		return antlrEngine, nil
	}

	engine, err := initAntlrEngine()
	if err != nil {
		return nil, fmt.Errorf("init ANTLR engine: %w", err)
	}

	// Prefer native binary: persistent subprocess with batch protocol.
	nativePath := findNativeBinary(grammarName, projectDir)
	if nativePath != "" {
		slog.Debug("loading ANTLR grammar (native binary)",
			"grammar", grammarName,
			"path", nativePath)

		if err := engine.RegisterNativeBinary(grammarName, nativePath); err != nil {
			slog.Warn("failed to start native parser, falling back to WASM",
				"grammar", grammarName, "error", err)
		} else {
			loadedAntlrGrammars.Store(grammarName, true)
			return engine, nil
		}
	}

	// Fallback: WASM interpreter (portable but slow).
	wasmPath := findGrammarWASM(grammarName, projectDir)
	if wasmPath == "" {
		return nil, fmt.Errorf("no grammar found for %q (searched as %s and %s.wasm)", grammarName, grammarName, grammarName)
	}

	wasmBytes, err := os.ReadFile(wasmPath)
	if err != nil {
		return nil, fmt.Errorf("read ANTLR grammar %q: %w", wasmPath, err)
	}

	slog.Debug("loading ANTLR grammar (WASM fallback)",
		"grammar", grammarName,
		"path", wasmPath,
		"size", len(wasmBytes))

	if err := engine.Compile(grammarName, wasmBytes); err != nil {
		return nil, fmt.Errorf("compile ANTLR module for %q: %w", grammarName, err)
	}

	loadedAntlrGrammars.Store(grammarName, true)
	return engine, nil
}

// findNativeBinary searches for an executable binary in the grammar directories.
func findNativeBinary(grammarName string, projectDir string) string {
	searchDirs := grammarSearchDirs(projectDir)

	for _, dir := range searchDirs {
		path := filepath.Join(dir, grammarName)
		info, err := os.Stat(path)
		if err == nil && !info.IsDir() && info.Mode()&0111 != 0 {
			return path
		}
	}
	return ""
}

