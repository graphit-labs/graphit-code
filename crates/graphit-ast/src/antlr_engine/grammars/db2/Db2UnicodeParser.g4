// Db2 LUW 11.5 Unicode constant and UESCAPE argument recognition.
parser grammar Db2UnicodeParser;
options { tokenVocab = Db2UnicodeLexer; }

escape_argument
    : ARGUMENT_OPEN ARGUMENT_SCALAR ARGUMENT_CLOSE EOF
    ;

unicode_literal
    : UNICODE_OPEN (ESCAPE_FOUR | ESCAPE_SIX | ESCAPE_PAIR | APOSTROPHE_PAIR | GLYPH)* BODY_CLOSE EOF
    ;
