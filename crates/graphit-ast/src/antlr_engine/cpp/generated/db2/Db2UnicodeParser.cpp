
// Generated from crates/graphit-ast/src/antlr_engine/grammars/db2/Db2UnicodeParser.g4 by ANTLR 4.13.2



#include "Db2UnicodeParser.h"


using namespace antlrcpp;
using namespace graphit::antlr::db2;

using namespace antlr4;

namespace {

struct Db2UnicodeParserStaticData final {
  Db2UnicodeParserStaticData(std::vector<std::string> ruleNames,
                        std::vector<std::string> literalNames,
                        std::vector<std::string> symbolicNames)
      : ruleNames(std::move(ruleNames)), literalNames(std::move(literalNames)),
        symbolicNames(std::move(symbolicNames)),
        vocabulary(this->literalNames, this->symbolicNames) {}

  Db2UnicodeParserStaticData(const Db2UnicodeParserStaticData&) = delete;
  Db2UnicodeParserStaticData(Db2UnicodeParserStaticData&&) = delete;
  Db2UnicodeParserStaticData& operator=(const Db2UnicodeParserStaticData&) = delete;
  Db2UnicodeParserStaticData& operator=(Db2UnicodeParserStaticData&&) = delete;

  std::vector<antlr4::dfa::DFA> decisionToDFA;
  antlr4::atn::PredictionContextCache sharedContextCache;
  const std::vector<std::string> ruleNames;
  const std::vector<std::string> literalNames;
  const std::vector<std::string> symbolicNames;
  const antlr4::dfa::Vocabulary vocabulary;
  antlr4::atn::SerializedATNView serializedATN;
  std::unique_ptr<antlr4::atn::ATN> atn;
};

::antlr4::internal::OnceFlag db2unicodeparserParserOnceFlag;
#if ANTLR4_USE_THREAD_LOCAL_CACHE
static thread_local
#endif
std::unique_ptr<Db2UnicodeParserStaticData> db2unicodeparserParserStaticData = nullptr;

void db2unicodeparserParserInitialize() {
#if ANTLR4_USE_THREAD_LOCAL_CACHE
  if (db2unicodeparserParserStaticData != nullptr) {
    return;
  }
#else
  assert(db2unicodeparserParserStaticData == nullptr);
#endif
  auto staticData = std::make_unique<Db2UnicodeParserStaticData>(
    std::vector<std::string>{
      "escape_argument", "unicode_literal"
    },
    std::vector<std::string>{
      "", "", "", "", "", "", "''''"
    },
    std::vector<std::string>{
      "", "UNICODE_OPEN", "ARGUMENT_OPEN", "ESCAPE_FOUR", "ESCAPE_SIX", 
      "ESCAPE_PAIR", "APOSTROPHE_PAIR", "GLYPH", "BODY_CLOSE", "INVALID_BODY", 
      "ARGUMENT_SCALAR", "ARGUMENT_CLOSE", "INVALID_ARGUMENT"
    }
  );
  static const int32_t serializedATNSegment[] = {
  	4,1,12,20,2,0,7,0,2,1,7,1,1,0,1,0,1,0,1,0,1,0,1,1,1,1,5,1,12,8,1,10,1,
  	12,1,15,9,1,1,1,1,1,1,1,1,1,0,0,2,0,2,0,1,1,0,3,7,18,0,4,1,0,0,0,2,9,
  	1,0,0,0,4,5,5,2,0,0,5,6,5,10,0,0,6,7,5,11,0,0,7,8,5,0,0,1,8,1,1,0,0,0,
  	9,13,5,1,0,0,10,12,7,0,0,0,11,10,1,0,0,0,12,15,1,0,0,0,13,11,1,0,0,0,
  	13,14,1,0,0,0,14,16,1,0,0,0,15,13,1,0,0,0,16,17,5,8,0,0,17,18,5,0,0,1,
  	18,3,1,0,0,0,1,13
  };
  staticData->serializedATN = antlr4::atn::SerializedATNView(serializedATNSegment, sizeof(serializedATNSegment) / sizeof(serializedATNSegment[0]));

  antlr4::atn::ATNDeserializer deserializer;
  staticData->atn = deserializer.deserialize(staticData->serializedATN);

  const size_t count = staticData->atn->getNumberOfDecisions();
  staticData->decisionToDFA.reserve(count);
  for (size_t i = 0; i < count; i++) { 
    staticData->decisionToDFA.emplace_back(staticData->atn->getDecisionState(i), i);
  }
  db2unicodeparserParserStaticData = std::move(staticData);
}

}

Db2UnicodeParser::Db2UnicodeParser(TokenStream *input) : Db2UnicodeParser(input, antlr4::atn::ParserATNSimulatorOptions()) {}

Db2UnicodeParser::Db2UnicodeParser(TokenStream *input, const antlr4::atn::ParserATNSimulatorOptions &options) : Parser(input) {
  Db2UnicodeParser::initialize();
  _interpreter = new atn::ParserATNSimulator(this, *db2unicodeparserParserStaticData->atn, db2unicodeparserParserStaticData->decisionToDFA, db2unicodeparserParserStaticData->sharedContextCache, options);
}

Db2UnicodeParser::~Db2UnicodeParser() {
  delete _interpreter;
}

const atn::ATN& Db2UnicodeParser::getATN() const {
  return *db2unicodeparserParserStaticData->atn;
}

std::string Db2UnicodeParser::getGrammarFileName() const {
  return "Db2UnicodeParser.g4";
}

const std::vector<std::string>& Db2UnicodeParser::getRuleNames() const {
  return db2unicodeparserParserStaticData->ruleNames;
}

const dfa::Vocabulary& Db2UnicodeParser::getVocabulary() const {
  return db2unicodeparserParserStaticData->vocabulary;
}

antlr4::atn::SerializedATNView Db2UnicodeParser::getSerializedATN() const {
  return db2unicodeparserParserStaticData->serializedATN;
}


//----------------- Escape_argumentContext ------------------------------------------------------------------

Db2UnicodeParser::Escape_argumentContext::Escape_argumentContext(ParserRuleContext *parent, size_t invokingState)
  : ParserRuleContext(parent, invokingState) {
}

tree::TerminalNode* Db2UnicodeParser::Escape_argumentContext::ARGUMENT_OPEN() {
  return getToken(Db2UnicodeParser::ARGUMENT_OPEN, 0);
}

tree::TerminalNode* Db2UnicodeParser::Escape_argumentContext::ARGUMENT_SCALAR() {
  return getToken(Db2UnicodeParser::ARGUMENT_SCALAR, 0);
}

tree::TerminalNode* Db2UnicodeParser::Escape_argumentContext::ARGUMENT_CLOSE() {
  return getToken(Db2UnicodeParser::ARGUMENT_CLOSE, 0);
}

tree::TerminalNode* Db2UnicodeParser::Escape_argumentContext::EOF() {
  return getToken(Db2UnicodeParser::EOF, 0);
}


size_t Db2UnicodeParser::Escape_argumentContext::getRuleIndex() const {
  return Db2UnicodeParser::RuleEscape_argument;
}


Db2UnicodeParser::Escape_argumentContext* Db2UnicodeParser::escape_argument() {
  Escape_argumentContext *_localctx = _tracker.createInstance<Escape_argumentContext>(_ctx, getState());
  enterRule(_localctx, 0, Db2UnicodeParser::RuleEscape_argument);

#if __cplusplus > 201703L
  auto onExit = finally([=, this] {
#else
  auto onExit = finally([=] {
#endif
    exitRule();
  });
  try {
    enterOuterAlt(_localctx, 1);
    setState(4);
    match(Db2UnicodeParser::ARGUMENT_OPEN);
    setState(5);
    match(Db2UnicodeParser::ARGUMENT_SCALAR);
    setState(6);
    match(Db2UnicodeParser::ARGUMENT_CLOSE);
    setState(7);
    match(Db2UnicodeParser::EOF);
   
  }
  catch (RecognitionException &e) {
    _errHandler->reportError(this, e);
    _localctx->exception = std::current_exception();
    _errHandler->recover(this, _localctx->exception);
  }

  return _localctx;
}

//----------------- Unicode_literalContext ------------------------------------------------------------------

Db2UnicodeParser::Unicode_literalContext::Unicode_literalContext(ParserRuleContext *parent, size_t invokingState)
  : ParserRuleContext(parent, invokingState) {
}

tree::TerminalNode* Db2UnicodeParser::Unicode_literalContext::UNICODE_OPEN() {
  return getToken(Db2UnicodeParser::UNICODE_OPEN, 0);
}

tree::TerminalNode* Db2UnicodeParser::Unicode_literalContext::BODY_CLOSE() {
  return getToken(Db2UnicodeParser::BODY_CLOSE, 0);
}

tree::TerminalNode* Db2UnicodeParser::Unicode_literalContext::EOF() {
  return getToken(Db2UnicodeParser::EOF, 0);
}

std::vector<tree::TerminalNode *> Db2UnicodeParser::Unicode_literalContext::ESCAPE_FOUR() {
  return getTokens(Db2UnicodeParser::ESCAPE_FOUR);
}

tree::TerminalNode* Db2UnicodeParser::Unicode_literalContext::ESCAPE_FOUR(size_t i) {
  return getToken(Db2UnicodeParser::ESCAPE_FOUR, i);
}

std::vector<tree::TerminalNode *> Db2UnicodeParser::Unicode_literalContext::ESCAPE_SIX() {
  return getTokens(Db2UnicodeParser::ESCAPE_SIX);
}

tree::TerminalNode* Db2UnicodeParser::Unicode_literalContext::ESCAPE_SIX(size_t i) {
  return getToken(Db2UnicodeParser::ESCAPE_SIX, i);
}

std::vector<tree::TerminalNode *> Db2UnicodeParser::Unicode_literalContext::ESCAPE_PAIR() {
  return getTokens(Db2UnicodeParser::ESCAPE_PAIR);
}

tree::TerminalNode* Db2UnicodeParser::Unicode_literalContext::ESCAPE_PAIR(size_t i) {
  return getToken(Db2UnicodeParser::ESCAPE_PAIR, i);
}

std::vector<tree::TerminalNode *> Db2UnicodeParser::Unicode_literalContext::APOSTROPHE_PAIR() {
  return getTokens(Db2UnicodeParser::APOSTROPHE_PAIR);
}

tree::TerminalNode* Db2UnicodeParser::Unicode_literalContext::APOSTROPHE_PAIR(size_t i) {
  return getToken(Db2UnicodeParser::APOSTROPHE_PAIR, i);
}

std::vector<tree::TerminalNode *> Db2UnicodeParser::Unicode_literalContext::GLYPH() {
  return getTokens(Db2UnicodeParser::GLYPH);
}

tree::TerminalNode* Db2UnicodeParser::Unicode_literalContext::GLYPH(size_t i) {
  return getToken(Db2UnicodeParser::GLYPH, i);
}


size_t Db2UnicodeParser::Unicode_literalContext::getRuleIndex() const {
  return Db2UnicodeParser::RuleUnicode_literal;
}


Db2UnicodeParser::Unicode_literalContext* Db2UnicodeParser::unicode_literal() {
  Unicode_literalContext *_localctx = _tracker.createInstance<Unicode_literalContext>(_ctx, getState());
  enterRule(_localctx, 2, Db2UnicodeParser::RuleUnicode_literal);
  size_t _la = 0;

#if __cplusplus > 201703L
  auto onExit = finally([=, this] {
#else
  auto onExit = finally([=] {
#endif
    exitRule();
  });
  try {
    enterOuterAlt(_localctx, 1);
    setState(9);
    match(Db2UnicodeParser::UNICODE_OPEN);
    setState(13);
    _errHandler->sync(this);
    _la = _input->LA(1);
    while ((((_la & ~ 0x3fULL) == 0) &&
      ((1ULL << _la) & 248) != 0)) {
      setState(10);
      _la = _input->LA(1);
      if (!((((_la & ~ 0x3fULL) == 0) &&
        ((1ULL << _la) & 248) != 0))) {
      _errHandler->recoverInline(this);
      }
      else {
        _errHandler->reportMatch(this);
        consume();
      }
      setState(15);
      _errHandler->sync(this);
      _la = _input->LA(1);
    }
    setState(16);
    match(Db2UnicodeParser::BODY_CLOSE);
    setState(17);
    match(Db2UnicodeParser::EOF);
   
  }
  catch (RecognitionException &e) {
    _errHandler->reportError(this, e);
    _localctx->exception = std::current_exception();
    _errHandler->recover(this, _localctx->exception);
  }

  return _localctx;
}

void Db2UnicodeParser::initialize() {
#if ANTLR4_USE_THREAD_LOCAL_CACHE
  db2unicodeparserParserInitialize();
#else
  ::antlr4::internal::call_once(db2unicodeparserParserOnceFlag, db2unicodeparserParserInitialize);
#endif
}
