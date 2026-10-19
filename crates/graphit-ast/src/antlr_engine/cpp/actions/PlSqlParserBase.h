#pragma once
#include "antlr4-runtime.h"

class PlSqlParserBase : public antlr4::Parser {
public:
  explicit PlSqlParserBase(antlr4::TokenStream *input) : antlr4::Parser(input) {}
  // The maintained union enables Oracle versions 10, 11 and 12.
  bool isVersion10() const noexcept { return true; }
  bool isVersion11() const noexcept { return true; }
  bool isVersion12() const noexcept { return true; }
  bool isNotStartOfJoin();
};
