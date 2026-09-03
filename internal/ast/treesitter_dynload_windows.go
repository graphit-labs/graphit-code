//go:build windows

package ast

/*
#include <stdlib.h>
#include <stdio.h>
#include <windows.h>

// dlopen_grammar opens a DLL and resolves the tree-sitter language symbol.
// Returns the TSLanguage pointer or NULL on error. Sets errmsg on failure.
// Uses the same function signature as the Unix version for code symmetry.
static void* dlopen_grammar(const char* path, const char* symbol, char** errmsg) {
    static char buf[512];

    HMODULE handle = LoadLibraryA(path);
    if (!handle) {
        snprintf(buf, sizeof(buf), "LoadLibraryA failed (error %lu)", GetLastError());
        *errmsg = buf;
        return NULL;
    }

    // The symbol is a function: const TSLanguage* tree_sitter_<lang>(void)
    typedef void* (*ts_lang_fn)(void);
    ts_lang_fn fn = (ts_lang_fn)GetProcAddress(handle, symbol);
    if (!fn) {
        snprintf(buf, sizeof(buf), "GetProcAddress failed for '%s' (error %lu)", symbol, GetLastError());
        *errmsg = buf;
        FreeLibrary(handle);
        return NULL;
    }

    void* lang = fn();
    if (!lang) {
        *errmsg = "symbol returned NULL";
        FreeLibrary(handle);
        return NULL;
    }

    *errmsg = NULL;
    return lang;
}

// dlclose_handle closes a previously loaded DLL by path.
static void dlclose_handle(const char* path) {
    HMODULE handle = GetModuleHandleA(path);
    if (handle) {
        FreeLibrary(handle);
    }
}
*/
import "C"

import (
	"fmt"
	"unsafe"

	sitter "github.com/tree-sitter/go-tree-sitter"
)

func (l *DynGrammarLoader) loadFromPath(lang, libPath string) (*sitter.Language, error) {
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
		return nil, fmt.Errorf("dynload: LoadLibrary %q symbol %q: %s", libPath, symName, errMsg)
	}

	language := sitter.NewLanguage(ptr)

	l.cache.Store(lang, language)
	l.loadedPaths.Store(lang, libPath)

	return language, nil
}

// Close releases all loaded DLL handles (Windows implementation).
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
