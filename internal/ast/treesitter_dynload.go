package ast

/*
#cgo linux LDFLAGS: -ldl
#cgo darwin LDFLAGS: -ldl
#include <stdlib.h>
#include <dlfcn.h>

// dlopen_grammar opens a shared library and resolves the tree-sitter language symbol.
// Returns the TSLanguage pointer or NULL on error. Sets errmsg on failure.
static void* dlopen_grammar(const char* path, const char* symbol, char** errmsg) {
    void* handle = dlopen(path, RTLD_NOW | RTLD_GLOBAL);
    if (!handle) {
        *errmsg = (char*)dlerror();
        return NULL;
    }

    // Clear any previous error.
    dlerror();

    // The symbol is a function: const TSLanguage* tree_sitter_<lang>(void)
    typedef void* (*ts_lang_fn)(void);
    ts_lang_fn fn = (ts_lang_fn)dlsym(handle, symbol);
    char* err = (char*)dlerror();
    if (err != NULL) {
        *errmsg = err;
        dlclose(handle);
        return NULL;
    }

    void* lang = fn();
    if (!lang) {
        *errmsg = "symbol returned NULL";
        dlclose(handle);
        return NULL;
    }

    *errmsg = NULL;
    return lang;
}

// dlclose_handle closes a previously opened shared library handle.
static void dlclose_handle(const char* path) {
    void* handle = dlopen(path, RTLD_NOLOAD);
    if (handle) {
        dlclose(handle); // decrement refcount from our dlopen
        dlclose(handle); // decrement refcount from RTLD_NOLOAD
    }
}
*/
import "C"

import (
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"unsafe"

	sitter "github.com/smacker/go-tree-sitter"
)

// DynGrammarLoader loads tree-sitter grammars from shared libraries at runtime
// using CGO dlopen/dlsym. The project already requires CGO for go-tree-sitter,
// so using the standard C dynamic linking API adds zero new dependencies and is
// battle-tested across all platforms.
//
// Shared libraries are expected to export a function named tree_sitter_<lang>()
// that returns a *TSLanguage pointer.
//
// Search order:
// 1. Project: .graphit/grammars/treesitter/
// 2. User:    ~/.graphit/grammars/treesitter/
// 3. Runtime: ~/.graphit/runtime/<version>/grammars/treesitter/
type DynGrammarLoader struct {
	// projectDir is the root of the project for project-local grammar lookup.
	projectDir string

	// version is the runtime version string for runtime-level grammar lookup.
	version string

	// extraPaths are additional directories to search for grammar shared libraries.
	extraPaths []string

	// cache stores loaded *sitter.Language keyed by language name.
	cache sync.Map

	// loadedPaths tracks which library paths are loaded (for cleanup).
	loadedPaths sync.Map
}

// DynGrammarLoaderOption configures the DynGrammarLoader.
type DynGrammarLoaderOption func(*DynGrammarLoader)

// WithProjectDir sets the project directory for project-local grammar search.
func WithProjectDir(dir string) DynGrammarLoaderOption {
	return func(l *DynGrammarLoader) {
		l.projectDir = dir
	}
}

// WithVersion sets the runtime version for versioned grammar search.
func WithVersion(v string) DynGrammarLoaderOption {
	return func(l *DynGrammarLoader) {
		l.version = v
	}
}

// WithExtraPaths adds additional search directories.
func WithExtraPaths(paths ...string) DynGrammarLoaderOption {
	return func(l *DynGrammarLoader) {
		l.extraPaths = append(l.extraPaths, paths...)
	}
}

// NewDynGrammarLoader creates a new dynamic grammar loader.
func NewDynGrammarLoader(opts ...DynGrammarLoaderOption) *DynGrammarLoader {
	l := &DynGrammarLoader{}
	for _, opt := range opts {
		opt(l)
	}
	return l
}

// Load loads a tree-sitter grammar for the given language name.
// It returns a cached *sitter.Language if already loaded, otherwise
// searches for and loads the shared library.
func (l *DynGrammarLoader) Load(lang string) (*sitter.Language, error) {
	// Check cache first.
	if cached, ok := l.cache.Load(lang); ok {
		return cached.(*sitter.Language), nil
	}

	// Find the shared library.
	libPath, err := l.findLibrary(lang)
	if err != nil {
		return nil, fmt.Errorf("dynload: grammar %q: %w", lang, err)
	}

	// Load the shared library directly.
	return l.loadFromPath(lang, libPath)
}

// LoadFromPath loads a tree-sitter grammar from an explicit shared library path.
// This bypasses the search path resolution and loads directly from the given file.
func (l *DynGrammarLoader) LoadFromPath(lang, libPath string) (*sitter.Language, error) {
	// Check cache first.
	if cached, ok := l.cache.Load(lang); ok {
		return cached.(*sitter.Language), nil
	}

	return l.loadFromPath(lang, libPath)
}

// loadFromPath does the actual dlopen + symbol resolution via CGO.
func (l *DynGrammarLoader) loadFromPath(lang, libPath string) (*sitter.Language, error) {
	// The symbol name follows tree-sitter convention: tree_sitter_<lang>
	symName := "tree_sitter_" + lang

	cPath := C.CString(libPath)
	defer C.free(unsafe.Pointer(cPath))

	cSym := C.CString(symName)
	defer C.free(unsafe.Pointer(cSym))

	var cErr *C.char
	ptr := C.dlopen_grammar(cPath, cSym, &cErr) //nolint:gocritic // CGO call, not a duplicate expression
	if ptr == nil {
		errMsg := "unknown error"
		if cErr != nil {
			errMsg = C.GoString(cErr)
		}
		return nil, fmt.Errorf("dynload: dlopen %q symbol %q: %s", libPath, symName, errMsg)
	}

	language := sitter.NewLanguage(ptr)

	// Cache the result.
	l.cache.Store(lang, language)
	l.loadedPaths.Store(lang, libPath)

	return language, nil
}

// Close releases all loaded shared library handles.
func (l *DynGrammarLoader) Close() {
	l.loadedPaths.Range(func(key, value any) bool {
		if path, ok := value.(string); ok {
			cPath := C.CString(path)
			C.dlclose_handle(cPath)
			C.free(unsafe.Pointer(cPath))
		}
		l.loadedPaths.Delete(key)
		l.cache.Delete(key)
		return true
	})
}

// Loaded returns the list of currently loaded language names.
func (l *DynGrammarLoader) Loaded() []string {
	var names []string
	l.cache.Range(func(key, _ any) bool {
		names = append(names, key.(string))
		return true
	})
	return names
}

// findLibrary searches for the grammar shared library in the configured search paths.
func (l *DynGrammarLoader) findLibrary(lang string) (string, error) {
	candidates := l.libraryCandidates(lang)
	searchDirs := l.searchDirs()

	for _, dir := range searchDirs {
		for _, candidate := range candidates {
			path := filepath.Join(dir, candidate)
			if _, err := os.Stat(path); err == nil {
				return path, nil
			}
		}
	}

	return "", fmt.Errorf("shared library not found for %q in search paths: %v", lang, searchDirs)
}

// searchDirs returns the ordered list of directories to search for grammar libraries.
func (l *DynGrammarLoader) searchDirs() []string {
	var dirs []string

	// 1. Project-local grammars.
	if l.projectDir != "" {
		dirs = append(dirs, filepath.Join(l.projectDir, ".graphit", "grammars", "treesitter"))
	}

	// 2. User-level grammars.
	if home, err := os.UserHomeDir(); err == nil {
		dirs = append(dirs, filepath.Join(home, ".graphit", "grammars", "treesitter"))

		// 3. Runtime-versioned grammars (explicit version).
		if l.version != "" {
			dirs = append(dirs, filepath.Join(home, ".graphit", "runtime", l.version, "grammars", "treesitter"))
		}
	}

	// 4. Auto-discover from executable path.
	// The launcher extracts the core binary and grammars to the same runtime directory:
	//   ~/.graphit/runtime/<version>/graphit-core
	//   ~/.graphit/runtime/<version>/grammars/treesitter/
	// So we look for grammars/ relative to the executable's directory.
	if exe, err := os.Executable(); err == nil {
		if resolved, err := filepath.EvalSymlinks(exe); err == nil {
			exe = resolved
		}
		exeDir := filepath.Dir(exe)
		candidate := filepath.Join(exeDir, "grammars", "treesitter")
		if info, err := os.Stat(candidate); err == nil && info.IsDir() {
			dirs = append(dirs, candidate)
		}
	}

	// 5. Extra paths.
	dirs = append(dirs, l.extraPaths...)

	return dirs
}

// libraryCandidates returns the list of candidate filenames to look for,
// from most specific (platform+arch) to least specific (generic name).
func (l *DynGrammarLoader) libraryCandidates(lang string) []string {
	ext := sharedLibExt()
	osName := runtime.GOOS
	archName := runtime.GOARCH

	// Normalize language name: replace hyphens with underscores for the base name.
	baseName := "tree-sitter-" + strings.ReplaceAll(lang, "_", "-")

	return []string{
		// Most specific: tree-sitter-go-linux-amd64.so
		fmt.Sprintf("%s-%s-%s%s", baseName, osName, archName, ext),
		// Platform only: tree-sitter-go-linux.so
		fmt.Sprintf("%s-%s%s", baseName, osName, ext),
		// Generic: tree-sitter-go.so
		fmt.Sprintf("%s%s", baseName, ext),
	}
}

// sharedLibExt returns the platform-appropriate shared library file extension.
func sharedLibExt() string {
	switch runtime.GOOS {
	case "darwin":
		return ".dylib"
	case "windows":
		return ".dll"
	default:
		return ".so"
	}
}
