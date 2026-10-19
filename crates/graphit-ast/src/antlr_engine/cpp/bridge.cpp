#include "graphit-ast/src/antlr_engine/bridge.rs.h"
#include "antlr_engine/cpp/bridge.hpp"
#include "antlr_engine/cpp/profile_queries.hpp"
#include "plsql/PlSqlLexer.h"
#include "plsql/PlSqlParser.h"
#include "postgresql/PostgreSQLLexer.h"
#include "postgresql/PostgreSQLParser.h"
#include "db2/Db2Lexer.h"
#include "db2/Db2Parser.h"
#include "db2/Db2UnicodeLexer.h"
#include "db2/Db2UnicodeParser.h"
#include "support/Utf8.h"
#include "tsql/TSqlLexer.h"
#include "tsql/TSqlParser.h"
#include "cobol85/Cobol85Lexer.h"
#include "cobol85/Cobol85Parser.h"
#include "cobol85/Cobol85PreprocessorLexer.h"
#include "cobol85/Cobol85PreprocessorParser.h"
#include "tree/xpath/XPath.h"
#include "tree/xpath/XPathElement.h"
#include "tree/xpath/XPathLexer.h"
#include <limits>
#include <stdexcept>
#include <type_traits>
#include <unordered_map>

namespace graphit::antlr {
namespace {
struct OperationStopped final : std::exception {};
void poll(OperationControl &control) {
  if (!check(control)) throw OperationStopped{};
}
std::uint64_t width(std::size_t value) {
  static_assert(sizeof(std::size_t) <= sizeof(std::uint64_t));
  return static_cast<std::uint64_t>(value);
}
NativeSpan unindexed(UnindexedReason reason) {
  return {SpanKind::Unindexed, 0, 0, reason};
}
NativeSpan interval(std::size_t start, std::size_t end, std::size_t count) {
  if (start == INVALID_INDEX || end == INVALID_INDEX)
    return unindexed(UnindexedReason::InvalidSdkIndex);
  if (start > end || end > count)
    throw std::runtime_error("ANTLR interval is outside the actual scalar stream");
  return {SpanKind::RegularHalfOpen, width(start), width(end), UnindexedReason::NotApplicable};
}
NativeSpan token_span(antlr4::Token *token, std::size_t count) {
  if (token == nullptr) return unindexed(UnindexedReason::MissingTokenBounds);
  const auto start = token->getStartIndex();
  // EOF type precedes the SDK's unsigned sentinel. A recovered EOF without a
  // stream position remains unindexed, even when its spelling is '<EOF>'.
  if (token->getType() == antlr4::Token::EOF) {
    if (token->getTokenIndex() == INVALID_INDEX)
      return unindexed(UnindexedReason::InsertedRecoveryToken);
    if (start == INVALID_INDEX) return unindexed(UnindexedReason::InvalidSdkIndex);
    if (start != count) throw std::runtime_error("ANTLR EOF cursor differs from stream size");
    return {SpanKind::Eof, width(start), width(start), UnindexedReason::NotApplicable};
  }
  if (token->getTokenIndex() == INVALID_INDEX)
    return unindexed(UnindexedReason::InsertedRecoveryToken);
  const auto stop = token->getStopIndex();
  if (start == INVALID_INDEX || stop == INVALID_INDEX)
    return unindexed(UnindexedReason::InvalidSdkIndex);
  if (stop == std::numeric_limits<std::size_t>::max())
    throw std::overflow_error("ANTLR inclusive token bound overflows");
  return interval(start, stop + 1, count);
}
NativeSpan rule_span(antlr4::ParserRuleContext *rule, std::size_t count) {
  if (rule->getStart() == nullptr || rule->getStop() == nullptr)
    return unindexed(UnindexedReason::MissingRuleBoundary);
  const auto start = token_span(rule->getStart(), count);
  const auto stop = token_span(rule->getStop(), count);
  if (start.kind == SpanKind::Unindexed) return start;
  if (stop.kind == SpanKind::Unindexed) return stop;
  if (start.start > stop.end) return unindexed(UnindexedReason::ReversedRuleBoundary);
  return {SpanKind::RegularHalfOpen, start.start, stop.end, UnindexedReason::NotApplicable};
}

class Diagnostics final : public antlr4::BaseErrorListener {
  NativeSyntax &result;
  OperationControl &control;
  antlr4::ANTLRInputStream &input;
  DiagnosticPhase phase;
  std::uint64_t grammar;
public:
  Diagnostics(NativeSyntax &result, OperationControl &control,
              antlr4::ANTLRInputStream &input, DiagnosticPhase phase, std::uint64_t grammar)
      : result(result), control(control), input(input), phase(phase), grammar(grammar) {}
  void syntaxError(antlr4::Recognizer *recognizer, antlr4::Token *token,
                   std::size_t line, std::size_t column, const std::string &message,
                   std::exception_ptr) override {
    poll(control);
    auto span = unindexed(UnindexedReason::MissingDiagnosticPosition);
    if (token != nullptr) {
      span = token_span(token, input.size());
    } else if (auto *lexer = dynamic_cast<antlr4::Lexer *>(recognizer)) {
      const auto start = lexer->tokenStartCharIndex;
      const auto cursor = input.index();
      // Lexer::notifyListeners reports tokenStartCharIndex..input.index()
      // inclusively. At EOF the cursor names a boundary, not another scalar.
      if (start != INVALID_INDEX && cursor != INVALID_INDEX)
        span = interval(start, cursor < input.size() ? cursor + 1 : cursor, input.size());
    }
    result.diagnostics.push_back({phase, grammar, width(line), width(column), rust::String(message), span});
  }
};

class QueryDiagnostics final : public antlr4::BaseErrorListener {
  OperationControl &control;
public:
  explicit QueryDiagnostics(OperationControl &control) : control(control) {}
  void syntaxError(antlr4::Recognizer *, antlr4::Token *, std::size_t,
                   std::size_t, const std::string &message, std::exception_ptr) override {
    poll(control);
    throw antlr4::IllegalArgumentException(message);
  }
};

template<class Lexer> class PollingLexer final : public Lexer {
  OperationControl &control;
public:
  PollingLexer(antlr4::CharStream *input, OperationControl &control) : Lexer(input), control(control) {}
  std::unique_ptr<antlr4::Token> nextToken() override {
    poll(control);
    auto token = Lexer::nextToken();
    poll(control);
    return token;
  }
};
template<class Parser> class PollingParser final : public Parser {
  OperationControl &control;
public:
  PollingParser(antlr4::TokenStream *stream, OperationControl &control)
      : Parser(stream), control(control) {}
  // SDK caches are static but their mutex belongs to each recognizer instance.
  // Resolve immutable SDK vocabulary locally for concurrent query operations.
  std::map<std::string, std::size_t> getRuleIndexMap() override {
    std::map<std::string, std::size_t> result;
    const auto &names = this->getRuleNames();
    if (names.empty())
      throw std::logic_error("ANTLR recognizer does not provide rule names");
    for (std::size_t index = 0; index < names.size(); ++index) {
      poll(control);
      result[names[index]] = index;
    }
    return result;
  }
  std::map<std::string_view, std::size_t> getTokenTypeMap() override {
    std::map<std::string_view, std::size_t> result;
    const auto &vocabulary = this->getVocabulary();
    for (std::size_t index = 0; index <= this->getATN().maxTokenType; ++index) {
      poll(control);
      const auto literal = vocabulary.getLiteralName(index);
      if (!literal.empty()) result[literal] = index;
      const auto symbolic = vocabulary.getSymbolicName(index);
      if (!symbolic.empty()) result[symbolic] = index;
    }
    result["EOF"] = antlr4::Token::EOF;
    return result;
  }
  antlr4::Token *consume() override {
    poll(control);
    auto *token = Parser::consume();
    poll(control);
    return token;
  }
  void enterRule(antlr4::ParserRuleContext *context, std::size_t state, std::size_t rule) override {
    poll(control);
    Parser::enterRule(context, state, rule);
  }
};
std::uint64_t token_kind(antlr4::Token *token, std::size_t max_type) {
  if (!token) throw std::logic_error("ANTLR tree terminal has no symbol");
  if (token->getType() == antlr4::Token::EOF) return width(max_type + 1);
  if (token->getType() > max_type) throw std::runtime_error("ANTLR token type has no vocabulary entry");
  return width(token->getType());
}

// Validate only native Db2 Unicode contexts during the existing tree walk.
void validate_db2_unicode(db2::Db2Parser::Unicode_string_constantContext &context,
                          NativeSyntax &result, OperationControl &control,
                          std::size_t scalar_count) {
  const auto checked_add = [](std::uint64_t left, std::uint64_t right) {
    if (right > std::numeric_limits<std::uint64_t>::max() - left)
      throw std::overflow_error("Db2 Unicode diagnostic offset overflows");
    return left + right;
  };
  const auto grammar_index = [&](const std::string &name) {
    for (std::size_t index = 0; index < result.grammar_names.size(); ++index)
      if (std::string(result.grammar_names[index]) == name) return width(index);
    const auto index = width(result.grammar_names.size());
    result.grammar_names.push_back(rust::String(name));
    return index;
  };
  const auto validate = [&](antlr4::tree::TerminalNode *terminal, bool argument,
                            char32_t escape) -> std::optional<char32_t> {
    if (!terminal || dynamic_cast<antlr4::tree::ErrorNode *>(terminal)) return std::nullopt;
    auto *token = terminal->getSymbol();
    const auto origin = token_span(token, scalar_count);
    if (origin.kind != SpanKind::RegularHalfOpen) return std::nullopt;
    poll(control);
    const auto text = token->getText();
    antlr4::ANTLRInputStream input(text.data(), text.size());
    poll(control);
    NativeSyntax local{};
    PollingLexer<db2::Db2UnicodeLexer> lexer(&input, control);
    lexer.setActiveEscape(escape);
    Diagnostics lexer_errors(local, control, input, DiagnosticPhase::Lexer,
                             grammar_index(lexer.getGrammarFileName()));
    lexer.removeErrorListeners();
    lexer.addErrorListener(&lexer_errors);
    antlr4::CommonTokenStream stream(&lexer);
    stream.fill();
    poll(control);
    PollingParser<db2::Db2UnicodeParser> parser(&stream, control);
    Diagnostics parser_errors(local, control, input, DiagnosticPhase::Parser,
                              grammar_index(parser.getGrammarFileName()));
    parser.removeErrorListeners();
    parser.addErrorListener(&parser_errors);
    std::optional<char32_t> selected;
    if (argument) {
      auto *tree = parser.escape_argument();
      if (local.diagnostics.empty()) {
        auto *scalar = tree->ARGUMENT_SCALAR();
        if (!scalar || dynamic_cast<antlr4::tree::ErrorNode *>(scalar))
          throw std::logic_error("Db2 Unicode argument has no actual scalar");
        const auto decoded = antlrcpp::Utf8::strictDecode(scalar->getText());
        if (!decoded || decoded->size() != 1)
          throw std::runtime_error("Db2 Unicode argument is not one UTF-8 scalar");
        selected = decoded->front();
      }
    } else {
      parser.unicode_literal();
      if (local.diagnostics.empty()) selected = escape;
    }
    poll(control);
    // Local EOF is a boundary inside the original token, never whole-source EOF.
    // Only these newly collected diagnostics are translated; main errors stay intact.
    for (auto &diagnostic : local.diagnostics) {
      poll(control);
      if (diagnostic.span.kind != SpanKind::Unindexed) {
        const auto begin = checked_add(origin.start, diagnostic.span.start);
        const auto end = checked_add(origin.start, diagnostic.span.end);
        if (begin > end || end > origin.end || end > scalar_count)
          throw std::runtime_error("Db2 Unicode diagnostic leaves its original token");
        diagnostic.span = interval(static_cast<std::size_t>(begin),
                                   static_cast<std::size_t>(end), scalar_count);
      }
      if (diagnostic.line == 0)
        throw std::runtime_error("Db2 Unicode diagnostic has no SDK line");
      if (diagnostic.line == 1)
        diagnostic.column = checked_add(width(token->getCharPositionInLine()), diagnostic.column);
      diagnostic.line = checked_add(width(token->getLine()), diagnostic.line - 1);
      result.diagnostics.push_back(std::move(diagnostic));
    }
    return selected;
  };
  char32_t escape = U'\\';
  if (auto *clause = context.unicode_escape_clause()) {
    const auto selected = validate(clause->STRING_LITERAL(), true, escape);
    // An invalid/recovered explicit clause must not fall back to the default.
    if (!selected) return;
    escape = *selected;
  }
  validate(context.UNICODE_STRING_LITERAL(), false, escape);
}

// At the DB2 batch boundary, the default epsilon transition can leave trailing
// malformed declaration tokens for the root's EOF recovery, losing later SQL.
// Resynchronize only here; the generated loop still owns SEMI and the next rule.
class Db2StatementRecovery final : public antlr4::DefaultErrorStrategy {
public:
  void sync(antlr4::Parser *parser) override {
    const auto *context = parser->getContext();
    const auto lookahead = parser->getTokenStream()->LA(1);
    if (context && context->getRuleIndex() == db2::Db2Parser::RuleBatch &&
        lookahead != db2::Db2Parser::SEMI && lookahead != antlr4::Token::EOF &&
        !parser->getExpectedTokens().contains(lookahead)) {
      reportUnwantedToken(parser);
      consumeUntil(parser, antlr4::misc::IntervalSet::of(db2::Db2Parser::SEMI));
    }
    antlr4::DefaultErrorStrategy::sync(parser);
  }
};

template<class Lexer, class Parser, class Entry>
NativeSyntax execute(rust::Str source, OperationControl &control, bool parse, rust::Slice<const rust::String> paths, Entry entry, const PreparedProfileQueries *prepared = nullptr) {
  poll(control);
  antlr4::ANTLRInputStream input(source.data(), source.size());
  poll(control);
  NativeSyntax result{};
  result.status = NativeStatus::Success;
  result.stream_scalar_count = width(input.size());
  PollingLexer<Lexer> lexer(&input, control);
  result.grammar_names.push_back(rust::String(lexer.getGrammarFileName()));
  Diagnostics lexer_errors(result, control, input, DiagnosticPhase::Lexer, 0);
  lexer.removeErrorListeners();
  lexer.addErrorListener(&lexer_errors);
  // Generated rule cleanup calls LT from a noexcept scope-exit destructor.
  // Buffer before parsing and poll normal parser operations instead of LT.
  antlr4::CommonTokenStream stream(&lexer);
  stream.fill();
  poll(control);
  const auto &vocabulary = lexer.getVocabulary();
  const auto max_type = vocabulary.getMaxTokenType();
  if (max_type == std::numeric_limits<std::size_t>::max())
    throw std::overflow_error("ANTLR vocabulary cannot represent an EOF entry");
  for (std::size_t index = 0; index <= max_type; ++index) {
    poll(control);
    auto name = vocabulary.getSymbolicName(index);
    if (name.empty()) name = vocabulary.getLiteralName(index);
    result.token_kind_names.push_back(rust::String(name.empty() ? std::to_string(index) : std::string(name)));
  }
  result.token_kind_names.push_back(rust::String("EOF"));
  for (auto *token : stream.getTokens()) {
    poll(control);
    result.tokens.push_back({token_kind(token, max_type), width(token->getChannel()),
      token_span(token, input.size()), token->getType() == antlr4::Token::EOF,
      rust::String(token->getText())});
  }
  if (!parse) return result;
  PollingParser<Parser> parser(&stream, control);
  result.grammar_names.push_back(rust::String(parser.getGrammarFileName()));
  Diagnostics parser_errors(result, control, input, DiagnosticPhase::Parser, 1);
  parser.removeErrorListeners();
  parser.addErrorListener(&parser_errors);
  if constexpr (std::is_same_v<Parser, db2::Db2Parser>)
    parser.setErrorHandler(std::make_shared<Db2StatementRecovery>());
  parser.template getInterpreter<antlr4::atn::ParserATNSimulator>()->setPredictionMode(antlr4::atn::PredictionMode::LL);
  poll(control);
  auto *tree = entry(parser);
  poll(control);
  for (const auto &name : parser.getRuleNames()) result.rule_names.push_back(rust::String(name));
  struct Pending { antlr4::tree::ParseTree *node; bool has_parent; std::uint64_t parent; };
  std::vector<Pending> pending{{tree, false, 0}};
  std::unordered_map<antlr4::ParserRuleContext *, std::uint64_t> rule_indexes;
  std::unordered_map<antlr4::tree::ParseTree *, std::size_t> node_indexes;
  while (!pending.empty()) {
    poll(control);
    const auto item = pending.back();
    pending.pop_back();
    if (!item.node) throw std::logic_error("ANTLR parse tree contains a null child");
    const auto index = width(result.nodes.size());
    NativeNode node{};
    node.has_parent = item.has_parent;
    node.parent = item.parent;
    if (auto *rule = dynamic_cast<antlr4::ParserRuleContext *>(item.node)) {
      node.kind = NodeKind::Rule;
      node.kind_id = width(rule->getRuleIndex());
      node.span = rule_span(rule, input.size());
      rule_indexes.emplace(rule, index);
      if constexpr (std::is_same_v<Parser, db2::Db2Parser>) {
        if (auto *unicode = dynamic_cast<db2::Db2Parser::Unicode_string_constantContext *>(rule))
          validate_db2_unicode(*unicode, result, control, input.size());
      }
    } else if (auto *terminal = dynamic_cast<antlr4::tree::TerminalNode *>(item.node)) {
      auto *token = terminal->getSymbol();
      node.kind = dynamic_cast<antlr4::tree::ErrorNode *>(item.node) ? NodeKind::Error : NodeKind::Terminal;
      node.kind_id = token_kind(token, max_type);
      node.span = token_span(token, input.size());
      node.missing = token->getTokenIndex() == INVALID_INDEX;
    } else {
      throw std::logic_error("ANTLR tree contains an unknown node kind");
    }
    if ((!paths.empty() || prepared != nullptr) && !node_indexes.emplace(item.node, result.nodes.size()).second)
      throw std::logic_error("ANTLR tree contains a repeated node pointer");
    result.nodes.push_back(node);
    for (auto child = item.node->children.rbegin(); child != item.node->children.rend(); ++child)
      pending.push_back({*child, true, index});
  }
  if constexpr (std::is_same_v<Parser, postgresql::PostgreSQLParser>) {
    for (const auto &body : parser.routine_bodies()) {
      poll(control);
      const auto parent = rule_indexes.find(body.host);
      if (parent == rule_indexes.end()) throw std::logic_error("ANTLR routine host is absent from the tree");
      auto span = unindexed(UnindexedReason::MissingTokenBounds);
      if (body.opening && body.closing) {
        const auto opening = token_span(body.opening, input.size());
        const auto closing = token_span(body.closing, input.size());
        if (opening.kind == SpanKind::Unindexed) span = opening;
        else if (closing.kind == SpanKind::Unindexed) span = closing;
        else if (opening.end > closing.start) span = unindexed(UnindexedReason::ReversedRuleBoundary);
        else span = {SpanKind::RegularHalfOpen, opening.end, closing.start, UnindexedReason::NotApplicable};
      }
      result.bodies.push_back({parent->second, span});
    }
  }
  // SDK XPath::evaluate uses a mutable static root. Compose its official
  // splitter/elements with an operation-local root instead; no query parser.
  antlr4::ParserRuleContext query_root;
  query_root.children = {tree};
  for (std::size_t query_index = 0; query_index < paths.size(); ++query_index) {
    if (!begin_query(control, query_index)) throw OperationStopped{};
    const auto &input_path = paths[query_index];
    const std::string path(input_path.data(), input_path.size());
    if (path.empty()) throw std::invalid_argument("empty ANTLR query path");
    if (path.find("**") != std::string::npos)
      throw std::invalid_argument("nearest-capture ** is not an ANTLR XPath operator");
    // The SDK splitter's default XPath lexer listener discards diagnostics.
    // Validate unchanged input with the SDK lexer and a strict observer first.
    antlr4::ANTLRInputStream path_input(path);
    PollingLexer<::XPathLexer> path_lexer(&path_input, control);
    QueryDiagnostics path_errors(control);
    path_lexer.removeErrorListeners();
    path_lexer.addErrorListener(&path_errors);
    antlr4::CommonTokenStream path_tokens(&path_lexer);
    path_tokens.fill();
    poll(control);
    antlr4::tree::xpath::XPath query(&parser, path);
    poll(control);
    auto elements = query.split(path);
    poll(control);
    std::vector<antlr4::tree::ParseTree *> work{&query_root};
    for (const auto &element : elements) {
      std::vector<antlr4::tree::ParseTree *> next;
      for (auto *node : work) {
        poll(control);
        if (!node->children.empty()) {
          auto selected = element->evaluate(node);
          poll(control);
          next.insert(next.end(), selected.begin(), selected.end());
        }
      }
      work = std::move(next);
    }
    std::vector<bool> selected(result.nodes.size(), false);
    for (auto *node : work) {
      poll(control);
      if (node == &query_root) continue;
      const auto found = node_indexes.find(node);
      if (found == node_indexes.end())
        throw std::logic_error("ANTLR XPath selected a foreign node");
      selected[found->second] = true;
    }
    for (std::size_t node = 0; node < selected.size(); ++node) {
      poll(control);
      if (selected[node]) result.query_matches.push_back({width(query_index), width(node), {}});
    }
  }
  if (prepared)
    select_prepared_profile_queries(parser, tree, node_indexes, *prepared, result, control, [&] { poll(control); });
  poll(control);
  return result;
}
NativeSyntax dispatch(NativeDialect dialect, rust::Str source, OperationControl &control, bool parse, rust::Slice<const rust::String> paths, const PreparedProfileQueries *prepared = nullptr) {
  switch (dialect) {
    case NativeDialect::PlSql:
      return execute<plsql::PlSqlLexer, plsql::PlSqlParser>(source, control, parse, paths, [](auto &parser) { return parser.sql_script(); }, prepared);
    case NativeDialect::PostgreSql:
      return execute<postgresql::PostgreSQLLexer, postgresql::PostgreSQLParser>(source, control, parse, paths, [](auto &parser) { return parser.root(); }, prepared);
    case NativeDialect::Db2:
      return execute<db2::Db2Lexer, db2::Db2Parser>(source, control, parse, paths, [](auto &parser) { return parser.db2_file(); }, prepared);
    case NativeDialect::TSql:
      return execute<tsql::TSqlLexer, tsql::TSqlParser>(source, control, parse, paths, [](auto &parser) { return parser.tsql_file(); }, prepared);
    case NativeDialect::Cobol85:
      return execute<cobol85::Cobol85Lexer, cobol85::Cobol85Parser>(source, control, parse, paths, [](auto &parser) { return parser.startRule(); }, prepared);
    default: throw std::invalid_argument("unknown ANTLR dialect");
  }
}
NativeSyntax controlled(NativeDialect dialect, rust::Str source, OperationControl &control, bool parse, rust::Slice<const rust::String> paths) {
  try { return dispatch(dialect, source, control, parse, paths); }
  catch (const OperationStopped &) {
    NativeSyntax result{};
    result.status = NativeStatus::Stopped;
    return result;
  }
}
} // namespace
template<class Parser>
std::unique_ptr<PreparedProfileQueries> prepare_for(rust::Slice<const NativeProfileQuery> queries,
    OperationControl &control) {
  antlr4::CommonTokenStream stream(nullptr);
  PollingParser<Parser> parser(&stream, control);
  return prepare_profile_queries_for_parser(parser, queries, control, [&] { poll(control); });
}
std::unique_ptr<PreparedProfileQueries> prepare_profile_queries(NativeDialect dialect, bool preprocessor,
    rust::Slice<const NativeProfileQuery> queries, OperationControl &control) {
  poll(control);
  if (preprocessor) {
    if (dialect != NativeDialect::Cobol85) throw std::invalid_argument("preprocessor requires COBOL grammar");
    return prepare_for<cobol85::Cobol85PreprocessorParser>(queries, control);
  }
  switch (dialect) {
    case NativeDialect::PlSql: return prepare_for<plsql::PlSqlParser>(queries, control);
    case NativeDialect::PostgreSql: return prepare_for<postgresql::PostgreSQLParser>(queries, control);
    case NativeDialect::Db2: return prepare_for<db2::Db2Parser>(queries, control);
    case NativeDialect::TSql: return prepare_for<tsql::TSqlParser>(queries, control);
    case NativeDialect::Cobol85: return prepare_for<cobol85::Cobol85Parser>(queries, control);
    default: throw std::invalid_argument("unknown ANTLR dialect");
  }
}
NativeSyntax recognize_prepared_profile_queries(NativeDialect dialect, bool preprocessor,
    rust::Str source, const PreparedProfileQueries &queries, OperationControl &control) {
  try {
    if (preprocessor) {
      if (dialect != NativeDialect::Cobol85) throw std::invalid_argument("preprocessor requires COBOL grammar");
      return execute<cobol85::Cobol85PreprocessorLexer, cobol85::Cobol85PreprocessorParser>(
          source, control, true, {}, [](auto &parser) { return parser.startRule(); }, &queries);
    }
    return dispatch(dialect, source, control, true, {}, &queries);
  } catch (const OperationStopped &) {
    NativeSyntax result{};
    result.status = NativeStatus::Stopped;
    return result;
  }
}
NativeSyntax recognize(NativeDialect dialect, rust::Str source, OperationControl &control) {
  return controlled(dialect, source, control, true, {});
}
NativeSyntax recognize_queries(NativeDialect dialect, rust::Str source, rust::Slice<const rust::String> paths, OperationControl &control) {
  return controlled(dialect, source, control, true, paths);
}
NativeSyntax lex(NativeDialect dialect, rust::Str source, OperationControl &control) {
  return controlled(dialect, source, control, false, {});
}
} // namespace graphit::antlr
