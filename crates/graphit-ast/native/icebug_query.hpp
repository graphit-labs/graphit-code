#pragma once
#include "rust/cxx.h"
#include <cstddef>
#include <cstdint>
#include <memory>
namespace lbug::main { class Database; }
namespace graphit::icebug {
struct GraphIo;
struct QuerySignal;
struct ResultSink;
struct QueryOptions;
void validate_live_query(rust::Str query);
void run_query(rust::Box<GraphIo>, rust::Box<QuerySignal>, rust::Str schema,
    rust::Str query, rust::Str parameters, const QueryOptions& options, ResultSink& result);
}
