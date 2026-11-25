#include "PostgreSQLParserBase.h"
#include "postgresql/PostgreSQLParser.h"
#include <stdexcept>
#include <string_view>

namespace {
bool plpgsql_name(std::string_view name) {
  constexpr std::string_view expected = "plpgsql";
  if (name.size() != expected.size()) {
    return false;
  }
  for (std::size_t index = 0; index < name.size(); ++index) {
    const auto ch = name[index];
    const auto folded = ch >= 'A' && ch <= 'Z' ? ch + ('a' - 'A') : ch;
    if (folded != expected[index]) {
      return false;
    }
  }
  return true;
}
}

void PostgreSQLParserBase::reset() {
  bodies_.clear();
  antlr4::Parser::reset();
}
bool PostgreSQLParserBase::OnlyAcceptableOps() {
  const auto *next = getTokenStream()->LT(1);
  if (next == nullptr) {
    throw std::logic_error("PostgreSQL operator predicate requires lookahead");
  }
  const auto text = next->getText();
  return text == "!" || text == "!!" || text == "!=-";
}
void PostgreSQLParserBase::ParseRoutineBody() {
  using Parser = graphit::antlr::postgresql::PostgreSQLParser;
  auto *host = dynamic_cast<Parser::Createfunc_opt_listContext *>(getContext());
  if (host == nullptr) {
    throw std::logic_error("PostgreSQL routine action requires createfunc_opt_list");
  }
  bool procedural = false;
  for (auto *item : host->createfunc_opt_item()) {
    if (item->LANGUAGE() == nullptr) {
      continue;
    }
    auto *language = item->nonreservedword_or_sconst();
    if (language == nullptr) {
      continue; // Recovery diagnostics remain on the real parser.
    }
    if (language->nonreservedword() != nullptr) {
      procedural = procedural || plpgsql_name(language->getText());
    } else if (auto *constant = language->sconst()) {
      auto *value = constant->anysconst();
      if (value != nullptr && value->StringConstant() != nullptr) {
        const auto text = value->StringConstant()->getText();
        if (text.size() >= 2 && text.front() == '\'' && text.back() == '\'') {
          procedural = procedural ||
                       plpgsql_name(std::string_view(text).substr(1, text.size() - 2));
        }
      }
    }
  }
  if (!procedural) {
    return;
  }
  for (auto *item : host->createfunc_opt_item()) {
    auto *definition = item->func_as();
    if (item->AS() == nullptr || definition == nullptr) {
      continue;
    }
    for (auto *constant : definition->sconst()) {
      auto *value = constant->anysconst();
      if (value == nullptr || value->BeginDollarStringConstant() == nullptr) {
        continue;
      }
      auto *end = value->EndDollarStringConstant();
      bodies_.push_back({host, value->BeginDollarStringConstant()->getSymbol(),
                         end == nullptr ? nullptr : end->getSymbol()});
    }
  }
}
