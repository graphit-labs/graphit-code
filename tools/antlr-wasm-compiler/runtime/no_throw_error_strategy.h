// Exception-free error strategy for ANTLR4 C++ on WASI.
//
// DefaultErrorStrategy.sync() throws InputMismatchException internally during
// error recovery. This subclass overrides the throwing methods to silently
// recover by consuming tokens, keeping the parse tree valid (though partial).
#pragma once

#include "antlr4-runtime.h"

namespace graphit {

class NoThrowErrorStrategy : public antlr4::DefaultErrorStrategy {
public:
    // Override sync to never throw. On error, consume one token and continue.
    void sync(antlr4::Parser *recognizer) override {
        antlr4::atn::ATNState *s = recognizer->getInterpreter<antlr4::atn::ParserATNSimulator>()->atn.states[recognizer->getState()];
        antlr4::misc::IntervalSet expecting = getExpectedTokens(recognizer);
        antlr4::TokenStream *tokenStream = recognizer->getTokenStream();
        size_t la = static_cast<size_t>(tokenStream->LA(1));

        if (la == static_cast<size_t>(antlr4::Token::EOF) || expecting.contains(la)) {
            return; // Already in sync
        }

        // Report and consume instead of throwing
        reportUnwantedToken(recognizer);
        consumeUntilOrEOF(recognizer, expecting);
    }

    // Override recover to consume until expected token or EOF instead of throwing.
    void recover(antlr4::Parser *recognizer, std::exception_ptr /*e*/) override {
        antlr4::misc::IntervalSet expecting = getErrorRecoverySet(recognizer);
        consumeUntilOrEOF(recognizer, expecting);
    }

    // Override recoverInline to return a missing token instead of throwing.
    antlr4::Token *recoverInline(antlr4::Parser *recognizer) override {
        reportMissingToken(recognizer);
        return getMissingSymbol(recognizer);
    }

private:
    void consumeUntilOrEOF(antlr4::Parser *recognizer, const antlr4::misc::IntervalSet &set) {
        size_t ttype = static_cast<size_t>(recognizer->getTokenStream()->LA(1));
        while (ttype != static_cast<size_t>(antlr4::Token::EOF) && !set.contains(ttype)) {
            recognizer->consume();
            ttype = static_cast<size_t>(recognizer->getTokenStream()->LA(1));
        }
    }
};

} // namespace graphit
