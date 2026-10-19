#pragma once
#include "antlr4-runtime.h"

class PlSqlLexerBase : public antlr4::Lexer {
public:
  explicit PlSqlLexerBase(antlr4::CharStream *input) : antlr4::Lexer(input) {}
  bool IsNewlineAtPos(int position) {
    const auto scalar = _input->LA(position);
    return scalar == antlr4::IntStream::EOF || scalar == '\n';
  }
};
