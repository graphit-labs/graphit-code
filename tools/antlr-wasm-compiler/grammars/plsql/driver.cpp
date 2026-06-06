// PL/SQL grammar driver — WASM multi-parse mode.
//
// Uses LL-only + NoThrowErrorStrategy because:
// - wazero v1.12 exnref has "invalid table access" with C++ vtables
// - Legacy WASM EH is rejected by wazero
// - LL + NoThrow is exception-free and handles most Oracle SQL correctly
//
// Switch to SLL_LL when wazero fixes exnref + indirect calls.
#include "PlSqlLexer.h"
#include "PlSqlParser.h"
#include "wasm_driver.h"

int main() {
    return graphit::wasmDriverMain<PlSqlLexer, PlSqlParser>(
        &PlSqlParser::sql_script,
        graphit::ParseMode::LL,
        graphit::ErrorStrategy::NoThrow
    );
}
