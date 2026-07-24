//go:build !windows

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
	"unsafe"

	sitter "github.com/tree-sitter/go-tree-sitter"
)

// loadFromPath does the actual dlopen + symbol resolution via CGO (Unix implementation).
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

// Close releases all loaded shared library handles (Unix implementation).
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
