// PL/SQL source preprocessor — normalizes DBMS_METADATA / Data Pump output.
//
// Oracle's DBMS_METADATA.GET_DDL and impdp SQLFILE omit terminators (`;`, `/`)
// between statements by design. This preprocessor injects missing semicolons
// so the ANTLR parser receives well-delimited input.
//
// Patterns handled:
//   1. DDL followed by BEGIN without separator → inject `;` before BEGIN
//   2. DDL followed by another DDL without separator → inject `;` before CREATE/ALTER/DROP/GRANT/...
//   3. Truncated LOB/physical-storage clauses with unmatched `(` → close with `)`
#pragma once

#include <string>
#include <cstring>

namespace graphit {
namespace plsql {

// Case-insensitive prefix check starting at src[pos], skipping leading whitespace.
static bool matchKeywordAt(const std::string &src, size_t pos, const char *kw) {
    size_t len = std::strlen(kw);
    if (pos + len > src.size()) return false;
    for (size_t i = 0; i < len; ++i) {
        char c = src[pos + i];
        if (c >= 'a' && c <= 'z') c -= 32; // toupper
        if (c != kw[i]) return false;
    }
    // Must be followed by whitespace, '(' , '"' or end-of-string (word boundary).
    if (pos + len < src.size()) {
        char next = src[pos + len];
        if ((next >= 'A' && next <= 'Z') || (next >= 'a' && next <= 'z') || (next >= '0' && next <= '9') || next == '_')
            return false;
    }
    return true;
}

// Tokens that begin a new top-level DDL statement.
// Only pure DDL keywords belong here. DML keywords (INSERT, UPDATE, DELETE,
// SELECT, MERGE) and PL/SQL block keywords (BEGIN, DECLARE) appear inside
// function/procedure bodies and MUST NOT trigger semicolon injection.
static bool isStatementStart(const std::string &src, size_t pos) {
    return matchKeywordAt(src, pos, "CREATE")
        || matchKeywordAt(src, pos, "ALTER")
        || matchKeywordAt(src, pos, "DROP")
        || matchKeywordAt(src, pos, "GRANT")
        || matchKeywordAt(src, pos, "REVOKE")
        || matchKeywordAt(src, pos, "COMMENT")
        || matchKeywordAt(src, pos, "TRUNCATE")
        || matchKeywordAt(src, pos, "ANALYZE")
        || matchKeywordAt(src, pos, "AUDIT")
        || matchKeywordAt(src, pos, "NOAUDIT")
        || matchKeywordAt(src, pos, "FLASHBACK")
        || matchKeywordAt(src, pos, "PURGE")
        || matchKeywordAt(src, pos, "RENAME");
}

static bool isWhitespace(char c) {
    return c == ' ' || c == '\t' || c == '\r' || c == '\n';
}

// Skip backwards over whitespace and comments, return position of last
// non-whitespace char (or 0 if all whitespace).
static size_t skipBackWhitespace(const std::string &src, size_t pos) {
    while (pos > 0 && isWhitespace(src[pos])) --pos;
    return pos;
}

// Strip Data Pump's internal USING clause from materialized views.
//
// Pattern: `) USING ("name", (params), ..., (params)) REFRESH`
// The USING block contains prebuilt metadata (snapshot IDs, timestamps)
// that is not valid SQL. We remove it, leaving `) REFRESH`.
static std::string stripMviewUsing(const std::string &src) {
    std::string out;
    out.reserve(src.size());
    size_t i = 0;
    size_t len = src.size();

    while (i < len) {
        // Look for ') USING (' pattern (case-insensitive).
        if (src[i] == ')') {
            size_t j = i + 1;
            while (j < len && (src[j] == ' ' || src[j] == '\t' || src[j] == '\n' || src[j] == '\r')) ++j;
            if (j < len && matchKeywordAt(src, j, "USING")) {
                size_t usingEnd = j + 5;
                while (usingEnd < len && (src[usingEnd] == ' ' || src[usingEnd] == '\t' || src[usingEnd] == '\n' || src[usingEnd] == '\r')) ++usingEnd;
                if (usingEnd < len && src[usingEnd] == '(') {
                    // Found ') USING (' — now skip the balanced parenthesis block.
                    int depth = 1;
                    size_t k = usingEnd + 1;
                    bool inSq = false;
                    bool inDq = false;
                    while (k < len && depth > 0) {
                        char c = src[k];
                        if (inSq) {
                            if (c == '\'' && k + 1 < len && src[k + 1] == '\'') { ++k; }
                            else if (c == '\'') inSq = false;
                        } else if (inDq) {
                            if (c == '"') inDq = false;
                        } else {
                            if (c == '\'') inSq = true;
                            else if (c == '"') inDq = true;
                            else if (c == '(') ++depth;
                            else if (c == ')') --depth;
                        }
                        ++k;
                    }
                    // Verify what follows: should be whitespace then REFRESH/AS/etc.
                    size_t afterUsing = k;
                    while (afterUsing < len && (src[afterUsing] == ' ' || src[afterUsing] == '\t' || src[afterUsing] == '\n' || src[afterUsing] == '\r')) ++afterUsing;
                    if (afterUsing < len && (matchKeywordAt(src, afterUsing, "REFRESH")
                                          || matchKeywordAt(src, afterUsing, "AS")
                                          || matchKeywordAt(src, afterUsing, "BUILD"))) {
                        // Strip: emit ')' and space, skip USING block.
                        out += ") ";
                        i = k;
                        continue;
                    }
                }
            }
        }
        out += src[i];
        ++i;
    }
    return out;
}

// Preprocess PL/SQL source: inject `;` where DBMS_METADATA omits terminators.
//
// Strategy: scan for positions where a new statement keyword appears at the
// start of a line (column 0 or after only whitespace on that line) and the
// previous non-whitespace character is NOT `;` or `/`. In that case, inject
// `;\n` before the keyword.
static std::string preprocess(const std::string &raw) {
    if (raw.empty()) return raw;

    // Phase 1: strip Data Pump USING metadata from materialized views.
    std::string src = stripMviewUsing(raw);

    // Phase 2: strip Oracle 12c+ edition-based redefinition keywords.
    // EDITIONABLE / NONEDITIONABLE appear in DBMS_METADATA exports but are
    // not in the ANTLR PL/SQL grammar. They always precede a DDL keyword
    // (FUNCTION, PROCEDURE, PACKAGE, TYPE, TRIGGER, VIEW, SYNONYM).
    {
        std::string cleaned;
        cleaned.reserve(src.size());
        size_t i2 = 0;
        while (i2 < src.size()) {
            bool matched = false;
            const char *kws[] = {"NONEDITIONABLE", "EDITIONABLE"};
            size_t kwlens[] = {14, 12};
            for (int k = 0; k < 2; ++k) {
                if (matchKeywordAt(src, i2, kws[k])) {
                    // Skip the keyword and trailing whitespace.
                    i2 += kwlens[k];
                    while (i2 < src.size() && isWhitespace(src[i2])) ++i2;
                    matched = true;
                    break;
                }
            }
            if (!matched) {
                cleaned += src[i2];
                ++i2;
            }
        }
        src = std::move(cleaned);
    }

    std::string out;
    out.reserve(src.size() + src.size() / 64); // ~1.5% overhead

    size_t i = 0;
    size_t len = src.size();

    // Track parenthesis depth for detecting truncated clauses.
    int parenDepth = 0;
    bool inString = false;   // inside '...'
    bool inDqString = false; // inside "..."
    bool inLineComment = false;
    bool inBlockComment = false;

    while (i < len) {
        char c = src[i];

        // Track string literals and comments for accurate parenthesis counting.
        if (inLineComment) {
            out += c;
            if (c == '\n') inLineComment = false;
            ++i;
            continue;
        }
        if (inBlockComment) {
            out += c;
            if (c == '*' && i + 1 < len && src[i + 1] == '/') {
                out += src[i + 1];
                i += 2;
                inBlockComment = false;
            } else {
                ++i;
            }
            continue;
        }
        if (inString) {
            out += c;
            if (c == '\'' && i + 1 < len && src[i + 1] == '\'') {
                out += src[i + 1]; // escaped quote
                i += 2;
            } else {
                if (c == '\'') inString = false;
                ++i;
            }
            continue;
        }
        if (inDqString) {
            out += c;
            if (c == '"') inDqString = false;
            ++i;
            continue;
        }

        // Detect comment/string starts.
        if (c == '-' && i + 1 < len && src[i + 1] == '-') {
            inLineComment = true;
            out += c;
            ++i;
            continue;
        }
        if (c == '/' && i + 1 < len && src[i + 1] == '*') {
            inBlockComment = true;
            out += c;
            ++i;
            continue;
        }
        if (c == '\'') {
            inString = true;
            out += c;
            ++i;
            continue;
        }
        if (c == '"') {
            inDqString = true;
            out += c;
            ++i;
            continue;
        }

        // Track parens.
        if (c == '(') ++parenDepth;
        if (c == ')') { if (parenDepth > 0) --parenDepth; }

        // Check: is this the start of a line (after newline + optional whitespace)?
        // If yes and a statement keyword follows, check if we need to inject `;`.
        if (c == '\n') {
            out += c;
            ++i;

            // Skip whitespace on the new line.
            size_t lineStart = i;
            while (i < len && (src[i] == ' ' || src[i] == '\t')) ++i;

            // Check for -- comments at start of line (skip entirely).
            if (i < len && src[i] == '-' && i + 1 < len && src[i + 1] == '-') {
                // Copy the whitespace we skipped.
                for (size_t j = lineStart; j < i; ++j) out += src[j];
                continue; // let the main loop handle the comment
            }

            // Check if a statement-starting keyword is here.
            if (i < len && isStatementStart(src, i)) {
                // Look back: find the last non-whitespace character before this line.
                size_t prevPos = skipBackWhitespace(out, out.size() > 0 ? out.size() - 1 : 0);
                char prev = (prevPos < out.size()) ? out[prevPos] : '\0';

                // If the previous significant char is NOT a terminator, inject one.
                if (prev != ';' && prev != '/' && prev != '\0') {
                    // If we have unmatched parens, close them first.
                    while (parenDepth > 0) {
                        out += ')';
                        --parenDepth;
                    }
                    out += ";\n";
                }
            }

            // Copy the whitespace we skipped.
            for (size_t j = lineStart; j < i; ++j) out += src[j];
            continue;
        }

        out += c;
        ++i;
    }

    // Handle trailing unmatched parens at end of file.
    if (parenDepth > 0) {
        while (parenDepth > 0) {
            out += ')';
            --parenDepth;
        }
        out += ';';
    }

    return out;
}

} // namespace plsql
} // namespace graphit
