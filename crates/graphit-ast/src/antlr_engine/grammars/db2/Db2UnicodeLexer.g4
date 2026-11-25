// Db2 LUW 11.5 Unicode constant syntax. Validation uses the official ANTLR SDK.
lexer grammar Db2UnicodeLexer;

@members {
  char32_t activeEscape = U'\\';
  void setActiveEscape(char32_t value) { activeEscape = value; }
}

UNICODE_OPEN : [Uu] '&' '\'' -> pushMode(BODY);
ARGUMENT_OPEN : '\'' -> pushMode(ARGUMENT);

mode BODY;
ESCAPE_FOUR : {_input->LA(1) == activeEscape}? . Hex Hex Hex Hex;
ESCAPE_SIX : {_input->LA(1) == activeEscape}? . '+' Hex Hex Hex Hex Hex Hex;
ESCAPE_PAIR : {_input->LA(1) == activeEscape}? . {_input->LA(1) == activeEscape}? .;
APOSTROPHE_PAIR : '\'\'';
GLYPH : {_input->LA(1) != activeEscape && _input->LA(1) != U'\''}? .;
BODY_CLOSE : '\'' -> popMode;
INVALID_BODY : .;
fragment Hex : [0-9a-fA-F];

mode ARGUMENT;
ARGUMENT_SCALAR : ~[ +\u0027"0-9a-fA-F];
ARGUMENT_CLOSE : '\'' -> popMode;
INVALID_ARGUMENT : .;
