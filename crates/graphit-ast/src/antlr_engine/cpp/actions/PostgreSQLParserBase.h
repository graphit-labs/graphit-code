#pragma once
#include "antlr4-runtime.h"
#include <vector>

struct RoutineBodyObservation {
  antlr4::ParserRuleContext *host;
  antlr4::Token *opening;
  antlr4::Token *closing;
};

class PostgreSQLParserBase : public antlr4::Parser {
public:
  explicit PostgreSQLParserBase(antlr4::TokenStream *input) : antlr4::Parser(input) {}
  void reset() override;
  void ParseRoutineBody();
  bool OnlyAcceptableOps();
  const std::vector<RoutineBodyObservation> &routine_bodies() const noexcept {
    return bodies_;
  }
private:
  std::vector<RoutineBodyObservation> bodies_;
};
