#pragma once
#include "rust/cxx.h"
#include <cstddef>
#include <cstdint>
#include <memory>
namespace graphit::icebug {
struct GraphIo;
struct QuerySignal;
struct ResultSink;
struct QueryOptions;
struct GraphMount;
class NativeCatalog {
public:
    struct Impl;
    std::unique_ptr<Impl> impl;
    NativeCatalog(rust::Str schema,GraphMount mount,const QueryOptions& options);
    ~NativeCatalog();
};
std::unique_ptr<NativeCatalog> prepare_catalog(rust::Box<GraphIo>,rust::Box<QuerySignal>,
    rust::Str schema,GraphMount mount,const QueryOptions& options);
void rebind_catalog(NativeCatalog&,rust::Box<GraphIo>,rust::Box<QuerySignal>,GraphMount mount);
void run_query(const NativeCatalog&,rust::Box<GraphIo>, rust::Box<QuerySignal>,
    rust::Str query, rust::Str parameters, const QueryOptions& options, ResultSink& result);
}
