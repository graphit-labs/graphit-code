#ifndef JSON_SERIALIZER_H
#define JSON_SERIALIZER_H

#include <string>
#include <sstream>
#include "antlr4-runtime.h"

namespace graphit {

inline void escapeJSON(std::ostream &out, const std::string &s) {
    for (char c : s) {
        switch (c) {
            case '"':  out << "\\\""; break;
            case '\\': out << "\\\\"; break;
            case '\n': out << "\\n";  break;
            case '\r': out << "\\r";  break;
            case '\t': out << "\\t";  break;
            default:
                if (static_cast<unsigned char>(c) < 0x20)
                    out << "\\u" << std::hex << (int)c << std::dec;
                else
                    out << c;
        }
    }
}

inline void treeToJSON(std::ostream &out,
                       antlr4::tree::ParseTree *node,
                       const std::vector<std::string> &ruleNames,
                       const antlr4::dfa::Vocabulary &vocab) {
    if (auto *term = dynamic_cast<antlr4::tree::TerminalNode *>(node)) {
        auto *tok = term->getSymbol();
        if (tok->getType() == antlr4::Token::EOF) return;

        out << "{\"token\":\"";
        std::string name = vocab.getDisplayName(tok->getType());
        escapeJSON(out, name);
        out << "\",\"text\":\"";
        escapeJSON(out, tok->getText());
        out << "\",\"start\":[" << tok->getLine() << "," << tok->getCharPositionInLine() << "]";
        out << ",\"end\":[" << tok->getLine() << ","
            << (tok->getCharPositionInLine() + (int)tok->getText().length() - 1) << "]}";
        return;
    }

    auto *rule = dynamic_cast<antlr4::ParserRuleContext *>(node);
    if (!rule) return;

    out << "{\"rule\":\"";
    if (rule->getRuleIndex() < ruleNames.size())
        escapeJSON(out, ruleNames[rule->getRuleIndex()]);
    out << "\"";

    auto *start = rule->getStart();
    auto *stop  = rule->getStop();
    if (start) out << ",\"start\":[" << start->getLine() << "," << start->getCharPositionInLine() << "]";
    if (stop) {
        int endCol = stop->getCharPositionInLine() + (int)stop->getText().length() - 1;
        out << ",\"end\":[" << stop->getLine() << "," << endCol << "]";
    }

    auto children = rule->children;
    if (!children.empty()) {
        out << ",\"children\":[";
        bool first = true;
        for (auto *child : children) {
            auto *t = dynamic_cast<antlr4::tree::TerminalNode *>(child);
            if (t && t->getSymbol()->getType() == antlr4::Token::EOF) continue;

            std::ostringstream tmp;
            treeToJSON(tmp, child, ruleNames, vocab);
            if (tmp.str().empty()) continue;

            if (!first) out << ",";
            out << tmp.str();
            first = false;
        }
        out << "]";
    }
    out << "}";
}

} // namespace graphit
#endif
