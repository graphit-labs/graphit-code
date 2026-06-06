// ANTLR4 WASM driver framework — shared across all grammars.
//
// Provides the binary protocol (length-prefixed stdin/stdout), parse loop,
// and configurable prediction modes + error strategies.
//
// Usage in grammar-specific driver.cpp:
//
//   #include "PlSqlLexer.h"
//   #include "PlSqlParser.h"
//   #include "wasm_driver.h"
//
//   int main() {
//       return graphit::wasmDriverMain<PlSqlLexer, PlSqlParser>(
//           &PlSqlParser::sql_script,
//           graphit::ParseMode::LL,
//           graphit::ErrorStrategy::NoThrow
//       );
//   }
//
// Parse modes:
//   SLL_LL  — SLL first, catch exception, fallback to LL (requires WASM EH)
//   LL      — LL-only + NoThrowErrorStrategy (no exceptions, robust)
//   SLL     — SLL-only + BailErrorStrategy (fastest, no error recovery)
//
// Error strategies:
//   Default — DefaultErrorStrategy (standard ANTLR4 error recovery)
//   NoThrow — NoThrowErrorStrategy (exception-free, for stubs builds)
//   Bail    — BailErrorStrategy (throws on first error, for SLL)
#pragma once

#include <cstdint>
#include <iostream>
#include <sstream>
#include <string>
#include <memory>

#include "antlr4-runtime.h"
#include "json_serializer.h"
#include "no_throw_error_strategy.h"

namespace graphit {

enum class ParseMode {
    SLL_LL,  // SLL + catch + LL fallback (needs real WASM EH)
    LL,      // LL-only (no exceptions needed)
    SLL      // SLL-only (fastest, no recovery)
};

enum class ErrorStrategy {
    Default,  // Standard ANTLR4 error recovery
    NoThrow,  // Exception-free (for builds without WASM EH)
    Bail      // Bail on first error
};

namespace detail {

static bool readExact(std::istream &in, char *buf, size_t n) {
    in.read(buf, static_cast<std::streamsize>(n));
    return in.gcount() == static_cast<std::streamsize>(n);
}

static void writeU32BE(std::ostream &out, uint32_t v) {
    char buf[4];
    buf[0] = static_cast<char>((v >> 24) & 0xFF);
    buf[1] = static_cast<char>((v >> 16) & 0xFF);
    buf[2] = static_cast<char>((v >>  8) & 0xFF);
    buf[3] = static_cast<char>( v        & 0xFF);
    out.write(buf, 4);
}

static void writeResponse(const std::string &json) {
    writeU32BE(std::cout, static_cast<uint32_t>(json.size()));
    std::cout.write(json.data(), static_cast<std::streamsize>(json.size()));
    std::cout.flush();
}

static std::shared_ptr<antlr4::ANTLRErrorStrategy> makeStrategy(ErrorStrategy s) {
    switch (s) {
        case ErrorStrategy::NoThrow:
            return std::make_shared<graphit::NoThrowErrorStrategy>();
        case ErrorStrategy::Bail:
            return std::make_shared<antlr4::BailErrorStrategy>();
        case ErrorStrategy::Default:
        default:
            return std::make_shared<antlr4::DefaultErrorStrategy>();
    }
}

static antlr4::atn::PredictionMode toPredictionMode(ParseMode m) {
    switch (m) {
        case ParseMode::SLL:
        case ParseMode::SLL_LL:
            return antlr4::atn::PredictionMode::SLL;
        case ParseMode::LL:
        default:
            return antlr4::atn::PredictionMode::LL;
    }
}

} // namespace detail

// wasmDriverMain — entry point for WASM grammar drivers.
//
// Template parameters:
//   Lexer  — ANTLR4 generated lexer class
//   Parser — ANTLR4 generated parser class
//
// Parameters:
//   entryRule — pointer to the parser's entry rule method (e.g., &Parser::sql_script)
//   mode      — prediction mode (SLL_LL, LL, SLL)
//   strategy  — error strategy for LL/SLL-only modes
//
// Returns 0 on clean exit (stdin closed).
template <typename Lexer, typename Parser, typename EntryRule>
int wasmDriverMain(
    EntryRule entryRule,
    ParseMode mode = ParseMode::LL,
    ErrorStrategy strategy = ErrorStrategy::NoThrow
) {
    std::ios_base::sync_with_stdio(false);
    std::cin.tie(nullptr);

    auto errorStrategy = detail::makeStrategy(strategy);

    char lenBuf[4];
    while (true) {
        if (!detail::readExact(std::cin, lenBuf, 4)) {
            break;
        }

        uint32_t srcLen = (static_cast<uint8_t>(lenBuf[0]) << 24) |
                          (static_cast<uint8_t>(lenBuf[1]) << 16) |
                          (static_cast<uint8_t>(lenBuf[2]) <<  8) |
                           static_cast<uint8_t>(lenBuf[3]);

        std::string source(srcLen, '\0');
        if (!detail::readExact(std::cin, &source[0], srcLen)) {
            return 1;
        }

        antlr4::ANTLRInputStream input(source);
        Lexer lexer(&input);
        lexer.removeErrorListeners();
        antlr4::CommonTokenStream tokens(&lexer);
        Parser parser(&tokens);
        parser.removeErrorListeners();
        parser.setBuildParseTree(true);

        antlr4::tree::ParseTree *tree = nullptr;

        if (mode == ParseMode::SLL_LL) {
            // Stage 1: SLL + BailErrorStrategy — fast, may throw on ambiguity
            parser.setErrorHandler(std::make_shared<antlr4::BailErrorStrategy>());
            parser.template getInterpreter<antlr4::atn::ParserATNSimulator>()->setPredictionMode(
                antlr4::atn::PredictionMode::SLL);
            try {
                tree = (parser.*entryRule)();
            } catch (...) {
                // Stage 2: LL + chosen strategy — full error recovery
                tokens.seek(0);
                parser.reset();
                parser.removeErrorListeners();
                parser.setErrorHandler(detail::makeStrategy(
                    strategy == ErrorStrategy::Bail ? ErrorStrategy::Default : strategy));
                parser.template getInterpreter<antlr4::atn::ParserATNSimulator>()->setPredictionMode(
                    antlr4::atn::PredictionMode::LL);
                tree = (parser.*entryRule)();
            }
        } else {
            // Single-mode parse (LL or SLL)
            parser.setErrorHandler(errorStrategy);
            parser.template getInterpreter<antlr4::atn::ParserATNSimulator>()->setPredictionMode(
                detail::toPredictionMode(mode));
            tree = (parser.*entryRule)();
        }

        std::ostringstream jsonOut;
        graphit::treeToJSON(jsonOut, tree, parser.getRuleNames(), parser.getVocabulary());
        detail::writeResponse(jsonOut.str());
    }

    return 0;
}

} // namespace graphit
