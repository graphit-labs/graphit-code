#include "graphit-ast/src/antlr_engine/bridge.rs.h"
#include "antlr_engine/cpp/profile_queries.hpp"
#include "tree/xpath/XPath.h"
#include "tree/xpath/XPathElement.h"
#include "tree/xpath/XPathLexer.h"
#include <algorithm>
#include <deque>
#include <optional>
#include <unordered_set>

namespace graphit::antlr {
namespace {
using Tree = antlr4::tree::ParseTree;
using Poll = std::function<void()>;
using Element = antlr4::tree::xpath::XPathElement;
struct XPathStep {
  std::unique_ptr<Element> selector;
  bool descendant;
  std::optional<std::size_t> rule;
  std::optional<std::size_t> token;
};
using Path = std::vector<XPathStep>;
class StrictErrors final : public antlr4::BaseErrorListener {
public:
  void syntaxError(antlr4::Recognizer *, antlr4::Token *, std::size_t line,
      std::size_t column, const std::string &message, std::exception_ptr) override {
    throw antlr4::IllegalArgumentException("XPath " + std::to_string(line) + ":" +
        std::to_string(column) + ": " + message);
  }
};
class XPathCompiler final : public antlr4::tree::xpath::XPath {
  struct Bound { bool descendant; std::optional<std::size_t> rule; std::optional<std::size_t> token; };
  std::vector<Bound> axes;
  std::unique_ptr<Element> getXPathElement(antlr4::Token *token, bool anywhere) override {
    Bound bound{anywhere, {}, {}};
    const auto name = token->getText();
    if (token->getType() == ::XPathLexer::RULE_REF) bound.rule = _parser->getRuleIndex(name);
    if (token->getType() == ::XPathLexer::TOKEN_REF || token->getType() == ::XPathLexer::STRING)
      bound.token = _parser->getTokenType(name);
    axes.push_back(bound);
    // Select direct children with the official element. Descendant traversal is
    // iterative below, so every visited node can check the operation deadline.
    return XPath::getXPathElement(token, false);
  }
public:
  explicit XPathCompiler(antlr4::Parser &parser) : XPath(&parser, "") {}
  Path compile(const rust::String &value, const Poll &poll) {
    poll();
    const std::string text(value.data(), value.size());
    if (text.empty() || text.front() != '/' || text.find("**") != std::string::npos ||
        text.find('\0') != std::string::npos || text.find("\xEF\xBB\xBF") != std::string::npos)
      throw antlr4::IllegalArgumentException("expected a nonempty native XPath beginning with / or //");
    // XPath::split's default error listener discards lexer diagnostics. Validate
    // unchanged text first with the official lexer and a throwing observer.
    antlr4::ANTLRInputStream input(text);
    ::XPathLexer lexer(&input);
    StrictErrors errors;
    lexer.removeErrorListeners();
    lexer.addErrorListener(&errors);
    for (;;) {
      poll();
      auto token = lexer.nextToken();
      if (token->getType() == antlr4::Token::EOF) break;
    }
    axes.clear();
    auto elements = split(text);
    poll();
    if (elements.empty()) throw antlr4::IllegalArgumentException("empty XPath");
    Path result;
    for (std::size_t i = 0; i < elements.size(); ++i)
      result.push_back({std::move(elements[i]), axes.at(i).descendant,
          axes.at(i).rule, axes.at(i).token});
    return result;
  }
};
std::size_t rule_id(antlr4::Parser &parser, const rust::String &name) {
  const auto id = parser.getRuleIndex(std::string(name.data(), name.size()));
  if (id == INVALID_INDEX) throw antlr4::IllegalArgumentException("unknown profile rule: " + std::string(name));
  return id;
}
bool rule_is(Tree *node, std::size_t id) {
  auto *rule = dynamic_cast<antlr4::ParserRuleContext *>(node);
  return rule && rule->getRuleIndex() == id;
}
struct CaptureStep {
  Path xpath;
  std::optional<std::size_t> descendant;
  std::optional<std::size_t> ancestor;
};
using Capture = std::vector<CaptureStep>;
struct Query {
  Path pattern;
  Path select;
  std::string keyword;
  std::optional<std::size_t> child_rule;
  bool negate;
  bool indexed_start;
  std::vector<Capture> captures;
};
bool ascii_equal(const std::string &left, const std::string &right) {
  if (left.size() != right.size()) return false;
  const auto fold = [](unsigned char c) { return c >= 'a' && c <= 'z' ? c - 'a' + 'A' : c; };
  for (std::size_t i = 0; i < left.size(); ++i)
    if (fold(left[i]) != fold(right[i])) return false;
  return true;
}
bool guard(Tree *node, const Query &query, const Poll &poll) {
  if (query.keyword.empty() && !query.child_rule) return true;
  if (!dynamic_cast<antlr4::ParserRuleContext *>(node))
    throw antlr4::IllegalArgumentException("profile guard requires a rule selector");
  bool found = false;
  for (auto *child : node->children) {
    poll();
    if (query.child_rule) found = rule_is(child, *query.child_rule);
    else if (auto *terminal = dynamic_cast<antlr4::tree::TerminalNode *>(child))
      found = ascii_equal(terminal->getText(), query.keyword);
    if (found) break;
  }
  return found != query.negate;
}
std::vector<Tree *> evaluate(std::vector<Tree *> work, const Path &path, const Poll &poll,
    std::size_t skip = 0) {
  for (std::size_t index = skip; index < path.size(); ++index) {
    const auto &step = path[index];
    std::vector<Tree *> next;
    std::unordered_set<Tree *> visited;
    std::unordered_set<Tree *> traversed;
    for (auto *node : work) {
      poll();
      std::vector<Tree *> pending{node};
      while (!pending.empty()) {
        poll();
        auto *parent = pending.back();
        pending.pop_back();
        if (step.descendant && !traversed.insert(parent).second) continue;
        for (auto *selected : step.selector->evaluate(parent)) {
          poll();
          if (visited.insert(selected).second) next.push_back(selected);
        }
        if (step.descendant)
          pending.insert(pending.end(), parent->children.rbegin(), parent->children.rend());
      }
    }
    work = std::move(next);
  }
  return work;
}
Tree *capture(Tree *node, const Capture &steps, const Poll &poll) {
  for (const auto &step : steps) {
    poll();
    Tree *found = nullptr;
    if (step.ancestor) {
      for (auto *parent = node->parent; parent; parent = parent->parent) {
        poll();
        if (rule_is(parent, *step.ancestor)) { found = parent; break; }
      }
    } else if (step.descendant) {
      std::deque<Tree *> pending(node->children.begin(), node->children.end());
      while (!pending.empty()) {
        poll();
        auto *child = pending.front();
        pending.pop_front();
        if (rule_is(child, *step.descendant)) { found = child; break; }
        pending.insert(pending.end(), child->children.begin(), child->children.end());
      }
    } else {
      // A capture's XPath is relative to its selected node, not a dummy root.
      auto selected = evaluate({node}, step.xpath, poll);
      if (!selected.empty()) found = selected.front();
    }
    if (!found) return nullptr;
    node = found;
  }
  return node;
}
} // namespace
struct PreparedProfileQueries::Impl {
  std::string grammar;
  std::vector<Query> queries;
};
std::unique_ptr<PreparedProfileQueries> prepare_profile_queries_for_parser(antlr4::Parser &parser,
    rust::Slice<const NativeProfileQuery> queries, OperationControl &control, const Poll &poll) {
  auto result = std::make_shared<PreparedProfileQueries::Impl>();
  result->grammar = parser.getGrammarFileName();
  XPathCompiler compiler(parser);
  for (std::size_t index = 0; index < queries.size(); ++index) {
    if (!begin_query(control, index)) poll();
    const auto &input = queries[index];
    Query query{compiler.compile(input.pattern, poll), {}, std::string(input.keyword), {}, input.negate,
        std::string_view(input.pattern.data(), input.pattern.size()).find('!') == std::string_view::npos, {}};
    if (!input.select.empty()) query.select = compiler.compile(input.select, poll);
    if (!input.child_rule.empty()) query.child_rule = rule_id(parser, input.child_rule);
    if ((!input.keyword.empty() || query.child_rule) && !query.pattern.back().rule)
      throw antlr4::IllegalArgumentException("profile guard requires a rule selector");
    if ((!input.keyword.empty() && query.child_rule) ||
        (input.negate && input.keyword.empty() && !query.child_rule))
      throw antlr4::IllegalArgumentException("expected one direct-child guard before negate");
    for (std::size_t slot = 0; slot < input.captures.size(); ++slot) {
      Capture steps;
      try {
        for (const auto &step : input.captures[slot].steps) {
          poll();
          const auto count = !step.xpath.empty() + !step.nearest_descendant.empty() + !step.nearest_ancestor.empty();
          if (count != 1) throw antlr4::IllegalArgumentException("capture step requires exactly one operation");
          CaptureStep compiled{};
          if (!step.xpath.empty()) compiled.xpath = compiler.compile(step.xpath, poll);
          else if (!step.nearest_descendant.empty()) compiled.descendant = rule_id(parser, step.nearest_descendant);
          else compiled.ancestor = rule_id(parser, step.nearest_ancestor);
          steps.push_back(std::move(compiled));
        }
      } catch (const antlr4::IllegalArgumentException &error) {
        throw antlr4::IllegalArgumentException("capture " + std::to_string(slot) + ": " + error.what());
      }
      query.captures.push_back(std::move(steps));
    }
    result->queries.push_back(std::move(query));
  }
  return std::make_unique<PreparedProfileQueries>(std::move(result));
}
void select_prepared_profile_queries(antlr4::Parser &parser, Tree *root,
    const std::unordered_map<Tree *, std::size_t> &indexes,
    const PreparedProfileQueries &prepared, NativeSyntax &result,
    OperationControl &control, const Poll &poll) {
  if (prepared.impl->grammar != parser.getGrammarFileName())
    throw antlr4::IllegalArgumentException("prepared profile belongs to another grammar");
  antlr4::ParserRuleContext dummy;
  dummy.children = {root};
  // Index each native node once for all profile queries. The common //rule and
  // //TOKEN first steps use this index instead of rescanning the entire file.
  std::unordered_map<std::size_t, std::vector<Tree *>> rules, tokens;
  for (const auto &[node, ordinal] : indexes) {
    poll();
    if (auto *rule = dynamic_cast<antlr4::ParserRuleContext *>(node))
      rules[rule->getRuleIndex()].push_back(node);
    else if (auto *terminal = dynamic_cast<antlr4::tree::TerminalNode *>(node))
      tokens[terminal->getSymbol()->getType()].push_back(node);
  }
  for (auto *groups : {&rules, &tokens}) {
    for (auto &[id, nodes] : *groups) {
      poll();
      std::sort(nodes.begin(), nodes.end(), [&](auto *a, auto *b) { return indexes.at(a) < indexes.at(b); });
    }
  }
  for (std::size_t query_index = 0; query_index < prepared.impl->queries.size(); ++query_index) {
    if (!begin_query(control, query_index)) poll();
    const auto &query = prepared.impl->queries[query_index];
    const auto &first = query.pattern.front();
    std::vector<Tree *> selected;
    // The official element retains ! inversion; inverted/wildcard paths keep
    // normal XPath evaluation instead of pretending an index is equivalent.
    if (first.descendant && query.indexed_start && (first.rule || first.token)) {
      auto &groups = first.rule ? rules : tokens;
      const auto found = groups.find(first.rule ? *first.rule : *first.token);
      if (found != groups.end()) selected = found->second;
      selected = evaluate(std::move(selected), query.pattern, poll, 1);
    } else selected = evaluate({&dummy}, query.pattern, poll);
    selected.erase(std::remove_if(selected.begin(), selected.end(), [&](auto *node) {
      return !guard(node, query, poll);
    }), selected.end());
    if (!query.select.empty()) selected = evaluate(std::move(selected), query.select, poll);
    // Ordinals are parse-tree preorder; sort just matches, never an array sized
    // to the whole tree for each query. Overlapping XPath roots deduplicate here.
    std::sort(selected.begin(), selected.end(), [&](auto *left, auto *right) {
      return indexes.at(left) < indexes.at(right);
    });
    selected.erase(std::unique(selected.begin(), selected.end()), selected.end());
    for (auto *node : selected) {
      poll();
      NativeQueryMatch item{query_index, indexes.at(node), {}};
      for (const auto &path : query.captures) {
        auto *target = capture(node, path, poll);
        item.captures.push_back({target != nullptr, target ? static_cast<std::uint64_t>(indexes.at(target)) : 0});
      }
      result.query_matches.push_back(std::move(item));
    }
  }
}
} // namespace graphit::antlr
