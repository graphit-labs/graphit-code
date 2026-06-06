// PL/SQL grammar driver — multi-parse mode with length-prefixed protocol.
//
// Protocol (binary, big-endian on stdin/stdout):
//   Request:  [4 bytes: source length N] [N bytes: PL/SQL source]
//   Response: [4 bytes: JSON length M]   [M bytes: JSON parse tree]
//
// The driver loops until stdin is closed (length read returns EOF).
// ATN tables are initialized once on first parse and reused across all parses.
//
// Parsing strategy: two-stage SLL→LL. SLL is a fast O(n) prediction mode that
// handles most inputs. When SLL reports errors (ambiguities or syntax issues),
// the parser resets and retries with full LL mode for better recovery.
// NoThrowErrorStrategy prevents C++ exceptions (required for WASI).
#include <cstdint>
#include <iostream>
#include <sstream>
#include <string>

#include "antlr4-runtime.h"
#include "PlSqlLexer.h"
#include "PlSqlParser.h"
#include "json_serializer.h"
#include "no_throw_error_strategy.h"

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

int main() {
    std::ios_base::sync_with_stdio(false);
    std::cin.tie(nullptr);

    auto errorStrategy = std::make_shared<graphit::NoThrowErrorStrategy>();

    char lenBuf[4];
    while (true) {
        if (!readExact(std::cin, lenBuf, 4)) {
            break;
        }

        uint32_t srcLen = (static_cast<uint8_t>(lenBuf[0]) << 24) |
                          (static_cast<uint8_t>(lenBuf[1]) << 16) |
                          (static_cast<uint8_t>(lenBuf[2]) <<  8) |
                           static_cast<uint8_t>(lenBuf[3]);

        std::string source(srcLen, '\0');
        if (!readExact(std::cin, &source[0], srcLen)) {
            return 1;
        }

        antlr4::ANTLRInputStream input(source);
        PlSqlLexer lexer(&input);
        lexer.removeErrorListeners();
        antlr4::CommonTokenStream tokens(&lexer);
        PlSqlParser parser(&tokens);
        parser.removeErrorListeners();
        parser.setErrorHandler(errorStrategy);
        parser.setBuildParseTree(true);

        // Stage 1: SLL — fast O(n) prediction, handles most clean inputs.
        parser.getInterpreter<antlr4::atn::ParserATNSimulator>()->setPredictionMode(antlr4::atn::PredictionMode::SLL);
        auto *tree = parser.sql_script();

        // Stage 2: LL — full power fallback when SLL hits ambiguities or errors.
        if (parser.getNumberOfSyntaxErrors() > 0) {
            tokens.seek(0);
            parser.reset();
            parser.removeErrorListeners();
            parser.setErrorHandler(errorStrategy);
            parser.getInterpreter<antlr4::atn::ParserATNSimulator>()->setPredictionMode(antlr4::atn::PredictionMode::LL);
            tree = parser.sql_script();
        }

        std::ostringstream jsonOut;
        graphit::treeToJSON(jsonOut, tree, parser.getRuleNames(), parser.getVocabulary());
        std::string json = jsonOut.str();

        writeResponse(json);
    }

    return 0;
}

