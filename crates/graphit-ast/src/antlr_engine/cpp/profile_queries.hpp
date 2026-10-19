#pragma once

#include "antlr4-runtime.h"
#include "rust/cxx.h"
#include <functional>
#include <memory>
#include <unordered_map>

namespace graphit::antlr {
struct NativeProfileQuery;
struct NativeSyntax;
struct OperationControl;
class PreparedProfileQueries {
public:
  struct Impl;
  std::shared_ptr<const Impl> impl;
  explicit PreparedProfileQueries(std::shared_ptr<const Impl> value) : impl(std::move(value)) {}
};
std::unique_ptr<PreparedProfileQueries> prepare_profile_queries_for_parser(antlr4::Parser &,
    rust::Slice<const NativeProfileQuery>, OperationControl &, const std::function<void()> &);
void select_prepared_profile_queries(antlr4::Parser &, antlr4::tree::ParseTree *,
    const std::unordered_map<antlr4::tree::ParseTree *, std::size_t> &,
    const PreparedProfileQueries &, NativeSyntax &, OperationControl &, const std::function<void()> &);

}
