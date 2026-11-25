
// Generated from crates/graphit-ast/src/antlr_engine/grammars/db2/Db2UnicodeLexer.g4 by ANTLR 4.13.2


#include "Db2UnicodeLexer.h"


using namespace antlr4;

using namespace graphit::antlr::db2;


using namespace antlr4;

namespace {

struct Db2UnicodeLexerStaticData final {
  Db2UnicodeLexerStaticData(std::vector<std::string> ruleNames,
                          std::vector<std::string> channelNames,
                          std::vector<std::string> modeNames,
                          std::vector<std::string> literalNames,
                          std::vector<std::string> symbolicNames)
      : ruleNames(std::move(ruleNames)), channelNames(std::move(channelNames)),
        modeNames(std::move(modeNames)), literalNames(std::move(literalNames)),
        symbolicNames(std::move(symbolicNames)),
        vocabulary(this->literalNames, this->symbolicNames) {}

  Db2UnicodeLexerStaticData(const Db2UnicodeLexerStaticData&) = delete;
  Db2UnicodeLexerStaticData(Db2UnicodeLexerStaticData&&) = delete;
  Db2UnicodeLexerStaticData& operator=(const Db2UnicodeLexerStaticData&) = delete;
  Db2UnicodeLexerStaticData& operator=(Db2UnicodeLexerStaticData&&) = delete;

  std::vector<antlr4::dfa::DFA> decisionToDFA;
  antlr4::atn::PredictionContextCache sharedContextCache;
  const std::vector<std::string> ruleNames;
  const std::vector<std::string> channelNames;
  const std::vector<std::string> modeNames;
  const std::vector<std::string> literalNames;
  const std::vector<std::string> symbolicNames;
  const antlr4::dfa::Vocabulary vocabulary;
  antlr4::atn::SerializedATNView serializedATN;
  std::unique_ptr<antlr4::atn::ATN> atn;
};

::antlr4::internal::OnceFlag db2unicodelexerLexerOnceFlag;
#if ANTLR4_USE_THREAD_LOCAL_CACHE
static thread_local
#endif
std::unique_ptr<Db2UnicodeLexerStaticData> db2unicodelexerLexerStaticData = nullptr;

void db2unicodelexerLexerInitialize() {
#if ANTLR4_USE_THREAD_LOCAL_CACHE
  if (db2unicodelexerLexerStaticData != nullptr) {
    return;
  }
#else
  assert(db2unicodelexerLexerStaticData == nullptr);
#endif
  auto staticData = std::make_unique<Db2UnicodeLexerStaticData>(
    std::vector<std::string>{
      "UNICODE_OPEN", "ARGUMENT_OPEN", "ESCAPE_FOUR", "ESCAPE_SIX", "ESCAPE_PAIR", 
      "APOSTROPHE_PAIR", "GLYPH", "BODY_CLOSE", "INVALID_BODY", "Hex", "ARGUMENT_SCALAR", 
      "ARGUMENT_CLOSE", "INVALID_ARGUMENT"
    },
    std::vector<std::string>{
      "DEFAULT_TOKEN_CHANNEL", "HIDDEN"
    },
    std::vector<std::string>{
      "DEFAULT_MODE", "BODY", "ARGUMENT"
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
  	4,0,12,83,6,-1,6,-1,6,-1,2,0,7,0,2,1,7,1,2,2,7,2,2,3,7,3,2,4,7,4,2,5,
  	7,5,2,6,7,6,2,7,7,7,2,8,7,8,2,9,7,9,2,10,7,10,2,11,7,11,2,12,7,12,1,0,
  	1,0,1,0,1,0,1,0,1,0,1,1,1,1,1,1,1,1,1,2,1,2,1,2,1,2,1,2,1,2,1,2,1,3,1,
  	3,1,3,1,3,1,3,1,3,1,3,1,3,1,3,1,3,1,4,1,4,1,4,1,4,1,4,1,5,1,5,1,5,1,6,
  	1,6,1,6,1,7,1,7,1,7,1,7,1,8,1,8,1,9,1,9,1,10,1,10,1,11,1,11,1,11,1,11,
  	1,12,1,12,0,0,13,3,1,5,2,7,3,9,4,11,5,13,6,15,7,17,8,19,9,21,0,23,10,
  	25,11,27,12,3,0,1,2,3,2,0,85,85,117,117,3,0,48,57,65,70,97,102,7,0,32,
  	32,34,34,39,39,43,43,48,57,65,70,97,102,79,0,3,1,0,0,0,0,5,1,0,0,0,1,
  	7,1,0,0,0,1,9,1,0,0,0,1,11,1,0,0,0,1,13,1,0,0,0,1,15,1,0,0,0,1,17,1,0,
  	0,0,1,19,1,0,0,0,2,23,1,0,0,0,2,25,1,0,0,0,2,27,1,0,0,0,3,29,1,0,0,0,
  	5,35,1,0,0,0,7,39,1,0,0,0,9,46,1,0,0,0,11,56,1,0,0,0,13,61,1,0,0,0,15,
  	64,1,0,0,0,17,67,1,0,0,0,19,71,1,0,0,0,21,73,1,0,0,0,23,75,1,0,0,0,25,
  	77,1,0,0,0,27,81,1,0,0,0,29,30,7,0,0,0,30,31,5,38,0,0,31,32,5,39,0,0,
  	32,33,1,0,0,0,33,34,6,0,0,0,34,4,1,0,0,0,35,36,5,39,0,0,36,37,1,0,0,0,
  	37,38,6,1,1,0,38,6,1,0,0,0,39,40,4,2,0,0,40,41,9,0,0,0,41,42,3,21,9,0,
  	42,43,3,21,9,0,43,44,3,21,9,0,44,45,3,21,9,0,45,8,1,0,0,0,46,47,4,3,1,
  	0,47,48,9,0,0,0,48,49,5,43,0,0,49,50,3,21,9,0,50,51,3,21,9,0,51,52,3,
  	21,9,0,52,53,3,21,9,0,53,54,3,21,9,0,54,55,3,21,9,0,55,10,1,0,0,0,56,
  	57,4,4,2,0,57,58,9,0,0,0,58,59,4,4,3,0,59,60,9,0,0,0,60,12,1,0,0,0,61,
  	62,5,39,0,0,62,63,5,39,0,0,63,14,1,0,0,0,64,65,4,6,4,0,65,66,9,0,0,0,
  	66,16,1,0,0,0,67,68,5,39,0,0,68,69,1,0,0,0,69,70,6,7,2,0,70,18,1,0,0,
  	0,71,72,9,0,0,0,72,20,1,0,0,0,73,74,7,1,0,0,74,22,1,0,0,0,75,76,8,2,0,
  	0,76,24,1,0,0,0,77,78,5,39,0,0,78,79,1,0,0,0,79,80,6,11,2,0,80,26,1,0,
  	0,0,81,82,9,0,0,0,82,28,1,0,0,0,3,0,1,2,3,5,1,0,5,2,0,4,0,0
  };
  staticData->serializedATN = antlr4::atn::SerializedATNView(serializedATNSegment, sizeof(serializedATNSegment) / sizeof(serializedATNSegment[0]));

  antlr4::atn::ATNDeserializer deserializer;
  staticData->atn = deserializer.deserialize(staticData->serializedATN);

  const size_t count = staticData->atn->getNumberOfDecisions();
  staticData->decisionToDFA.reserve(count);
  for (size_t i = 0; i < count; i++) { 
    staticData->decisionToDFA.emplace_back(staticData->atn->getDecisionState(i), i);
  }
  db2unicodelexerLexerStaticData = std::move(staticData);
}

}

Db2UnicodeLexer::Db2UnicodeLexer(CharStream *input) : Lexer(input) {
  Db2UnicodeLexer::initialize();
  _interpreter = new atn::LexerATNSimulator(this, *db2unicodelexerLexerStaticData->atn, db2unicodelexerLexerStaticData->decisionToDFA, db2unicodelexerLexerStaticData->sharedContextCache);
}

Db2UnicodeLexer::~Db2UnicodeLexer() {
  delete _interpreter;
}

std::string Db2UnicodeLexer::getGrammarFileName() const {
  return "Db2UnicodeLexer.g4";
}

const std::vector<std::string>& Db2UnicodeLexer::getRuleNames() const {
  return db2unicodelexerLexerStaticData->ruleNames;
}

const std::vector<std::string>& Db2UnicodeLexer::getChannelNames() const {
  return db2unicodelexerLexerStaticData->channelNames;
}

const std::vector<std::string>& Db2UnicodeLexer::getModeNames() const {
  return db2unicodelexerLexerStaticData->modeNames;
}

const dfa::Vocabulary& Db2UnicodeLexer::getVocabulary() const {
  return db2unicodelexerLexerStaticData->vocabulary;
}

antlr4::atn::SerializedATNView Db2UnicodeLexer::getSerializedATN() const {
  return db2unicodelexerLexerStaticData->serializedATN;
}

const atn::ATN& Db2UnicodeLexer::getATN() const {
  return *db2unicodelexerLexerStaticData->atn;
}


bool Db2UnicodeLexer::sempred(RuleContext *context, size_t ruleIndex, size_t predicateIndex) {
  switch (ruleIndex) {
    case 2: return ESCAPE_FOURSempred(antlrcpp::downCast<antlr4::RuleContext *>(context), predicateIndex);
    case 3: return ESCAPE_SIXSempred(antlrcpp::downCast<antlr4::RuleContext *>(context), predicateIndex);
    case 4: return ESCAPE_PAIRSempred(antlrcpp::downCast<antlr4::RuleContext *>(context), predicateIndex);
    case 6: return GLYPHSempred(antlrcpp::downCast<antlr4::RuleContext *>(context), predicateIndex);

  default:
    break;
  }
  return true;
}


bool Db2UnicodeLexer::ESCAPE_FOURSempred(antlr4::RuleContext *_localctx, size_t predicateIndex) {
  switch (predicateIndex) {
    case 0: return _input->LA(1) == activeEscape;

  default:
    break;
  }
  return true;
}

bool Db2UnicodeLexer::ESCAPE_SIXSempred(antlr4::RuleContext *_localctx, size_t predicateIndex) {
  switch (predicateIndex) {
    case 1: return _input->LA(1) == activeEscape;

  default:
    break;
  }
  return true;
}

bool Db2UnicodeLexer::ESCAPE_PAIRSempred(antlr4::RuleContext *_localctx, size_t predicateIndex) {
  switch (predicateIndex) {
    case 2: return _input->LA(1) == activeEscape;
    case 3: return _input->LA(1) == activeEscape;

  default:
    break;
  }
  return true;
}

bool Db2UnicodeLexer::GLYPHSempred(antlr4::RuleContext *_localctx, size_t predicateIndex) {
  switch (predicateIndex) {
    case 4: return _input->LA(1) != activeEscape && _input->LA(1) != U'\'';

  default:
    break;
  }
  return true;
}


void Db2UnicodeLexer::initialize() {
#if ANTLR4_USE_THREAD_LOCAL_CACHE
  db2unicodelexerLexerInitialize();
#else
  ::antlr4::internal::call_once(db2unicodelexerLexerOnceFlag, db2unicodelexerLexerInitialize);
#endif
}
