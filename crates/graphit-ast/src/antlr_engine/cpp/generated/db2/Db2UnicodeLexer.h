
// Generated from crates/graphit-ast/src/antlr_engine/grammars/db2/Db2UnicodeLexer.g4 by ANTLR 4.13.2

#pragma once


#include "antlr4-runtime.h"


namespace graphit::antlr::db2 {


class  Db2UnicodeLexer : public antlr4::Lexer {
public:
  enum {
    UNICODE_OPEN = 1, ARGUMENT_OPEN = 2, ESCAPE_FOUR = 3, ESCAPE_SIX = 4, 
    ESCAPE_PAIR = 5, APOSTROPHE_PAIR = 6, GLYPH = 7, BODY_CLOSE = 8, INVALID_BODY = 9, 
    ARGUMENT_SCALAR = 10, ARGUMENT_CLOSE = 11, INVALID_ARGUMENT = 12
  };

  enum {
    BODY = 1, ARGUMENT = 2
  };

  explicit Db2UnicodeLexer(antlr4::CharStream *input);

  ~Db2UnicodeLexer() override;


    char32_t activeEscape = U'\\';
    void setActiveEscape(char32_t value) { activeEscape = value; }


  std::string getGrammarFileName() const override;

  const std::vector<std::string>& getRuleNames() const override;

  const std::vector<std::string>& getChannelNames() const override;

  const std::vector<std::string>& getModeNames() const override;

  const antlr4::dfa::Vocabulary& getVocabulary() const override;

  antlr4::atn::SerializedATNView getSerializedATN() const override;

  const antlr4::atn::ATN& getATN() const override;

  bool sempred(antlr4::RuleContext *_localctx, size_t ruleIndex, size_t predicateIndex) override;

  // By default the static state used to implement the lexer is lazily initialized during the first
  // call to the constructor. You can call this function if you wish to initialize the static state
  // ahead of time.
  static void initialize();

private:

  // Individual action functions triggered by action() above.

  // Individual semantic predicate functions triggered by sempred() above.
  bool ESCAPE_FOURSempred(antlr4::RuleContext *_localctx, size_t predicateIndex);
  bool ESCAPE_SIXSempred(antlr4::RuleContext *_localctx, size_t predicateIndex);
  bool ESCAPE_PAIRSempred(antlr4::RuleContext *_localctx, size_t predicateIndex);
  bool GLYPHSempred(antlr4::RuleContext *_localctx, size_t predicateIndex);

};

}  // namespace graphit::antlr::db2
