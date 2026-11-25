#pragma once
#include "antlr4-runtime.h"
#include <string>
#include <vector>

class PostgreSQLLexerBase : public antlr4::Lexer {
public:
  explicit PostgreSQLLexerBase(antlr4::CharStream *input) : antlr4::Lexer(input) {}
  void reset() override;
  void PushTag();
  bool IsTag();
  void PopTag();
  bool CheckLaMinus();
  bool CheckLaStar();
  void HandleNumericFail();
  void HandleLessLessGreaterGreater();
  bool IsSemiColon();
private:
  std::vector<std::string> tags_;
};
