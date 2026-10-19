
// Generated from crates/graphit-ast/src/antlr_engine/grammars/db2/Db2UnicodeParser.g4 by ANTLR 4.13.2

#pragma once


#include "antlr4-runtime.h"


namespace graphit::antlr::db2 {


class  Db2UnicodeParser : public antlr4::Parser {
public:
  enum {
    UNICODE_OPEN = 1, ARGUMENT_OPEN = 2, ESCAPE_FOUR = 3, ESCAPE_SIX = 4, 
    ESCAPE_PAIR = 5, APOSTROPHE_PAIR = 6, GLYPH = 7, BODY_CLOSE = 8, INVALID_BODY = 9, 
    ARGUMENT_SCALAR = 10, ARGUMENT_CLOSE = 11, INVALID_ARGUMENT = 12
  };

  enum {
    RuleEscape_argument = 0, RuleUnicode_literal = 1
  };

  explicit Db2UnicodeParser(antlr4::TokenStream *input);

  Db2UnicodeParser(antlr4::TokenStream *input, const antlr4::atn::ParserATNSimulatorOptions &options);

  ~Db2UnicodeParser() override;

  std::string getGrammarFileName() const override;

  const antlr4::atn::ATN& getATN() const override;

  const std::vector<std::string>& getRuleNames() const override;

  const antlr4::dfa::Vocabulary& getVocabulary() const override;

  antlr4::atn::SerializedATNView getSerializedATN() const override;


  class Escape_argumentContext;
  class Unicode_literalContext; 

  class  Escape_argumentContext : public antlr4::ParserRuleContext {
  public:
    Escape_argumentContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ARGUMENT_OPEN();
    antlr4::tree::TerminalNode *ARGUMENT_SCALAR();
    antlr4::tree::TerminalNode *ARGUMENT_CLOSE();
    antlr4::tree::TerminalNode *EOF();

   
  };

  Escape_argumentContext* escape_argument();

  class  Unicode_literalContext : public antlr4::ParserRuleContext {
  public:
    Unicode_literalContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *UNICODE_OPEN();
    antlr4::tree::TerminalNode *BODY_CLOSE();
    antlr4::tree::TerminalNode *EOF();
    std::vector<antlr4::tree::TerminalNode *> ESCAPE_FOUR();
    antlr4::tree::TerminalNode* ESCAPE_FOUR(size_t i);
    std::vector<antlr4::tree::TerminalNode *> ESCAPE_SIX();
    antlr4::tree::TerminalNode* ESCAPE_SIX(size_t i);
    std::vector<antlr4::tree::TerminalNode *> ESCAPE_PAIR();
    antlr4::tree::TerminalNode* ESCAPE_PAIR(size_t i);
    std::vector<antlr4::tree::TerminalNode *> APOSTROPHE_PAIR();
    antlr4::tree::TerminalNode* APOSTROPHE_PAIR(size_t i);
    std::vector<antlr4::tree::TerminalNode *> GLYPH();
    antlr4::tree::TerminalNode* GLYPH(size_t i);

   
  };

  Unicode_literalContext* unicode_literal();


  // By default the static state used to implement the parser is lazily initialized during the first
  // call to the constructor. You can call this function if you wish to initialize the static state
  // ahead of time.
  static void initialize();

private:
};

}  // namespace graphit::antlr::db2
