#pragma once

#include "rust/cxx.h"
#include <cstdint>
#include "antlr_engine/cpp/profile_queries.hpp"

namespace graphit::antlr {
enum class NativeDialect : std::uint8_t;
struct NativeSyntax;
struct NativeProfileQuery;
struct OperationControl;
std::unique_ptr<PreparedProfileQueries> prepare_profile_queries(NativeDialect, bool, rust::Slice<const NativeProfileQuery>, OperationControl &);
NativeSyntax recognize_prepared_profile_queries(NativeDialect, bool, rust::Str, const PreparedProfileQueries &, OperationControl &);
NativeSyntax recognize(NativeDialect, rust::Str, OperationControl &);
NativeSyntax recognize_queries(NativeDialect, rust::Str, rust::Slice<const rust::String>, OperationControl &);
NativeSyntax lex(NativeDialect, rust::Str, OperationControl &);
}
