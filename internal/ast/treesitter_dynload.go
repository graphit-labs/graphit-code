package ast

import (
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"sync"

	"github.com/graphit-labs/graphit-code/internal/brand"
	sitter "github.com/tree-sitter/go-tree-sitter"
)

// DynGrammarLoader loads tree-sitter grammars from shared libraries via dlopen.
// Libraries must export tree_sitter_<lang>() returning *TSLanguage.
type DynGrammarLoader struct {
	projectDir string

	cache sync.Map

	loadedPaths sync.Map
}

type DynGrammarLoaderOption func(*DynGrammarLoader)

func WithProjectDir(dir string) DynGrammarLoaderOption {
	return func(l *DynGrammarLoader) {
		l.projectDir = dir
	}
}

func NewDynGrammarLoader(opts ...DynGrammarLoaderOption) *DynGrammarLoader {
	l := &DynGrammarLoader{}
	for _, opt := range opts {
		opt(l)
	}
	return l
}

func (l *DynGrammarLoader) Load(lang string) (*sitter.Language, error) {
	if cached, ok := l.cache.Load(lang); ok {
		return cached.(*sitter.Language), nil
	}

	libPath, err := l.findLibrary(lang)
	if err != nil {
		return nil, fmt.Errorf("dynload: grammar %q: %w", lang, err)
	}

	return l.loadFromPath(lang, libPath)
}

// LoadFromPath loads a tree-sitter grammar from an explicit shared library path.
// This bypasses the search path resolution and loads directly from the given file.
func (l *DynGrammarLoader) LoadFromPath(lang, libPath string) (*sitter.Language, error) {
	if cached, ok := l.cache.Load(lang); ok {
		return cached.(*sitter.Language), nil
	}

	return l.loadFromPath(lang, libPath)
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

func (l *DynGrammarLoader) searchDirs() []string {
	var dirs []string

	if l.projectDir != "" {
		dirs = append(dirs, filepath.Join(l.projectDir, brand.DotDir(), "grammars", "treesitter"))
	}

	if global := brand.GlobalDir(); global != "" {
		dirs = append(dirs, filepath.Join(global, "grammars", "treesitter"))
	}

	return dirs
}

func (l *DynGrammarLoader) libraryCandidates(lang string) []string {
	ext := sharedLibExt()
	osName := runtime.GOOS
	archName := runtime.GOARCH

	baseName := "tree-sitter-" + strings.ReplaceAll(lang, "_", "-")

	candidates := []string{
		fmt.Sprintf("%s-%s-%s%s", baseName, osName, archName, ext),
		fmt.Sprintf("%s-%s%s", baseName, osName, ext),
		fmt.Sprintf("%s%s", baseName, ext),
	}

	if ext != ".so" {
		candidates = append(candidates, fmt.Sprintf("%s.so", baseName))
	}

	return candidates
}

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
