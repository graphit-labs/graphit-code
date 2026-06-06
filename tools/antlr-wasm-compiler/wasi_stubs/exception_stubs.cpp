// Minimal C++ exception ABI for WASI (wasi-sdk 33, wasip1 target).
//
// Since WASI doesn't have stable EH support (-fwasm-exceptions requires
// the Exception Handling proposal which isn't stable with LTO, and
// setjmp/longjmp also requires it), we implement a fake exception mechanism
// using global state. This works because:
//
// 1. WASM is single-threaded — no TLS needed
// 2. ANTLR's generated parser wraps every rule in try/catch
// 3. The catch blocks call __cxa_begin_catch which returns the exception obj
// 4. The throw→catch flow is: __cxa_throw → (stack unwind) → __cxa_begin_catch
//
// WITHOUT real unwinding, we can't actually jump from throw to catch.
// So __cxa_throw must abort(). The NoThrowErrorStrategy's job is to prevent
// all throws by intercepting error recovery BEFORE ANTLR calls throw.
//
// This file provides the link-time symbols so the binary compiles.
// If any path actually calls __cxa_throw at runtime, it aborts.
#include <cstdlib>
#include <cstdio>

extern "C" {

void *__cxa_allocate_exception(unsigned long size) {
    (void)size;
    // Allocate a dummy — we'll abort in __cxa_throw anyway
    static char dummy[256];
    return dummy;
}

[[noreturn]]
void __cxa_throw(void *, void *, void (*)(void *)) {
    fprintf(stderr, "ANTLR: __cxa_throw reached (aborting)\n");
    abort();
}

void *__cxa_init_primary_exception(void *obj, void *, void (*)(void *)) {
    return obj;
}

void *__cxa_begin_catch(void *exnObj) {
    return exnObj;
}

void __cxa_end_catch() {}

void __cxa_free_exception(void *obj) {
    (void)obj;
}

int _Unwind_CallPersonality(void *) { return 0; }
int __cpp_exception = 0;

} // extern "C"
