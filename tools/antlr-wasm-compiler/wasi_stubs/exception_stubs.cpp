// Minimal C++ exception ABI for WASI (wasi-sdk 33, wasip1 target).
//
// Strategy: "soft throw" — __cxa_throw sets a global error flag and calls
// _exit(73). The Go host detects exit code 73 as "parse error" and restarts
// the WASM process for subsequent files.
#include <cstdlib>
#include <unistd.h>

extern "C" {

int g_antlr_error_flag = 0;

void *__cxa_allocate_exception(unsigned long size) {
    (void)size;
    static char dummy[512];
    return dummy;
}

[[noreturn]]
void __cxa_throw(void *, void *, void (*)(void *)) {
    g_antlr_error_flag = 1;
    _exit(73);
}

void *__cxa_init_primary_exception(void *obj, void *, void (*)(void *)) {
    return obj;
}

void *__cxa_begin_catch(void *exnObj) { return exnObj; }
void __cxa_end_catch() {}
void __cxa_free_exception(void *obj) { (void)obj; }

int _Unwind_CallPersonality(void *) { return 0; }
int __cpp_exception = 0;

} // extern "C"
