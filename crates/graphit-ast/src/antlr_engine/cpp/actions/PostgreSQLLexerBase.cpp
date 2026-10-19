#include "PostgreSQLLexerBase.h"
#include "postgresql/PostgreSQLLexer.h"
#include <stdexcept>

void PostgreSQLLexerBase::reset() {
  tags_.clear();
  antlr4::Lexer::reset();
}
void PostgreSQLLexerBase::PushTag() { tags_.push_back(getText()); }
bool PostgreSQLLexerBase::IsTag() {
  return !tags_.empty() && getText() == tags_.back();
}
void PostgreSQLLexerBase::PopTag() {
  if (tags_.empty()) {
    throw std::logic_error("PostgreSQL dollar string has no opening tag");
  }
  tags_.pop_back();
}
bool PostgreSQLLexerBase::CheckLaMinus() { return _input->LA(1) != '-'; }
bool PostgreSQLLexerBase::CheckLaStar() { return _input->LA(1) != '*'; }
bool PostgreSQLLexerBase::IsSemiColon() { return _input->LA(1) == ';'; }
void PostgreSQLLexerBase::HandleNumericFail() {
  const auto index = _input->index();
  const auto column = getCharPositionInLine();
  if (index < 2 || column < 2) {
    throw std::logic_error("PostgreSQL numeric rollback requires two scalars");
  }
  _input->seek(index - 2);
  // Both consumed dots belong to the following token, including its column.
  setCharPositionInLine(column - 2);
  setType(graphit::antlr::postgresql::PostgreSQLLexer::Integral);
}
void PostgreSQLLexerBase::HandleLessLessGreaterGreater() {
  using Lexer = graphit::antlr::postgresql::PostgreSQLLexer;
  const auto text = getText();
  if (text == "<<") {
    setType(Lexer::LESS_LESS);
  } else if (text == ">>") {
    setType(Lexer::GREATER_GREATER);
  }
}
