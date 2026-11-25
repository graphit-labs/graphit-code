#include "PlSqlParserBase.h"
#include "plsql/PlSqlParser.h"
#include <stdexcept>

bool PlSqlParserBase::isNotStartOfJoin() {
  using Parser = graphit::antlr::plsql::PlSqlParser;
  const auto *next = getTokenStream()->LT(1);
  if (next == nullptr) {
    throw std::logic_error("PL/SQL join predicate requires a lookahead token");
  }
  switch (next->getType()) {
  case Parser::INNER:
  case Parser::CROSS:
  case Parser::NATURAL:
  case Parser::PARTITION:
  case Parser::FULL:
  case Parser::LEFT:
  case Parser::RIGHT:
  case Parser::OUTER:
    return false;
  default:
    return true;
  }
}
