
// Generated from Cobol85.g4 by ANTLR 4.13.2

#pragma once


#include "antlr4-runtime.h"


namespace graphit::antlr::cobol85 {


class  Cobol85Parser : public antlr4::Parser {
public:
  enum {
    ABORT = 1, ACCEPT = 2, ACCESS = 3, ADD = 4, ADDRESS = 5, ADVANCING = 6, 
    AFTER = 7, ALIGNED = 8, ALL = 9, ALPHABET = 10, ALPHABETIC = 11, ALPHABETIC_LOWER = 12, 
    ALPHABETIC_UPPER = 13, ALPHANUMERIC = 14, ALPHANUMERIC_EDITED = 15, 
    ALSO = 16, ALTER = 17, ALTERNATE = 18, AND = 19, ANY = 20, ARE = 21, 
    AREA = 22, AREAS = 23, AS = 24, ASCENDING = 25, ASCII = 26, ASSIGN = 27, 
    ASSOCIATED_DATA = 28, ASSOCIATED_DATA_LENGTH = 29, AT = 30, ATTRIBUTE = 31, 
    AUTHOR = 32, AUTO = 33, AUTO_SKIP = 34, BACKGROUND_COLOR = 35, BACKGROUND_COLOUR = 36, 
    BASIS = 37, BEEP = 38, BEFORE = 39, BEGINNING = 40, BELL = 41, BINARY = 42, 
    BIT = 43, BLANK = 44, BLINK = 45, BLOCK = 46, BOUNDS = 47, BOTTOM = 48, 
    BY = 49, BYFUNCTION = 50, BYTITLE = 51, CALL = 52, CANCEL = 53, CAPABLE = 54, 
    CCSVERSION = 55, CD = 56, CF = 57, CH = 58, CHAINING = 59, CHANGED = 60, 
    CHANNEL = 61, CHARACTER = 62, CHARACTERS = 63, CLASS = 64, CLASS_ID = 65, 
    CLOCK_UNITS = 66, CLOSE = 67, CLOSE_DISPOSITION = 68, COBOL = 69, CODE = 70, 
    CODE_SET = 71, COLLATING = 72, COL = 73, COLUMN = 74, COM_REG = 75, 
    COMMA = 76, COMMITMENT = 77, COMMON = 78, COMMUNICATION = 79, COMP = 80, 
    COMP_1 = 81, COMP_2 = 82, COMP_3 = 83, COMP_4 = 84, COMP_5 = 85, COMPUTATIONAL = 86, 
    COMPUTATIONAL_1 = 87, COMPUTATIONAL_2 = 88, COMPUTATIONAL_3 = 89, COMPUTATIONAL_4 = 90, 
    COMPUTATIONAL_5 = 91, COMPUTE = 92, CONFIGURATION = 93, CONTAINS = 94, 
    CONTENT = 95, CONTINUE = 96, CONTROL = 97, CONTROL_POINT = 98, CONTROLS = 99, 
    CONVENTION = 100, CONVERTING = 101, COPY = 102, CORR = 103, CORRESPONDING = 104, 
    COUNT = 105, CRUNCH = 106, CURRENCY = 107, CURSOR = 108, DATA = 109, 
    DATA_BASE = 110, DATE = 111, DATE_COMPILED = 112, DATE_WRITTEN = 113, 
    DAY = 114, DAY_OF_WEEK = 115, DBCS = 116, DE = 117, DEBUG_CONTENTS = 118, 
    DEBUG_ITEM = 119, DEBUG_LINE = 120, DEBUG_NAME = 121, DEBUG_SUB_1 = 122, 
    DEBUG_SUB_2 = 123, DEBUG_SUB_3 = 124, DEBUGGING = 125, DECIMAL_POINT = 126, 
    DECLARATIVES = 127, DEFAULT = 128, DEFAULT_DISPLAY = 129, DEFINITION = 130, 
    DELETE = 131, DELIMITED = 132, DELIMITER = 133, DEPENDING = 134, DESCENDING = 135, 
    DESTINATION = 136, DETAIL = 137, DFHRESP = 138, DFHVALUE = 139, DISABLE = 140, 
    DISK = 141, DISPLAY = 142, DISPLAY_1 = 143, DIVIDE = 144, DIVISION = 145, 
    DONTCARE = 146, DOUBLE = 147, DOWN = 148, DUPLICATES = 149, DYNAMIC = 150, 
    EBCDIC = 151, EGCS = 152, EGI = 153, ELSE = 154, EMI = 155, EMPTY_CHECK = 156, 
    ENABLE = 157, END = 158, END_ACCEPT = 159, END_ADD = 160, END_CALL = 161, 
    END_COMPUTE = 162, END_DELETE = 163, END_DIVIDE = 164, END_EVALUATE = 165, 
    END_IF = 166, END_MULTIPLY = 167, END_OF_PAGE = 168, END_PERFORM = 169, 
    END_READ = 170, END_RECEIVE = 171, END_RETURN = 172, END_REWRITE = 173, 
    END_SEARCH = 174, END_START = 175, END_STRING = 176, END_SUBTRACT = 177, 
    END_UNSTRING = 178, END_WRITE = 179, ENDING = 180, ENTER = 181, ENTRY = 182, 
    ENTRY_PROCEDURE = 183, ENVIRONMENT = 184, EOP = 185, EQUAL = 186, ERASE = 187, 
    ERROR = 188, EOL = 189, EOS = 190, ESCAPE = 191, ESI = 192, EVALUATE = 193, 
    EVENT = 194, EVERY = 195, EXCEPTION = 196, EXCLUSIVE = 197, EXHIBIT = 198, 
    EXIT = 199, EXPORT = 200, EXTEND = 201, EXTENDED = 202, EXTERNAL = 203, 
    FALSE = 204, FD = 205, FILE = 206, FILE_CONTROL = 207, FILLER = 208, 
    FINAL = 209, FIRST = 210, FOOTING = 211, FOR = 212, FOREGROUND_COLOR = 213, 
    FOREGROUND_COLOUR = 214, FROM = 215, FULL = 216, FUNCTION = 217, FUNCTIONNAME = 218, 
    FUNCTION_POINTER = 219, GENERATE = 220, GOBACK = 221, GIVING = 222, 
    GLOBAL = 223, GO = 224, GREATER = 225, GRID = 226, GROUP = 227, HEADING = 228, 
    HIGHLIGHT = 229, HIGH_VALUE = 230, HIGH_VALUES = 231, I_O = 232, I_O_CONTROL = 233, 
    ID = 234, IDENTIFICATION = 235, IF = 236, IMPLICIT = 237, IMPORT = 238, 
    IN = 239, INDEX = 240, INDEXED = 241, INDICATE = 242, INITIAL = 243, 
    INITIALIZE = 244, INITIATE = 245, INPUT = 246, INPUT_OUTPUT = 247, INSPECT = 248, 
    INSTALLATION = 249, INTEGER = 250, INTO = 251, INVALID = 252, INVOKE = 253, 
    IS = 254, JUST = 255, JUSTIFIED = 256, KANJI = 257, KEPT = 258, KEY = 259, 
    KEYBOARD = 260, LABEL = 261, LANGUAGE = 262, LAST = 263, LB = 264, LD = 265, 
    LEADING = 266, LEFT = 267, LEFTLINE = 268, LENGTH = 269, LENGTH_CHECK = 270, 
    LESS = 271, LIBACCESS = 272, LIBPARAMETER = 273, LIBRARY = 274, LIMIT = 275, 
    LIMITS = 276, LINAGE = 277, LINAGE_COUNTER = 278, LINE = 279, LINES = 280, 
    LINE_COUNTER = 281, LINKAGE = 282, LIST = 283, LOCAL = 284, LOCAL_STORAGE = 285, 
    LOCK = 286, LONG_DATE = 287, LONG_TIME = 288, LOWER = 289, LOWLIGHT = 290, 
    LOW_VALUE = 291, LOW_VALUES = 292, MEMORY = 293, MERGE = 294, MESSAGE = 295, 
    MMDDYYYY = 296, MODE = 297, MODULES = 298, MORE_LABELS = 299, MOVE = 300, 
    MULTIPLE = 301, MULTIPLY = 302, NAMED = 303, NATIONAL = 304, NATIONAL_EDITED = 305, 
    NATIVE = 306, NEGATIVE = 307, NETWORK = 308, NEXT = 309, NO = 310, NO_ECHO = 311, 
    NOT = 312, NULL_ = 313, NULLS = 314, NUMBER = 315, NUMERIC = 316, NUMERIC_DATE = 317, 
    NUMERIC_EDITED = 318, NUMERIC_TIME = 319, OBJECT_COMPUTER = 320, OCCURS = 321, 
    ODT = 322, OF = 323, OFF = 324, OMITTED = 325, ON = 326, OPEN = 327, 
    OPTIONAL = 328, OR = 329, ORDER = 330, ORDERLY = 331, ORGANIZATION = 332, 
    OTHER = 333, OUTPUT = 334, OVERFLOW = 335, OVERLINE = 336, OWN = 337, 
    PACKED_DECIMAL = 338, PADDING = 339, PAGE = 340, PAGE_COUNTER = 341, 
    PASSWORD = 342, PERFORM = 343, PF = 344, PH = 345, PIC = 346, PICTURE = 347, 
    PLUS = 348, POINTER = 349, POSITION = 350, POSITIVE = 351, PORT = 352, 
    PRINTER = 353, PRINTING = 354, PRIVATE = 355, PROCEDURE = 356, PROCEDURE_POINTER = 357, 
    PROCEDURES = 358, PROCEED = 359, PROCESS = 360, PROGRAM = 361, PROGRAM_ID = 362, 
    PROGRAM_LIBRARY = 363, PROMPT = 364, PURGE = 365, QUEUE = 366, QUOTE = 367, 
    QUOTES = 368, RANDOM = 369, READER = 370, REMOTE = 371, RD = 372, REAL = 373, 
    READ = 374, RECEIVE = 375, RECEIVED = 376, RECORD = 377, RECORDING = 378, 
    RECORDS = 379, RECURSIVE = 380, REDEFINES = 381, REEL = 382, REF = 383, 
    REFERENCE = 384, REFERENCES = 385, RELATIVE = 386, RELEASE = 387, REMAINDER = 388, 
    REMARKS = 389, REMOVAL = 390, REMOVE = 391, RENAMES = 392, REPLACE = 393, 
    REPLACING = 394, REPORT = 395, REPORTING = 396, REPORTS = 397, REQUIRED = 398, 
    RERUN = 399, RESERVE = 400, REVERSE_VIDEO = 401, RESET = 402, RETURN = 403, 
    RETURN_CODE = 404, RETURNING = 405, REVERSED = 406, REWIND = 407, REWRITE = 408, 
    RF = 409, RH = 410, RIGHT = 411, ROUNDED = 412, RUN = 413, SAME = 414, 
    SAVE = 415, SCREEN = 416, SD = 417, SEARCH = 418, SECTION = 419, SECURE = 420, 
    SECURITY = 421, SEGMENT = 422, SEGMENT_LIMIT = 423, SELECT = 424, SEND = 425, 
    SENTENCE = 426, SEPARATE = 427, SEQUENCE = 428, SEQUENTIAL = 429, SET = 430, 
    SHARED = 431, SHAREDBYALL = 432, SHAREDBYRUNUNIT = 433, SHARING = 434, 
    SHIFT_IN = 435, SHIFT_OUT = 436, SHORT_DATE = 437, SIGN = 438, SIZE = 439, 
    SORT = 440, SORT_CONTROL = 441, SORT_CORE_SIZE = 442, SORT_FILE_SIZE = 443, 
    SORT_MERGE = 444, SORT_MESSAGE = 445, SORT_MODE_SIZE = 446, SORT_RETURN = 447, 
    SOURCE = 448, SOURCE_COMPUTER = 449, SPACE = 450, SPACES = 451, SPECIAL_NAMES = 452, 
    STANDARD = 453, STANDARD_1 = 454, STANDARD_2 = 455, START = 456, STATUS = 457, 
    STOP = 458, STRING = 459, SUB_QUEUE_1 = 460, SUB_QUEUE_2 = 461, SUB_QUEUE_3 = 462, 
    SUBTRACT = 463, SUM = 464, SUPPRESS = 465, SYMBOL = 466, SYMBOLIC = 467, 
    SYNC = 468, SYNCHRONIZED = 469, TABLE = 470, TALLY = 471, TALLYING = 472, 
    TASK = 473, TAPE = 474, TERMINAL = 475, TERMINATE = 476, TEST = 477, 
    TEXT = 478, THAN = 479, THEN = 480, THREAD = 481, THREAD_LOCAL = 482, 
    THROUGH = 483, THRU = 484, TIME = 485, TIMER = 486, TIMES = 487, TITLE = 488, 
    TO = 489, TODAYS_DATE = 490, TODAYS_NAME = 491, TOP = 492, TRAILING = 493, 
    TRUE = 494, TRUNCATED = 495, TYPE = 496, TYPEDEF = 497, UNDERLINE = 498, 
    UNIT = 499, UNSTRING = 500, UNTIL = 501, UP = 502, UPON = 503, USAGE = 504, 
    USE = 505, USING = 506, VALUE = 507, VALUES = 508, VARYING = 509, VIRTUAL = 510, 
    WAIT = 511, WHEN = 512, WHEN_COMPILED = 513, WITH = 514, WORDS = 515, 
    WORKING_STORAGE = 516, WRITE = 517, YEAR = 518, YYYYMMDD = 519, YYYYDDD = 520, 
    ZERO = 521, ZERO_FILL = 522, ZEROS = 523, ZEROES = 524, AMPCHAR = 525, 
    ASTERISKCHAR = 526, DOUBLEASTERISKCHAR = 527, COLONCHAR = 528, COMMACHAR = 529, 
    COMMENTENTRYTAG = 530, COMMENTTAG = 531, DOLLARCHAR = 532, DOUBLEQUOTE = 533, 
    DOT_FS = 534, DOT = 535, EQUALCHAR = 536, EXECCICSTAG = 537, EXECSQLTAG = 538, 
    EXECSQLIMSTAG = 539, LESSTHANCHAR = 540, LESSTHANOREQUAL = 541, LPARENCHAR = 542, 
    MINUSCHAR = 543, MORETHANCHAR = 544, MORETHANOREQUAL = 545, NOTEQUALCHAR = 546, 
    PLUSCHAR = 547, SINGLEQUOTE = 548, RPARENCHAR = 549, SLASHCHAR = 550, 
    NONNUMERICLITERAL = 551, LEVEL_NUMBER_66 = 552, LEVEL_NUMBER_77 = 553, 
    LEVEL_NUMBER_88 = 554, INTEGERLITERAL = 555, NUMERICLITERAL = 556, IDENTIFIER = 557, 
    NEWLINE = 558, EXECCICSLINE = 559, EXECSQLIMSLINE = 560, EXECSQLLINE = 561, 
    COMMENTENTRYLINE = 562, COMMENTLINE = 563, WS = 564, SEPARATOR = 565
  };

  enum {
    RuleStartRule = 0, RuleCompilationUnit = 1, RuleProgramUnit = 2, RuleEndProgramStatement = 3, 
    RuleIdentificationDivision = 4, RuleIdentificationDivisionBody = 5, 
    RuleProgramIdParagraph = 6, RuleAuthorParagraph = 7, RuleInstallationParagraph = 8, 
    RuleDateWrittenParagraph = 9, RuleDateCompiledParagraph = 10, RuleSecurityParagraph = 11, 
    RuleRemarksParagraph = 12, RuleEnvironmentDivision = 13, RuleEnvironmentDivisionBody = 14, 
    RuleConfigurationSection = 15, RuleConfigurationSectionParagraph = 16, 
    RuleSourceComputerParagraph = 17, RuleObjectComputerParagraph = 18, 
    RuleObjectComputerClause = 19, RuleMemorySizeClause = 20, RuleDiskSizeClause = 21, 
    RuleCollatingSequenceClause = 22, RuleCollatingSequenceClauseAlphanumeric = 23, 
    RuleCollatingSequenceClauseNational = 24, RuleSegmentLimitClause = 25, 
    RuleCharacterSetClause = 26, RuleSpecialNamesParagraph = 27, RuleSpecialNameClause = 28, 
    RuleAlphabetClause = 29, RuleAlphabetClauseFormat1 = 30, RuleAlphabetLiterals = 31, 
    RuleAlphabetThrough = 32, RuleAlphabetAlso = 33, RuleAlphabetClauseFormat2 = 34, 
    RuleChannelClause = 35, RuleClassClause = 36, RuleClassClauseThrough = 37, 
    RuleClassClauseFrom = 38, RuleClassClauseTo = 39, RuleCurrencySignClause = 40, 
    RuleDecimalPointClause = 41, RuleDefaultComputationalSignClause = 42, 
    RuleDefaultDisplaySignClause = 43, RuleEnvironmentSwitchNameClause = 44, 
    RuleEnvironmentSwitchNameSpecialNamesStatusPhrase = 45, RuleOdtClause = 46, 
    RuleReserveNetworkClause = 47, RuleSymbolicCharactersClause = 48, RuleSymbolicCharacters = 49, 
    RuleInputOutputSection = 50, RuleInputOutputSectionParagraph = 51, RuleFileControlParagraph = 52, 
    RuleFileControlEntry = 53, RuleSelectClause = 54, RuleFileControlClause = 55, 
    RuleAssignClause = 56, RuleReserveClause = 57, RuleOrganizationClause = 58, 
    RulePaddingCharacterClause = 59, RuleRecordDelimiterClause = 60, RuleAccessModeClause = 61, 
    RuleRecordKeyClause = 62, RuleAlternateRecordKeyClause = 63, RulePasswordClause = 64, 
    RuleFileStatusClause = 65, RuleRelativeKeyClause = 66, RuleIoControlParagraph = 67, 
    RuleIoControlClause = 68, RuleRerunClause = 69, RuleRerunEveryRecords = 70, 
    RuleRerunEveryOf = 71, RuleRerunEveryClock = 72, RuleSameClause = 73, 
    RuleMultipleFileClause = 74, RuleMultipleFilePosition = 75, RuleCommitmentControlClause = 76, 
    RuleDataDivision = 77, RuleDataDivisionSection = 78, RuleFileSection = 79, 
    RuleFileDescriptionEntry = 80, RuleFileDescriptionEntryClause = 81, 
    RuleExternalClause = 82, RuleGlobalClause = 83, RuleBlockContainsClause = 84, 
    RuleBlockContainsTo = 85, RuleRecordContainsClause = 86, RuleRecordContainsClauseFormat1 = 87, 
    RuleRecordContainsClauseFormat2 = 88, RuleRecordContainsClauseFormat3 = 89, 
    RuleRecordContainsTo = 90, RuleLabelRecordsClause = 91, RuleValueOfClause = 92, 
    RuleValuePair = 93, RuleDataRecordsClause = 94, RuleLinageClause = 95, 
    RuleLinageAt = 96, RuleLinageFootingAt = 97, RuleLinageLinesAtTop = 98, 
    RuleLinageLinesAtBottom = 99, RuleRecordingModeClause = 100, RuleModeStatement = 101, 
    RuleCodeSetClause = 102, RuleReportClause = 103, RuleDataBaseSection = 104, 
    RuleDataBaseSectionEntry = 105, RuleWorkingStorageSection = 106, RuleLinkageSection = 107, 
    RuleCommunicationSection = 108, RuleCommunicationDescriptionEntry = 109, 
    RuleCommunicationDescriptionEntryFormat1 = 110, RuleCommunicationDescriptionEntryFormat2 = 111, 
    RuleCommunicationDescriptionEntryFormat3 = 112, RuleDestinationCountClause = 113, 
    RuleDestinationTableClause = 114, RuleEndKeyClause = 115, RuleErrorKeyClause = 116, 
    RuleMessageCountClause = 117, RuleMessageDateClause = 118, RuleMessageTimeClause = 119, 
    RuleStatusKeyClause = 120, RuleSymbolicDestinationClause = 121, RuleSymbolicQueueClause = 122, 
    RuleSymbolicSourceClause = 123, RuleSymbolicTerminalClause = 124, RuleSymbolicSubQueueClause = 125, 
    RuleTextLengthClause = 126, RuleLocalStorageSection = 127, RuleScreenSection = 128, 
    RuleScreenDescriptionEntry = 129, RuleScreenDescriptionBlankClause = 130, 
    RuleScreenDescriptionBellClause = 131, RuleScreenDescriptionBlinkClause = 132, 
    RuleScreenDescriptionEraseClause = 133, RuleScreenDescriptionLightClause = 134, 
    RuleScreenDescriptionGridClause = 135, RuleScreenDescriptionReverseVideoClause = 136, 
    RuleScreenDescriptionUnderlineClause = 137, RuleScreenDescriptionSizeClause = 138, 
    RuleScreenDescriptionLineClause = 139, RuleScreenDescriptionColumnClause = 140, 
    RuleScreenDescriptionForegroundColorClause = 141, RuleScreenDescriptionBackgroundColorClause = 142, 
    RuleScreenDescriptionControlClause = 143, RuleScreenDescriptionValueClause = 144, 
    RuleScreenDescriptionPictureClause = 145, RuleScreenDescriptionFromClause = 146, 
    RuleScreenDescriptionToClause = 147, RuleScreenDescriptionUsingClause = 148, 
    RuleScreenDescriptionUsageClause = 149, RuleScreenDescriptionBlankWhenZeroClause = 150, 
    RuleScreenDescriptionJustifiedClause = 151, RuleScreenDescriptionSignClause = 152, 
    RuleScreenDescriptionAutoClause = 153, RuleScreenDescriptionSecureClause = 154, 
    RuleScreenDescriptionRequiredClause = 155, RuleScreenDescriptionPromptClause = 156, 
    RuleScreenDescriptionPromptOccursClause = 157, RuleScreenDescriptionFullClause = 158, 
    RuleScreenDescriptionZeroFillClause = 159, RuleReportSection = 160, 
    RuleReportDescription = 161, RuleReportDescriptionEntry = 162, RuleReportDescriptionGlobalClause = 163, 
    RuleReportDescriptionPageLimitClause = 164, RuleReportDescriptionHeadingClause = 165, 
    RuleReportDescriptionFirstDetailClause = 166, RuleReportDescriptionLastDetailClause = 167, 
    RuleReportDescriptionFootingClause = 168, RuleReportGroupDescriptionEntry = 169, 
    RuleReportGroupDescriptionEntryFormat1 = 170, RuleReportGroupDescriptionEntryFormat2 = 171, 
    RuleReportGroupDescriptionEntryFormat3 = 172, RuleReportGroupBlankWhenZeroClause = 173, 
    RuleReportGroupColumnNumberClause = 174, RuleReportGroupIndicateClause = 175, 
    RuleReportGroupJustifiedClause = 176, RuleReportGroupLineNumberClause = 177, 
    RuleReportGroupLineNumberNextPage = 178, RuleReportGroupLineNumberPlus = 179, 
    RuleReportGroupNextGroupClause = 180, RuleReportGroupNextGroupPlus = 181, 
    RuleReportGroupNextGroupNextPage = 182, RuleReportGroupPictureClause = 183, 
    RuleReportGroupResetClause = 184, RuleReportGroupSignClause = 185, RuleReportGroupSourceClause = 186, 
    RuleReportGroupSumClause = 187, RuleReportGroupTypeClause = 188, RuleReportGroupTypeReportHeading = 189, 
    RuleReportGroupTypePageHeading = 190, RuleReportGroupTypeControlHeading = 191, 
    RuleReportGroupTypeDetail = 192, RuleReportGroupTypeControlFooting = 193, 
    RuleReportGroupUsageClause = 194, RuleReportGroupTypePageFooting = 195, 
    RuleReportGroupTypeReportFooting = 196, RuleReportGroupValueClause = 197, 
    RuleProgramLibrarySection = 198, RuleLibraryDescriptionEntry = 199, 
    RuleLibraryDescriptionEntryFormat1 = 200, RuleLibraryDescriptionEntryFormat2 = 201, 
    RuleLibraryAttributeClauseFormat1 = 202, RuleLibraryAttributeClauseFormat2 = 203, 
    RuleLibraryAttributeFunction = 204, RuleLibraryAttributeParameter = 205, 
    RuleLibraryAttributeTitle = 206, RuleLibraryEntryProcedureClauseFormat1 = 207, 
    RuleLibraryEntryProcedureClauseFormat2 = 208, RuleLibraryEntryProcedureForClause = 209, 
    RuleLibraryEntryProcedureGivingClause = 210, RuleLibraryEntryProcedureUsingClause = 211, 
    RuleLibraryEntryProcedureUsingName = 212, RuleLibraryEntryProcedureWithClause = 213, 
    RuleLibraryEntryProcedureWithName = 214, RuleLibraryIsCommonClause = 215, 
    RuleLibraryIsGlobalClause = 216, RuleDataDescriptionEntry = 217, RuleDataDescriptionEntryFormat1 = 218, 
    RuleDataDescriptionEntryFormat2 = 219, RuleDataDescriptionEntryFormat3 = 220, 
    RuleDataDescriptionEntryExecSql = 221, RuleDataAlignedClause = 222, 
    RuleDataBlankWhenZeroClause = 223, RuleDataCommonOwnLocalClause = 224, 
    RuleDataExternalClause = 225, RuleDataGlobalClause = 226, RuleDataIntegerStringClause = 227, 
    RuleDataJustifiedClause = 228, RuleDataOccursClause = 229, RuleDataOccursTo = 230, 
    RuleDataOccursSort = 231, RuleDataPictureClause = 232, RulePictureString = 233, 
    RulePictureChars = 234, RulePictureCardinality = 235, RuleDataReceivedByClause = 236, 
    RuleDataRecordAreaClause = 237, RuleDataRedefinesClause = 238, RuleDataRenamesClause = 239, 
    RuleDataSignClause = 240, RuleDataSynchronizedClause = 241, RuleDataThreadLocalClause = 242, 
    RuleDataTypeClause = 243, RuleDataTypeDefClause = 244, RuleDataUsageClause = 245, 
    RuleDataUsingClause = 246, RuleDataValueClause = 247, RuleDataValueInterval = 248, 
    RuleDataValueIntervalFrom = 249, RuleDataValueIntervalTo = 250, RuleDataWithLowerBoundsClause = 251, 
    RuleProcedureDivision = 252, RuleProcedureDivisionUsingClause = 253, 
    RuleProcedureDivisionGivingClause = 254, RuleProcedureDivisionUsingParameter = 255, 
    RuleProcedureDivisionByReferencePhrase = 256, RuleProcedureDivisionByReference = 257, 
    RuleProcedureDivisionByValuePhrase = 258, RuleProcedureDivisionByValue = 259, 
    RuleProcedureDeclaratives = 260, RuleProcedureDeclarative = 261, RuleProcedureSectionHeader = 262, 
    RuleProcedureDivisionBody = 263, RuleCopyDirective = 264, RuleCopyDirectiveSource = 265, 
    RuleCopyDirectiveLibrary = 266, RuleCopyDirectiveLiteral = 267, RuleCopyDirectiveDigits = 268, 
    RuleCopyDirectiveFilename = 269, RuleCopyDirectiveReplacingPhrase = 270, 
    RuleCopyDirectiveReplaceClause = 271, RuleCopyDirectiveDirectoryPhrase = 272, 
    RuleCopyDirectiveFamilyPhrase = 273, RuleCopyDirectiveReplaceable = 274, 
    RuleCopyDirectivePseudoText = 275, RuleCopyDirectivePseudoDelimiter = 276, 
    RuleProcedureSection = 277, RuleParagraphs = 278, RuleParagraph = 279, 
    RuleSentence = 280, RuleStatement = 281, RuleAcceptStatement = 282, 
    RuleAcceptFromDateStatement = 283, RuleAcceptFromMnemonicStatement = 284, 
    RuleAcceptFromEscapeKeyStatement = 285, RuleAcceptMessageCountStatement = 286, 
    RuleAddStatement = 287, RuleAddToStatement = 288, RuleAddToGivingStatement = 289, 
    RuleAddCorrespondingStatement = 290, RuleAddFrom = 291, RuleAddTo = 292, 
    RuleAddToGiving = 293, RuleAddGiving = 294, RuleAlteredGoTo = 295, RuleAlterStatement = 296, 
    RuleAlterProceedTo = 297, RuleCallStatement = 298, RuleCallUsingPhrase = 299, 
    RuleCallUsingParameter = 300, RuleCallByReferencePhrase = 301, RuleCallByReference = 302, 
    RuleCallByValuePhrase = 303, RuleCallByValue = 304, RuleCallByContentPhrase = 305, 
    RuleCallByContent = 306, RuleCallGivingPhrase = 307, RuleCancelStatement = 308, 
    RuleCancelCall = 309, RuleCloseStatement = 310, RuleCloseFile = 311, 
    RuleCloseReelUnitStatement = 312, RuleCloseRelativeStatement = 313, 
    RuleClosePortFileIOStatement = 314, RuleClosePortFileIOUsing = 315, 
    RuleClosePortFileIOUsingCloseDisposition = 316, RuleClosePortFileIOUsingAssociatedData = 317, 
    RuleClosePortFileIOUsingAssociatedDataLength = 318, RuleComputeStatement = 319, 
    RuleComputeStore = 320, RuleContinueStatement = 321, RuleDeleteStatement = 322, 
    RuleDisableStatement = 323, RuleDisplayStatement = 324, RuleDisplayOperand = 325, 
    RuleDisplayAt = 326, RuleDisplayUpon = 327, RuleDisplayWith = 328, RuleDivideStatement = 329, 
    RuleDivideIntoStatement = 330, RuleDivideIntoGivingStatement = 331, 
    RuleDivideByGivingStatement = 332, RuleDivideGivingPhrase = 333, RuleDivideInto = 334, 
    RuleDivideGiving = 335, RuleDivideRemainder = 336, RuleEnableStatement = 337, 
    RuleEntryStatement = 338, RuleEvaluateStatement = 339, RuleEvaluateSelect = 340, 
    RuleEvaluateAlsoSelect = 341, RuleEvaluateWhenPhrase = 342, RuleEvaluateWhen = 343, 
    RuleEvaluateCondition = 344, RuleEvaluateThrough = 345, RuleEvaluateAlsoCondition = 346, 
    RuleEvaluateWhenOther = 347, RuleEvaluateValue = 348, RuleExecCicsStatement = 349, 
    RuleExecSqlStatement = 350, RuleExecSqlImsStatement = 351, RuleExhibitStatement = 352, 
    RuleExhibitOperand = 353, RuleExitStatement = 354, RuleGenerateStatement = 355, 
    RuleGobackStatement = 356, RuleGoToStatement = 357, RuleGoToStatementSimple = 358, 
    RuleGoToDependingOnStatement = 359, RuleIfStatement = 360, RuleIfThen = 361, 
    RuleIfElse = 362, RuleInitializeStatement = 363, RuleInitializeReplacingPhrase = 364, 
    RuleInitializeReplacingBy = 365, RuleInitiateStatement = 366, RuleInspectStatement = 367, 
    RuleInspectTallyingPhrase = 368, RuleInspectReplacingPhrase = 369, RuleInspectTallyingReplacingPhrase = 370, 
    RuleInspectConvertingPhrase = 371, RuleInspectFor = 372, RuleInspectCharacters = 373, 
    RuleInspectReplacingCharacters = 374, RuleInspectAllLeadings = 375, 
    RuleInspectReplacingAllLeadings = 376, RuleInspectAllLeading = 377, 
    RuleInspectReplacingAllLeading = 378, RuleInspectBy = 379, RuleInspectTo = 380, 
    RuleInspectBeforeAfter = 381, RuleMergeStatement = 382, RuleMergeOnKeyClause = 383, 
    RuleMergeCollatingSequencePhrase = 384, RuleMergeCollatingAlphanumeric = 385, 
    RuleMergeCollatingNational = 386, RuleMergeUsing = 387, RuleMergeOutputProcedurePhrase = 388, 
    RuleMergeOutputThrough = 389, RuleMergeGivingPhrase = 390, RuleMergeGiving = 391, 
    RuleMoveStatement = 392, RuleMoveToStatement = 393, RuleMoveToSendingArea = 394, 
    RuleMoveCorrespondingToStatement = 395, RuleMoveCorrespondingToSendingArea = 396, 
    RuleMultiplyStatement = 397, RuleMultiplyRegular = 398, RuleMultiplyRegularOperand = 399, 
    RuleMultiplyGiving = 400, RuleMultiplyGivingOperand = 401, RuleMultiplyGivingResult = 402, 
    RuleOpenStatement = 403, RuleOpenInputStatement = 404, RuleOpenInput = 405, 
    RuleOpenOutputStatement = 406, RuleOpenOutput = 407, RuleOpenIOStatement = 408, 
    RuleOpenExtendStatement = 409, RulePerformStatement = 410, RulePerformInlineStatement = 411, 
    RulePerformProcedureStatement = 412, RulePerformType = 413, RulePerformTimes = 414, 
    RulePerformUntil = 415, RulePerformVarying = 416, RulePerformVaryingClause = 417, 
    RulePerformVaryingPhrase = 418, RulePerformAfter = 419, RulePerformFrom = 420, 
    RulePerformBy = 421, RulePerformTestClause = 422, RulePurgeStatement = 423, 
    RuleReadStatement = 424, RuleReadInto = 425, RuleReadWith = 426, RuleReadKey = 427, 
    RuleReceiveStatement = 428, RuleReceiveFromStatement = 429, RuleReceiveFrom = 430, 
    RuleReceiveIntoStatement = 431, RuleReceiveNoData = 432, RuleReceiveWithData = 433, 
    RuleReceiveBefore = 434, RuleReceiveWith = 435, RuleReceiveThread = 436, 
    RuleReceiveSize = 437, RuleReceiveStatus = 438, RuleReleaseStatement = 439, 
    RuleReturnStatement = 440, RuleReturnInto = 441, RuleRewriteStatement = 442, 
    RuleRewriteFrom = 443, RuleSearchStatement = 444, RuleSearchVarying = 445, 
    RuleSearchWhen = 446, RuleSendStatement = 447, RuleSendStatementSync = 448, 
    RuleSendStatementAsync = 449, RuleSendFromPhrase = 450, RuleSendWithPhrase = 451, 
    RuleSendReplacingPhrase = 452, RuleSendAdvancingPhrase = 453, RuleSendAdvancingPage = 454, 
    RuleSendAdvancingLines = 455, RuleSendAdvancingMnemonic = 456, RuleSetStatement = 457, 
    RuleSetToStatement = 458, RuleSetUpDownByStatement = 459, RuleSetTo = 460, 
    RuleSetToValue = 461, RuleSetByValue = 462, RuleSortStatement = 463, 
    RuleSortOnKeyClause = 464, RuleSortDuplicatesPhrase = 465, RuleSortCollatingSequencePhrase = 466, 
    RuleSortCollatingAlphanumeric = 467, RuleSortCollatingNational = 468, 
    RuleSortInputProcedurePhrase = 469, RuleSortInputThrough = 470, RuleSortUsing = 471, 
    RuleSortOutputProcedurePhrase = 472, RuleSortOutputThrough = 473, RuleSortGivingPhrase = 474, 
    RuleSortGiving = 475, RuleStartStatement = 476, RuleStartKey = 477, 
    RuleStopStatement = 478, RuleStringStatement = 479, RuleStringSendingPhrase = 480, 
    RuleStringSending = 481, RuleStringDelimitedByPhrase = 482, RuleStringForPhrase = 483, 
    RuleStringIntoPhrase = 484, RuleStringWithPointerPhrase = 485, RuleSubtractStatement = 486, 
    RuleSubtractFromStatement = 487, RuleSubtractFromGivingStatement = 488, 
    RuleSubtractCorrespondingStatement = 489, RuleSubtractSubtrahend = 490, 
    RuleSubtractMinuend = 491, RuleSubtractMinuendGiving = 492, RuleSubtractGiving = 493, 
    RuleSubtractMinuendCorresponding = 494, RuleTerminateStatement = 495, 
    RuleUnstringStatement = 496, RuleUnstringSendingPhrase = 497, RuleUnstringDelimitedByPhrase = 498, 
    RuleUnstringOrAllPhrase = 499, RuleUnstringIntoPhrase = 500, RuleUnstringInto = 501, 
    RuleUnstringDelimiterIn = 502, RuleUnstringCountIn = 503, RuleUnstringWithPointerPhrase = 504, 
    RuleUnstringTallyingPhrase = 505, RuleUseStatement = 506, RuleUseAfterClause = 507, 
    RuleUseAfterOn = 508, RuleUseDebugClause = 509, RuleUseDebugOn = 510, 
    RuleWriteStatement = 511, RuleWriteFromPhrase = 512, RuleWriteAdvancingPhrase = 513, 
    RuleWriteAdvancingPage = 514, RuleWriteAdvancingLines = 515, RuleWriteAdvancingMnemonic = 516, 
    RuleWriteAtEndOfPagePhrase = 517, RuleWriteNotAtEndOfPagePhrase = 518, 
    RuleAtEndPhrase = 519, RuleNotAtEndPhrase = 520, RuleInvalidKeyPhrase = 521, 
    RuleNotInvalidKeyPhrase = 522, RuleOnOverflowPhrase = 523, RuleNotOnOverflowPhrase = 524, 
    RuleOnSizeErrorPhrase = 525, RuleNotOnSizeErrorPhrase = 526, RuleOnExceptionClause = 527, 
    RuleNotOnExceptionClause = 528, RuleArithmeticExpression = 529, RulePlusMinus = 530, 
    RuleMultDivs = 531, RuleMultDiv = 532, RulePowers = 533, RulePower = 534, 
    RuleBasis = 535, RuleCondition = 536, RuleAndOrCondition = 537, RuleCombinableCondition = 538, 
    RuleSimpleCondition = 539, RuleClassCondition = 540, RuleConditionNameReference = 541, 
    RuleConditionNameSubscriptReference = 542, RuleRelationCondition = 543, 
    RuleRelationSignCondition = 544, RuleRelationArithmeticComparison = 545, 
    RuleRelationCombinedComparison = 546, RuleRelationCombinedCondition = 547, 
    RuleRelationalOperator = 548, RuleAbbreviation = 549, RuleIdentifier = 550, 
    RuleTableCall = 551, RuleFunctionCall = 552, RuleReferenceModifier = 553, 
    RuleCharacterPosition = 554, RuleLength = 555, RuleSubscript_ = 556, 
    RuleArgument = 557, RuleQualifiedDataName = 558, RuleQualifiedDataNameFormat1 = 559, 
    RuleQualifiedDataNameFormat2 = 560, RuleQualifiedDataNameFormat3 = 561, 
    RuleQualifiedDataNameFormat4 = 562, RuleQualifiedInData = 563, RuleInData = 564, 
    RuleInFile = 565, RuleInMnemonic = 566, RuleInSection = 567, RuleInLibrary = 568, 
    RuleInTable = 569, RuleAlphabetName = 570, RuleAssignmentName = 571, 
    RuleBasisName = 572, RuleCdName = 573, RuleClassName = 574, RuleComputerName = 575, 
    RuleConditionName = 576, RuleDataName = 577, RuleDataDescName = 578, 
    RuleEnvironmentName = 579, RuleFileName = 580, RuleFunctionName = 581, 
    RuleIndexName = 582, RuleLanguageName = 583, RuleLibraryName = 584, 
    RuleLocalName = 585, RuleMnemonicName = 586, RuleParagraphName = 587, 
    RuleProcedureName = 588, RuleProgramName = 589, RuleRecordName = 590, 
    RuleReportName = 591, RuleRoutineName = 592, RuleScreenName = 593, RuleSectionName = 594, 
    RuleSystemName = 595, RuleSymbolicCharacter = 596, RuleTextName = 597, 
    RuleCobolWord = 598, RuleLiteral = 599, RuleBooleanLiteral = 600, RuleNumericLiteral = 601, 
    RuleIntegerLiteral = 602, RuleCicsDfhRespLiteral = 603, RuleCicsDfhValueLiteral = 604, 
    RuleFigurativeConstant = 605, RuleSpecialRegister = 606, RuleCommentEntry = 607
  };

  explicit Cobol85Parser(antlr4::TokenStream *input);

  Cobol85Parser(antlr4::TokenStream *input, const antlr4::atn::ParserATNSimulatorOptions &options);

  ~Cobol85Parser() override;

  std::string getGrammarFileName() const override;

  const antlr4::atn::ATN& getATN() const override;

  const std::vector<std::string>& getRuleNames() const override;

  const antlr4::dfa::Vocabulary& getVocabulary() const override;

  antlr4::atn::SerializedATNView getSerializedATN() const override;


      bool copyDirectiveQuotedLiteral() {
          const auto text = _input->LT(1)->getText();
          return !text.empty() && (text[0] == '\'' || text[0] == '"' || text[0] == 'X' || text[0] == 'x');
      }


  class StartRuleContext;
  class CompilationUnitContext;
  class ProgramUnitContext;
  class EndProgramStatementContext;
  class IdentificationDivisionContext;
  class IdentificationDivisionBodyContext;
  class ProgramIdParagraphContext;
  class AuthorParagraphContext;
  class InstallationParagraphContext;
  class DateWrittenParagraphContext;
  class DateCompiledParagraphContext;
  class SecurityParagraphContext;
  class RemarksParagraphContext;
  class EnvironmentDivisionContext;
  class EnvironmentDivisionBodyContext;
  class ConfigurationSectionContext;
  class ConfigurationSectionParagraphContext;
  class SourceComputerParagraphContext;
  class ObjectComputerParagraphContext;
  class ObjectComputerClauseContext;
  class MemorySizeClauseContext;
  class DiskSizeClauseContext;
  class CollatingSequenceClauseContext;
  class CollatingSequenceClauseAlphanumericContext;
  class CollatingSequenceClauseNationalContext;
  class SegmentLimitClauseContext;
  class CharacterSetClauseContext;
  class SpecialNamesParagraphContext;
  class SpecialNameClauseContext;
  class AlphabetClauseContext;
  class AlphabetClauseFormat1Context;
  class AlphabetLiteralsContext;
  class AlphabetThroughContext;
  class AlphabetAlsoContext;
  class AlphabetClauseFormat2Context;
  class ChannelClauseContext;
  class ClassClauseContext;
  class ClassClauseThroughContext;
  class ClassClauseFromContext;
  class ClassClauseToContext;
  class CurrencySignClauseContext;
  class DecimalPointClauseContext;
  class DefaultComputationalSignClauseContext;
  class DefaultDisplaySignClauseContext;
  class EnvironmentSwitchNameClauseContext;
  class EnvironmentSwitchNameSpecialNamesStatusPhraseContext;
  class OdtClauseContext;
  class ReserveNetworkClauseContext;
  class SymbolicCharactersClauseContext;
  class SymbolicCharactersContext;
  class InputOutputSectionContext;
  class InputOutputSectionParagraphContext;
  class FileControlParagraphContext;
  class FileControlEntryContext;
  class SelectClauseContext;
  class FileControlClauseContext;
  class AssignClauseContext;
  class ReserveClauseContext;
  class OrganizationClauseContext;
  class PaddingCharacterClauseContext;
  class RecordDelimiterClauseContext;
  class AccessModeClauseContext;
  class RecordKeyClauseContext;
  class AlternateRecordKeyClauseContext;
  class PasswordClauseContext;
  class FileStatusClauseContext;
  class RelativeKeyClauseContext;
  class IoControlParagraphContext;
  class IoControlClauseContext;
  class RerunClauseContext;
  class RerunEveryRecordsContext;
  class RerunEveryOfContext;
  class RerunEveryClockContext;
  class SameClauseContext;
  class MultipleFileClauseContext;
  class MultipleFilePositionContext;
  class CommitmentControlClauseContext;
  class DataDivisionContext;
  class DataDivisionSectionContext;
  class FileSectionContext;
  class FileDescriptionEntryContext;
  class FileDescriptionEntryClauseContext;
  class ExternalClauseContext;
  class GlobalClauseContext;
  class BlockContainsClauseContext;
  class BlockContainsToContext;
  class RecordContainsClauseContext;
  class RecordContainsClauseFormat1Context;
  class RecordContainsClauseFormat2Context;
  class RecordContainsClauseFormat3Context;
  class RecordContainsToContext;
  class LabelRecordsClauseContext;
  class ValueOfClauseContext;
  class ValuePairContext;
  class DataRecordsClauseContext;
  class LinageClauseContext;
  class LinageAtContext;
  class LinageFootingAtContext;
  class LinageLinesAtTopContext;
  class LinageLinesAtBottomContext;
  class RecordingModeClauseContext;
  class ModeStatementContext;
  class CodeSetClauseContext;
  class ReportClauseContext;
  class DataBaseSectionContext;
  class DataBaseSectionEntryContext;
  class WorkingStorageSectionContext;
  class LinkageSectionContext;
  class CommunicationSectionContext;
  class CommunicationDescriptionEntryContext;
  class CommunicationDescriptionEntryFormat1Context;
  class CommunicationDescriptionEntryFormat2Context;
  class CommunicationDescriptionEntryFormat3Context;
  class DestinationCountClauseContext;
  class DestinationTableClauseContext;
  class EndKeyClauseContext;
  class ErrorKeyClauseContext;
  class MessageCountClauseContext;
  class MessageDateClauseContext;
  class MessageTimeClauseContext;
  class StatusKeyClauseContext;
  class SymbolicDestinationClauseContext;
  class SymbolicQueueClauseContext;
  class SymbolicSourceClauseContext;
  class SymbolicTerminalClauseContext;
  class SymbolicSubQueueClauseContext;
  class TextLengthClauseContext;
  class LocalStorageSectionContext;
  class ScreenSectionContext;
  class ScreenDescriptionEntryContext;
  class ScreenDescriptionBlankClauseContext;
  class ScreenDescriptionBellClauseContext;
  class ScreenDescriptionBlinkClauseContext;
  class ScreenDescriptionEraseClauseContext;
  class ScreenDescriptionLightClauseContext;
  class ScreenDescriptionGridClauseContext;
  class ScreenDescriptionReverseVideoClauseContext;
  class ScreenDescriptionUnderlineClauseContext;
  class ScreenDescriptionSizeClauseContext;
  class ScreenDescriptionLineClauseContext;
  class ScreenDescriptionColumnClauseContext;
  class ScreenDescriptionForegroundColorClauseContext;
  class ScreenDescriptionBackgroundColorClauseContext;
  class ScreenDescriptionControlClauseContext;
  class ScreenDescriptionValueClauseContext;
  class ScreenDescriptionPictureClauseContext;
  class ScreenDescriptionFromClauseContext;
  class ScreenDescriptionToClauseContext;
  class ScreenDescriptionUsingClauseContext;
  class ScreenDescriptionUsageClauseContext;
  class ScreenDescriptionBlankWhenZeroClauseContext;
  class ScreenDescriptionJustifiedClauseContext;
  class ScreenDescriptionSignClauseContext;
  class ScreenDescriptionAutoClauseContext;
  class ScreenDescriptionSecureClauseContext;
  class ScreenDescriptionRequiredClauseContext;
  class ScreenDescriptionPromptClauseContext;
  class ScreenDescriptionPromptOccursClauseContext;
  class ScreenDescriptionFullClauseContext;
  class ScreenDescriptionZeroFillClauseContext;
  class ReportSectionContext;
  class ReportDescriptionContext;
  class ReportDescriptionEntryContext;
  class ReportDescriptionGlobalClauseContext;
  class ReportDescriptionPageLimitClauseContext;
  class ReportDescriptionHeadingClauseContext;
  class ReportDescriptionFirstDetailClauseContext;
  class ReportDescriptionLastDetailClauseContext;
  class ReportDescriptionFootingClauseContext;
  class ReportGroupDescriptionEntryContext;
  class ReportGroupDescriptionEntryFormat1Context;
  class ReportGroupDescriptionEntryFormat2Context;
  class ReportGroupDescriptionEntryFormat3Context;
  class ReportGroupBlankWhenZeroClauseContext;
  class ReportGroupColumnNumberClauseContext;
  class ReportGroupIndicateClauseContext;
  class ReportGroupJustifiedClauseContext;
  class ReportGroupLineNumberClauseContext;
  class ReportGroupLineNumberNextPageContext;
  class ReportGroupLineNumberPlusContext;
  class ReportGroupNextGroupClauseContext;
  class ReportGroupNextGroupPlusContext;
  class ReportGroupNextGroupNextPageContext;
  class ReportGroupPictureClauseContext;
  class ReportGroupResetClauseContext;
  class ReportGroupSignClauseContext;
  class ReportGroupSourceClauseContext;
  class ReportGroupSumClauseContext;
  class ReportGroupTypeClauseContext;
  class ReportGroupTypeReportHeadingContext;
  class ReportGroupTypePageHeadingContext;
  class ReportGroupTypeControlHeadingContext;
  class ReportGroupTypeDetailContext;
  class ReportGroupTypeControlFootingContext;
  class ReportGroupUsageClauseContext;
  class ReportGroupTypePageFootingContext;
  class ReportGroupTypeReportFootingContext;
  class ReportGroupValueClauseContext;
  class ProgramLibrarySectionContext;
  class LibraryDescriptionEntryContext;
  class LibraryDescriptionEntryFormat1Context;
  class LibraryDescriptionEntryFormat2Context;
  class LibraryAttributeClauseFormat1Context;
  class LibraryAttributeClauseFormat2Context;
  class LibraryAttributeFunctionContext;
  class LibraryAttributeParameterContext;
  class LibraryAttributeTitleContext;
  class LibraryEntryProcedureClauseFormat1Context;
  class LibraryEntryProcedureClauseFormat2Context;
  class LibraryEntryProcedureForClauseContext;
  class LibraryEntryProcedureGivingClauseContext;
  class LibraryEntryProcedureUsingClauseContext;
  class LibraryEntryProcedureUsingNameContext;
  class LibraryEntryProcedureWithClauseContext;
  class LibraryEntryProcedureWithNameContext;
  class LibraryIsCommonClauseContext;
  class LibraryIsGlobalClauseContext;
  class DataDescriptionEntryContext;
  class DataDescriptionEntryFormat1Context;
  class DataDescriptionEntryFormat2Context;
  class DataDescriptionEntryFormat3Context;
  class DataDescriptionEntryExecSqlContext;
  class DataAlignedClauseContext;
  class DataBlankWhenZeroClauseContext;
  class DataCommonOwnLocalClauseContext;
  class DataExternalClauseContext;
  class DataGlobalClauseContext;
  class DataIntegerStringClauseContext;
  class DataJustifiedClauseContext;
  class DataOccursClauseContext;
  class DataOccursToContext;
  class DataOccursSortContext;
  class DataPictureClauseContext;
  class PictureStringContext;
  class PictureCharsContext;
  class PictureCardinalityContext;
  class DataReceivedByClauseContext;
  class DataRecordAreaClauseContext;
  class DataRedefinesClauseContext;
  class DataRenamesClauseContext;
  class DataSignClauseContext;
  class DataSynchronizedClauseContext;
  class DataThreadLocalClauseContext;
  class DataTypeClauseContext;
  class DataTypeDefClauseContext;
  class DataUsageClauseContext;
  class DataUsingClauseContext;
  class DataValueClauseContext;
  class DataValueIntervalContext;
  class DataValueIntervalFromContext;
  class DataValueIntervalToContext;
  class DataWithLowerBoundsClauseContext;
  class ProcedureDivisionContext;
  class ProcedureDivisionUsingClauseContext;
  class ProcedureDivisionGivingClauseContext;
  class ProcedureDivisionUsingParameterContext;
  class ProcedureDivisionByReferencePhraseContext;
  class ProcedureDivisionByReferenceContext;
  class ProcedureDivisionByValuePhraseContext;
  class ProcedureDivisionByValueContext;
  class ProcedureDeclarativesContext;
  class ProcedureDeclarativeContext;
  class ProcedureSectionHeaderContext;
  class ProcedureDivisionBodyContext;
  class CopyDirectiveContext;
  class CopyDirectiveSourceContext;
  class CopyDirectiveLibraryContext;
  class CopyDirectiveLiteralContext;
  class CopyDirectiveDigitsContext;
  class CopyDirectiveFilenameContext;
  class CopyDirectiveReplacingPhraseContext;
  class CopyDirectiveReplaceClauseContext;
  class CopyDirectiveDirectoryPhraseContext;
  class CopyDirectiveFamilyPhraseContext;
  class CopyDirectiveReplaceableContext;
  class CopyDirectivePseudoTextContext;
  class CopyDirectivePseudoDelimiterContext;
  class ProcedureSectionContext;
  class ParagraphsContext;
  class ParagraphContext;
  class SentenceContext;
  class StatementContext;
  class AcceptStatementContext;
  class AcceptFromDateStatementContext;
  class AcceptFromMnemonicStatementContext;
  class AcceptFromEscapeKeyStatementContext;
  class AcceptMessageCountStatementContext;
  class AddStatementContext;
  class AddToStatementContext;
  class AddToGivingStatementContext;
  class AddCorrespondingStatementContext;
  class AddFromContext;
  class AddToContext;
  class AddToGivingContext;
  class AddGivingContext;
  class AlteredGoToContext;
  class AlterStatementContext;
  class AlterProceedToContext;
  class CallStatementContext;
  class CallUsingPhraseContext;
  class CallUsingParameterContext;
  class CallByReferencePhraseContext;
  class CallByReferenceContext;
  class CallByValuePhraseContext;
  class CallByValueContext;
  class CallByContentPhraseContext;
  class CallByContentContext;
  class CallGivingPhraseContext;
  class CancelStatementContext;
  class CancelCallContext;
  class CloseStatementContext;
  class CloseFileContext;
  class CloseReelUnitStatementContext;
  class CloseRelativeStatementContext;
  class ClosePortFileIOStatementContext;
  class ClosePortFileIOUsingContext;
  class ClosePortFileIOUsingCloseDispositionContext;
  class ClosePortFileIOUsingAssociatedDataContext;
  class ClosePortFileIOUsingAssociatedDataLengthContext;
  class ComputeStatementContext;
  class ComputeStoreContext;
  class ContinueStatementContext;
  class DeleteStatementContext;
  class DisableStatementContext;
  class DisplayStatementContext;
  class DisplayOperandContext;
  class DisplayAtContext;
  class DisplayUponContext;
  class DisplayWithContext;
  class DivideStatementContext;
  class DivideIntoStatementContext;
  class DivideIntoGivingStatementContext;
  class DivideByGivingStatementContext;
  class DivideGivingPhraseContext;
  class DivideIntoContext;
  class DivideGivingContext;
  class DivideRemainderContext;
  class EnableStatementContext;
  class EntryStatementContext;
  class EvaluateStatementContext;
  class EvaluateSelectContext;
  class EvaluateAlsoSelectContext;
  class EvaluateWhenPhraseContext;
  class EvaluateWhenContext;
  class EvaluateConditionContext;
  class EvaluateThroughContext;
  class EvaluateAlsoConditionContext;
  class EvaluateWhenOtherContext;
  class EvaluateValueContext;
  class ExecCicsStatementContext;
  class ExecSqlStatementContext;
  class ExecSqlImsStatementContext;
  class ExhibitStatementContext;
  class ExhibitOperandContext;
  class ExitStatementContext;
  class GenerateStatementContext;
  class GobackStatementContext;
  class GoToStatementContext;
  class GoToStatementSimpleContext;
  class GoToDependingOnStatementContext;
  class IfStatementContext;
  class IfThenContext;
  class IfElseContext;
  class InitializeStatementContext;
  class InitializeReplacingPhraseContext;
  class InitializeReplacingByContext;
  class InitiateStatementContext;
  class InspectStatementContext;
  class InspectTallyingPhraseContext;
  class InspectReplacingPhraseContext;
  class InspectTallyingReplacingPhraseContext;
  class InspectConvertingPhraseContext;
  class InspectForContext;
  class InspectCharactersContext;
  class InspectReplacingCharactersContext;
  class InspectAllLeadingsContext;
  class InspectReplacingAllLeadingsContext;
  class InspectAllLeadingContext;
  class InspectReplacingAllLeadingContext;
  class InspectByContext;
  class InspectToContext;
  class InspectBeforeAfterContext;
  class MergeStatementContext;
  class MergeOnKeyClauseContext;
  class MergeCollatingSequencePhraseContext;
  class MergeCollatingAlphanumericContext;
  class MergeCollatingNationalContext;
  class MergeUsingContext;
  class MergeOutputProcedurePhraseContext;
  class MergeOutputThroughContext;
  class MergeGivingPhraseContext;
  class MergeGivingContext;
  class MoveStatementContext;
  class MoveToStatementContext;
  class MoveToSendingAreaContext;
  class MoveCorrespondingToStatementContext;
  class MoveCorrespondingToSendingAreaContext;
  class MultiplyStatementContext;
  class MultiplyRegularContext;
  class MultiplyRegularOperandContext;
  class MultiplyGivingContext;
  class MultiplyGivingOperandContext;
  class MultiplyGivingResultContext;
  class OpenStatementContext;
  class OpenInputStatementContext;
  class OpenInputContext;
  class OpenOutputStatementContext;
  class OpenOutputContext;
  class OpenIOStatementContext;
  class OpenExtendStatementContext;
  class PerformStatementContext;
  class PerformInlineStatementContext;
  class PerformProcedureStatementContext;
  class PerformTypeContext;
  class PerformTimesContext;
  class PerformUntilContext;
  class PerformVaryingContext;
  class PerformVaryingClauseContext;
  class PerformVaryingPhraseContext;
  class PerformAfterContext;
  class PerformFromContext;
  class PerformByContext;
  class PerformTestClauseContext;
  class PurgeStatementContext;
  class ReadStatementContext;
  class ReadIntoContext;
  class ReadWithContext;
  class ReadKeyContext;
  class ReceiveStatementContext;
  class ReceiveFromStatementContext;
  class ReceiveFromContext;
  class ReceiveIntoStatementContext;
  class ReceiveNoDataContext;
  class ReceiveWithDataContext;
  class ReceiveBeforeContext;
  class ReceiveWithContext;
  class ReceiveThreadContext;
  class ReceiveSizeContext;
  class ReceiveStatusContext;
  class ReleaseStatementContext;
  class ReturnStatementContext;
  class ReturnIntoContext;
  class RewriteStatementContext;
  class RewriteFromContext;
  class SearchStatementContext;
  class SearchVaryingContext;
  class SearchWhenContext;
  class SendStatementContext;
  class SendStatementSyncContext;
  class SendStatementAsyncContext;
  class SendFromPhraseContext;
  class SendWithPhraseContext;
  class SendReplacingPhraseContext;
  class SendAdvancingPhraseContext;
  class SendAdvancingPageContext;
  class SendAdvancingLinesContext;
  class SendAdvancingMnemonicContext;
  class SetStatementContext;
  class SetToStatementContext;
  class SetUpDownByStatementContext;
  class SetToContext;
  class SetToValueContext;
  class SetByValueContext;
  class SortStatementContext;
  class SortOnKeyClauseContext;
  class SortDuplicatesPhraseContext;
  class SortCollatingSequencePhraseContext;
  class SortCollatingAlphanumericContext;
  class SortCollatingNationalContext;
  class SortInputProcedurePhraseContext;
  class SortInputThroughContext;
  class SortUsingContext;
  class SortOutputProcedurePhraseContext;
  class SortOutputThroughContext;
  class SortGivingPhraseContext;
  class SortGivingContext;
  class StartStatementContext;
  class StartKeyContext;
  class StopStatementContext;
  class StringStatementContext;
  class StringSendingPhraseContext;
  class StringSendingContext;
  class StringDelimitedByPhraseContext;
  class StringForPhraseContext;
  class StringIntoPhraseContext;
  class StringWithPointerPhraseContext;
  class SubtractStatementContext;
  class SubtractFromStatementContext;
  class SubtractFromGivingStatementContext;
  class SubtractCorrespondingStatementContext;
  class SubtractSubtrahendContext;
  class SubtractMinuendContext;
  class SubtractMinuendGivingContext;
  class SubtractGivingContext;
  class SubtractMinuendCorrespondingContext;
  class TerminateStatementContext;
  class UnstringStatementContext;
  class UnstringSendingPhraseContext;
  class UnstringDelimitedByPhraseContext;
  class UnstringOrAllPhraseContext;
  class UnstringIntoPhraseContext;
  class UnstringIntoContext;
  class UnstringDelimiterInContext;
  class UnstringCountInContext;
  class UnstringWithPointerPhraseContext;
  class UnstringTallyingPhraseContext;
  class UseStatementContext;
  class UseAfterClauseContext;
  class UseAfterOnContext;
  class UseDebugClauseContext;
  class UseDebugOnContext;
  class WriteStatementContext;
  class WriteFromPhraseContext;
  class WriteAdvancingPhraseContext;
  class WriteAdvancingPageContext;
  class WriteAdvancingLinesContext;
  class WriteAdvancingMnemonicContext;
  class WriteAtEndOfPagePhraseContext;
  class WriteNotAtEndOfPagePhraseContext;
  class AtEndPhraseContext;
  class NotAtEndPhraseContext;
  class InvalidKeyPhraseContext;
  class NotInvalidKeyPhraseContext;
  class OnOverflowPhraseContext;
  class NotOnOverflowPhraseContext;
  class OnSizeErrorPhraseContext;
  class NotOnSizeErrorPhraseContext;
  class OnExceptionClauseContext;
  class NotOnExceptionClauseContext;
  class ArithmeticExpressionContext;
  class PlusMinusContext;
  class MultDivsContext;
  class MultDivContext;
  class PowersContext;
  class PowerContext;
  class BasisContext;
  class ConditionContext;
  class AndOrConditionContext;
  class CombinableConditionContext;
  class SimpleConditionContext;
  class ClassConditionContext;
  class ConditionNameReferenceContext;
  class ConditionNameSubscriptReferenceContext;
  class RelationConditionContext;
  class RelationSignConditionContext;
  class RelationArithmeticComparisonContext;
  class RelationCombinedComparisonContext;
  class RelationCombinedConditionContext;
  class RelationalOperatorContext;
  class AbbreviationContext;
  class IdentifierContext;
  class TableCallContext;
  class FunctionCallContext;
  class ReferenceModifierContext;
  class CharacterPositionContext;
  class LengthContext;
  class Subscript_Context;
  class ArgumentContext;
  class QualifiedDataNameContext;
  class QualifiedDataNameFormat1Context;
  class QualifiedDataNameFormat2Context;
  class QualifiedDataNameFormat3Context;
  class QualifiedDataNameFormat4Context;
  class QualifiedInDataContext;
  class InDataContext;
  class InFileContext;
  class InMnemonicContext;
  class InSectionContext;
  class InLibraryContext;
  class InTableContext;
  class AlphabetNameContext;
  class AssignmentNameContext;
  class BasisNameContext;
  class CdNameContext;
  class ClassNameContext;
  class ComputerNameContext;
  class ConditionNameContext;
  class DataNameContext;
  class DataDescNameContext;
  class EnvironmentNameContext;
  class FileNameContext;
  class FunctionNameContext;
  class IndexNameContext;
  class LanguageNameContext;
  class LibraryNameContext;
  class LocalNameContext;
  class MnemonicNameContext;
  class ParagraphNameContext;
  class ProcedureNameContext;
  class ProgramNameContext;
  class RecordNameContext;
  class ReportNameContext;
  class RoutineNameContext;
  class ScreenNameContext;
  class SectionNameContext;
  class SystemNameContext;
  class SymbolicCharacterContext;
  class TextNameContext;
  class CobolWordContext;
  class LiteralContext;
  class BooleanLiteralContext;
  class NumericLiteralContext;
  class IntegerLiteralContext;
  class CicsDfhRespLiteralContext;
  class CicsDfhValueLiteralContext;
  class FigurativeConstantContext;
  class SpecialRegisterContext;
  class CommentEntryContext; 

  class  StartRuleContext : public antlr4::ParserRuleContext {
  public:
    StartRuleContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    CompilationUnitContext *compilationUnit();
    antlr4::tree::TerminalNode *EOF();

   
  };

  StartRuleContext* startRule();

  class  CompilationUnitContext : public antlr4::ParserRuleContext {
  public:
    CompilationUnitContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<ProgramUnitContext *> programUnit();
    ProgramUnitContext* programUnit(size_t i);

   
  };

  CompilationUnitContext* compilationUnit();

  class  ProgramUnitContext : public antlr4::ParserRuleContext {
  public:
    ProgramUnitContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentificationDivisionContext *identificationDivision();
    EnvironmentDivisionContext *environmentDivision();
    DataDivisionContext *dataDivision();
    ProcedureDivisionContext *procedureDivision();
    std::vector<ProgramUnitContext *> programUnit();
    ProgramUnitContext* programUnit(size_t i);
    EndProgramStatementContext *endProgramStatement();

   
  };

  ProgramUnitContext* programUnit();

  class  EndProgramStatementContext : public antlr4::ParserRuleContext {
  public:
    EndProgramStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *END();
    antlr4::tree::TerminalNode *PROGRAM();
    ProgramNameContext *programName();
    antlr4::tree::TerminalNode *DOT_FS();

   
  };

  EndProgramStatementContext* endProgramStatement();

  class  IdentificationDivisionContext : public antlr4::ParserRuleContext {
  public:
    IdentificationDivisionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DIVISION();
    antlr4::tree::TerminalNode *DOT_FS();
    ProgramIdParagraphContext *programIdParagraph();
    antlr4::tree::TerminalNode *IDENTIFICATION();
    antlr4::tree::TerminalNode *ID();
    std::vector<IdentificationDivisionBodyContext *> identificationDivisionBody();
    IdentificationDivisionBodyContext* identificationDivisionBody(size_t i);

   
  };

  IdentificationDivisionContext* identificationDivision();

  class  IdentificationDivisionBodyContext : public antlr4::ParserRuleContext {
  public:
    IdentificationDivisionBodyContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    AuthorParagraphContext *authorParagraph();
    InstallationParagraphContext *installationParagraph();
    DateWrittenParagraphContext *dateWrittenParagraph();
    DateCompiledParagraphContext *dateCompiledParagraph();
    SecurityParagraphContext *securityParagraph();
    RemarksParagraphContext *remarksParagraph();

   
  };

  IdentificationDivisionBodyContext* identificationDivisionBody();

  class  ProgramIdParagraphContext : public antlr4::ParserRuleContext {
  public:
    ProgramIdParagraphContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PROGRAM_ID();
    std::vector<antlr4::tree::TerminalNode *> DOT_FS();
    antlr4::tree::TerminalNode* DOT_FS(size_t i);
    ProgramNameContext *programName();
    CommentEntryContext *commentEntry();
    antlr4::tree::TerminalNode *COMMON();
    antlr4::tree::TerminalNode *INITIAL();
    antlr4::tree::TerminalNode *LIBRARY();
    antlr4::tree::TerminalNode *DEFINITION();
    antlr4::tree::TerminalNode *RECURSIVE();
    antlr4::tree::TerminalNode *IS();
    antlr4::tree::TerminalNode *PROGRAM();

   
  };

  ProgramIdParagraphContext* programIdParagraph();

  class  AuthorParagraphContext : public antlr4::ParserRuleContext {
  public:
    AuthorParagraphContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *AUTHOR();
    antlr4::tree::TerminalNode *DOT_FS();
    CommentEntryContext *commentEntry();

   
  };

  AuthorParagraphContext* authorParagraph();

  class  InstallationParagraphContext : public antlr4::ParserRuleContext {
  public:
    InstallationParagraphContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INSTALLATION();
    antlr4::tree::TerminalNode *DOT_FS();
    CommentEntryContext *commentEntry();

   
  };

  InstallationParagraphContext* installationParagraph();

  class  DateWrittenParagraphContext : public antlr4::ParserRuleContext {
  public:
    DateWrittenParagraphContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DATE_WRITTEN();
    antlr4::tree::TerminalNode *DOT_FS();
    CommentEntryContext *commentEntry();

   
  };

  DateWrittenParagraphContext* dateWrittenParagraph();

  class  DateCompiledParagraphContext : public antlr4::ParserRuleContext {
  public:
    DateCompiledParagraphContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DATE_COMPILED();
    antlr4::tree::TerminalNode *DOT_FS();
    CommentEntryContext *commentEntry();

   
  };

  DateCompiledParagraphContext* dateCompiledParagraph();

  class  SecurityParagraphContext : public antlr4::ParserRuleContext {
  public:
    SecurityParagraphContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SECURITY();
    antlr4::tree::TerminalNode *DOT_FS();
    CommentEntryContext *commentEntry();

   
  };

  SecurityParagraphContext* securityParagraph();

  class  RemarksParagraphContext : public antlr4::ParserRuleContext {
  public:
    RemarksParagraphContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REMARKS();
    antlr4::tree::TerminalNode *DOT_FS();
    CommentEntryContext *commentEntry();

   
  };

  RemarksParagraphContext* remarksParagraph();

  class  EnvironmentDivisionContext : public antlr4::ParserRuleContext {
  public:
    EnvironmentDivisionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ENVIRONMENT();
    antlr4::tree::TerminalNode *DIVISION();
    antlr4::tree::TerminalNode *DOT_FS();
    std::vector<EnvironmentDivisionBodyContext *> environmentDivisionBody();
    EnvironmentDivisionBodyContext* environmentDivisionBody(size_t i);

   
  };

  EnvironmentDivisionContext* environmentDivision();

  class  EnvironmentDivisionBodyContext : public antlr4::ParserRuleContext {
  public:
    EnvironmentDivisionBodyContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ConfigurationSectionContext *configurationSection();
    SpecialNamesParagraphContext *specialNamesParagraph();
    InputOutputSectionContext *inputOutputSection();

   
  };

  EnvironmentDivisionBodyContext* environmentDivisionBody();

  class  ConfigurationSectionContext : public antlr4::ParserRuleContext {
  public:
    ConfigurationSectionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CONFIGURATION();
    antlr4::tree::TerminalNode *SECTION();
    antlr4::tree::TerminalNode *DOT_FS();
    std::vector<ConfigurationSectionParagraphContext *> configurationSectionParagraph();
    ConfigurationSectionParagraphContext* configurationSectionParagraph(size_t i);

   
  };

  ConfigurationSectionContext* configurationSection();

  class  ConfigurationSectionParagraphContext : public antlr4::ParserRuleContext {
  public:
    ConfigurationSectionParagraphContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    SourceComputerParagraphContext *sourceComputerParagraph();
    ObjectComputerParagraphContext *objectComputerParagraph();
    SpecialNamesParagraphContext *specialNamesParagraph();

   
  };

  ConfigurationSectionParagraphContext* configurationSectionParagraph();

  class  SourceComputerParagraphContext : public antlr4::ParserRuleContext {
  public:
    SourceComputerParagraphContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SOURCE_COMPUTER();
    std::vector<antlr4::tree::TerminalNode *> DOT_FS();
    antlr4::tree::TerminalNode* DOT_FS(size_t i);
    ComputerNameContext *computerName();
    antlr4::tree::TerminalNode *DEBUGGING();
    antlr4::tree::TerminalNode *MODE();
    antlr4::tree::TerminalNode *WITH();

   
  };

  SourceComputerParagraphContext* sourceComputerParagraph();

  class  ObjectComputerParagraphContext : public antlr4::ParserRuleContext {
  public:
    ObjectComputerParagraphContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OBJECT_COMPUTER();
    std::vector<antlr4::tree::TerminalNode *> DOT_FS();
    antlr4::tree::TerminalNode* DOT_FS(size_t i);
    ComputerNameContext *computerName();
    std::vector<ObjectComputerClauseContext *> objectComputerClause();
    ObjectComputerClauseContext* objectComputerClause(size_t i);

   
  };

  ObjectComputerParagraphContext* objectComputerParagraph();

  class  ObjectComputerClauseContext : public antlr4::ParserRuleContext {
  public:
    ObjectComputerClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    MemorySizeClauseContext *memorySizeClause();
    DiskSizeClauseContext *diskSizeClause();
    CollatingSequenceClauseContext *collatingSequenceClause();
    SegmentLimitClauseContext *segmentLimitClause();
    CharacterSetClauseContext *characterSetClause();

   
  };

  ObjectComputerClauseContext* objectComputerClause();

  class  MemorySizeClauseContext : public antlr4::ParserRuleContext {
  public:
    MemorySizeClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *MEMORY();
    IntegerLiteralContext *integerLiteral();
    CobolWordContext *cobolWord();
    antlr4::tree::TerminalNode *SIZE();
    antlr4::tree::TerminalNode *WORDS();
    antlr4::tree::TerminalNode *CHARACTERS();
    antlr4::tree::TerminalNode *MODULES();

   
  };

  MemorySizeClauseContext* memorySizeClause();

  class  DiskSizeClauseContext : public antlr4::ParserRuleContext {
  public:
    DiskSizeClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DISK();
    IntegerLiteralContext *integerLiteral();
    CobolWordContext *cobolWord();
    antlr4::tree::TerminalNode *SIZE();
    antlr4::tree::TerminalNode *IS();
    antlr4::tree::TerminalNode *WORDS();
    antlr4::tree::TerminalNode *MODULES();

   
  };

  DiskSizeClauseContext* diskSizeClause();

  class  CollatingSequenceClauseContext : public antlr4::ParserRuleContext {
  public:
    CollatingSequenceClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SEQUENCE();
    antlr4::tree::TerminalNode *PROGRAM();
    antlr4::tree::TerminalNode *COLLATING();
    CollatingSequenceClauseAlphanumericContext *collatingSequenceClauseAlphanumeric();
    CollatingSequenceClauseNationalContext *collatingSequenceClauseNational();
    antlr4::tree::TerminalNode *IS();
    std::vector<AlphabetNameContext *> alphabetName();
    AlphabetNameContext* alphabetName(size_t i);

   
  };

  CollatingSequenceClauseContext* collatingSequenceClause();

  class  CollatingSequenceClauseAlphanumericContext : public antlr4::ParserRuleContext {
  public:
    CollatingSequenceClauseAlphanumericContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALPHANUMERIC();
    AlphabetNameContext *alphabetName();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *IS();

   
  };

  CollatingSequenceClauseAlphanumericContext* collatingSequenceClauseAlphanumeric();

  class  CollatingSequenceClauseNationalContext : public antlr4::ParserRuleContext {
  public:
    CollatingSequenceClauseNationalContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NATIONAL();
    AlphabetNameContext *alphabetName();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *IS();

   
  };

  CollatingSequenceClauseNationalContext* collatingSequenceClauseNational();

  class  SegmentLimitClauseContext : public antlr4::ParserRuleContext {
  public:
    SegmentLimitClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SEGMENT_LIMIT();
    IntegerLiteralContext *integerLiteral();
    antlr4::tree::TerminalNode *IS();

   
  };

  SegmentLimitClauseContext* segmentLimitClause();

  class  CharacterSetClauseContext : public antlr4::ParserRuleContext {
  public:
    CharacterSetClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CHARACTER();
    antlr4::tree::TerminalNode *SET();
    antlr4::tree::TerminalNode *DOT_FS();

   
  };

  CharacterSetClauseContext* characterSetClause();

  class  SpecialNamesParagraphContext : public antlr4::ParserRuleContext {
  public:
    SpecialNamesParagraphContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SPECIAL_NAMES();
    std::vector<antlr4::tree::TerminalNode *> DOT_FS();
    antlr4::tree::TerminalNode* DOT_FS(size_t i);
    std::vector<SpecialNameClauseContext *> specialNameClause();
    SpecialNameClauseContext* specialNameClause(size_t i);

   
  };

  SpecialNamesParagraphContext* specialNamesParagraph();

  class  SpecialNameClauseContext : public antlr4::ParserRuleContext {
  public:
    SpecialNameClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ChannelClauseContext *channelClause();
    OdtClauseContext *odtClause();
    AlphabetClauseContext *alphabetClause();
    ClassClauseContext *classClause();
    CurrencySignClauseContext *currencySignClause();
    DecimalPointClauseContext *decimalPointClause();
    SymbolicCharactersClauseContext *symbolicCharactersClause();
    EnvironmentSwitchNameClauseContext *environmentSwitchNameClause();
    DefaultDisplaySignClauseContext *defaultDisplaySignClause();
    DefaultComputationalSignClauseContext *defaultComputationalSignClause();
    ReserveNetworkClauseContext *reserveNetworkClause();

   
  };

  SpecialNameClauseContext* specialNameClause();

  class  AlphabetClauseContext : public antlr4::ParserRuleContext {
  public:
    AlphabetClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    AlphabetClauseFormat1Context *alphabetClauseFormat1();
    AlphabetClauseFormat2Context *alphabetClauseFormat2();

   
  };

  AlphabetClauseContext* alphabetClause();

  class  AlphabetClauseFormat1Context : public antlr4::ParserRuleContext {
  public:
    AlphabetClauseFormat1Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALPHABET();
    AlphabetNameContext *alphabetName();
    antlr4::tree::TerminalNode *EBCDIC();
    antlr4::tree::TerminalNode *ASCII();
    antlr4::tree::TerminalNode *STANDARD_1();
    antlr4::tree::TerminalNode *STANDARD_2();
    antlr4::tree::TerminalNode *NATIVE();
    CobolWordContext *cobolWord();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *ALPHANUMERIC();
    antlr4::tree::TerminalNode *IS();
    std::vector<AlphabetLiteralsContext *> alphabetLiterals();
    AlphabetLiteralsContext* alphabetLiterals(size_t i);

   
  };

  AlphabetClauseFormat1Context* alphabetClauseFormat1();

  class  AlphabetLiteralsContext : public antlr4::ParserRuleContext {
  public:
    AlphabetLiteralsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    LiteralContext *literal();
    AlphabetThroughContext *alphabetThrough();
    std::vector<AlphabetAlsoContext *> alphabetAlso();
    AlphabetAlsoContext* alphabetAlso(size_t i);

   
  };

  AlphabetLiteralsContext* alphabetLiterals();

  class  AlphabetThroughContext : public antlr4::ParserRuleContext {
  public:
    AlphabetThroughContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    LiteralContext *literal();
    antlr4::tree::TerminalNode *THROUGH();
    antlr4::tree::TerminalNode *THRU();

   
  };

  AlphabetThroughContext* alphabetThrough();

  class  AlphabetAlsoContext : public antlr4::ParserRuleContext {
  public:
    AlphabetAlsoContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALSO();
    std::vector<LiteralContext *> literal();
    LiteralContext* literal(size_t i);

   
  };

  AlphabetAlsoContext* alphabetAlso();

  class  AlphabetClauseFormat2Context : public antlr4::ParserRuleContext {
  public:
    AlphabetClauseFormat2Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALPHABET();
    AlphabetNameContext *alphabetName();
    antlr4::tree::TerminalNode *NATIONAL();
    antlr4::tree::TerminalNode *NATIVE();
    antlr4::tree::TerminalNode *CCSVERSION();
    LiteralContext *literal();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *IS();

   
  };

  AlphabetClauseFormat2Context* alphabetClauseFormat2();

  class  ChannelClauseContext : public antlr4::ParserRuleContext {
  public:
    ChannelClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CHANNEL();
    IntegerLiteralContext *integerLiteral();
    MnemonicNameContext *mnemonicName();
    antlr4::tree::TerminalNode *IS();

   
  };

  ChannelClauseContext* channelClause();

  class  ClassClauseContext : public antlr4::ParserRuleContext {
  public:
    ClassClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CLASS();
    ClassNameContext *className();
    antlr4::tree::TerminalNode *IS();
    std::vector<ClassClauseThroughContext *> classClauseThrough();
    ClassClauseThroughContext* classClauseThrough(size_t i);
    antlr4::tree::TerminalNode *ALPHANUMERIC();
    antlr4::tree::TerminalNode *NATIONAL();
    antlr4::tree::TerminalNode *FOR();

   
  };

  ClassClauseContext* classClause();

  class  ClassClauseThroughContext : public antlr4::ParserRuleContext {
  public:
    ClassClauseThroughContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ClassClauseFromContext *classClauseFrom();
    ClassClauseToContext *classClauseTo();
    antlr4::tree::TerminalNode *THROUGH();
    antlr4::tree::TerminalNode *THRU();

   
  };

  ClassClauseThroughContext* classClauseThrough();

  class  ClassClauseFromContext : public antlr4::ParserRuleContext {
  public:
    ClassClauseFromContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    LiteralContext *literal();

   
  };

  ClassClauseFromContext* classClauseFrom();

  class  ClassClauseToContext : public antlr4::ParserRuleContext {
  public:
    ClassClauseToContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    LiteralContext *literal();

   
  };

  ClassClauseToContext* classClauseTo();

  class  CurrencySignClauseContext : public antlr4::ParserRuleContext {
  public:
    CurrencySignClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CURRENCY();
    std::vector<LiteralContext *> literal();
    LiteralContext* literal(size_t i);
    antlr4::tree::TerminalNode *SIGN();
    antlr4::tree::TerminalNode *IS();
    antlr4::tree::TerminalNode *PICTURE();
    antlr4::tree::TerminalNode *SYMBOL();
    antlr4::tree::TerminalNode *WITH();

   
  };

  CurrencySignClauseContext* currencySignClause();

  class  DecimalPointClauseContext : public antlr4::ParserRuleContext {
  public:
    DecimalPointClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DECIMAL_POINT();
    antlr4::tree::TerminalNode *COMMA();
    antlr4::tree::TerminalNode *IS();

   
  };

  DecimalPointClauseContext* decimalPointClause();

  class  DefaultComputationalSignClauseContext : public antlr4::ParserRuleContext {
  public:
    DefaultComputationalSignClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DEFAULT();
    antlr4::tree::TerminalNode *SEPARATE();
    antlr4::tree::TerminalNode *SIGN();
    antlr4::tree::TerminalNode *COMPUTATIONAL();
    antlr4::tree::TerminalNode *COMP();
    antlr4::tree::TerminalNode *LEADING();
    antlr4::tree::TerminalNode *TRAILING();
    antlr4::tree::TerminalNode *CHARACTER();
    antlr4::tree::TerminalNode *IS();

   
  };

  DefaultComputationalSignClauseContext* defaultComputationalSignClause();

  class  DefaultDisplaySignClauseContext : public antlr4::ParserRuleContext {
  public:
    DefaultDisplaySignClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DEFAULT_DISPLAY();
    antlr4::tree::TerminalNode *LEADING();
    antlr4::tree::TerminalNode *TRAILING();
    antlr4::tree::TerminalNode *SIGN();
    antlr4::tree::TerminalNode *SEPARATE();
    antlr4::tree::TerminalNode *IS();
    antlr4::tree::TerminalNode *CHARACTER();

   
  };

  DefaultDisplaySignClauseContext* defaultDisplaySignClause();

  class  EnvironmentSwitchNameClauseContext : public antlr4::ParserRuleContext {
  public:
    EnvironmentSwitchNameClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    EnvironmentNameContext *environmentName();
    MnemonicNameContext *mnemonicName();
    antlr4::tree::TerminalNode *IS();
    EnvironmentSwitchNameSpecialNamesStatusPhraseContext *environmentSwitchNameSpecialNamesStatusPhrase();

   
  };

  EnvironmentSwitchNameClauseContext* environmentSwitchNameClause();

  class  EnvironmentSwitchNameSpecialNamesStatusPhraseContext : public antlr4::ParserRuleContext {
  public:
    EnvironmentSwitchNameSpecialNamesStatusPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ON();
    std::vector<ConditionContext *> condition();
    ConditionContext* condition(size_t i);
    std::vector<antlr4::tree::TerminalNode *> STATUS();
    antlr4::tree::TerminalNode* STATUS(size_t i);
    std::vector<antlr4::tree::TerminalNode *> IS();
    antlr4::tree::TerminalNode* IS(size_t i);
    antlr4::tree::TerminalNode *OFF();

   
  };

  EnvironmentSwitchNameSpecialNamesStatusPhraseContext* environmentSwitchNameSpecialNamesStatusPhrase();

  class  OdtClauseContext : public antlr4::ParserRuleContext {
  public:
    OdtClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ODT();
    MnemonicNameContext *mnemonicName();
    antlr4::tree::TerminalNode *IS();

   
  };

  OdtClauseContext* odtClause();

  class  ReserveNetworkClauseContext : public antlr4::ParserRuleContext {
  public:
    ReserveNetworkClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RESERVE();
    antlr4::tree::TerminalNode *NETWORK();
    antlr4::tree::TerminalNode *WORDS();
    antlr4::tree::TerminalNode *LIST();
    antlr4::tree::TerminalNode *IS();
    antlr4::tree::TerminalNode *CAPABLE();

   
  };

  ReserveNetworkClauseContext* reserveNetworkClause();

  class  SymbolicCharactersClauseContext : public antlr4::ParserRuleContext {
  public:
    SymbolicCharactersClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SYMBOLIC();
    antlr4::tree::TerminalNode *CHARACTERS();
    std::vector<SymbolicCharactersContext *> symbolicCharacters();
    SymbolicCharactersContext* symbolicCharacters(size_t i);
    antlr4::tree::TerminalNode *IN();
    AlphabetNameContext *alphabetName();
    antlr4::tree::TerminalNode *ALPHANUMERIC();
    antlr4::tree::TerminalNode *NATIONAL();
    antlr4::tree::TerminalNode *FOR();

   
  };

  SymbolicCharactersClauseContext* symbolicCharactersClause();

  class  SymbolicCharactersContext : public antlr4::ParserRuleContext {
  public:
    SymbolicCharactersContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<SymbolicCharacterContext *> symbolicCharacter();
    SymbolicCharacterContext* symbolicCharacter(size_t i);
    std::vector<IntegerLiteralContext *> integerLiteral();
    IntegerLiteralContext* integerLiteral(size_t i);
    antlr4::tree::TerminalNode *IS();
    antlr4::tree::TerminalNode *ARE();

   
  };

  SymbolicCharactersContext* symbolicCharacters();

  class  InputOutputSectionContext : public antlr4::ParserRuleContext {
  public:
    InputOutputSectionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INPUT_OUTPUT();
    antlr4::tree::TerminalNode *SECTION();
    antlr4::tree::TerminalNode *DOT_FS();
    std::vector<InputOutputSectionParagraphContext *> inputOutputSectionParagraph();
    InputOutputSectionParagraphContext* inputOutputSectionParagraph(size_t i);

   
  };

  InputOutputSectionContext* inputOutputSection();

  class  InputOutputSectionParagraphContext : public antlr4::ParserRuleContext {
  public:
    InputOutputSectionParagraphContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    FileControlParagraphContext *fileControlParagraph();
    IoControlParagraphContext *ioControlParagraph();

   
  };

  InputOutputSectionParagraphContext* inputOutputSectionParagraph();

  class  FileControlParagraphContext : public antlr4::ParserRuleContext {
  public:
    FileControlParagraphContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FILE_CONTROL();
    std::vector<antlr4::tree::TerminalNode *> DOT_FS();
    antlr4::tree::TerminalNode* DOT_FS(size_t i);
    std::vector<FileControlEntryContext *> fileControlEntry();
    FileControlEntryContext* fileControlEntry(size_t i);

   
  };

  FileControlParagraphContext* fileControlParagraph();

  class  FileControlEntryContext : public antlr4::ParserRuleContext {
  public:
    FileControlEntryContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    SelectClauseContext *selectClause();
    std::vector<FileControlClauseContext *> fileControlClause();
    FileControlClauseContext* fileControlClause(size_t i);

   
  };

  FileControlEntryContext* fileControlEntry();

  class  SelectClauseContext : public antlr4::ParserRuleContext {
  public:
    SelectClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SELECT();
    FileNameContext *fileName();
    antlr4::tree::TerminalNode *OPTIONAL();

   
  };

  SelectClauseContext* selectClause();

  class  FileControlClauseContext : public antlr4::ParserRuleContext {
  public:
    FileControlClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    AssignClauseContext *assignClause();
    ReserveClauseContext *reserveClause();
    OrganizationClauseContext *organizationClause();
    PaddingCharacterClauseContext *paddingCharacterClause();
    RecordDelimiterClauseContext *recordDelimiterClause();
    AccessModeClauseContext *accessModeClause();
    RecordKeyClauseContext *recordKeyClause();
    AlternateRecordKeyClauseContext *alternateRecordKeyClause();
    FileStatusClauseContext *fileStatusClause();
    PasswordClauseContext *passwordClause();
    RelativeKeyClauseContext *relativeKeyClause();

   
  };

  FileControlClauseContext* fileControlClause();

  class  AssignClauseContext : public antlr4::ParserRuleContext {
  public:
    AssignClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ASSIGN();
    antlr4::tree::TerminalNode *DISK();
    antlr4::tree::TerminalNode *DISPLAY();
    antlr4::tree::TerminalNode *KEYBOARD();
    antlr4::tree::TerminalNode *PORT();
    antlr4::tree::TerminalNode *PRINTER();
    antlr4::tree::TerminalNode *READER();
    antlr4::tree::TerminalNode *REMOTE();
    antlr4::tree::TerminalNode *TAPE();
    antlr4::tree::TerminalNode *VIRTUAL();
    AssignmentNameContext *assignmentName();
    LiteralContext *literal();
    antlr4::tree::TerminalNode *TO();

   
  };

  AssignClauseContext* assignClause();

  class  ReserveClauseContext : public antlr4::ParserRuleContext {
  public:
    ReserveClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RESERVE();
    antlr4::tree::TerminalNode *NO();
    IntegerLiteralContext *integerLiteral();
    antlr4::tree::TerminalNode *ALTERNATE();
    antlr4::tree::TerminalNode *AREA();
    antlr4::tree::TerminalNode *AREAS();

   
  };

  ReserveClauseContext* reserveClause();

  class  OrganizationClauseContext : public antlr4::ParserRuleContext {
  public:
    OrganizationClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SEQUENTIAL();
    antlr4::tree::TerminalNode *RELATIVE();
    antlr4::tree::TerminalNode *INDEXED();
    antlr4::tree::TerminalNode *ORGANIZATION();
    antlr4::tree::TerminalNode *LINE();
    antlr4::tree::TerminalNode *RECORD();
    antlr4::tree::TerminalNode *BINARY();
    antlr4::tree::TerminalNode *IS();

   
  };

  OrganizationClauseContext* organizationClause();

  class  PaddingCharacterClauseContext : public antlr4::ParserRuleContext {
  public:
    PaddingCharacterClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PADDING();
    QualifiedDataNameContext *qualifiedDataName();
    LiteralContext *literal();
    antlr4::tree::TerminalNode *CHARACTER();
    antlr4::tree::TerminalNode *IS();

   
  };

  PaddingCharacterClauseContext* paddingCharacterClause();

  class  RecordDelimiterClauseContext : public antlr4::ParserRuleContext {
  public:
    RecordDelimiterClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RECORD();
    antlr4::tree::TerminalNode *DELIMITER();
    antlr4::tree::TerminalNode *STANDARD_1();
    antlr4::tree::TerminalNode *IMPLICIT();
    AssignmentNameContext *assignmentName();
    antlr4::tree::TerminalNode *IS();

   
  };

  RecordDelimiterClauseContext* recordDelimiterClause();

  class  AccessModeClauseContext : public antlr4::ParserRuleContext {
  public:
    AccessModeClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ACCESS();
    antlr4::tree::TerminalNode *SEQUENTIAL();
    antlr4::tree::TerminalNode *RANDOM();
    antlr4::tree::TerminalNode *DYNAMIC();
    antlr4::tree::TerminalNode *EXCLUSIVE();
    antlr4::tree::TerminalNode *MODE();
    antlr4::tree::TerminalNode *IS();

   
  };

  AccessModeClauseContext* accessModeClause();

  class  RecordKeyClauseContext : public antlr4::ParserRuleContext {
  public:
    RecordKeyClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RECORD();
    QualifiedDataNameContext *qualifiedDataName();
    antlr4::tree::TerminalNode *KEY();
    antlr4::tree::TerminalNode *IS();
    PasswordClauseContext *passwordClause();
    antlr4::tree::TerminalNode *DUPLICATES();
    antlr4::tree::TerminalNode *WITH();

   
  };

  RecordKeyClauseContext* recordKeyClause();

  class  AlternateRecordKeyClauseContext : public antlr4::ParserRuleContext {
  public:
    AlternateRecordKeyClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTERNATE();
    antlr4::tree::TerminalNode *RECORD();
    QualifiedDataNameContext *qualifiedDataName();
    antlr4::tree::TerminalNode *KEY();
    antlr4::tree::TerminalNode *IS();
    PasswordClauseContext *passwordClause();
    antlr4::tree::TerminalNode *DUPLICATES();
    antlr4::tree::TerminalNode *WITH();

   
  };

  AlternateRecordKeyClauseContext* alternateRecordKeyClause();

  class  PasswordClauseContext : public antlr4::ParserRuleContext {
  public:
    PasswordClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PASSWORD();
    DataNameContext *dataName();
    antlr4::tree::TerminalNode *IS();

   
  };

  PasswordClauseContext* passwordClause();

  class  FileStatusClauseContext : public antlr4::ParserRuleContext {
  public:
    FileStatusClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *STATUS();
    std::vector<QualifiedDataNameContext *> qualifiedDataName();
    QualifiedDataNameContext* qualifiedDataName(size_t i);
    antlr4::tree::TerminalNode *FILE();
    antlr4::tree::TerminalNode *IS();

   
  };

  FileStatusClauseContext* fileStatusClause();

  class  RelativeKeyClauseContext : public antlr4::ParserRuleContext {
  public:
    RelativeKeyClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RELATIVE();
    QualifiedDataNameContext *qualifiedDataName();
    antlr4::tree::TerminalNode *KEY();
    antlr4::tree::TerminalNode *IS();

   
  };

  RelativeKeyClauseContext* relativeKeyClause();

  class  IoControlParagraphContext : public antlr4::ParserRuleContext {
  public:
    IoControlParagraphContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *I_O_CONTROL();
    std::vector<antlr4::tree::TerminalNode *> DOT_FS();
    antlr4::tree::TerminalNode* DOT_FS(size_t i);
    FileNameContext *fileName();
    std::vector<IoControlClauseContext *> ioControlClause();
    IoControlClauseContext* ioControlClause(size_t i);

   
  };

  IoControlParagraphContext* ioControlParagraph();

  class  IoControlClauseContext : public antlr4::ParserRuleContext {
  public:
    IoControlClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    RerunClauseContext *rerunClause();
    SameClauseContext *sameClause();
    MultipleFileClauseContext *multipleFileClause();
    CommitmentControlClauseContext *commitmentControlClause();

   
  };

  IoControlClauseContext* ioControlClause();

  class  RerunClauseContext : public antlr4::ParserRuleContext {
  public:
    RerunClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RERUN();
    antlr4::tree::TerminalNode *EVERY();
    RerunEveryRecordsContext *rerunEveryRecords();
    RerunEveryOfContext *rerunEveryOf();
    RerunEveryClockContext *rerunEveryClock();
    antlr4::tree::TerminalNode *ON();
    AssignmentNameContext *assignmentName();
    FileNameContext *fileName();

   
  };

  RerunClauseContext* rerunClause();

  class  RerunEveryRecordsContext : public antlr4::ParserRuleContext {
  public:
    RerunEveryRecordsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IntegerLiteralContext *integerLiteral();
    antlr4::tree::TerminalNode *RECORDS();

   
  };

  RerunEveryRecordsContext* rerunEveryRecords();

  class  RerunEveryOfContext : public antlr4::ParserRuleContext {
  public:
    RerunEveryOfContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<antlr4::tree::TerminalNode *> OF();
    antlr4::tree::TerminalNode* OF(size_t i);
    FileNameContext *fileName();
    antlr4::tree::TerminalNode *REEL();
    antlr4::tree::TerminalNode *UNIT();
    antlr4::tree::TerminalNode *END();

   
  };

  RerunEveryOfContext* rerunEveryOf();

  class  RerunEveryClockContext : public antlr4::ParserRuleContext {
  public:
    RerunEveryClockContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IntegerLiteralContext *integerLiteral();
    antlr4::tree::TerminalNode *CLOCK_UNITS();

   
  };

  RerunEveryClockContext* rerunEveryClock();

  class  SameClauseContext : public antlr4::ParserRuleContext {
  public:
    SameClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SAME();
    antlr4::tree::TerminalNode *AREA();
    antlr4::tree::TerminalNode *FOR();
    std::vector<FileNameContext *> fileName();
    FileNameContext* fileName(size_t i);
    antlr4::tree::TerminalNode *RECORD();
    antlr4::tree::TerminalNode *SORT();
    antlr4::tree::TerminalNode *SORT_MERGE();

   
  };

  SameClauseContext* sameClause();

  class  MultipleFileClauseContext : public antlr4::ParserRuleContext {
  public:
    MultipleFileClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *MULTIPLE();
    antlr4::tree::TerminalNode *FILE();
    antlr4::tree::TerminalNode *TAPE();
    antlr4::tree::TerminalNode *CONTAINS();
    std::vector<MultipleFilePositionContext *> multipleFilePosition();
    MultipleFilePositionContext* multipleFilePosition(size_t i);

   
  };

  MultipleFileClauseContext* multipleFileClause();

  class  MultipleFilePositionContext : public antlr4::ParserRuleContext {
  public:
    MultipleFilePositionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    FileNameContext *fileName();
    antlr4::tree::TerminalNode *POSITION();
    IntegerLiteralContext *integerLiteral();

   
  };

  MultipleFilePositionContext* multipleFilePosition();

  class  CommitmentControlClauseContext : public antlr4::ParserRuleContext {
  public:
    CommitmentControlClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *COMMITMENT();
    antlr4::tree::TerminalNode *CONTROL();
    FileNameContext *fileName();
    antlr4::tree::TerminalNode *FOR();

   
  };

  CommitmentControlClauseContext* commitmentControlClause();

  class  DataDivisionContext : public antlr4::ParserRuleContext {
  public:
    DataDivisionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *DIVISION();
    antlr4::tree::TerminalNode *DOT_FS();
    std::vector<DataDivisionSectionContext *> dataDivisionSection();
    DataDivisionSectionContext* dataDivisionSection(size_t i);

   
  };

  DataDivisionContext* dataDivision();

  class  DataDivisionSectionContext : public antlr4::ParserRuleContext {
  public:
    DataDivisionSectionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    FileSectionContext *fileSection();
    DataBaseSectionContext *dataBaseSection();
    WorkingStorageSectionContext *workingStorageSection();
    LinkageSectionContext *linkageSection();
    CommunicationSectionContext *communicationSection();
    LocalStorageSectionContext *localStorageSection();
    ScreenSectionContext *screenSection();
    ReportSectionContext *reportSection();
    ProgramLibrarySectionContext *programLibrarySection();
    CopyDirectiveContext *copyDirective();

   
  };

  DataDivisionSectionContext* dataDivisionSection();

  class  FileSectionContext : public antlr4::ParserRuleContext {
  public:
    FileSectionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FILE();
    antlr4::tree::TerminalNode *SECTION();
    antlr4::tree::TerminalNode *DOT_FS();
    std::vector<FileDescriptionEntryContext *> fileDescriptionEntry();
    FileDescriptionEntryContext* fileDescriptionEntry(size_t i);
    std::vector<CopyDirectiveContext *> copyDirective();
    CopyDirectiveContext* copyDirective(size_t i);

   
  };

  FileSectionContext* fileSection();

  class  FileDescriptionEntryContext : public antlr4::ParserRuleContext {
  public:
    FileDescriptionEntryContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    FileNameContext *fileName();
    std::vector<antlr4::tree::TerminalNode *> DOT_FS();
    antlr4::tree::TerminalNode* DOT_FS(size_t i);
    antlr4::tree::TerminalNode *FD();
    antlr4::tree::TerminalNode *SD();
    std::vector<FileDescriptionEntryClauseContext *> fileDescriptionEntryClause();
    FileDescriptionEntryClauseContext* fileDescriptionEntryClause(size_t i);
    std::vector<DataDescriptionEntryContext *> dataDescriptionEntry();
    DataDescriptionEntryContext* dataDescriptionEntry(size_t i);

   
  };

  FileDescriptionEntryContext* fileDescriptionEntry();

  class  FileDescriptionEntryClauseContext : public antlr4::ParserRuleContext {
  public:
    FileDescriptionEntryClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ExternalClauseContext *externalClause();
    GlobalClauseContext *globalClause();
    BlockContainsClauseContext *blockContainsClause();
    RecordContainsClauseContext *recordContainsClause();
    LabelRecordsClauseContext *labelRecordsClause();
    ValueOfClauseContext *valueOfClause();
    DataRecordsClauseContext *dataRecordsClause();
    LinageClauseContext *linageClause();
    CodeSetClauseContext *codeSetClause();
    ReportClauseContext *reportClause();
    RecordingModeClauseContext *recordingModeClause();

   
  };

  FileDescriptionEntryClauseContext* fileDescriptionEntryClause();

  class  ExternalClauseContext : public antlr4::ParserRuleContext {
  public:
    ExternalClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *EXTERNAL();
    antlr4::tree::TerminalNode *IS();

   
  };

  ExternalClauseContext* externalClause();

  class  GlobalClauseContext : public antlr4::ParserRuleContext {
  public:
    GlobalClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GLOBAL();
    antlr4::tree::TerminalNode *IS();

   
  };

  GlobalClauseContext* globalClause();

  class  BlockContainsClauseContext : public antlr4::ParserRuleContext {
  public:
    BlockContainsClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BLOCK();
    IntegerLiteralContext *integerLiteral();
    antlr4::tree::TerminalNode *CONTAINS();
    BlockContainsToContext *blockContainsTo();
    antlr4::tree::TerminalNode *RECORDS();
    antlr4::tree::TerminalNode *CHARACTERS();

   
  };

  BlockContainsClauseContext* blockContainsClause();

  class  BlockContainsToContext : public antlr4::ParserRuleContext {
  public:
    BlockContainsToContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TO();
    IntegerLiteralContext *integerLiteral();

   
  };

  BlockContainsToContext* blockContainsTo();

  class  RecordContainsClauseContext : public antlr4::ParserRuleContext {
  public:
    RecordContainsClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RECORD();
    RecordContainsClauseFormat1Context *recordContainsClauseFormat1();
    RecordContainsClauseFormat2Context *recordContainsClauseFormat2();
    RecordContainsClauseFormat3Context *recordContainsClauseFormat3();

   
  };

  RecordContainsClauseContext* recordContainsClause();

  class  RecordContainsClauseFormat1Context : public antlr4::ParserRuleContext {
  public:
    RecordContainsClauseFormat1Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IntegerLiteralContext *integerLiteral();
    antlr4::tree::TerminalNode *CONTAINS();
    antlr4::tree::TerminalNode *CHARACTERS();

   
  };

  RecordContainsClauseFormat1Context* recordContainsClauseFormat1();

  class  RecordContainsClauseFormat2Context : public antlr4::ParserRuleContext {
  public:
    RecordContainsClauseFormat2Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *VARYING();
    antlr4::tree::TerminalNode *IS();
    antlr4::tree::TerminalNode *IN();
    antlr4::tree::TerminalNode *SIZE();
    IntegerLiteralContext *integerLiteral();
    antlr4::tree::TerminalNode *DEPENDING();
    QualifiedDataNameContext *qualifiedDataName();
    antlr4::tree::TerminalNode *FROM();
    RecordContainsToContext *recordContainsTo();
    antlr4::tree::TerminalNode *CHARACTERS();
    antlr4::tree::TerminalNode *ON();

   
  };

  RecordContainsClauseFormat2Context* recordContainsClauseFormat2();

  class  RecordContainsClauseFormat3Context : public antlr4::ParserRuleContext {
  public:
    RecordContainsClauseFormat3Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IntegerLiteralContext *integerLiteral();
    RecordContainsToContext *recordContainsTo();
    antlr4::tree::TerminalNode *CONTAINS();
    antlr4::tree::TerminalNode *CHARACTERS();

   
  };

  RecordContainsClauseFormat3Context* recordContainsClauseFormat3();

  class  RecordContainsToContext : public antlr4::ParserRuleContext {
  public:
    RecordContainsToContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TO();
    IntegerLiteralContext *integerLiteral();

   
  };

  RecordContainsToContext* recordContainsTo();

  class  LabelRecordsClauseContext : public antlr4::ParserRuleContext {
  public:
    LabelRecordsClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LABEL();
    antlr4::tree::TerminalNode *RECORD();
    antlr4::tree::TerminalNode *RECORDS();
    antlr4::tree::TerminalNode *OMITTED();
    antlr4::tree::TerminalNode *STANDARD();
    antlr4::tree::TerminalNode *IS();
    antlr4::tree::TerminalNode *ARE();
    std::vector<DataNameContext *> dataName();
    DataNameContext* dataName(size_t i);

   
  };

  LabelRecordsClauseContext* labelRecordsClause();

  class  ValueOfClauseContext : public antlr4::ParserRuleContext {
  public:
    ValueOfClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *VALUE();
    antlr4::tree::TerminalNode *OF();
    std::vector<ValuePairContext *> valuePair();
    ValuePairContext* valuePair(size_t i);

   
  };

  ValueOfClauseContext* valueOfClause();

  class  ValuePairContext : public antlr4::ParserRuleContext {
  public:
    ValuePairContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    SystemNameContext *systemName();
    QualifiedDataNameContext *qualifiedDataName();
    LiteralContext *literal();
    antlr4::tree::TerminalNode *IS();

   
  };

  ValuePairContext* valuePair();

  class  DataRecordsClauseContext : public antlr4::ParserRuleContext {
  public:
    DataRecordsClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *RECORD();
    antlr4::tree::TerminalNode *RECORDS();
    std::vector<DataNameContext *> dataName();
    DataNameContext* dataName(size_t i);
    antlr4::tree::TerminalNode *IS();
    antlr4::tree::TerminalNode *ARE();

   
  };

  DataRecordsClauseContext* dataRecordsClause();

  class  LinageClauseContext : public antlr4::ParserRuleContext {
  public:
    LinageClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LINAGE();
    DataNameContext *dataName();
    IntegerLiteralContext *integerLiteral();
    antlr4::tree::TerminalNode *IS();
    antlr4::tree::TerminalNode *LINES();
    std::vector<LinageAtContext *> linageAt();
    LinageAtContext* linageAt(size_t i);

   
  };

  LinageClauseContext* linageClause();

  class  LinageAtContext : public antlr4::ParserRuleContext {
  public:
    LinageAtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    LinageFootingAtContext *linageFootingAt();
    LinageLinesAtTopContext *linageLinesAtTop();
    LinageLinesAtBottomContext *linageLinesAtBottom();

   
  };

  LinageAtContext* linageAt();

  class  LinageFootingAtContext : public antlr4::ParserRuleContext {
  public:
    LinageFootingAtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FOOTING();
    DataNameContext *dataName();
    IntegerLiteralContext *integerLiteral();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *AT();

   
  };

  LinageFootingAtContext* linageFootingAt();

  class  LinageLinesAtTopContext : public antlr4::ParserRuleContext {
  public:
    LinageLinesAtTopContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TOP();
    DataNameContext *dataName();
    IntegerLiteralContext *integerLiteral();
    antlr4::tree::TerminalNode *LINES();
    antlr4::tree::TerminalNode *AT();

   
  };

  LinageLinesAtTopContext* linageLinesAtTop();

  class  LinageLinesAtBottomContext : public antlr4::ParserRuleContext {
  public:
    LinageLinesAtBottomContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BOTTOM();
    DataNameContext *dataName();
    IntegerLiteralContext *integerLiteral();
    antlr4::tree::TerminalNode *LINES();
    antlr4::tree::TerminalNode *AT();

   
  };

  LinageLinesAtBottomContext* linageLinesAtBottom();

  class  RecordingModeClauseContext : public antlr4::ParserRuleContext {
  public:
    RecordingModeClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RECORDING();
    ModeStatementContext *modeStatement();
    antlr4::tree::TerminalNode *MODE();
    antlr4::tree::TerminalNode *IS();

   
  };

  RecordingModeClauseContext* recordingModeClause();

  class  ModeStatementContext : public antlr4::ParserRuleContext {
  public:
    ModeStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    CobolWordContext *cobolWord();

   
  };

  ModeStatementContext* modeStatement();

  class  CodeSetClauseContext : public antlr4::ParserRuleContext {
  public:
    CodeSetClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CODE_SET();
    AlphabetNameContext *alphabetName();
    antlr4::tree::TerminalNode *IS();

   
  };

  CodeSetClauseContext* codeSetClause();

  class  ReportClauseContext : public antlr4::ParserRuleContext {
  public:
    ReportClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REPORT();
    antlr4::tree::TerminalNode *REPORTS();
    std::vector<ReportNameContext *> reportName();
    ReportNameContext* reportName(size_t i);
    antlr4::tree::TerminalNode *IS();
    antlr4::tree::TerminalNode *ARE();

   
  };

  ReportClauseContext* reportClause();

  class  DataBaseSectionContext : public antlr4::ParserRuleContext {
  public:
    DataBaseSectionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DATA_BASE();
    antlr4::tree::TerminalNode *SECTION();
    antlr4::tree::TerminalNode *DOT_FS();
    std::vector<DataBaseSectionEntryContext *> dataBaseSectionEntry();
    DataBaseSectionEntryContext* dataBaseSectionEntry(size_t i);

   
  };

  DataBaseSectionContext* dataBaseSection();

  class  DataBaseSectionEntryContext : public antlr4::ParserRuleContext {
  public:
    DataBaseSectionEntryContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IntegerLiteralContext *integerLiteral();
    std::vector<LiteralContext *> literal();
    LiteralContext* literal(size_t i);
    antlr4::tree::TerminalNode *INVOKE();

   
  };

  DataBaseSectionEntryContext* dataBaseSectionEntry();

  class  WorkingStorageSectionContext : public antlr4::ParserRuleContext {
  public:
    WorkingStorageSectionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WORKING_STORAGE();
    antlr4::tree::TerminalNode *SECTION();
    antlr4::tree::TerminalNode *DOT_FS();
    std::vector<DataDescriptionEntryContext *> dataDescriptionEntry();
    DataDescriptionEntryContext* dataDescriptionEntry(size_t i);

   
  };

  WorkingStorageSectionContext* workingStorageSection();

  class  LinkageSectionContext : public antlr4::ParserRuleContext {
  public:
    LinkageSectionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LINKAGE();
    antlr4::tree::TerminalNode *SECTION();
    antlr4::tree::TerminalNode *DOT_FS();
    std::vector<DataDescriptionEntryContext *> dataDescriptionEntry();
    DataDescriptionEntryContext* dataDescriptionEntry(size_t i);

   
  };

  LinkageSectionContext* linkageSection();

  class  CommunicationSectionContext : public antlr4::ParserRuleContext {
  public:
    CommunicationSectionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *COMMUNICATION();
    antlr4::tree::TerminalNode *SECTION();
    antlr4::tree::TerminalNode *DOT_FS();
    std::vector<CommunicationDescriptionEntryContext *> communicationDescriptionEntry();
    CommunicationDescriptionEntryContext* communicationDescriptionEntry(size_t i);
    std::vector<DataDescriptionEntryContext *> dataDescriptionEntry();
    DataDescriptionEntryContext* dataDescriptionEntry(size_t i);

   
  };

  CommunicationSectionContext* communicationSection();

  class  CommunicationDescriptionEntryContext : public antlr4::ParserRuleContext {
  public:
    CommunicationDescriptionEntryContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    CommunicationDescriptionEntryFormat1Context *communicationDescriptionEntryFormat1();
    CommunicationDescriptionEntryFormat2Context *communicationDescriptionEntryFormat2();
    CommunicationDescriptionEntryFormat3Context *communicationDescriptionEntryFormat3();

   
  };

  CommunicationDescriptionEntryContext* communicationDescriptionEntry();

  class  CommunicationDescriptionEntryFormat1Context : public antlr4::ParserRuleContext {
  public:
    CommunicationDescriptionEntryFormat1Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CD();
    CdNameContext *cdName();
    antlr4::tree::TerminalNode *INPUT();
    antlr4::tree::TerminalNode *DOT_FS();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *INITIAL();
    std::vector<DataDescNameContext *> dataDescName();
    DataDescNameContext* dataDescName(size_t i);
    std::vector<SymbolicQueueClauseContext *> symbolicQueueClause();
    SymbolicQueueClauseContext* symbolicQueueClause(size_t i);
    std::vector<SymbolicSubQueueClauseContext *> symbolicSubQueueClause();
    SymbolicSubQueueClauseContext* symbolicSubQueueClause(size_t i);
    std::vector<MessageDateClauseContext *> messageDateClause();
    MessageDateClauseContext* messageDateClause(size_t i);
    std::vector<MessageTimeClauseContext *> messageTimeClause();
    MessageTimeClauseContext* messageTimeClause(size_t i);
    std::vector<SymbolicSourceClauseContext *> symbolicSourceClause();
    SymbolicSourceClauseContext* symbolicSourceClause(size_t i);
    std::vector<TextLengthClauseContext *> textLengthClause();
    TextLengthClauseContext* textLengthClause(size_t i);
    std::vector<EndKeyClauseContext *> endKeyClause();
    EndKeyClauseContext* endKeyClause(size_t i);
    std::vector<StatusKeyClauseContext *> statusKeyClause();
    StatusKeyClauseContext* statusKeyClause(size_t i);
    std::vector<MessageCountClauseContext *> messageCountClause();
    MessageCountClauseContext* messageCountClause(size_t i);

   
  };

  CommunicationDescriptionEntryFormat1Context* communicationDescriptionEntryFormat1();

  class  CommunicationDescriptionEntryFormat2Context : public antlr4::ParserRuleContext {
  public:
    CommunicationDescriptionEntryFormat2Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CD();
    CdNameContext *cdName();
    antlr4::tree::TerminalNode *OUTPUT();
    antlr4::tree::TerminalNode *DOT_FS();
    antlr4::tree::TerminalNode *FOR();
    std::vector<DestinationCountClauseContext *> destinationCountClause();
    DestinationCountClauseContext* destinationCountClause(size_t i);
    std::vector<TextLengthClauseContext *> textLengthClause();
    TextLengthClauseContext* textLengthClause(size_t i);
    std::vector<StatusKeyClauseContext *> statusKeyClause();
    StatusKeyClauseContext* statusKeyClause(size_t i);
    std::vector<DestinationTableClauseContext *> destinationTableClause();
    DestinationTableClauseContext* destinationTableClause(size_t i);
    std::vector<ErrorKeyClauseContext *> errorKeyClause();
    ErrorKeyClauseContext* errorKeyClause(size_t i);
    std::vector<SymbolicDestinationClauseContext *> symbolicDestinationClause();
    SymbolicDestinationClauseContext* symbolicDestinationClause(size_t i);

   
  };

  CommunicationDescriptionEntryFormat2Context* communicationDescriptionEntryFormat2();

  class  CommunicationDescriptionEntryFormat3Context : public antlr4::ParserRuleContext {
  public:
    CommunicationDescriptionEntryFormat3Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CD();
    CdNameContext *cdName();
    antlr4::tree::TerminalNode *INITIAL();
    antlr4::tree::TerminalNode *I_O();
    antlr4::tree::TerminalNode *DOT_FS();
    antlr4::tree::TerminalNode *FOR();
    std::vector<DataDescNameContext *> dataDescName();
    DataDescNameContext* dataDescName(size_t i);
    std::vector<MessageDateClauseContext *> messageDateClause();
    MessageDateClauseContext* messageDateClause(size_t i);
    std::vector<MessageTimeClauseContext *> messageTimeClause();
    MessageTimeClauseContext* messageTimeClause(size_t i);
    std::vector<SymbolicTerminalClauseContext *> symbolicTerminalClause();
    SymbolicTerminalClauseContext* symbolicTerminalClause(size_t i);
    std::vector<TextLengthClauseContext *> textLengthClause();
    TextLengthClauseContext* textLengthClause(size_t i);
    std::vector<EndKeyClauseContext *> endKeyClause();
    EndKeyClauseContext* endKeyClause(size_t i);
    std::vector<StatusKeyClauseContext *> statusKeyClause();
    StatusKeyClauseContext* statusKeyClause(size_t i);

   
  };

  CommunicationDescriptionEntryFormat3Context* communicationDescriptionEntryFormat3();

  class  DestinationCountClauseContext : public antlr4::ParserRuleContext {
  public:
    DestinationCountClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DESTINATION();
    antlr4::tree::TerminalNode *COUNT();
    DataDescNameContext *dataDescName();
    antlr4::tree::TerminalNode *IS();

   
  };

  DestinationCountClauseContext* destinationCountClause();

  class  DestinationTableClauseContext : public antlr4::ParserRuleContext {
  public:
    DestinationTableClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DESTINATION();
    antlr4::tree::TerminalNode *TABLE();
    antlr4::tree::TerminalNode *OCCURS();
    IntegerLiteralContext *integerLiteral();
    antlr4::tree::TerminalNode *TIMES();
    antlr4::tree::TerminalNode *INDEXED();
    antlr4::tree::TerminalNode *BY();
    std::vector<IndexNameContext *> indexName();
    IndexNameContext* indexName(size_t i);

   
  };

  DestinationTableClauseContext* destinationTableClause();

  class  EndKeyClauseContext : public antlr4::ParserRuleContext {
  public:
    EndKeyClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *END();
    antlr4::tree::TerminalNode *KEY();
    DataDescNameContext *dataDescName();
    antlr4::tree::TerminalNode *IS();

   
  };

  EndKeyClauseContext* endKeyClause();

  class  ErrorKeyClauseContext : public antlr4::ParserRuleContext {
  public:
    ErrorKeyClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ERROR();
    antlr4::tree::TerminalNode *KEY();
    DataDescNameContext *dataDescName();
    antlr4::tree::TerminalNode *IS();

   
  };

  ErrorKeyClauseContext* errorKeyClause();

  class  MessageCountClauseContext : public antlr4::ParserRuleContext {
  public:
    MessageCountClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *COUNT();
    DataDescNameContext *dataDescName();
    antlr4::tree::TerminalNode *MESSAGE();
    antlr4::tree::TerminalNode *IS();

   
  };

  MessageCountClauseContext* messageCountClause();

  class  MessageDateClauseContext : public antlr4::ParserRuleContext {
  public:
    MessageDateClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *MESSAGE();
    antlr4::tree::TerminalNode *DATE();
    DataDescNameContext *dataDescName();
    antlr4::tree::TerminalNode *IS();

   
  };

  MessageDateClauseContext* messageDateClause();

  class  MessageTimeClauseContext : public antlr4::ParserRuleContext {
  public:
    MessageTimeClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *MESSAGE();
    antlr4::tree::TerminalNode *TIME();
    DataDescNameContext *dataDescName();
    antlr4::tree::TerminalNode *IS();

   
  };

  MessageTimeClauseContext* messageTimeClause();

  class  StatusKeyClauseContext : public antlr4::ParserRuleContext {
  public:
    StatusKeyClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *STATUS();
    antlr4::tree::TerminalNode *KEY();
    DataDescNameContext *dataDescName();
    antlr4::tree::TerminalNode *IS();

   
  };

  StatusKeyClauseContext* statusKeyClause();

  class  SymbolicDestinationClauseContext : public antlr4::ParserRuleContext {
  public:
    SymbolicDestinationClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DESTINATION();
    DataDescNameContext *dataDescName();
    antlr4::tree::TerminalNode *SYMBOLIC();
    antlr4::tree::TerminalNode *IS();

   
  };

  SymbolicDestinationClauseContext* symbolicDestinationClause();

  class  SymbolicQueueClauseContext : public antlr4::ParserRuleContext {
  public:
    SymbolicQueueClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *QUEUE();
    DataDescNameContext *dataDescName();
    antlr4::tree::TerminalNode *SYMBOLIC();
    antlr4::tree::TerminalNode *IS();

   
  };

  SymbolicQueueClauseContext* symbolicQueueClause();

  class  SymbolicSourceClauseContext : public antlr4::ParserRuleContext {
  public:
    SymbolicSourceClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SOURCE();
    DataDescNameContext *dataDescName();
    antlr4::tree::TerminalNode *SYMBOLIC();
    antlr4::tree::TerminalNode *IS();

   
  };

  SymbolicSourceClauseContext* symbolicSourceClause();

  class  SymbolicTerminalClauseContext : public antlr4::ParserRuleContext {
  public:
    SymbolicTerminalClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TERMINAL();
    DataDescNameContext *dataDescName();
    antlr4::tree::TerminalNode *SYMBOLIC();
    antlr4::tree::TerminalNode *IS();

   
  };

  SymbolicTerminalClauseContext* symbolicTerminalClause();

  class  SymbolicSubQueueClauseContext : public antlr4::ParserRuleContext {
  public:
    SymbolicSubQueueClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    DataDescNameContext *dataDescName();
    antlr4::tree::TerminalNode *SUB_QUEUE_1();
    antlr4::tree::TerminalNode *SUB_QUEUE_2();
    antlr4::tree::TerminalNode *SUB_QUEUE_3();
    antlr4::tree::TerminalNode *SYMBOLIC();
    antlr4::tree::TerminalNode *IS();

   
  };

  SymbolicSubQueueClauseContext* symbolicSubQueueClause();

  class  TextLengthClauseContext : public antlr4::ParserRuleContext {
  public:
    TextLengthClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TEXT();
    antlr4::tree::TerminalNode *LENGTH();
    DataDescNameContext *dataDescName();
    antlr4::tree::TerminalNode *IS();

   
  };

  TextLengthClauseContext* textLengthClause();

  class  LocalStorageSectionContext : public antlr4::ParserRuleContext {
  public:
    LocalStorageSectionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LOCAL_STORAGE();
    antlr4::tree::TerminalNode *SECTION();
    std::vector<antlr4::tree::TerminalNode *> DOT_FS();
    antlr4::tree::TerminalNode* DOT_FS(size_t i);
    antlr4::tree::TerminalNode *LD();
    LocalNameContext *localName();
    std::vector<DataDescriptionEntryContext *> dataDescriptionEntry();
    DataDescriptionEntryContext* dataDescriptionEntry(size_t i);

   
  };

  LocalStorageSectionContext* localStorageSection();

  class  ScreenSectionContext : public antlr4::ParserRuleContext {
  public:
    ScreenSectionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SCREEN();
    antlr4::tree::TerminalNode *SECTION();
    antlr4::tree::TerminalNode *DOT_FS();
    std::vector<ScreenDescriptionEntryContext *> screenDescriptionEntry();
    ScreenDescriptionEntryContext* screenDescriptionEntry(size_t i);

   
  };

  ScreenSectionContext* screenSection();

  class  ScreenDescriptionEntryContext : public antlr4::ParserRuleContext {
  public:
    ScreenDescriptionEntryContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INTEGERLITERAL();
    antlr4::tree::TerminalNode *DOT_FS();
    antlr4::tree::TerminalNode *FILLER();
    ScreenNameContext *screenName();
    std::vector<ScreenDescriptionBlankClauseContext *> screenDescriptionBlankClause();
    ScreenDescriptionBlankClauseContext* screenDescriptionBlankClause(size_t i);
    std::vector<ScreenDescriptionBellClauseContext *> screenDescriptionBellClause();
    ScreenDescriptionBellClauseContext* screenDescriptionBellClause(size_t i);
    std::vector<ScreenDescriptionBlinkClauseContext *> screenDescriptionBlinkClause();
    ScreenDescriptionBlinkClauseContext* screenDescriptionBlinkClause(size_t i);
    std::vector<ScreenDescriptionEraseClauseContext *> screenDescriptionEraseClause();
    ScreenDescriptionEraseClauseContext* screenDescriptionEraseClause(size_t i);
    std::vector<ScreenDescriptionLightClauseContext *> screenDescriptionLightClause();
    ScreenDescriptionLightClauseContext* screenDescriptionLightClause(size_t i);
    std::vector<ScreenDescriptionGridClauseContext *> screenDescriptionGridClause();
    ScreenDescriptionGridClauseContext* screenDescriptionGridClause(size_t i);
    std::vector<ScreenDescriptionReverseVideoClauseContext *> screenDescriptionReverseVideoClause();
    ScreenDescriptionReverseVideoClauseContext* screenDescriptionReverseVideoClause(size_t i);
    std::vector<ScreenDescriptionUnderlineClauseContext *> screenDescriptionUnderlineClause();
    ScreenDescriptionUnderlineClauseContext* screenDescriptionUnderlineClause(size_t i);
    std::vector<ScreenDescriptionSizeClauseContext *> screenDescriptionSizeClause();
    ScreenDescriptionSizeClauseContext* screenDescriptionSizeClause(size_t i);
    std::vector<ScreenDescriptionLineClauseContext *> screenDescriptionLineClause();
    ScreenDescriptionLineClauseContext* screenDescriptionLineClause(size_t i);
    std::vector<ScreenDescriptionColumnClauseContext *> screenDescriptionColumnClause();
    ScreenDescriptionColumnClauseContext* screenDescriptionColumnClause(size_t i);
    std::vector<ScreenDescriptionForegroundColorClauseContext *> screenDescriptionForegroundColorClause();
    ScreenDescriptionForegroundColorClauseContext* screenDescriptionForegroundColorClause(size_t i);
    std::vector<ScreenDescriptionBackgroundColorClauseContext *> screenDescriptionBackgroundColorClause();
    ScreenDescriptionBackgroundColorClauseContext* screenDescriptionBackgroundColorClause(size_t i);
    std::vector<ScreenDescriptionControlClauseContext *> screenDescriptionControlClause();
    ScreenDescriptionControlClauseContext* screenDescriptionControlClause(size_t i);
    std::vector<ScreenDescriptionValueClauseContext *> screenDescriptionValueClause();
    ScreenDescriptionValueClauseContext* screenDescriptionValueClause(size_t i);
    std::vector<ScreenDescriptionPictureClauseContext *> screenDescriptionPictureClause();
    ScreenDescriptionPictureClauseContext* screenDescriptionPictureClause(size_t i);
    std::vector<ScreenDescriptionUsageClauseContext *> screenDescriptionUsageClause();
    ScreenDescriptionUsageClauseContext* screenDescriptionUsageClause(size_t i);
    std::vector<ScreenDescriptionBlankWhenZeroClauseContext *> screenDescriptionBlankWhenZeroClause();
    ScreenDescriptionBlankWhenZeroClauseContext* screenDescriptionBlankWhenZeroClause(size_t i);
    std::vector<ScreenDescriptionJustifiedClauseContext *> screenDescriptionJustifiedClause();
    ScreenDescriptionJustifiedClauseContext* screenDescriptionJustifiedClause(size_t i);
    std::vector<ScreenDescriptionSignClauseContext *> screenDescriptionSignClause();
    ScreenDescriptionSignClauseContext* screenDescriptionSignClause(size_t i);
    std::vector<ScreenDescriptionAutoClauseContext *> screenDescriptionAutoClause();
    ScreenDescriptionAutoClauseContext* screenDescriptionAutoClause(size_t i);
    std::vector<ScreenDescriptionSecureClauseContext *> screenDescriptionSecureClause();
    ScreenDescriptionSecureClauseContext* screenDescriptionSecureClause(size_t i);
    std::vector<ScreenDescriptionRequiredClauseContext *> screenDescriptionRequiredClause();
    ScreenDescriptionRequiredClauseContext* screenDescriptionRequiredClause(size_t i);
    std::vector<ScreenDescriptionPromptClauseContext *> screenDescriptionPromptClause();
    ScreenDescriptionPromptClauseContext* screenDescriptionPromptClause(size_t i);
    std::vector<ScreenDescriptionFullClauseContext *> screenDescriptionFullClause();
    ScreenDescriptionFullClauseContext* screenDescriptionFullClause(size_t i);
    std::vector<ScreenDescriptionZeroFillClauseContext *> screenDescriptionZeroFillClause();
    ScreenDescriptionZeroFillClauseContext* screenDescriptionZeroFillClause(size_t i);
    std::vector<ScreenDescriptionFromClauseContext *> screenDescriptionFromClause();
    ScreenDescriptionFromClauseContext* screenDescriptionFromClause(size_t i);
    std::vector<ScreenDescriptionUsingClauseContext *> screenDescriptionUsingClause();
    ScreenDescriptionUsingClauseContext* screenDescriptionUsingClause(size_t i);

   
  };

  ScreenDescriptionEntryContext* screenDescriptionEntry();

  class  ScreenDescriptionBlankClauseContext : public antlr4::ParserRuleContext {
  public:
    ScreenDescriptionBlankClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BLANK();
    antlr4::tree::TerminalNode *SCREEN();
    antlr4::tree::TerminalNode *LINE();

   
  };

  ScreenDescriptionBlankClauseContext* screenDescriptionBlankClause();

  class  ScreenDescriptionBellClauseContext : public antlr4::ParserRuleContext {
  public:
    ScreenDescriptionBellClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BELL();
    antlr4::tree::TerminalNode *BEEP();

   
  };

  ScreenDescriptionBellClauseContext* screenDescriptionBellClause();

  class  ScreenDescriptionBlinkClauseContext : public antlr4::ParserRuleContext {
  public:
    ScreenDescriptionBlinkClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BLINK();

   
  };

  ScreenDescriptionBlinkClauseContext* screenDescriptionBlinkClause();

  class  ScreenDescriptionEraseClauseContext : public antlr4::ParserRuleContext {
  public:
    ScreenDescriptionEraseClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ERASE();
    antlr4::tree::TerminalNode *EOL();
    antlr4::tree::TerminalNode *EOS();

   
  };

  ScreenDescriptionEraseClauseContext* screenDescriptionEraseClause();

  class  ScreenDescriptionLightClauseContext : public antlr4::ParserRuleContext {
  public:
    ScreenDescriptionLightClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *HIGHLIGHT();
    antlr4::tree::TerminalNode *LOWLIGHT();

   
  };

  ScreenDescriptionLightClauseContext* screenDescriptionLightClause();

  class  ScreenDescriptionGridClauseContext : public antlr4::ParserRuleContext {
  public:
    ScreenDescriptionGridClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GRID();
    antlr4::tree::TerminalNode *LEFTLINE();
    antlr4::tree::TerminalNode *OVERLINE();

   
  };

  ScreenDescriptionGridClauseContext* screenDescriptionGridClause();

  class  ScreenDescriptionReverseVideoClauseContext : public antlr4::ParserRuleContext {
  public:
    ScreenDescriptionReverseVideoClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REVERSE_VIDEO();

   
  };

  ScreenDescriptionReverseVideoClauseContext* screenDescriptionReverseVideoClause();

  class  ScreenDescriptionUnderlineClauseContext : public antlr4::ParserRuleContext {
  public:
    ScreenDescriptionUnderlineClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *UNDERLINE();

   
  };

  ScreenDescriptionUnderlineClauseContext* screenDescriptionUnderlineClause();

  class  ScreenDescriptionSizeClauseContext : public antlr4::ParserRuleContext {
  public:
    ScreenDescriptionSizeClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SIZE();
    IdentifierContext *identifier();
    IntegerLiteralContext *integerLiteral();
    antlr4::tree::TerminalNode *IS();

   
  };

  ScreenDescriptionSizeClauseContext* screenDescriptionSizeClause();

  class  ScreenDescriptionLineClauseContext : public antlr4::ParserRuleContext {
  public:
    ScreenDescriptionLineClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LINE();
    IdentifierContext *identifier();
    IntegerLiteralContext *integerLiteral();
    antlr4::tree::TerminalNode *PLUS();
    antlr4::tree::TerminalNode *PLUSCHAR();
    antlr4::tree::TerminalNode *MINUSCHAR();
    antlr4::tree::TerminalNode *NUMBER();
    antlr4::tree::TerminalNode *IS();

   
  };

  ScreenDescriptionLineClauseContext* screenDescriptionLineClause();

  class  ScreenDescriptionColumnClauseContext : public antlr4::ParserRuleContext {
  public:
    ScreenDescriptionColumnClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *COLUMN();
    antlr4::tree::TerminalNode *COL();
    IdentifierContext *identifier();
    IntegerLiteralContext *integerLiteral();
    antlr4::tree::TerminalNode *PLUS();
    antlr4::tree::TerminalNode *PLUSCHAR();
    antlr4::tree::TerminalNode *MINUSCHAR();
    antlr4::tree::TerminalNode *NUMBER();
    antlr4::tree::TerminalNode *IS();

   
  };

  ScreenDescriptionColumnClauseContext* screenDescriptionColumnClause();

  class  ScreenDescriptionForegroundColorClauseContext : public antlr4::ParserRuleContext {
  public:
    ScreenDescriptionForegroundColorClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FOREGROUND_COLOR();
    antlr4::tree::TerminalNode *FOREGROUND_COLOUR();
    IdentifierContext *identifier();
    IntegerLiteralContext *integerLiteral();
    antlr4::tree::TerminalNode *IS();

   
  };

  ScreenDescriptionForegroundColorClauseContext* screenDescriptionForegroundColorClause();

  class  ScreenDescriptionBackgroundColorClauseContext : public antlr4::ParserRuleContext {
  public:
    ScreenDescriptionBackgroundColorClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BACKGROUND_COLOR();
    antlr4::tree::TerminalNode *BACKGROUND_COLOUR();
    IdentifierContext *identifier();
    IntegerLiteralContext *integerLiteral();
    antlr4::tree::TerminalNode *IS();

   
  };

  ScreenDescriptionBackgroundColorClauseContext* screenDescriptionBackgroundColorClause();

  class  ScreenDescriptionControlClauseContext : public antlr4::ParserRuleContext {
  public:
    ScreenDescriptionControlClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CONTROL();
    IdentifierContext *identifier();
    antlr4::tree::TerminalNode *IS();

   
  };

  ScreenDescriptionControlClauseContext* screenDescriptionControlClause();

  class  ScreenDescriptionValueClauseContext : public antlr4::ParserRuleContext {
  public:
    ScreenDescriptionValueClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    LiteralContext *literal();
    antlr4::tree::TerminalNode *VALUE();
    antlr4::tree::TerminalNode *IS();

   
  };

  ScreenDescriptionValueClauseContext* screenDescriptionValueClause();

  class  ScreenDescriptionPictureClauseContext : public antlr4::ParserRuleContext {
  public:
    ScreenDescriptionPictureClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    PictureStringContext *pictureString();
    antlr4::tree::TerminalNode *PICTURE();
    antlr4::tree::TerminalNode *PIC();
    antlr4::tree::TerminalNode *IS();

   
  };

  ScreenDescriptionPictureClauseContext* screenDescriptionPictureClause();

  class  ScreenDescriptionFromClauseContext : public antlr4::ParserRuleContext {
  public:
    ScreenDescriptionFromClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FROM();
    IdentifierContext *identifier();
    LiteralContext *literal();
    ScreenDescriptionToClauseContext *screenDescriptionToClause();

   
  };

  ScreenDescriptionFromClauseContext* screenDescriptionFromClause();

  class  ScreenDescriptionToClauseContext : public antlr4::ParserRuleContext {
  public:
    ScreenDescriptionToClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TO();
    IdentifierContext *identifier();

   
  };

  ScreenDescriptionToClauseContext* screenDescriptionToClause();

  class  ScreenDescriptionUsingClauseContext : public antlr4::ParserRuleContext {
  public:
    ScreenDescriptionUsingClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *USING();
    IdentifierContext *identifier();

   
  };

  ScreenDescriptionUsingClauseContext* screenDescriptionUsingClause();

  class  ScreenDescriptionUsageClauseContext : public antlr4::ParserRuleContext {
  public:
    ScreenDescriptionUsageClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DISPLAY();
    antlr4::tree::TerminalNode *DISPLAY_1();
    antlr4::tree::TerminalNode *USAGE();
    antlr4::tree::TerminalNode *IS();

   
  };

  ScreenDescriptionUsageClauseContext* screenDescriptionUsageClause();

  class  ScreenDescriptionBlankWhenZeroClauseContext : public antlr4::ParserRuleContext {
  public:
    ScreenDescriptionBlankWhenZeroClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BLANK();
    antlr4::tree::TerminalNode *ZERO();
    antlr4::tree::TerminalNode *WHEN();

   
  };

  ScreenDescriptionBlankWhenZeroClauseContext* screenDescriptionBlankWhenZeroClause();

  class  ScreenDescriptionJustifiedClauseContext : public antlr4::ParserRuleContext {
  public:
    ScreenDescriptionJustifiedClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *JUSTIFIED();
    antlr4::tree::TerminalNode *JUST();
    antlr4::tree::TerminalNode *RIGHT();

   
  };

  ScreenDescriptionJustifiedClauseContext* screenDescriptionJustifiedClause();

  class  ScreenDescriptionSignClauseContext : public antlr4::ParserRuleContext {
  public:
    ScreenDescriptionSignClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEADING();
    antlr4::tree::TerminalNode *TRAILING();
    antlr4::tree::TerminalNode *SIGN();
    antlr4::tree::TerminalNode *SEPARATE();
    antlr4::tree::TerminalNode *IS();
    antlr4::tree::TerminalNode *CHARACTER();

   
  };

  ScreenDescriptionSignClauseContext* screenDescriptionSignClause();

  class  ScreenDescriptionAutoClauseContext : public antlr4::ParserRuleContext {
  public:
    ScreenDescriptionAutoClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *AUTO();
    antlr4::tree::TerminalNode *AUTO_SKIP();

   
  };

  ScreenDescriptionAutoClauseContext* screenDescriptionAutoClause();

  class  ScreenDescriptionSecureClauseContext : public antlr4::ParserRuleContext {
  public:
    ScreenDescriptionSecureClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SECURE();
    antlr4::tree::TerminalNode *NO_ECHO();

   
  };

  ScreenDescriptionSecureClauseContext* screenDescriptionSecureClause();

  class  ScreenDescriptionRequiredClauseContext : public antlr4::ParserRuleContext {
  public:
    ScreenDescriptionRequiredClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REQUIRED();
    antlr4::tree::TerminalNode *EMPTY_CHECK();

   
  };

  ScreenDescriptionRequiredClauseContext* screenDescriptionRequiredClause();

  class  ScreenDescriptionPromptClauseContext : public antlr4::ParserRuleContext {
  public:
    ScreenDescriptionPromptClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PROMPT();
    IdentifierContext *identifier();
    LiteralContext *literal();
    antlr4::tree::TerminalNode *CHARACTER();
    antlr4::tree::TerminalNode *IS();
    ScreenDescriptionPromptOccursClauseContext *screenDescriptionPromptOccursClause();

   
  };

  ScreenDescriptionPromptClauseContext* screenDescriptionPromptClause();

  class  ScreenDescriptionPromptOccursClauseContext : public antlr4::ParserRuleContext {
  public:
    ScreenDescriptionPromptOccursClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OCCURS();
    IntegerLiteralContext *integerLiteral();
    antlr4::tree::TerminalNode *TIMES();

   
  };

  ScreenDescriptionPromptOccursClauseContext* screenDescriptionPromptOccursClause();

  class  ScreenDescriptionFullClauseContext : public antlr4::ParserRuleContext {
  public:
    ScreenDescriptionFullClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FULL();
    antlr4::tree::TerminalNode *LENGTH_CHECK();

   
  };

  ScreenDescriptionFullClauseContext* screenDescriptionFullClause();

  class  ScreenDescriptionZeroFillClauseContext : public antlr4::ParserRuleContext {
  public:
    ScreenDescriptionZeroFillClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ZERO_FILL();

   
  };

  ScreenDescriptionZeroFillClauseContext* screenDescriptionZeroFillClause();

  class  ReportSectionContext : public antlr4::ParserRuleContext {
  public:
    ReportSectionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REPORT();
    antlr4::tree::TerminalNode *SECTION();
    antlr4::tree::TerminalNode *DOT_FS();
    std::vector<ReportDescriptionContext *> reportDescription();
    ReportDescriptionContext* reportDescription(size_t i);

   
  };

  ReportSectionContext* reportSection();

  class  ReportDescriptionContext : public antlr4::ParserRuleContext {
  public:
    ReportDescriptionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ReportDescriptionEntryContext *reportDescriptionEntry();
    std::vector<ReportGroupDescriptionEntryContext *> reportGroupDescriptionEntry();
    ReportGroupDescriptionEntryContext* reportGroupDescriptionEntry(size_t i);

   
  };

  ReportDescriptionContext* reportDescription();

  class  ReportDescriptionEntryContext : public antlr4::ParserRuleContext {
  public:
    ReportDescriptionEntryContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RD();
    ReportNameContext *reportName();
    antlr4::tree::TerminalNode *DOT_FS();
    ReportDescriptionGlobalClauseContext *reportDescriptionGlobalClause();
    ReportDescriptionPageLimitClauseContext *reportDescriptionPageLimitClause();
    ReportDescriptionHeadingClauseContext *reportDescriptionHeadingClause();
    ReportDescriptionFirstDetailClauseContext *reportDescriptionFirstDetailClause();
    ReportDescriptionLastDetailClauseContext *reportDescriptionLastDetailClause();
    ReportDescriptionFootingClauseContext *reportDescriptionFootingClause();

   
  };

  ReportDescriptionEntryContext* reportDescriptionEntry();

  class  ReportDescriptionGlobalClauseContext : public antlr4::ParserRuleContext {
  public:
    ReportDescriptionGlobalClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GLOBAL();
    antlr4::tree::TerminalNode *IS();

   
  };

  ReportDescriptionGlobalClauseContext* reportDescriptionGlobalClause();

  class  ReportDescriptionPageLimitClauseContext : public antlr4::ParserRuleContext {
  public:
    ReportDescriptionPageLimitClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PAGE();
    IntegerLiteralContext *integerLiteral();
    antlr4::tree::TerminalNode *LIMIT();
    antlr4::tree::TerminalNode *LIMITS();
    antlr4::tree::TerminalNode *LINE();
    antlr4::tree::TerminalNode *LINES();
    antlr4::tree::TerminalNode *IS();
    antlr4::tree::TerminalNode *ARE();

   
  };

  ReportDescriptionPageLimitClauseContext* reportDescriptionPageLimitClause();

  class  ReportDescriptionHeadingClauseContext : public antlr4::ParserRuleContext {
  public:
    ReportDescriptionHeadingClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *HEADING();
    IntegerLiteralContext *integerLiteral();

   
  };

  ReportDescriptionHeadingClauseContext* reportDescriptionHeadingClause();

  class  ReportDescriptionFirstDetailClauseContext : public antlr4::ParserRuleContext {
  public:
    ReportDescriptionFirstDetailClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FIRST();
    antlr4::tree::TerminalNode *DETAIL();
    IntegerLiteralContext *integerLiteral();

   
  };

  ReportDescriptionFirstDetailClauseContext* reportDescriptionFirstDetailClause();

  class  ReportDescriptionLastDetailClauseContext : public antlr4::ParserRuleContext {
  public:
    ReportDescriptionLastDetailClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LAST();
    antlr4::tree::TerminalNode *DETAIL();
    IntegerLiteralContext *integerLiteral();

   
  };

  ReportDescriptionLastDetailClauseContext* reportDescriptionLastDetailClause();

  class  ReportDescriptionFootingClauseContext : public antlr4::ParserRuleContext {
  public:
    ReportDescriptionFootingClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FOOTING();
    IntegerLiteralContext *integerLiteral();

   
  };

  ReportDescriptionFootingClauseContext* reportDescriptionFootingClause();

  class  ReportGroupDescriptionEntryContext : public antlr4::ParserRuleContext {
  public:
    ReportGroupDescriptionEntryContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ReportGroupDescriptionEntryFormat1Context *reportGroupDescriptionEntryFormat1();
    ReportGroupDescriptionEntryFormat2Context *reportGroupDescriptionEntryFormat2();
    ReportGroupDescriptionEntryFormat3Context *reportGroupDescriptionEntryFormat3();

   
  };

  ReportGroupDescriptionEntryContext* reportGroupDescriptionEntry();

  class  ReportGroupDescriptionEntryFormat1Context : public antlr4::ParserRuleContext {
  public:
    ReportGroupDescriptionEntryFormat1Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IntegerLiteralContext *integerLiteral();
    DataNameContext *dataName();
    ReportGroupTypeClauseContext *reportGroupTypeClause();
    antlr4::tree::TerminalNode *DOT_FS();
    ReportGroupLineNumberClauseContext *reportGroupLineNumberClause();
    ReportGroupNextGroupClauseContext *reportGroupNextGroupClause();
    ReportGroupUsageClauseContext *reportGroupUsageClause();

   
  };

  ReportGroupDescriptionEntryFormat1Context* reportGroupDescriptionEntryFormat1();

  class  ReportGroupDescriptionEntryFormat2Context : public antlr4::ParserRuleContext {
  public:
    ReportGroupDescriptionEntryFormat2Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IntegerLiteralContext *integerLiteral();
    ReportGroupUsageClauseContext *reportGroupUsageClause();
    antlr4::tree::TerminalNode *DOT_FS();
    DataNameContext *dataName();
    ReportGroupLineNumberClauseContext *reportGroupLineNumberClause();

   
  };

  ReportGroupDescriptionEntryFormat2Context* reportGroupDescriptionEntryFormat2();

  class  ReportGroupDescriptionEntryFormat3Context : public antlr4::ParserRuleContext {
  public:
    ReportGroupDescriptionEntryFormat3Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IntegerLiteralContext *integerLiteral();
    antlr4::tree::TerminalNode *DOT_FS();
    DataNameContext *dataName();
    std::vector<ReportGroupPictureClauseContext *> reportGroupPictureClause();
    ReportGroupPictureClauseContext* reportGroupPictureClause(size_t i);
    std::vector<ReportGroupUsageClauseContext *> reportGroupUsageClause();
    ReportGroupUsageClauseContext* reportGroupUsageClause(size_t i);
    std::vector<ReportGroupSignClauseContext *> reportGroupSignClause();
    ReportGroupSignClauseContext* reportGroupSignClause(size_t i);
    std::vector<ReportGroupJustifiedClauseContext *> reportGroupJustifiedClause();
    ReportGroupJustifiedClauseContext* reportGroupJustifiedClause(size_t i);
    std::vector<ReportGroupBlankWhenZeroClauseContext *> reportGroupBlankWhenZeroClause();
    ReportGroupBlankWhenZeroClauseContext* reportGroupBlankWhenZeroClause(size_t i);
    std::vector<ReportGroupLineNumberClauseContext *> reportGroupLineNumberClause();
    ReportGroupLineNumberClauseContext* reportGroupLineNumberClause(size_t i);
    std::vector<ReportGroupColumnNumberClauseContext *> reportGroupColumnNumberClause();
    ReportGroupColumnNumberClauseContext* reportGroupColumnNumberClause(size_t i);
    std::vector<ReportGroupIndicateClauseContext *> reportGroupIndicateClause();
    ReportGroupIndicateClauseContext* reportGroupIndicateClause(size_t i);
    std::vector<ReportGroupSourceClauseContext *> reportGroupSourceClause();
    ReportGroupSourceClauseContext* reportGroupSourceClause(size_t i);
    std::vector<ReportGroupValueClauseContext *> reportGroupValueClause();
    ReportGroupValueClauseContext* reportGroupValueClause(size_t i);
    std::vector<ReportGroupSumClauseContext *> reportGroupSumClause();
    ReportGroupSumClauseContext* reportGroupSumClause(size_t i);
    std::vector<ReportGroupResetClauseContext *> reportGroupResetClause();
    ReportGroupResetClauseContext* reportGroupResetClause(size_t i);

   
  };

  ReportGroupDescriptionEntryFormat3Context* reportGroupDescriptionEntryFormat3();

  class  ReportGroupBlankWhenZeroClauseContext : public antlr4::ParserRuleContext {
  public:
    ReportGroupBlankWhenZeroClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BLANK();
    antlr4::tree::TerminalNode *ZERO();
    antlr4::tree::TerminalNode *WHEN();

   
  };

  ReportGroupBlankWhenZeroClauseContext* reportGroupBlankWhenZeroClause();

  class  ReportGroupColumnNumberClauseContext : public antlr4::ParserRuleContext {
  public:
    ReportGroupColumnNumberClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *COLUMN();
    IntegerLiteralContext *integerLiteral();
    antlr4::tree::TerminalNode *NUMBER();
    antlr4::tree::TerminalNode *IS();

   
  };

  ReportGroupColumnNumberClauseContext* reportGroupColumnNumberClause();

  class  ReportGroupIndicateClauseContext : public antlr4::ParserRuleContext {
  public:
    ReportGroupIndicateClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GROUP();
    antlr4::tree::TerminalNode *INDICATE();

   
  };

  ReportGroupIndicateClauseContext* reportGroupIndicateClause();

  class  ReportGroupJustifiedClauseContext : public antlr4::ParserRuleContext {
  public:
    ReportGroupJustifiedClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *JUSTIFIED();
    antlr4::tree::TerminalNode *JUST();
    antlr4::tree::TerminalNode *RIGHT();

   
  };

  ReportGroupJustifiedClauseContext* reportGroupJustifiedClause();

  class  ReportGroupLineNumberClauseContext : public antlr4::ParserRuleContext {
  public:
    ReportGroupLineNumberClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ReportGroupLineNumberNextPageContext *reportGroupLineNumberNextPage();
    ReportGroupLineNumberPlusContext *reportGroupLineNumberPlus();
    antlr4::tree::TerminalNode *LINE();
    antlr4::tree::TerminalNode *NUMBER();
    antlr4::tree::TerminalNode *IS();

   
  };

  ReportGroupLineNumberClauseContext* reportGroupLineNumberClause();

  class  ReportGroupLineNumberNextPageContext : public antlr4::ParserRuleContext {
  public:
    ReportGroupLineNumberNextPageContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IntegerLiteralContext *integerLiteral();
    antlr4::tree::TerminalNode *NEXT();
    antlr4::tree::TerminalNode *PAGE();
    antlr4::tree::TerminalNode *ON();

   
  };

  ReportGroupLineNumberNextPageContext* reportGroupLineNumberNextPage();

  class  ReportGroupLineNumberPlusContext : public antlr4::ParserRuleContext {
  public:
    ReportGroupLineNumberPlusContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PLUS();
    IntegerLiteralContext *integerLiteral();

   
  };

  ReportGroupLineNumberPlusContext* reportGroupLineNumberPlus();

  class  ReportGroupNextGroupClauseContext : public antlr4::ParserRuleContext {
  public:
    ReportGroupNextGroupClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NEXT();
    antlr4::tree::TerminalNode *GROUP();
    IntegerLiteralContext *integerLiteral();
    ReportGroupNextGroupNextPageContext *reportGroupNextGroupNextPage();
    ReportGroupNextGroupPlusContext *reportGroupNextGroupPlus();
    antlr4::tree::TerminalNode *IS();

   
  };

  ReportGroupNextGroupClauseContext* reportGroupNextGroupClause();

  class  ReportGroupNextGroupPlusContext : public antlr4::ParserRuleContext {
  public:
    ReportGroupNextGroupPlusContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PLUS();
    IntegerLiteralContext *integerLiteral();

   
  };

  ReportGroupNextGroupPlusContext* reportGroupNextGroupPlus();

  class  ReportGroupNextGroupNextPageContext : public antlr4::ParserRuleContext {
  public:
    ReportGroupNextGroupNextPageContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NEXT();
    antlr4::tree::TerminalNode *PAGE();

   
  };

  ReportGroupNextGroupNextPageContext* reportGroupNextGroupNextPage();

  class  ReportGroupPictureClauseContext : public antlr4::ParserRuleContext {
  public:
    ReportGroupPictureClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    PictureStringContext *pictureString();
    antlr4::tree::TerminalNode *PICTURE();
    antlr4::tree::TerminalNode *PIC();
    antlr4::tree::TerminalNode *IS();

   
  };

  ReportGroupPictureClauseContext* reportGroupPictureClause();

  class  ReportGroupResetClauseContext : public antlr4::ParserRuleContext {
  public:
    ReportGroupResetClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RESET();
    antlr4::tree::TerminalNode *FINAL();
    DataNameContext *dataName();
    antlr4::tree::TerminalNode *ON();

   
  };

  ReportGroupResetClauseContext* reportGroupResetClause();

  class  ReportGroupSignClauseContext : public antlr4::ParserRuleContext {
  public:
    ReportGroupSignClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SIGN();
    antlr4::tree::TerminalNode *SEPARATE();
    antlr4::tree::TerminalNode *LEADING();
    antlr4::tree::TerminalNode *TRAILING();
    antlr4::tree::TerminalNode *IS();
    antlr4::tree::TerminalNode *CHARACTER();

   
  };

  ReportGroupSignClauseContext* reportGroupSignClause();

  class  ReportGroupSourceClauseContext : public antlr4::ParserRuleContext {
  public:
    ReportGroupSourceClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SOURCE();
    IdentifierContext *identifier();
    antlr4::tree::TerminalNode *IS();

   
  };

  ReportGroupSourceClauseContext* reportGroupSourceClause();

  class  ReportGroupSumClauseContext : public antlr4::ParserRuleContext {
  public:
    ReportGroupSumClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SUM();
    std::vector<IdentifierContext *> identifier();
    IdentifierContext* identifier(size_t i);
    antlr4::tree::TerminalNode *UPON();
    std::vector<DataNameContext *> dataName();
    DataNameContext* dataName(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMACHAR();
    antlr4::tree::TerminalNode* COMMACHAR(size_t i);

   
  };

  ReportGroupSumClauseContext* reportGroupSumClause();

  class  ReportGroupTypeClauseContext : public antlr4::ParserRuleContext {
  public:
    ReportGroupTypeClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TYPE();
    ReportGroupTypeReportHeadingContext *reportGroupTypeReportHeading();
    ReportGroupTypePageHeadingContext *reportGroupTypePageHeading();
    ReportGroupTypeControlHeadingContext *reportGroupTypeControlHeading();
    ReportGroupTypeDetailContext *reportGroupTypeDetail();
    ReportGroupTypeControlFootingContext *reportGroupTypeControlFooting();
    ReportGroupTypePageFootingContext *reportGroupTypePageFooting();
    ReportGroupTypeReportFootingContext *reportGroupTypeReportFooting();
    antlr4::tree::TerminalNode *IS();

   
  };

  ReportGroupTypeClauseContext* reportGroupTypeClause();

  class  ReportGroupTypeReportHeadingContext : public antlr4::ParserRuleContext {
  public:
    ReportGroupTypeReportHeadingContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REPORT();
    antlr4::tree::TerminalNode *HEADING();
    antlr4::tree::TerminalNode *RH();

   
  };

  ReportGroupTypeReportHeadingContext* reportGroupTypeReportHeading();

  class  ReportGroupTypePageHeadingContext : public antlr4::ParserRuleContext {
  public:
    ReportGroupTypePageHeadingContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PAGE();
    antlr4::tree::TerminalNode *HEADING();
    antlr4::tree::TerminalNode *PH();

   
  };

  ReportGroupTypePageHeadingContext* reportGroupTypePageHeading();

  class  ReportGroupTypeControlHeadingContext : public antlr4::ParserRuleContext {
  public:
    ReportGroupTypeControlHeadingContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CONTROL();
    antlr4::tree::TerminalNode *HEADING();
    antlr4::tree::TerminalNode *CH();
    antlr4::tree::TerminalNode *FINAL();
    DataNameContext *dataName();

   
  };

  ReportGroupTypeControlHeadingContext* reportGroupTypeControlHeading();

  class  ReportGroupTypeDetailContext : public antlr4::ParserRuleContext {
  public:
    ReportGroupTypeDetailContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DETAIL();
    antlr4::tree::TerminalNode *DE();

   
  };

  ReportGroupTypeDetailContext* reportGroupTypeDetail();

  class  ReportGroupTypeControlFootingContext : public antlr4::ParserRuleContext {
  public:
    ReportGroupTypeControlFootingContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CONTROL();
    antlr4::tree::TerminalNode *FOOTING();
    antlr4::tree::TerminalNode *CF();
    antlr4::tree::TerminalNode *FINAL();
    DataNameContext *dataName();

   
  };

  ReportGroupTypeControlFootingContext* reportGroupTypeControlFooting();

  class  ReportGroupUsageClauseContext : public antlr4::ParserRuleContext {
  public:
    ReportGroupUsageClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DISPLAY();
    antlr4::tree::TerminalNode *DISPLAY_1();
    antlr4::tree::TerminalNode *USAGE();
    antlr4::tree::TerminalNode *IS();

   
  };

  ReportGroupUsageClauseContext* reportGroupUsageClause();

  class  ReportGroupTypePageFootingContext : public antlr4::ParserRuleContext {
  public:
    ReportGroupTypePageFootingContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PAGE();
    antlr4::tree::TerminalNode *FOOTING();
    antlr4::tree::TerminalNode *PF();

   
  };

  ReportGroupTypePageFootingContext* reportGroupTypePageFooting();

  class  ReportGroupTypeReportFootingContext : public antlr4::ParserRuleContext {
  public:
    ReportGroupTypeReportFootingContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REPORT();
    antlr4::tree::TerminalNode *FOOTING();
    antlr4::tree::TerminalNode *RF();

   
  };

  ReportGroupTypeReportFootingContext* reportGroupTypeReportFooting();

  class  ReportGroupValueClauseContext : public antlr4::ParserRuleContext {
  public:
    ReportGroupValueClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *VALUE();
    LiteralContext *literal();
    antlr4::tree::TerminalNode *IS();

   
  };

  ReportGroupValueClauseContext* reportGroupValueClause();

  class  ProgramLibrarySectionContext : public antlr4::ParserRuleContext {
  public:
    ProgramLibrarySectionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PROGRAM_LIBRARY();
    antlr4::tree::TerminalNode *SECTION();
    antlr4::tree::TerminalNode *DOT_FS();
    std::vector<LibraryDescriptionEntryContext *> libraryDescriptionEntry();
    LibraryDescriptionEntryContext* libraryDescriptionEntry(size_t i);

   
  };

  ProgramLibrarySectionContext* programLibrarySection();

  class  LibraryDescriptionEntryContext : public antlr4::ParserRuleContext {
  public:
    LibraryDescriptionEntryContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    LibraryDescriptionEntryFormat1Context *libraryDescriptionEntryFormat1();
    LibraryDescriptionEntryFormat2Context *libraryDescriptionEntryFormat2();

   
  };

  LibraryDescriptionEntryContext* libraryDescriptionEntry();

  class  LibraryDescriptionEntryFormat1Context : public antlr4::ParserRuleContext {
  public:
    LibraryDescriptionEntryFormat1Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LD();
    LibraryNameContext *libraryName();
    antlr4::tree::TerminalNode *EXPORT();
    LibraryAttributeClauseFormat1Context *libraryAttributeClauseFormat1();
    LibraryEntryProcedureClauseFormat1Context *libraryEntryProcedureClauseFormat1();

   
  };

  LibraryDescriptionEntryFormat1Context* libraryDescriptionEntryFormat1();

  class  LibraryDescriptionEntryFormat2Context : public antlr4::ParserRuleContext {
  public:
    LibraryDescriptionEntryFormat2Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LB();
    LibraryNameContext *libraryName();
    antlr4::tree::TerminalNode *IMPORT();
    LibraryIsGlobalClauseContext *libraryIsGlobalClause();
    LibraryIsCommonClauseContext *libraryIsCommonClause();
    std::vector<LibraryAttributeClauseFormat2Context *> libraryAttributeClauseFormat2();
    LibraryAttributeClauseFormat2Context* libraryAttributeClauseFormat2(size_t i);
    std::vector<LibraryEntryProcedureClauseFormat2Context *> libraryEntryProcedureClauseFormat2();
    LibraryEntryProcedureClauseFormat2Context* libraryEntryProcedureClauseFormat2(size_t i);

   
  };

  LibraryDescriptionEntryFormat2Context* libraryDescriptionEntryFormat2();

  class  LibraryAttributeClauseFormat1Context : public antlr4::ParserRuleContext {
  public:
    LibraryAttributeClauseFormat1Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ATTRIBUTE();
    antlr4::tree::TerminalNode *SHARING();
    antlr4::tree::TerminalNode *DONTCARE();
    antlr4::tree::TerminalNode *PRIVATE();
    antlr4::tree::TerminalNode *SHAREDBYRUNUNIT();
    antlr4::tree::TerminalNode *SHAREDBYALL();
    antlr4::tree::TerminalNode *IS();

   
  };

  LibraryAttributeClauseFormat1Context* libraryAttributeClauseFormat1();

  class  LibraryAttributeClauseFormat2Context : public antlr4::ParserRuleContext {
  public:
    LibraryAttributeClauseFormat2Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ATTRIBUTE();
    LibraryAttributeFunctionContext *libraryAttributeFunction();
    antlr4::tree::TerminalNode *LIBACCESS();
    LibraryAttributeParameterContext *libraryAttributeParameter();
    LibraryAttributeTitleContext *libraryAttributeTitle();
    antlr4::tree::TerminalNode *BYFUNCTION();
    antlr4::tree::TerminalNode *BYTITLE();
    antlr4::tree::TerminalNode *IS();

   
  };

  LibraryAttributeClauseFormat2Context* libraryAttributeClauseFormat2();

  class  LibraryAttributeFunctionContext : public antlr4::ParserRuleContext {
  public:
    LibraryAttributeFunctionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FUNCTIONNAME();
    antlr4::tree::TerminalNode *IS();
    LiteralContext *literal();

   
  };

  LibraryAttributeFunctionContext* libraryAttributeFunction();

  class  LibraryAttributeParameterContext : public antlr4::ParserRuleContext {
  public:
    LibraryAttributeParameterContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LIBPARAMETER();
    LiteralContext *literal();
    antlr4::tree::TerminalNode *IS();

   
  };

  LibraryAttributeParameterContext* libraryAttributeParameter();

  class  LibraryAttributeTitleContext : public antlr4::ParserRuleContext {
  public:
    LibraryAttributeTitleContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TITLE();
    LiteralContext *literal();
    antlr4::tree::TerminalNode *IS();

   
  };

  LibraryAttributeTitleContext* libraryAttributeTitle();

  class  LibraryEntryProcedureClauseFormat1Context : public antlr4::ParserRuleContext {
  public:
    LibraryEntryProcedureClauseFormat1Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ENTRY_PROCEDURE();
    ProgramNameContext *programName();
    LibraryEntryProcedureForClauseContext *libraryEntryProcedureForClause();

   
  };

  LibraryEntryProcedureClauseFormat1Context* libraryEntryProcedureClauseFormat1();

  class  LibraryEntryProcedureClauseFormat2Context : public antlr4::ParserRuleContext {
  public:
    LibraryEntryProcedureClauseFormat2Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ENTRY_PROCEDURE();
    ProgramNameContext *programName();
    LibraryEntryProcedureForClauseContext *libraryEntryProcedureForClause();
    LibraryEntryProcedureWithClauseContext *libraryEntryProcedureWithClause();
    LibraryEntryProcedureUsingClauseContext *libraryEntryProcedureUsingClause();
    LibraryEntryProcedureGivingClauseContext *libraryEntryProcedureGivingClause();

   
  };

  LibraryEntryProcedureClauseFormat2Context* libraryEntryProcedureClauseFormat2();

  class  LibraryEntryProcedureForClauseContext : public antlr4::ParserRuleContext {
  public:
    LibraryEntryProcedureForClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FOR();
    LiteralContext *literal();

   
  };

  LibraryEntryProcedureForClauseContext* libraryEntryProcedureForClause();

  class  LibraryEntryProcedureGivingClauseContext : public antlr4::ParserRuleContext {
  public:
    LibraryEntryProcedureGivingClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GIVING();
    DataNameContext *dataName();

   
  };

  LibraryEntryProcedureGivingClauseContext* libraryEntryProcedureGivingClause();

  class  LibraryEntryProcedureUsingClauseContext : public antlr4::ParserRuleContext {
  public:
    LibraryEntryProcedureUsingClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *USING();
    std::vector<LibraryEntryProcedureUsingNameContext *> libraryEntryProcedureUsingName();
    LibraryEntryProcedureUsingNameContext* libraryEntryProcedureUsingName(size_t i);

   
  };

  LibraryEntryProcedureUsingClauseContext* libraryEntryProcedureUsingClause();

  class  LibraryEntryProcedureUsingNameContext : public antlr4::ParserRuleContext {
  public:
    LibraryEntryProcedureUsingNameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    DataNameContext *dataName();
    FileNameContext *fileName();

   
  };

  LibraryEntryProcedureUsingNameContext* libraryEntryProcedureUsingName();

  class  LibraryEntryProcedureWithClauseContext : public antlr4::ParserRuleContext {
  public:
    LibraryEntryProcedureWithClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WITH();
    std::vector<LibraryEntryProcedureWithNameContext *> libraryEntryProcedureWithName();
    LibraryEntryProcedureWithNameContext* libraryEntryProcedureWithName(size_t i);

   
  };

  LibraryEntryProcedureWithClauseContext* libraryEntryProcedureWithClause();

  class  LibraryEntryProcedureWithNameContext : public antlr4::ParserRuleContext {
  public:
    LibraryEntryProcedureWithNameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    LocalNameContext *localName();
    FileNameContext *fileName();

   
  };

  LibraryEntryProcedureWithNameContext* libraryEntryProcedureWithName();

  class  LibraryIsCommonClauseContext : public antlr4::ParserRuleContext {
  public:
    LibraryIsCommonClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *COMMON();
    antlr4::tree::TerminalNode *IS();

   
  };

  LibraryIsCommonClauseContext* libraryIsCommonClause();

  class  LibraryIsGlobalClauseContext : public antlr4::ParserRuleContext {
  public:
    LibraryIsGlobalClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GLOBAL();
    antlr4::tree::TerminalNode *IS();

   
  };

  LibraryIsGlobalClauseContext* libraryIsGlobalClause();

  class  DataDescriptionEntryContext : public antlr4::ParserRuleContext {
  public:
    DataDescriptionEntryContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    CopyDirectiveContext *copyDirective();
    DataDescriptionEntryFormat1Context *dataDescriptionEntryFormat1();
    DataDescriptionEntryFormat2Context *dataDescriptionEntryFormat2();
    DataDescriptionEntryFormat3Context *dataDescriptionEntryFormat3();
    DataDescriptionEntryExecSqlContext *dataDescriptionEntryExecSql();

   
  };

  DataDescriptionEntryContext* dataDescriptionEntry();

  class  DataDescriptionEntryFormat1Context : public antlr4::ParserRuleContext {
  public:
    DataDescriptionEntryFormat1Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DOT_FS();
    antlr4::tree::TerminalNode *INTEGERLITERAL();
    antlr4::tree::TerminalNode *LEVEL_NUMBER_77();
    antlr4::tree::TerminalNode *FILLER();
    DataNameContext *dataName();
    std::vector<DataRedefinesClauseContext *> dataRedefinesClause();
    DataRedefinesClauseContext* dataRedefinesClause(size_t i);
    std::vector<DataIntegerStringClauseContext *> dataIntegerStringClause();
    DataIntegerStringClauseContext* dataIntegerStringClause(size_t i);
    std::vector<DataExternalClauseContext *> dataExternalClause();
    DataExternalClauseContext* dataExternalClause(size_t i);
    std::vector<DataGlobalClauseContext *> dataGlobalClause();
    DataGlobalClauseContext* dataGlobalClause(size_t i);
    std::vector<DataTypeDefClauseContext *> dataTypeDefClause();
    DataTypeDefClauseContext* dataTypeDefClause(size_t i);
    std::vector<DataThreadLocalClauseContext *> dataThreadLocalClause();
    DataThreadLocalClauseContext* dataThreadLocalClause(size_t i);
    std::vector<DataPictureClauseContext *> dataPictureClause();
    DataPictureClauseContext* dataPictureClause(size_t i);
    std::vector<DataCommonOwnLocalClauseContext *> dataCommonOwnLocalClause();
    DataCommonOwnLocalClauseContext* dataCommonOwnLocalClause(size_t i);
    std::vector<DataTypeClauseContext *> dataTypeClause();
    DataTypeClauseContext* dataTypeClause(size_t i);
    std::vector<DataUsingClauseContext *> dataUsingClause();
    DataUsingClauseContext* dataUsingClause(size_t i);
    std::vector<DataUsageClauseContext *> dataUsageClause();
    DataUsageClauseContext* dataUsageClause(size_t i);
    std::vector<DataValueClauseContext *> dataValueClause();
    DataValueClauseContext* dataValueClause(size_t i);
    std::vector<DataReceivedByClauseContext *> dataReceivedByClause();
    DataReceivedByClauseContext* dataReceivedByClause(size_t i);
    std::vector<DataOccursClauseContext *> dataOccursClause();
    DataOccursClauseContext* dataOccursClause(size_t i);
    std::vector<DataSignClauseContext *> dataSignClause();
    DataSignClauseContext* dataSignClause(size_t i);
    std::vector<DataSynchronizedClauseContext *> dataSynchronizedClause();
    DataSynchronizedClauseContext* dataSynchronizedClause(size_t i);
    std::vector<DataJustifiedClauseContext *> dataJustifiedClause();
    DataJustifiedClauseContext* dataJustifiedClause(size_t i);
    std::vector<DataBlankWhenZeroClauseContext *> dataBlankWhenZeroClause();
    DataBlankWhenZeroClauseContext* dataBlankWhenZeroClause(size_t i);
    std::vector<DataWithLowerBoundsClauseContext *> dataWithLowerBoundsClause();
    DataWithLowerBoundsClauseContext* dataWithLowerBoundsClause(size_t i);
    std::vector<DataAlignedClauseContext *> dataAlignedClause();
    DataAlignedClauseContext* dataAlignedClause(size_t i);
    std::vector<DataRecordAreaClauseContext *> dataRecordAreaClause();
    DataRecordAreaClauseContext* dataRecordAreaClause(size_t i);

   
  };

  DataDescriptionEntryFormat1Context* dataDescriptionEntryFormat1();

  class  DataDescriptionEntryFormat2Context : public antlr4::ParserRuleContext {
  public:
    DataDescriptionEntryFormat2Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEVEL_NUMBER_66();
    DataNameContext *dataName();
    DataRenamesClauseContext *dataRenamesClause();
    antlr4::tree::TerminalNode *DOT_FS();

   
  };

  DataDescriptionEntryFormat2Context* dataDescriptionEntryFormat2();

  class  DataDescriptionEntryFormat3Context : public antlr4::ParserRuleContext {
  public:
    DataDescriptionEntryFormat3Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEVEL_NUMBER_88();
    ConditionNameContext *conditionName();
    DataValueClauseContext *dataValueClause();
    antlr4::tree::TerminalNode *DOT_FS();

   
  };

  DataDescriptionEntryFormat3Context* dataDescriptionEntryFormat3();

  class  DataDescriptionEntryExecSqlContext : public antlr4::ParserRuleContext {
  public:
    DataDescriptionEntryExecSqlContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<antlr4::tree::TerminalNode *> EXECSQLLINE();
    antlr4::tree::TerminalNode* EXECSQLLINE(size_t i);
    antlr4::tree::TerminalNode *DOT_FS();

   
  };

  DataDescriptionEntryExecSqlContext* dataDescriptionEntryExecSql();

  class  DataAlignedClauseContext : public antlr4::ParserRuleContext {
  public:
    DataAlignedClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALIGNED();

   
  };

  DataAlignedClauseContext* dataAlignedClause();

  class  DataBlankWhenZeroClauseContext : public antlr4::ParserRuleContext {
  public:
    DataBlankWhenZeroClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BLANK();
    antlr4::tree::TerminalNode *ZERO();
    antlr4::tree::TerminalNode *ZEROS();
    antlr4::tree::TerminalNode *ZEROES();
    antlr4::tree::TerminalNode *WHEN();

   
  };

  DataBlankWhenZeroClauseContext* dataBlankWhenZeroClause();

  class  DataCommonOwnLocalClauseContext : public antlr4::ParserRuleContext {
  public:
    DataCommonOwnLocalClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *COMMON();
    antlr4::tree::TerminalNode *OWN();
    antlr4::tree::TerminalNode *LOCAL();

   
  };

  DataCommonOwnLocalClauseContext* dataCommonOwnLocalClause();

  class  DataExternalClauseContext : public antlr4::ParserRuleContext {
  public:
    DataExternalClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *EXTERNAL();
    antlr4::tree::TerminalNode *IS();
    antlr4::tree::TerminalNode *BY();
    LiteralContext *literal();

   
  };

  DataExternalClauseContext* dataExternalClause();

  class  DataGlobalClauseContext : public antlr4::ParserRuleContext {
  public:
    DataGlobalClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GLOBAL();
    antlr4::tree::TerminalNode *IS();

   
  };

  DataGlobalClauseContext* dataGlobalClause();

  class  DataIntegerStringClauseContext : public antlr4::ParserRuleContext {
  public:
    DataIntegerStringClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INTEGER();
    antlr4::tree::TerminalNode *STRING();

   
  };

  DataIntegerStringClauseContext* dataIntegerStringClause();

  class  DataJustifiedClauseContext : public antlr4::ParserRuleContext {
  public:
    DataJustifiedClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *JUSTIFIED();
    antlr4::tree::TerminalNode *JUST();
    antlr4::tree::TerminalNode *RIGHT();

   
  };

  DataJustifiedClauseContext* dataJustifiedClause();

  class  DataOccursClauseContext : public antlr4::ParserRuleContext {
  public:
    DataOccursClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OCCURS();
    IntegerLiteralContext *integerLiteral();
    DataOccursToContext *dataOccursTo();
    antlr4::tree::TerminalNode *TIMES();
    antlr4::tree::TerminalNode *DEPENDING();
    QualifiedDataNameContext *qualifiedDataName();
    std::vector<DataOccursSortContext *> dataOccursSort();
    DataOccursSortContext* dataOccursSort(size_t i);
    antlr4::tree::TerminalNode *INDEXED();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *BY();
    antlr4::tree::TerminalNode *LOCAL();
    std::vector<IndexNameContext *> indexName();
    IndexNameContext* indexName(size_t i);

   
  };

  DataOccursClauseContext* dataOccursClause();

  class  DataOccursToContext : public antlr4::ParserRuleContext {
  public:
    DataOccursToContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TO();
    IntegerLiteralContext *integerLiteral();

   
  };

  DataOccursToContext* dataOccursTo();

  class  DataOccursSortContext : public antlr4::ParserRuleContext {
  public:
    DataOccursSortContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ASCENDING();
    antlr4::tree::TerminalNode *DESCENDING();
    antlr4::tree::TerminalNode *KEY();
    antlr4::tree::TerminalNode *IS();
    std::vector<QualifiedDataNameContext *> qualifiedDataName();
    QualifiedDataNameContext* qualifiedDataName(size_t i);

   
  };

  DataOccursSortContext* dataOccursSort();

  class  DataPictureClauseContext : public antlr4::ParserRuleContext {
  public:
    DataPictureClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    PictureStringContext *pictureString();
    antlr4::tree::TerminalNode *PICTURE();
    antlr4::tree::TerminalNode *PIC();
    antlr4::tree::TerminalNode *IS();

   
  };

  DataPictureClauseContext* dataPictureClause();

  class  PictureStringContext : public antlr4::ParserRuleContext {
  public:
    PictureStringContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<PictureCharsContext *> pictureChars();
    PictureCharsContext* pictureChars(size_t i);
    std::vector<PictureCardinalityContext *> pictureCardinality();
    PictureCardinalityContext* pictureCardinality(size_t i);

   
  };

  PictureStringContext* pictureString();

  class  PictureCharsContext : public antlr4::ParserRuleContext {
  public:
    PictureCharsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DOLLARCHAR();
    antlr4::tree::TerminalNode *IDENTIFIER();
    antlr4::tree::TerminalNode *NUMERICLITERAL();
    antlr4::tree::TerminalNode *SLASHCHAR();
    antlr4::tree::TerminalNode *COMMACHAR();
    antlr4::tree::TerminalNode *DOT();
    antlr4::tree::TerminalNode *COLONCHAR();
    antlr4::tree::TerminalNode *ASTERISKCHAR();
    antlr4::tree::TerminalNode *DOUBLEASTERISKCHAR();
    antlr4::tree::TerminalNode *LPARENCHAR();
    antlr4::tree::TerminalNode *RPARENCHAR();
    antlr4::tree::TerminalNode *PLUSCHAR();
    antlr4::tree::TerminalNode *MINUSCHAR();
    antlr4::tree::TerminalNode *LESSTHANCHAR();
    antlr4::tree::TerminalNode *MORETHANCHAR();
    IntegerLiteralContext *integerLiteral();

   
  };

  PictureCharsContext* pictureChars();

  class  PictureCardinalityContext : public antlr4::ParserRuleContext {
  public:
    PictureCardinalityContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LPARENCHAR();
    IntegerLiteralContext *integerLiteral();
    antlr4::tree::TerminalNode *RPARENCHAR();

   
  };

  PictureCardinalityContext* pictureCardinality();

  class  DataReceivedByClauseContext : public antlr4::ParserRuleContext {
  public:
    DataReceivedByClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CONTENT();
    antlr4::tree::TerminalNode *REFERENCE();
    antlr4::tree::TerminalNode *REF();
    antlr4::tree::TerminalNode *RECEIVED();
    antlr4::tree::TerminalNode *BY();

   
  };

  DataReceivedByClauseContext* dataReceivedByClause();

  class  DataRecordAreaClauseContext : public antlr4::ParserRuleContext {
  public:
    DataRecordAreaClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RECORD();
    antlr4::tree::TerminalNode *AREA();

   
  };

  DataRecordAreaClauseContext* dataRecordAreaClause();

  class  DataRedefinesClauseContext : public antlr4::ParserRuleContext {
  public:
    DataRedefinesClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REDEFINES();
    DataNameContext *dataName();

   
  };

  DataRedefinesClauseContext* dataRedefinesClause();

  class  DataRenamesClauseContext : public antlr4::ParserRuleContext {
  public:
    DataRenamesClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RENAMES();
    std::vector<QualifiedDataNameContext *> qualifiedDataName();
    QualifiedDataNameContext* qualifiedDataName(size_t i);
    antlr4::tree::TerminalNode *THROUGH();
    antlr4::tree::TerminalNode *THRU();

   
  };

  DataRenamesClauseContext* dataRenamesClause();

  class  DataSignClauseContext : public antlr4::ParserRuleContext {
  public:
    DataSignClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEADING();
    antlr4::tree::TerminalNode *TRAILING();
    antlr4::tree::TerminalNode *SIGN();
    antlr4::tree::TerminalNode *SEPARATE();
    antlr4::tree::TerminalNode *IS();
    antlr4::tree::TerminalNode *CHARACTER();

   
  };

  DataSignClauseContext* dataSignClause();

  class  DataSynchronizedClauseContext : public antlr4::ParserRuleContext {
  public:
    DataSynchronizedClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SYNCHRONIZED();
    antlr4::tree::TerminalNode *SYNC();
    antlr4::tree::TerminalNode *LEFT();
    antlr4::tree::TerminalNode *RIGHT();

   
  };

  DataSynchronizedClauseContext* dataSynchronizedClause();

  class  DataThreadLocalClauseContext : public antlr4::ParserRuleContext {
  public:
    DataThreadLocalClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *THREAD_LOCAL();
    antlr4::tree::TerminalNode *IS();

   
  };

  DataThreadLocalClauseContext* dataThreadLocalClause();

  class  DataTypeClauseContext : public antlr4::ParserRuleContext {
  public:
    DataTypeClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TYPE();
    antlr4::tree::TerminalNode *SHORT_DATE();
    antlr4::tree::TerminalNode *LONG_DATE();
    antlr4::tree::TerminalNode *NUMERIC_DATE();
    antlr4::tree::TerminalNode *NUMERIC_TIME();
    antlr4::tree::TerminalNode *LONG_TIME();
    antlr4::tree::TerminalNode *IS();

   
  };

  DataTypeClauseContext* dataTypeClause();

  class  DataTypeDefClauseContext : public antlr4::ParserRuleContext {
  public:
    DataTypeDefClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TYPEDEF();
    antlr4::tree::TerminalNode *IS();

   
  };

  DataTypeDefClauseContext* dataTypeDefClause();

  class  DataUsageClauseContext : public antlr4::ParserRuleContext {
  public:
    DataUsageClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BINARY();
    antlr4::tree::TerminalNode *BIT();
    antlr4::tree::TerminalNode *COMP();
    antlr4::tree::TerminalNode *COMP_1();
    antlr4::tree::TerminalNode *COMP_2();
    antlr4::tree::TerminalNode *COMP_3();
    antlr4::tree::TerminalNode *COMP_4();
    antlr4::tree::TerminalNode *COMP_5();
    antlr4::tree::TerminalNode *COMPUTATIONAL();
    antlr4::tree::TerminalNode *COMPUTATIONAL_1();
    antlr4::tree::TerminalNode *COMPUTATIONAL_2();
    antlr4::tree::TerminalNode *COMPUTATIONAL_3();
    antlr4::tree::TerminalNode *COMPUTATIONAL_4();
    antlr4::tree::TerminalNode *COMPUTATIONAL_5();
    antlr4::tree::TerminalNode *CONTROL_POINT();
    antlr4::tree::TerminalNode *DATE();
    antlr4::tree::TerminalNode *DISPLAY();
    antlr4::tree::TerminalNode *DISPLAY_1();
    antlr4::tree::TerminalNode *DOUBLE();
    antlr4::tree::TerminalNode *EVENT();
    antlr4::tree::TerminalNode *FUNCTION_POINTER();
    antlr4::tree::TerminalNode *INDEX();
    antlr4::tree::TerminalNode *KANJI();
    antlr4::tree::TerminalNode *LOCK();
    antlr4::tree::TerminalNode *NATIONAL();
    antlr4::tree::TerminalNode *PACKED_DECIMAL();
    antlr4::tree::TerminalNode *POINTER();
    antlr4::tree::TerminalNode *PROCEDURE_POINTER();
    antlr4::tree::TerminalNode *REAL();
    antlr4::tree::TerminalNode *TASK();
    antlr4::tree::TerminalNode *USAGE();
    antlr4::tree::TerminalNode *IS();
    antlr4::tree::TerminalNode *TRUNCATED();
    antlr4::tree::TerminalNode *EXTENDED();

   
  };

  DataUsageClauseContext* dataUsageClause();

  class  DataUsingClauseContext : public antlr4::ParserRuleContext {
  public:
    DataUsingClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *USING();
    antlr4::tree::TerminalNode *LANGUAGE();
    antlr4::tree::TerminalNode *CONVENTION();
    CobolWordContext *cobolWord();
    DataNameContext *dataName();
    antlr4::tree::TerminalNode *OF();

   
  };

  DataUsingClauseContext* dataUsingClause();

  class  DataValueClauseContext : public antlr4::ParserRuleContext {
  public:
    DataValueClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<DataValueIntervalContext *> dataValueInterval();
    DataValueIntervalContext* dataValueInterval(size_t i);
    antlr4::tree::TerminalNode *VALUE();
    antlr4::tree::TerminalNode *VALUES();
    antlr4::tree::TerminalNode *IS();
    antlr4::tree::TerminalNode *ARE();
    std::vector<antlr4::tree::TerminalNode *> COMMACHAR();
    antlr4::tree::TerminalNode* COMMACHAR(size_t i);

   
  };

  DataValueClauseContext* dataValueClause();

  class  DataValueIntervalContext : public antlr4::ParserRuleContext {
  public:
    DataValueIntervalContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    DataValueIntervalFromContext *dataValueIntervalFrom();
    DataValueIntervalToContext *dataValueIntervalTo();

   
  };

  DataValueIntervalContext* dataValueInterval();

  class  DataValueIntervalFromContext : public antlr4::ParserRuleContext {
  public:
    DataValueIntervalFromContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    LiteralContext *literal();
    CobolWordContext *cobolWord();

   
  };

  DataValueIntervalFromContext* dataValueIntervalFrom();

  class  DataValueIntervalToContext : public antlr4::ParserRuleContext {
  public:
    DataValueIntervalToContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    LiteralContext *literal();
    antlr4::tree::TerminalNode *THROUGH();
    antlr4::tree::TerminalNode *THRU();

   
  };

  DataValueIntervalToContext* dataValueIntervalTo();

  class  DataWithLowerBoundsClauseContext : public antlr4::ParserRuleContext {
  public:
    DataWithLowerBoundsClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LOWER();
    antlr4::tree::TerminalNode *BOUNDS();
    antlr4::tree::TerminalNode *WITH();

   
  };

  DataWithLowerBoundsClauseContext* dataWithLowerBoundsClause();

  class  ProcedureDivisionContext : public antlr4::ParserRuleContext {
  public:
    ProcedureDivisionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PROCEDURE();
    antlr4::tree::TerminalNode *DIVISION();
    antlr4::tree::TerminalNode *DOT_FS();
    ProcedureDivisionBodyContext *procedureDivisionBody();
    ProcedureDivisionUsingClauseContext *procedureDivisionUsingClause();
    ProcedureDivisionGivingClauseContext *procedureDivisionGivingClause();
    ProcedureDeclarativesContext *procedureDeclaratives();

   
  };

  ProcedureDivisionContext* procedureDivision();

  class  ProcedureDivisionUsingClauseContext : public antlr4::ParserRuleContext {
  public:
    ProcedureDivisionUsingClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *USING();
    antlr4::tree::TerminalNode *CHAINING();
    std::vector<ProcedureDivisionUsingParameterContext *> procedureDivisionUsingParameter();
    ProcedureDivisionUsingParameterContext* procedureDivisionUsingParameter(size_t i);

   
  };

  ProcedureDivisionUsingClauseContext* procedureDivisionUsingClause();

  class  ProcedureDivisionGivingClauseContext : public antlr4::ParserRuleContext {
  public:
    ProcedureDivisionGivingClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    DataNameContext *dataName();
    antlr4::tree::TerminalNode *GIVING();
    antlr4::tree::TerminalNode *RETURNING();

   
  };

  ProcedureDivisionGivingClauseContext* procedureDivisionGivingClause();

  class  ProcedureDivisionUsingParameterContext : public antlr4::ParserRuleContext {
  public:
    ProcedureDivisionUsingParameterContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ProcedureDivisionByReferencePhraseContext *procedureDivisionByReferencePhrase();
    ProcedureDivisionByValuePhraseContext *procedureDivisionByValuePhrase();

   
  };

  ProcedureDivisionUsingParameterContext* procedureDivisionUsingParameter();

  class  ProcedureDivisionByReferencePhraseContext : public antlr4::ParserRuleContext {
  public:
    ProcedureDivisionByReferencePhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REFERENCE();
    std::vector<ProcedureDivisionByReferenceContext *> procedureDivisionByReference();
    ProcedureDivisionByReferenceContext* procedureDivisionByReference(size_t i);
    antlr4::tree::TerminalNode *BY();

   
  };

  ProcedureDivisionByReferencePhraseContext* procedureDivisionByReferencePhrase();

  class  ProcedureDivisionByReferenceContext : public antlr4::ParserRuleContext {
  public:
    ProcedureDivisionByReferenceContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    FileNameContext *fileName();
    antlr4::tree::TerminalNode *OPTIONAL();
    antlr4::tree::TerminalNode *ANY();

   
  };

  ProcedureDivisionByReferenceContext* procedureDivisionByReference();

  class  ProcedureDivisionByValuePhraseContext : public antlr4::ParserRuleContext {
  public:
    ProcedureDivisionByValuePhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *VALUE();
    antlr4::tree::TerminalNode *BY();
    std::vector<ProcedureDivisionByValueContext *> procedureDivisionByValue();
    ProcedureDivisionByValueContext* procedureDivisionByValue(size_t i);

   
  };

  ProcedureDivisionByValuePhraseContext* procedureDivisionByValuePhrase();

  class  ProcedureDivisionByValueContext : public antlr4::ParserRuleContext {
  public:
    ProcedureDivisionByValueContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    LiteralContext *literal();
    antlr4::tree::TerminalNode *ANY();

   
  };

  ProcedureDivisionByValueContext* procedureDivisionByValue();

  class  ProcedureDeclarativesContext : public antlr4::ParserRuleContext {
  public:
    ProcedureDeclarativesContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<antlr4::tree::TerminalNode *> DECLARATIVES();
    antlr4::tree::TerminalNode* DECLARATIVES(size_t i);
    std::vector<antlr4::tree::TerminalNode *> DOT_FS();
    antlr4::tree::TerminalNode* DOT_FS(size_t i);
    antlr4::tree::TerminalNode *END();
    std::vector<ProcedureDeclarativeContext *> procedureDeclarative();
    ProcedureDeclarativeContext* procedureDeclarative(size_t i);

   
  };

  ProcedureDeclarativesContext* procedureDeclaratives();

  class  ProcedureDeclarativeContext : public antlr4::ParserRuleContext {
  public:
    ProcedureDeclarativeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ProcedureSectionHeaderContext *procedureSectionHeader();
    std::vector<antlr4::tree::TerminalNode *> DOT_FS();
    antlr4::tree::TerminalNode* DOT_FS(size_t i);
    UseStatementContext *useStatement();
    ParagraphsContext *paragraphs();

   
  };

  ProcedureDeclarativeContext* procedureDeclarative();

  class  ProcedureSectionHeaderContext : public antlr4::ParserRuleContext {
  public:
    ProcedureSectionHeaderContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    SectionNameContext *sectionName();
    antlr4::tree::TerminalNode *SECTION();
    IntegerLiteralContext *integerLiteral();

   
  };

  ProcedureSectionHeaderContext* procedureSectionHeader();

  class  ProcedureDivisionBodyContext : public antlr4::ParserRuleContext {
  public:
    ProcedureDivisionBodyContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ParagraphsContext *paragraphs();
    std::vector<ProcedureSectionContext *> procedureSection();
    ProcedureSectionContext* procedureSection(size_t i);

   
  };

  ProcedureDivisionBodyContext* procedureDivisionBody();

  class  CopyDirectiveContext : public antlr4::ParserRuleContext {
  public:
    CopyDirectiveContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *COPY();
    CopyDirectiveSourceContext *copyDirectiveSource();
    antlr4::tree::TerminalNode *DOT_FS();
    std::vector<CopyDirectiveDirectoryPhraseContext *> copyDirectiveDirectoryPhrase();
    CopyDirectiveDirectoryPhraseContext* copyDirectiveDirectoryPhrase(size_t i);
    std::vector<CopyDirectiveFamilyPhraseContext *> copyDirectiveFamilyPhrase();
    CopyDirectiveFamilyPhraseContext* copyDirectiveFamilyPhrase(size_t i);
    std::vector<CopyDirectiveReplacingPhraseContext *> copyDirectiveReplacingPhrase();
    CopyDirectiveReplacingPhraseContext* copyDirectiveReplacingPhrase(size_t i);
    std::vector<antlr4::tree::TerminalNode *> SUPPRESS();
    antlr4::tree::TerminalNode* SUPPRESS(size_t i);

   
  };

  CopyDirectiveContext* copyDirective();

  class  CopyDirectiveSourceContext : public antlr4::ParserRuleContext {
  public:
    CopyDirectiveSourceContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    CopyDirectiveLiteralContext *copyDirectiveLiteral();
    CobolWordContext *cobolWord();
    CopyDirectiveFilenameContext *copyDirectiveFilename();
    CopyDirectiveLibraryContext *copyDirectiveLibrary();
    antlr4::tree::TerminalNode *OF();
    antlr4::tree::TerminalNode *IN();

   
  };

  CopyDirectiveSourceContext* copyDirectiveSource();

  class  CopyDirectiveLibraryContext : public antlr4::ParserRuleContext {
  public:
    CopyDirectiveLibraryContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    CopyDirectiveLiteralContext *copyDirectiveLiteral();
    CobolWordContext *cobolWord();

   
  };

  CopyDirectiveLibraryContext* copyDirectiveLibrary();

  class  CopyDirectiveLiteralContext : public antlr4::ParserRuleContext {
  public:
    CopyDirectiveLiteralContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    CopyDirectiveDigitsContext *copyDirectiveDigits();
    antlr4::tree::TerminalNode *NONNUMERICLITERAL();

   
  };

  CopyDirectiveLiteralContext* copyDirectiveLiteral();

  class  CopyDirectiveDigitsContext : public antlr4::ParserRuleContext {
  public:
    CopyDirectiveDigitsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INTEGERLITERAL();

   
  };

  CopyDirectiveDigitsContext* copyDirectiveDigits();

  class  CopyDirectiveFilenameContext : public antlr4::ParserRuleContext {
  public:
    CopyDirectiveFilenameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DOT();
    std::vector<CobolWordContext *> cobolWord();
    CobolWordContext* cobolWord(size_t i);
    std::vector<CopyDirectiveDigitsContext *> copyDirectiveDigits();
    CopyDirectiveDigitsContext* copyDirectiveDigits(size_t i);
    antlr4::tree::TerminalNode *NUMERICLITERAL();

   
  };

  CopyDirectiveFilenameContext* copyDirectiveFilename();

  class  CopyDirectiveReplacingPhraseContext : public antlr4::ParserRuleContext {
  public:
    CopyDirectiveReplacingPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REPLACING();
    std::vector<CopyDirectiveReplaceClauseContext *> copyDirectiveReplaceClause();
    CopyDirectiveReplaceClauseContext* copyDirectiveReplaceClause(size_t i);

   
  };

  CopyDirectiveReplacingPhraseContext* copyDirectiveReplacingPhrase();

  class  CopyDirectiveReplaceClauseContext : public antlr4::ParserRuleContext {
  public:
    CopyDirectiveReplaceClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<CopyDirectiveReplaceableContext *> copyDirectiveReplaceable();
    CopyDirectiveReplaceableContext* copyDirectiveReplaceable(size_t i);
    antlr4::tree::TerminalNode *BY();
    CopyDirectiveDirectoryPhraseContext *copyDirectiveDirectoryPhrase();
    CopyDirectiveFamilyPhraseContext *copyDirectiveFamilyPhrase();

   
  };

  CopyDirectiveReplaceClauseContext* copyDirectiveReplaceClause();

  class  CopyDirectiveDirectoryPhraseContext : public antlr4::ParserRuleContext {
  public:
    CopyDirectiveDirectoryPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OF();
    antlr4::tree::TerminalNode *IN();
    CopyDirectiveLiteralContext *copyDirectiveLiteral();
    CobolWordContext *cobolWord();

   
  };

  CopyDirectiveDirectoryPhraseContext* copyDirectiveDirectoryPhrase();

  class  CopyDirectiveFamilyPhraseContext : public antlr4::ParserRuleContext {
  public:
    CopyDirectiveFamilyPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ON();
    CopyDirectiveLiteralContext *copyDirectiveLiteral();
    CobolWordContext *cobolWord();

   
  };

  CopyDirectiveFamilyPhraseContext* copyDirectiveFamilyPhrase();

  class  CopyDirectiveReplaceableContext : public antlr4::ParserRuleContext {
  public:
    CopyDirectiveReplaceableContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    CopyDirectivePseudoTextContext *copyDirectivePseudoText();
    std::vector<CobolWordContext *> cobolWord();
    CobolWordContext* cobolWord(size_t i);
    std::vector<CopyDirectiveLiteralContext *> copyDirectiveLiteral();
    CopyDirectiveLiteralContext* copyDirectiveLiteral(size_t i);
    std::vector<CopyDirectiveFilenameContext *> copyDirectiveFilename();
    CopyDirectiveFilenameContext* copyDirectiveFilename(size_t i);
    std::vector<antlr4::tree::TerminalNode *> DOT();
    antlr4::tree::TerminalNode* DOT(size_t i);
    std::vector<antlr4::tree::TerminalNode *> LPARENCHAR();
    antlr4::tree::TerminalNode* LPARENCHAR(size_t i);
    std::vector<antlr4::tree::TerminalNode *> RPARENCHAR();
    antlr4::tree::TerminalNode* RPARENCHAR(size_t i);

   
  };

  CopyDirectiveReplaceableContext* copyDirectiveReplaceable();

  class  CopyDirectivePseudoTextContext : public antlr4::ParserRuleContext {
  public:
    CopyDirectivePseudoTextContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<CopyDirectivePseudoDelimiterContext *> copyDirectivePseudoDelimiter();
    CopyDirectivePseudoDelimiterContext* copyDirectivePseudoDelimiter(size_t i);
    std::vector<antlr4::tree::TerminalNode *> EQUALCHAR();
    antlr4::tree::TerminalNode* EQUALCHAR(size_t i);

   
  };

  CopyDirectivePseudoTextContext* copyDirectivePseudoText();

  class  CopyDirectivePseudoDelimiterContext : public antlr4::ParserRuleContext {
  public:
    CopyDirectivePseudoDelimiterContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<antlr4::tree::TerminalNode *> EQUALCHAR();
    antlr4::tree::TerminalNode* EQUALCHAR(size_t i);

   
  };

  CopyDirectivePseudoDelimiterContext* copyDirectivePseudoDelimiter();

  class  ProcedureSectionContext : public antlr4::ParserRuleContext {
  public:
    ProcedureSectionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ProcedureSectionHeaderContext *procedureSectionHeader();
    antlr4::tree::TerminalNode *DOT_FS();
    ParagraphsContext *paragraphs();

   
  };

  ProcedureSectionContext* procedureSection();

  class  ParagraphsContext : public antlr4::ParserRuleContext {
  public:
    ParagraphsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<SentenceContext *> sentence();
    SentenceContext* sentence(size_t i);
    std::vector<CopyDirectiveContext *> copyDirective();
    CopyDirectiveContext* copyDirective(size_t i);
    std::vector<ParagraphContext *> paragraph();
    ParagraphContext* paragraph(size_t i);

   
  };

  ParagraphsContext* paragraphs();

  class  ParagraphContext : public antlr4::ParserRuleContext {
  public:
    ParagraphContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ParagraphNameContext *paragraphName();
    antlr4::tree::TerminalNode *DOT_FS();
    AlteredGoToContext *alteredGoTo();
    std::vector<SentenceContext *> sentence();
    SentenceContext* sentence(size_t i);
    std::vector<CopyDirectiveContext *> copyDirective();
    CopyDirectiveContext* copyDirective(size_t i);

   
  };

  ParagraphContext* paragraph();

  class  SentenceContext : public antlr4::ParserRuleContext {
  public:
    SentenceContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DOT_FS();
    std::vector<StatementContext *> statement();
    StatementContext* statement(size_t i);

   
  };

  SentenceContext* sentence();

  class  StatementContext : public antlr4::ParserRuleContext {
  public:
    StatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    AcceptStatementContext *acceptStatement();
    AddStatementContext *addStatement();
    AlterStatementContext *alterStatement();
    CallStatementContext *callStatement();
    CancelStatementContext *cancelStatement();
    CloseStatementContext *closeStatement();
    ComputeStatementContext *computeStatement();
    ContinueStatementContext *continueStatement();
    DeleteStatementContext *deleteStatement();
    DisableStatementContext *disableStatement();
    DisplayStatementContext *displayStatement();
    DivideStatementContext *divideStatement();
    EnableStatementContext *enableStatement();
    EntryStatementContext *entryStatement();
    EvaluateStatementContext *evaluateStatement();
    ExhibitStatementContext *exhibitStatement();
    ExecCicsStatementContext *execCicsStatement();
    ExecSqlStatementContext *execSqlStatement();
    ExecSqlImsStatementContext *execSqlImsStatement();
    ExitStatementContext *exitStatement();
    GenerateStatementContext *generateStatement();
    GobackStatementContext *gobackStatement();
    GoToStatementContext *goToStatement();
    IfStatementContext *ifStatement();
    InitializeStatementContext *initializeStatement();
    InitiateStatementContext *initiateStatement();
    InspectStatementContext *inspectStatement();
    MergeStatementContext *mergeStatement();
    MoveStatementContext *moveStatement();
    MultiplyStatementContext *multiplyStatement();
    OpenStatementContext *openStatement();
    PerformStatementContext *performStatement();
    PurgeStatementContext *purgeStatement();
    ReadStatementContext *readStatement();
    ReceiveStatementContext *receiveStatement();
    ReleaseStatementContext *releaseStatement();
    ReturnStatementContext *returnStatement();
    RewriteStatementContext *rewriteStatement();
    SearchStatementContext *searchStatement();
    SendStatementContext *sendStatement();
    SetStatementContext *setStatement();
    SortStatementContext *sortStatement();
    StartStatementContext *startStatement();
    StopStatementContext *stopStatement();
    StringStatementContext *stringStatement();
    SubtractStatementContext *subtractStatement();
    TerminateStatementContext *terminateStatement();
    UnstringStatementContext *unstringStatement();
    WriteStatementContext *writeStatement();

   
  };

  StatementContext* statement();

  class  AcceptStatementContext : public antlr4::ParserRuleContext {
  public:
    AcceptStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ACCEPT();
    IdentifierContext *identifier();
    AcceptFromDateStatementContext *acceptFromDateStatement();
    AcceptFromEscapeKeyStatementContext *acceptFromEscapeKeyStatement();
    AcceptFromMnemonicStatementContext *acceptFromMnemonicStatement();
    AcceptMessageCountStatementContext *acceptMessageCountStatement();
    OnExceptionClauseContext *onExceptionClause();
    NotOnExceptionClauseContext *notOnExceptionClause();
    antlr4::tree::TerminalNode *END_ACCEPT();

   
  };

  AcceptStatementContext* acceptStatement();

  class  AcceptFromDateStatementContext : public antlr4::ParserRuleContext {
  public:
    AcceptFromDateStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FROM();
    antlr4::tree::TerminalNode *DATE();
    antlr4::tree::TerminalNode *DAY();
    antlr4::tree::TerminalNode *DAY_OF_WEEK();
    antlr4::tree::TerminalNode *TIME();
    antlr4::tree::TerminalNode *TIMER();
    antlr4::tree::TerminalNode *TODAYS_DATE();
    antlr4::tree::TerminalNode *TODAYS_NAME();
    antlr4::tree::TerminalNode *YEAR();
    antlr4::tree::TerminalNode *YYYYMMDD();
    antlr4::tree::TerminalNode *YYYYDDD();
    antlr4::tree::TerminalNode *MMDDYYYY();

   
  };

  AcceptFromDateStatementContext* acceptFromDateStatement();

  class  AcceptFromMnemonicStatementContext : public antlr4::ParserRuleContext {
  public:
    AcceptFromMnemonicStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FROM();
    MnemonicNameContext *mnemonicName();

   
  };

  AcceptFromMnemonicStatementContext* acceptFromMnemonicStatement();

  class  AcceptFromEscapeKeyStatementContext : public antlr4::ParserRuleContext {
  public:
    AcceptFromEscapeKeyStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FROM();
    antlr4::tree::TerminalNode *ESCAPE();
    antlr4::tree::TerminalNode *KEY();

   
  };

  AcceptFromEscapeKeyStatementContext* acceptFromEscapeKeyStatement();

  class  AcceptMessageCountStatementContext : public antlr4::ParserRuleContext {
  public:
    AcceptMessageCountStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *COUNT();
    antlr4::tree::TerminalNode *MESSAGE();

   
  };

  AcceptMessageCountStatementContext* acceptMessageCountStatement();

  class  AddStatementContext : public antlr4::ParserRuleContext {
  public:
    AddStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ADD();
    AddToStatementContext *addToStatement();
    AddToGivingStatementContext *addToGivingStatement();
    AddCorrespondingStatementContext *addCorrespondingStatement();
    OnSizeErrorPhraseContext *onSizeErrorPhrase();
    NotOnSizeErrorPhraseContext *notOnSizeErrorPhrase();
    antlr4::tree::TerminalNode *END_ADD();

   
  };

  AddStatementContext* addStatement();

  class  AddToStatementContext : public antlr4::ParserRuleContext {
  public:
    AddToStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TO();
    std::vector<AddFromContext *> addFrom();
    AddFromContext* addFrom(size_t i);
    std::vector<AddToContext *> addTo();
    AddToContext* addTo(size_t i);

   
  };

  AddToStatementContext* addToStatement();

  class  AddToGivingStatementContext : public antlr4::ParserRuleContext {
  public:
    AddToGivingStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GIVING();
    std::vector<AddFromContext *> addFrom();
    AddFromContext* addFrom(size_t i);
    antlr4::tree::TerminalNode *TO();
    std::vector<AddGivingContext *> addGiving();
    AddGivingContext* addGiving(size_t i);
    std::vector<AddToGivingContext *> addToGiving();
    AddToGivingContext* addToGiving(size_t i);

   
  };

  AddToGivingStatementContext* addToGivingStatement();

  class  AddCorrespondingStatementContext : public antlr4::ParserRuleContext {
  public:
    AddCorrespondingStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    antlr4::tree::TerminalNode *TO();
    AddToContext *addTo();
    antlr4::tree::TerminalNode *CORRESPONDING();
    antlr4::tree::TerminalNode *CORR();

   
  };

  AddCorrespondingStatementContext* addCorrespondingStatement();

  class  AddFromContext : public antlr4::ParserRuleContext {
  public:
    AddFromContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    LiteralContext *literal();

   
  };

  AddFromContext* addFrom();

  class  AddToContext : public antlr4::ParserRuleContext {
  public:
    AddToContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    antlr4::tree::TerminalNode *ROUNDED();

   
  };

  AddToContext* addTo();

  class  AddToGivingContext : public antlr4::ParserRuleContext {
  public:
    AddToGivingContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    LiteralContext *literal();

   
  };

  AddToGivingContext* addToGiving();

  class  AddGivingContext : public antlr4::ParserRuleContext {
  public:
    AddGivingContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    antlr4::tree::TerminalNode *ROUNDED();

   
  };

  AddGivingContext* addGiving();

  class  AlteredGoToContext : public antlr4::ParserRuleContext {
  public:
    AlteredGoToContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GO();
    antlr4::tree::TerminalNode *DOT_FS();
    antlr4::tree::TerminalNode *TO();

   
  };

  AlteredGoToContext* alteredGoTo();

  class  AlterStatementContext : public antlr4::ParserRuleContext {
  public:
    AlterStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    std::vector<AlterProceedToContext *> alterProceedTo();
    AlterProceedToContext* alterProceedTo(size_t i);

   
  };

  AlterStatementContext* alterStatement();

  class  AlterProceedToContext : public antlr4::ParserRuleContext {
  public:
    AlterProceedToContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<ProcedureNameContext *> procedureName();
    ProcedureNameContext* procedureName(size_t i);
    std::vector<antlr4::tree::TerminalNode *> TO();
    antlr4::tree::TerminalNode* TO(size_t i);
    antlr4::tree::TerminalNode *PROCEED();

   
  };

  AlterProceedToContext* alterProceedTo();

  class  CallStatementContext : public antlr4::ParserRuleContext {
  public:
    CallStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CALL();
    IdentifierContext *identifier();
    LiteralContext *literal();
    CallUsingPhraseContext *callUsingPhrase();
    CallGivingPhraseContext *callGivingPhrase();
    OnOverflowPhraseContext *onOverflowPhrase();
    OnExceptionClauseContext *onExceptionClause();
    NotOnExceptionClauseContext *notOnExceptionClause();
    antlr4::tree::TerminalNode *END_CALL();

   
  };

  CallStatementContext* callStatement();

  class  CallUsingPhraseContext : public antlr4::ParserRuleContext {
  public:
    CallUsingPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *USING();
    std::vector<CallUsingParameterContext *> callUsingParameter();
    CallUsingParameterContext* callUsingParameter(size_t i);

   
  };

  CallUsingPhraseContext* callUsingPhrase();

  class  CallUsingParameterContext : public antlr4::ParserRuleContext {
  public:
    CallUsingParameterContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    CallByReferencePhraseContext *callByReferencePhrase();
    CallByValuePhraseContext *callByValuePhrase();
    CallByContentPhraseContext *callByContentPhrase();

   
  };

  CallUsingParameterContext* callUsingParameter();

  class  CallByReferencePhraseContext : public antlr4::ParserRuleContext {
  public:
    CallByReferencePhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REFERENCE();
    std::vector<CallByReferenceContext *> callByReference();
    CallByReferenceContext* callByReference(size_t i);
    antlr4::tree::TerminalNode *BY();

   
  };

  CallByReferencePhraseContext* callByReferencePhrase();

  class  CallByReferenceContext : public antlr4::ParserRuleContext {
  public:
    CallByReferenceContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    LiteralContext *literal();
    FileNameContext *fileName();
    antlr4::tree::TerminalNode *ADDRESS();
    antlr4::tree::TerminalNode *OF();
    antlr4::tree::TerminalNode *INTEGER();
    antlr4::tree::TerminalNode *STRING();
    antlr4::tree::TerminalNode *OMITTED();

   
  };

  CallByReferenceContext* callByReference();

  class  CallByValuePhraseContext : public antlr4::ParserRuleContext {
  public:
    CallByValuePhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *VALUE();
    antlr4::tree::TerminalNode *BY();
    std::vector<CallByValueContext *> callByValue();
    CallByValueContext* callByValue(size_t i);

   
  };

  CallByValuePhraseContext* callByValuePhrase();

  class  CallByValueContext : public antlr4::ParserRuleContext {
  public:
    CallByValueContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    LiteralContext *literal();
    antlr4::tree::TerminalNode *ADDRESS();
    antlr4::tree::TerminalNode *OF();
    antlr4::tree::TerminalNode *LENGTH();

   
  };

  CallByValueContext* callByValue();

  class  CallByContentPhraseContext : public antlr4::ParserRuleContext {
  public:
    CallByContentPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CONTENT();
    antlr4::tree::TerminalNode *BY();
    std::vector<CallByContentContext *> callByContent();
    CallByContentContext* callByContent(size_t i);

   
  };

  CallByContentPhraseContext* callByContentPhrase();

  class  CallByContentContext : public antlr4::ParserRuleContext {
  public:
    CallByContentContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    antlr4::tree::TerminalNode *ADDRESS();
    antlr4::tree::TerminalNode *OF();
    antlr4::tree::TerminalNode *LENGTH();
    LiteralContext *literal();
    antlr4::tree::TerminalNode *OMITTED();

   
  };

  CallByContentContext* callByContent();

  class  CallGivingPhraseContext : public antlr4::ParserRuleContext {
  public:
    CallGivingPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    antlr4::tree::TerminalNode *GIVING();
    antlr4::tree::TerminalNode *RETURNING();

   
  };

  CallGivingPhraseContext* callGivingPhrase();

  class  CancelStatementContext : public antlr4::ParserRuleContext {
  public:
    CancelStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CANCEL();
    std::vector<CancelCallContext *> cancelCall();
    CancelCallContext* cancelCall(size_t i);

   
  };

  CancelStatementContext* cancelStatement();

  class  CancelCallContext : public antlr4::ParserRuleContext {
  public:
    CancelCallContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    LibraryNameContext *libraryName();
    antlr4::tree::TerminalNode *BYTITLE();
    antlr4::tree::TerminalNode *BYFUNCTION();
    IdentifierContext *identifier();
    LiteralContext *literal();

   
  };

  CancelCallContext* cancelCall();

  class  CloseStatementContext : public antlr4::ParserRuleContext {
  public:
    CloseStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CLOSE();
    std::vector<CloseFileContext *> closeFile();
    CloseFileContext* closeFile(size_t i);

   
  };

  CloseStatementContext* closeStatement();

  class  CloseFileContext : public antlr4::ParserRuleContext {
  public:
    CloseFileContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    FileNameContext *fileName();
    CloseReelUnitStatementContext *closeReelUnitStatement();
    CloseRelativeStatementContext *closeRelativeStatement();
    ClosePortFileIOStatementContext *closePortFileIOStatement();

   
  };

  CloseFileContext* closeFile();

  class  CloseReelUnitStatementContext : public antlr4::ParserRuleContext {
  public:
    CloseReelUnitStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REEL();
    antlr4::tree::TerminalNode *UNIT();
    antlr4::tree::TerminalNode *REMOVAL();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *REWIND();
    antlr4::tree::TerminalNode *LOCK();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *WITH();

   
  };

  CloseReelUnitStatementContext* closeReelUnitStatement();

  class  CloseRelativeStatementContext : public antlr4::ParserRuleContext {
  public:
    CloseRelativeStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *REWIND();
    antlr4::tree::TerminalNode *LOCK();
    antlr4::tree::TerminalNode *WITH();

   
  };

  CloseRelativeStatementContext* closeRelativeStatement();

  class  ClosePortFileIOStatementContext : public antlr4::ParserRuleContext {
  public:
    ClosePortFileIOStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *WAIT();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *USING();
    std::vector<ClosePortFileIOUsingContext *> closePortFileIOUsing();
    ClosePortFileIOUsingContext* closePortFileIOUsing(size_t i);

   
  };

  ClosePortFileIOStatementContext* closePortFileIOStatement();

  class  ClosePortFileIOUsingContext : public antlr4::ParserRuleContext {
  public:
    ClosePortFileIOUsingContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ClosePortFileIOUsingCloseDispositionContext *closePortFileIOUsingCloseDisposition();
    ClosePortFileIOUsingAssociatedDataContext *closePortFileIOUsingAssociatedData();
    ClosePortFileIOUsingAssociatedDataLengthContext *closePortFileIOUsingAssociatedDataLength();

   
  };

  ClosePortFileIOUsingContext* closePortFileIOUsing();

  class  ClosePortFileIOUsingCloseDispositionContext : public antlr4::ParserRuleContext {
  public:
    ClosePortFileIOUsingCloseDispositionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CLOSE_DISPOSITION();
    antlr4::tree::TerminalNode *ABORT();
    antlr4::tree::TerminalNode *ORDERLY();
    antlr4::tree::TerminalNode *OF();

   
  };

  ClosePortFileIOUsingCloseDispositionContext* closePortFileIOUsingCloseDisposition();

  class  ClosePortFileIOUsingAssociatedDataContext : public antlr4::ParserRuleContext {
  public:
    ClosePortFileIOUsingAssociatedDataContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ASSOCIATED_DATA();
    IdentifierContext *identifier();
    IntegerLiteralContext *integerLiteral();

   
  };

  ClosePortFileIOUsingAssociatedDataContext* closePortFileIOUsingAssociatedData();

  class  ClosePortFileIOUsingAssociatedDataLengthContext : public antlr4::ParserRuleContext {
  public:
    ClosePortFileIOUsingAssociatedDataLengthContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ASSOCIATED_DATA_LENGTH();
    IdentifierContext *identifier();
    IntegerLiteralContext *integerLiteral();
    antlr4::tree::TerminalNode *OF();

   
  };

  ClosePortFileIOUsingAssociatedDataLengthContext* closePortFileIOUsingAssociatedDataLength();

  class  ComputeStatementContext : public antlr4::ParserRuleContext {
  public:
    ComputeStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *COMPUTE();
    ArithmeticExpressionContext *arithmeticExpression();
    antlr4::tree::TerminalNode *EQUALCHAR();
    antlr4::tree::TerminalNode *EQUAL();
    std::vector<ComputeStoreContext *> computeStore();
    ComputeStoreContext* computeStore(size_t i);
    OnSizeErrorPhraseContext *onSizeErrorPhrase();
    NotOnSizeErrorPhraseContext *notOnSizeErrorPhrase();
    antlr4::tree::TerminalNode *END_COMPUTE();

   
  };

  ComputeStatementContext* computeStatement();

  class  ComputeStoreContext : public antlr4::ParserRuleContext {
  public:
    ComputeStoreContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    antlr4::tree::TerminalNode *ROUNDED();

   
  };

  ComputeStoreContext* computeStore();

  class  ContinueStatementContext : public antlr4::ParserRuleContext {
  public:
    ContinueStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CONTINUE();

   
  };

  ContinueStatementContext* continueStatement();

  class  DeleteStatementContext : public antlr4::ParserRuleContext {
  public:
    DeleteStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DELETE();
    FileNameContext *fileName();
    antlr4::tree::TerminalNode *RECORD();
    InvalidKeyPhraseContext *invalidKeyPhrase();
    NotInvalidKeyPhraseContext *notInvalidKeyPhrase();
    antlr4::tree::TerminalNode *END_DELETE();

   
  };

  DeleteStatementContext* deleteStatement();

  class  DisableStatementContext : public antlr4::ParserRuleContext {
  public:
    DisableStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DISABLE();
    CdNameContext *cdName();
    antlr4::tree::TerminalNode *KEY();
    antlr4::tree::TerminalNode *INPUT();
    antlr4::tree::TerminalNode *I_O();
    antlr4::tree::TerminalNode *TERMINAL();
    antlr4::tree::TerminalNode *OUTPUT();
    IdentifierContext *identifier();
    LiteralContext *literal();
    antlr4::tree::TerminalNode *WITH();

   
  };

  DisableStatementContext* disableStatement();

  class  DisplayStatementContext : public antlr4::ParserRuleContext {
  public:
    DisplayStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DISPLAY();
    std::vector<DisplayOperandContext *> displayOperand();
    DisplayOperandContext* displayOperand(size_t i);
    DisplayAtContext *displayAt();
    DisplayUponContext *displayUpon();
    DisplayWithContext *displayWith();

   
  };

  DisplayStatementContext* displayStatement();

  class  DisplayOperandContext : public antlr4::ParserRuleContext {
  public:
    DisplayOperandContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    LiteralContext *literal();

   
  };

  DisplayOperandContext* displayOperand();

  class  DisplayAtContext : public antlr4::ParserRuleContext {
  public:
    DisplayAtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *AT();
    IdentifierContext *identifier();
    LiteralContext *literal();

   
  };

  DisplayAtContext* displayAt();

  class  DisplayUponContext : public antlr4::ParserRuleContext {
  public:
    DisplayUponContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *UPON();
    MnemonicNameContext *mnemonicName();
    EnvironmentNameContext *environmentName();

   
  };

  DisplayUponContext* displayUpon();

  class  DisplayWithContext : public antlr4::ParserRuleContext {
  public:
    DisplayWithContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *ADVANCING();
    antlr4::tree::TerminalNode *WITH();

   
  };

  DisplayWithContext* displayWith();

  class  DivideStatementContext : public antlr4::ParserRuleContext {
  public:
    DivideStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DIVIDE();
    IdentifierContext *identifier();
    LiteralContext *literal();
    DivideIntoStatementContext *divideIntoStatement();
    DivideIntoGivingStatementContext *divideIntoGivingStatement();
    DivideByGivingStatementContext *divideByGivingStatement();
    DivideRemainderContext *divideRemainder();
    OnSizeErrorPhraseContext *onSizeErrorPhrase();
    NotOnSizeErrorPhraseContext *notOnSizeErrorPhrase();
    antlr4::tree::TerminalNode *END_DIVIDE();

   
  };

  DivideStatementContext* divideStatement();

  class  DivideIntoStatementContext : public antlr4::ParserRuleContext {
  public:
    DivideIntoStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INTO();
    std::vector<DivideIntoContext *> divideInto();
    DivideIntoContext* divideInto(size_t i);

   
  };

  DivideIntoStatementContext* divideIntoStatement();

  class  DivideIntoGivingStatementContext : public antlr4::ParserRuleContext {
  public:
    DivideIntoGivingStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INTO();
    IdentifierContext *identifier();
    LiteralContext *literal();
    DivideGivingPhraseContext *divideGivingPhrase();

   
  };

  DivideIntoGivingStatementContext* divideIntoGivingStatement();

  class  DivideByGivingStatementContext : public antlr4::ParserRuleContext {
  public:
    DivideByGivingStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BY();
    IdentifierContext *identifier();
    LiteralContext *literal();
    DivideGivingPhraseContext *divideGivingPhrase();

   
  };

  DivideByGivingStatementContext* divideByGivingStatement();

  class  DivideGivingPhraseContext : public antlr4::ParserRuleContext {
  public:
    DivideGivingPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GIVING();
    std::vector<DivideGivingContext *> divideGiving();
    DivideGivingContext* divideGiving(size_t i);

   
  };

  DivideGivingPhraseContext* divideGivingPhrase();

  class  DivideIntoContext : public antlr4::ParserRuleContext {
  public:
    DivideIntoContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    antlr4::tree::TerminalNode *ROUNDED();

   
  };

  DivideIntoContext* divideInto();

  class  DivideGivingContext : public antlr4::ParserRuleContext {
  public:
    DivideGivingContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    antlr4::tree::TerminalNode *ROUNDED();

   
  };

  DivideGivingContext* divideGiving();

  class  DivideRemainderContext : public antlr4::ParserRuleContext {
  public:
    DivideRemainderContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REMAINDER();
    IdentifierContext *identifier();

   
  };

  DivideRemainderContext* divideRemainder();

  class  EnableStatementContext : public antlr4::ParserRuleContext {
  public:
    EnableStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ENABLE();
    CdNameContext *cdName();
    antlr4::tree::TerminalNode *KEY();
    antlr4::tree::TerminalNode *INPUT();
    antlr4::tree::TerminalNode *I_O();
    antlr4::tree::TerminalNode *TERMINAL();
    antlr4::tree::TerminalNode *OUTPUT();
    LiteralContext *literal();
    IdentifierContext *identifier();
    antlr4::tree::TerminalNode *WITH();

   
  };

  EnableStatementContext* enableStatement();

  class  EntryStatementContext : public antlr4::ParserRuleContext {
  public:
    EntryStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ENTRY();
    LiteralContext *literal();
    antlr4::tree::TerminalNode *USING();
    std::vector<IdentifierContext *> identifier();
    IdentifierContext* identifier(size_t i);

   
  };

  EntryStatementContext* entryStatement();

  class  EvaluateStatementContext : public antlr4::ParserRuleContext {
  public:
    EvaluateStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *EVALUATE();
    EvaluateSelectContext *evaluateSelect();
    std::vector<EvaluateAlsoSelectContext *> evaluateAlsoSelect();
    EvaluateAlsoSelectContext* evaluateAlsoSelect(size_t i);
    std::vector<EvaluateWhenPhraseContext *> evaluateWhenPhrase();
    EvaluateWhenPhraseContext* evaluateWhenPhrase(size_t i);
    EvaluateWhenOtherContext *evaluateWhenOther();
    antlr4::tree::TerminalNode *END_EVALUATE();

   
  };

  EvaluateStatementContext* evaluateStatement();

  class  EvaluateSelectContext : public antlr4::ParserRuleContext {
  public:
    EvaluateSelectContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    LiteralContext *literal();
    ArithmeticExpressionContext *arithmeticExpression();
    ConditionContext *condition();

   
  };

  EvaluateSelectContext* evaluateSelect();

  class  EvaluateAlsoSelectContext : public antlr4::ParserRuleContext {
  public:
    EvaluateAlsoSelectContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALSO();
    EvaluateSelectContext *evaluateSelect();

   
  };

  EvaluateAlsoSelectContext* evaluateAlsoSelect();

  class  EvaluateWhenPhraseContext : public antlr4::ParserRuleContext {
  public:
    EvaluateWhenPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<EvaluateWhenContext *> evaluateWhen();
    EvaluateWhenContext* evaluateWhen(size_t i);
    std::vector<StatementContext *> statement();
    StatementContext* statement(size_t i);

   
  };

  EvaluateWhenPhraseContext* evaluateWhenPhrase();

  class  EvaluateWhenContext : public antlr4::ParserRuleContext {
  public:
    EvaluateWhenContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WHEN();
    EvaluateConditionContext *evaluateCondition();
    std::vector<EvaluateAlsoConditionContext *> evaluateAlsoCondition();
    EvaluateAlsoConditionContext* evaluateAlsoCondition(size_t i);

   
  };

  EvaluateWhenContext* evaluateWhen();

  class  EvaluateConditionContext : public antlr4::ParserRuleContext {
  public:
    EvaluateConditionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ANY();
    EvaluateValueContext *evaluateValue();
    antlr4::tree::TerminalNode *NOT();
    EvaluateThroughContext *evaluateThrough();
    ConditionContext *condition();
    BooleanLiteralContext *booleanLiteral();

   
  };

  EvaluateConditionContext* evaluateCondition();

  class  EvaluateThroughContext : public antlr4::ParserRuleContext {
  public:
    EvaluateThroughContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    EvaluateValueContext *evaluateValue();
    antlr4::tree::TerminalNode *THROUGH();
    antlr4::tree::TerminalNode *THRU();

   
  };

  EvaluateThroughContext* evaluateThrough();

  class  EvaluateAlsoConditionContext : public antlr4::ParserRuleContext {
  public:
    EvaluateAlsoConditionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALSO();
    EvaluateConditionContext *evaluateCondition();

   
  };

  EvaluateAlsoConditionContext* evaluateAlsoCondition();

  class  EvaluateWhenOtherContext : public antlr4::ParserRuleContext {
  public:
    EvaluateWhenOtherContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WHEN();
    antlr4::tree::TerminalNode *OTHER();
    std::vector<StatementContext *> statement();
    StatementContext* statement(size_t i);

   
  };

  EvaluateWhenOtherContext* evaluateWhenOther();

  class  EvaluateValueContext : public antlr4::ParserRuleContext {
  public:
    EvaluateValueContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    LiteralContext *literal();
    ArithmeticExpressionContext *arithmeticExpression();

   
  };

  EvaluateValueContext* evaluateValue();

  class  ExecCicsStatementContext : public antlr4::ParserRuleContext {
  public:
    ExecCicsStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<antlr4::tree::TerminalNode *> EXECCICSLINE();
    antlr4::tree::TerminalNode* EXECCICSLINE(size_t i);

   
  };

  ExecCicsStatementContext* execCicsStatement();

  class  ExecSqlStatementContext : public antlr4::ParserRuleContext {
  public:
    ExecSqlStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<antlr4::tree::TerminalNode *> EXECSQLLINE();
    antlr4::tree::TerminalNode* EXECSQLLINE(size_t i);

   
  };

  ExecSqlStatementContext* execSqlStatement();

  class  ExecSqlImsStatementContext : public antlr4::ParserRuleContext {
  public:
    ExecSqlImsStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<antlr4::tree::TerminalNode *> EXECSQLIMSLINE();
    antlr4::tree::TerminalNode* EXECSQLIMSLINE(size_t i);

   
  };

  ExecSqlImsStatementContext* execSqlImsStatement();

  class  ExhibitStatementContext : public antlr4::ParserRuleContext {
  public:
    ExhibitStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *EXHIBIT();
    antlr4::tree::TerminalNode *NAMED();
    antlr4::tree::TerminalNode *CHANGED();
    std::vector<ExhibitOperandContext *> exhibitOperand();
    ExhibitOperandContext* exhibitOperand(size_t i);

   
  };

  ExhibitStatementContext* exhibitStatement();

  class  ExhibitOperandContext : public antlr4::ParserRuleContext {
  public:
    ExhibitOperandContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    LiteralContext *literal();

   
  };

  ExhibitOperandContext* exhibitOperand();

  class  ExitStatementContext : public antlr4::ParserRuleContext {
  public:
    ExitStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *EXIT();
    antlr4::tree::TerminalNode *PROGRAM();

   
  };

  ExitStatementContext* exitStatement();

  class  GenerateStatementContext : public antlr4::ParserRuleContext {
  public:
    GenerateStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GENERATE();
    ReportNameContext *reportName();

   
  };

  GenerateStatementContext* generateStatement();

  class  GobackStatementContext : public antlr4::ParserRuleContext {
  public:
    GobackStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GOBACK();

   
  };

  GobackStatementContext* gobackStatement();

  class  GoToStatementContext : public antlr4::ParserRuleContext {
  public:
    GoToStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GO();
    GoToStatementSimpleContext *goToStatementSimple();
    GoToDependingOnStatementContext *goToDependingOnStatement();
    antlr4::tree::TerminalNode *TO();

   
  };

  GoToStatementContext* goToStatement();

  class  GoToStatementSimpleContext : public antlr4::ParserRuleContext {
  public:
    GoToStatementSimpleContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ProcedureNameContext *procedureName();

   
  };

  GoToStatementSimpleContext* goToStatementSimple();

  class  GoToDependingOnStatementContext : public antlr4::ParserRuleContext {
  public:
    GoToDependingOnStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *MORE_LABELS();
    std::vector<ProcedureNameContext *> procedureName();
    ProcedureNameContext* procedureName(size_t i);
    antlr4::tree::TerminalNode *DEPENDING();
    IdentifierContext *identifier();
    antlr4::tree::TerminalNode *ON();

   
  };

  GoToDependingOnStatementContext* goToDependingOnStatement();

  class  IfStatementContext : public antlr4::ParserRuleContext {
  public:
    IfStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *IF();
    ConditionContext *condition();
    IfThenContext *ifThen();
    IfElseContext *ifElse();
    antlr4::tree::TerminalNode *END_IF();

   
  };

  IfStatementContext* ifStatement();

  class  IfThenContext : public antlr4::ParserRuleContext {
  public:
    IfThenContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NEXT();
    antlr4::tree::TerminalNode *SENTENCE();
    antlr4::tree::TerminalNode *THEN();
    std::vector<StatementContext *> statement();
    StatementContext* statement(size_t i);

   
  };

  IfThenContext* ifThen();

  class  IfElseContext : public antlr4::ParserRuleContext {
  public:
    IfElseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ELSE();
    antlr4::tree::TerminalNode *NEXT();
    antlr4::tree::TerminalNode *SENTENCE();
    std::vector<StatementContext *> statement();
    StatementContext* statement(size_t i);

   
  };

  IfElseContext* ifElse();

  class  InitializeStatementContext : public antlr4::ParserRuleContext {
  public:
    InitializeStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INITIALIZE();
    std::vector<IdentifierContext *> identifier();
    IdentifierContext* identifier(size_t i);
    InitializeReplacingPhraseContext *initializeReplacingPhrase();

   
  };

  InitializeStatementContext* initializeStatement();

  class  InitializeReplacingPhraseContext : public antlr4::ParserRuleContext {
  public:
    InitializeReplacingPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REPLACING();
    std::vector<InitializeReplacingByContext *> initializeReplacingBy();
    InitializeReplacingByContext* initializeReplacingBy(size_t i);

   
  };

  InitializeReplacingPhraseContext* initializeReplacingPhrase();

  class  InitializeReplacingByContext : public antlr4::ParserRuleContext {
  public:
    InitializeReplacingByContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BY();
    antlr4::tree::TerminalNode *ALPHABETIC();
    antlr4::tree::TerminalNode *ALPHANUMERIC();
    antlr4::tree::TerminalNode *ALPHANUMERIC_EDITED();
    antlr4::tree::TerminalNode *NATIONAL();
    antlr4::tree::TerminalNode *NATIONAL_EDITED();
    antlr4::tree::TerminalNode *NUMERIC();
    antlr4::tree::TerminalNode *NUMERIC_EDITED();
    antlr4::tree::TerminalNode *DBCS();
    antlr4::tree::TerminalNode *EGCS();
    IdentifierContext *identifier();
    LiteralContext *literal();
    antlr4::tree::TerminalNode *DATA();

   
  };

  InitializeReplacingByContext* initializeReplacingBy();

  class  InitiateStatementContext : public antlr4::ParserRuleContext {
  public:
    InitiateStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INITIATE();
    std::vector<ReportNameContext *> reportName();
    ReportNameContext* reportName(size_t i);

   
  };

  InitiateStatementContext* initiateStatement();

  class  InspectStatementContext : public antlr4::ParserRuleContext {
  public:
    InspectStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INSPECT();
    IdentifierContext *identifier();
    InspectTallyingPhraseContext *inspectTallyingPhrase();
    InspectReplacingPhraseContext *inspectReplacingPhrase();
    InspectTallyingReplacingPhraseContext *inspectTallyingReplacingPhrase();
    InspectConvertingPhraseContext *inspectConvertingPhrase();

   
  };

  InspectStatementContext* inspectStatement();

  class  InspectTallyingPhraseContext : public antlr4::ParserRuleContext {
  public:
    InspectTallyingPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TALLYING();
    std::vector<InspectForContext *> inspectFor();
    InspectForContext* inspectFor(size_t i);

   
  };

  InspectTallyingPhraseContext* inspectTallyingPhrase();

  class  InspectReplacingPhraseContext : public antlr4::ParserRuleContext {
  public:
    InspectReplacingPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REPLACING();
    std::vector<InspectReplacingCharactersContext *> inspectReplacingCharacters();
    InspectReplacingCharactersContext* inspectReplacingCharacters(size_t i);
    std::vector<InspectReplacingAllLeadingsContext *> inspectReplacingAllLeadings();
    InspectReplacingAllLeadingsContext* inspectReplacingAllLeadings(size_t i);

   
  };

  InspectReplacingPhraseContext* inspectReplacingPhrase();

  class  InspectTallyingReplacingPhraseContext : public antlr4::ParserRuleContext {
  public:
    InspectTallyingReplacingPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TALLYING();
    std::vector<InspectForContext *> inspectFor();
    InspectForContext* inspectFor(size_t i);
    std::vector<InspectReplacingPhraseContext *> inspectReplacingPhrase();
    InspectReplacingPhraseContext* inspectReplacingPhrase(size_t i);

   
  };

  InspectTallyingReplacingPhraseContext* inspectTallyingReplacingPhrase();

  class  InspectConvertingPhraseContext : public antlr4::ParserRuleContext {
  public:
    InspectConvertingPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CONVERTING();
    InspectToContext *inspectTo();
    IdentifierContext *identifier();
    LiteralContext *literal();
    std::vector<InspectBeforeAfterContext *> inspectBeforeAfter();
    InspectBeforeAfterContext* inspectBeforeAfter(size_t i);

   
  };

  InspectConvertingPhraseContext* inspectConvertingPhrase();

  class  InspectForContext : public antlr4::ParserRuleContext {
  public:
    InspectForContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    antlr4::tree::TerminalNode *FOR();
    std::vector<InspectCharactersContext *> inspectCharacters();
    InspectCharactersContext* inspectCharacters(size_t i);
    std::vector<InspectAllLeadingsContext *> inspectAllLeadings();
    InspectAllLeadingsContext* inspectAllLeadings(size_t i);

   
  };

  InspectForContext* inspectFor();

  class  InspectCharactersContext : public antlr4::ParserRuleContext {
  public:
    InspectCharactersContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CHARACTERS();
    std::vector<InspectBeforeAfterContext *> inspectBeforeAfter();
    InspectBeforeAfterContext* inspectBeforeAfter(size_t i);

   
  };

  InspectCharactersContext* inspectCharacters();

  class  InspectReplacingCharactersContext : public antlr4::ParserRuleContext {
  public:
    InspectReplacingCharactersContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CHARACTERS();
    InspectByContext *inspectBy();
    std::vector<InspectBeforeAfterContext *> inspectBeforeAfter();
    InspectBeforeAfterContext* inspectBeforeAfter(size_t i);

   
  };

  InspectReplacingCharactersContext* inspectReplacingCharacters();

  class  InspectAllLeadingsContext : public antlr4::ParserRuleContext {
  public:
    InspectAllLeadingsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *LEADING();
    std::vector<InspectAllLeadingContext *> inspectAllLeading();
    InspectAllLeadingContext* inspectAllLeading(size_t i);

   
  };

  InspectAllLeadingsContext* inspectAllLeadings();

  class  InspectReplacingAllLeadingsContext : public antlr4::ParserRuleContext {
  public:
    InspectReplacingAllLeadingsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *LEADING();
    antlr4::tree::TerminalNode *FIRST();
    std::vector<InspectReplacingAllLeadingContext *> inspectReplacingAllLeading();
    InspectReplacingAllLeadingContext* inspectReplacingAllLeading(size_t i);

   
  };

  InspectReplacingAllLeadingsContext* inspectReplacingAllLeadings();

  class  InspectAllLeadingContext : public antlr4::ParserRuleContext {
  public:
    InspectAllLeadingContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    LiteralContext *literal();
    std::vector<InspectBeforeAfterContext *> inspectBeforeAfter();
    InspectBeforeAfterContext* inspectBeforeAfter(size_t i);

   
  };

  InspectAllLeadingContext* inspectAllLeading();

  class  InspectReplacingAllLeadingContext : public antlr4::ParserRuleContext {
  public:
    InspectReplacingAllLeadingContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    InspectByContext *inspectBy();
    IdentifierContext *identifier();
    LiteralContext *literal();
    std::vector<InspectBeforeAfterContext *> inspectBeforeAfter();
    InspectBeforeAfterContext* inspectBeforeAfter(size_t i);

   
  };

  InspectReplacingAllLeadingContext* inspectReplacingAllLeading();

  class  InspectByContext : public antlr4::ParserRuleContext {
  public:
    InspectByContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BY();
    IdentifierContext *identifier();
    LiteralContext *literal();

   
  };

  InspectByContext* inspectBy();

  class  InspectToContext : public antlr4::ParserRuleContext {
  public:
    InspectToContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TO();
    IdentifierContext *identifier();
    LiteralContext *literal();

   
  };

  InspectToContext* inspectTo();

  class  InspectBeforeAfterContext : public antlr4::ParserRuleContext {
  public:
    InspectBeforeAfterContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BEFORE();
    antlr4::tree::TerminalNode *AFTER();
    IdentifierContext *identifier();
    LiteralContext *literal();
    antlr4::tree::TerminalNode *INITIAL();

   
  };

  InspectBeforeAfterContext* inspectBeforeAfter();

  class  MergeStatementContext : public antlr4::ParserRuleContext {
  public:
    MergeStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *MERGE();
    FileNameContext *fileName();
    std::vector<MergeOnKeyClauseContext *> mergeOnKeyClause();
    MergeOnKeyClauseContext* mergeOnKeyClause(size_t i);
    MergeCollatingSequencePhraseContext *mergeCollatingSequencePhrase();
    std::vector<MergeUsingContext *> mergeUsing();
    MergeUsingContext* mergeUsing(size_t i);
    MergeOutputProcedurePhraseContext *mergeOutputProcedurePhrase();
    std::vector<MergeGivingPhraseContext *> mergeGivingPhrase();
    MergeGivingPhraseContext* mergeGivingPhrase(size_t i);

   
  };

  MergeStatementContext* mergeStatement();

  class  MergeOnKeyClauseContext : public antlr4::ParserRuleContext {
  public:
    MergeOnKeyClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ASCENDING();
    antlr4::tree::TerminalNode *DESCENDING();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *KEY();
    std::vector<QualifiedDataNameContext *> qualifiedDataName();
    QualifiedDataNameContext* qualifiedDataName(size_t i);

   
  };

  MergeOnKeyClauseContext* mergeOnKeyClause();

  class  MergeCollatingSequencePhraseContext : public antlr4::ParserRuleContext {
  public:
    MergeCollatingSequencePhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SEQUENCE();
    antlr4::tree::TerminalNode *COLLATING();
    antlr4::tree::TerminalNode *IS();
    std::vector<AlphabetNameContext *> alphabetName();
    AlphabetNameContext* alphabetName(size_t i);
    MergeCollatingAlphanumericContext *mergeCollatingAlphanumeric();
    MergeCollatingNationalContext *mergeCollatingNational();

   
  };

  MergeCollatingSequencePhraseContext* mergeCollatingSequencePhrase();

  class  MergeCollatingAlphanumericContext : public antlr4::ParserRuleContext {
  public:
    MergeCollatingAlphanumericContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALPHANUMERIC();
    antlr4::tree::TerminalNode *IS();
    AlphabetNameContext *alphabetName();
    antlr4::tree::TerminalNode *FOR();

   
  };

  MergeCollatingAlphanumericContext* mergeCollatingAlphanumeric();

  class  MergeCollatingNationalContext : public antlr4::ParserRuleContext {
  public:
    MergeCollatingNationalContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NATIONAL();
    AlphabetNameContext *alphabetName();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *IS();

   
  };

  MergeCollatingNationalContext* mergeCollatingNational();

  class  MergeUsingContext : public antlr4::ParserRuleContext {
  public:
    MergeUsingContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *USING();
    std::vector<FileNameContext *> fileName();
    FileNameContext* fileName(size_t i);

   
  };

  MergeUsingContext* mergeUsing();

  class  MergeOutputProcedurePhraseContext : public antlr4::ParserRuleContext {
  public:
    MergeOutputProcedurePhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OUTPUT();
    antlr4::tree::TerminalNode *PROCEDURE();
    ProcedureNameContext *procedureName();
    antlr4::tree::TerminalNode *IS();
    MergeOutputThroughContext *mergeOutputThrough();

   
  };

  MergeOutputProcedurePhraseContext* mergeOutputProcedurePhrase();

  class  MergeOutputThroughContext : public antlr4::ParserRuleContext {
  public:
    MergeOutputThroughContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ProcedureNameContext *procedureName();
    antlr4::tree::TerminalNode *THROUGH();
    antlr4::tree::TerminalNode *THRU();

   
  };

  MergeOutputThroughContext* mergeOutputThrough();

  class  MergeGivingPhraseContext : public antlr4::ParserRuleContext {
  public:
    MergeGivingPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GIVING();
    std::vector<MergeGivingContext *> mergeGiving();
    MergeGivingContext* mergeGiving(size_t i);

   
  };

  MergeGivingPhraseContext* mergeGivingPhrase();

  class  MergeGivingContext : public antlr4::ParserRuleContext {
  public:
    MergeGivingContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    FileNameContext *fileName();
    antlr4::tree::TerminalNode *LOCK();
    antlr4::tree::TerminalNode *SAVE();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *REWIND();
    antlr4::tree::TerminalNode *CRUNCH();
    antlr4::tree::TerminalNode *RELEASE();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *REMOVE();

   
  };

  MergeGivingContext* mergeGiving();

  class  MoveStatementContext : public antlr4::ParserRuleContext {
  public:
    MoveStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *MOVE();
    MoveToStatementContext *moveToStatement();
    MoveCorrespondingToStatementContext *moveCorrespondingToStatement();
    antlr4::tree::TerminalNode *ALL();

   
  };

  MoveStatementContext* moveStatement();

  class  MoveToStatementContext : public antlr4::ParserRuleContext {
  public:
    MoveToStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    MoveToSendingAreaContext *moveToSendingArea();
    antlr4::tree::TerminalNode *TO();
    std::vector<IdentifierContext *> identifier();
    IdentifierContext* identifier(size_t i);

   
  };

  MoveToStatementContext* moveToStatement();

  class  MoveToSendingAreaContext : public antlr4::ParserRuleContext {
  public:
    MoveToSendingAreaContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    LiteralContext *literal();

   
  };

  MoveToSendingAreaContext* moveToSendingArea();

  class  MoveCorrespondingToStatementContext : public antlr4::ParserRuleContext {
  public:
    MoveCorrespondingToStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    MoveCorrespondingToSendingAreaContext *moveCorrespondingToSendingArea();
    antlr4::tree::TerminalNode *TO();
    antlr4::tree::TerminalNode *CORRESPONDING();
    antlr4::tree::TerminalNode *CORR();
    std::vector<IdentifierContext *> identifier();
    IdentifierContext* identifier(size_t i);

   
  };

  MoveCorrespondingToStatementContext* moveCorrespondingToStatement();

  class  MoveCorrespondingToSendingAreaContext : public antlr4::ParserRuleContext {
  public:
    MoveCorrespondingToSendingAreaContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();

   
  };

  MoveCorrespondingToSendingAreaContext* moveCorrespondingToSendingArea();

  class  MultiplyStatementContext : public antlr4::ParserRuleContext {
  public:
    MultiplyStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *MULTIPLY();
    antlr4::tree::TerminalNode *BY();
    IdentifierContext *identifier();
    LiteralContext *literal();
    MultiplyRegularContext *multiplyRegular();
    MultiplyGivingContext *multiplyGiving();
    OnSizeErrorPhraseContext *onSizeErrorPhrase();
    NotOnSizeErrorPhraseContext *notOnSizeErrorPhrase();
    antlr4::tree::TerminalNode *END_MULTIPLY();

   
  };

  MultiplyStatementContext* multiplyStatement();

  class  MultiplyRegularContext : public antlr4::ParserRuleContext {
  public:
    MultiplyRegularContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<MultiplyRegularOperandContext *> multiplyRegularOperand();
    MultiplyRegularOperandContext* multiplyRegularOperand(size_t i);

   
  };

  MultiplyRegularContext* multiplyRegular();

  class  MultiplyRegularOperandContext : public antlr4::ParserRuleContext {
  public:
    MultiplyRegularOperandContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    antlr4::tree::TerminalNode *ROUNDED();

   
  };

  MultiplyRegularOperandContext* multiplyRegularOperand();

  class  MultiplyGivingContext : public antlr4::ParserRuleContext {
  public:
    MultiplyGivingContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    MultiplyGivingOperandContext *multiplyGivingOperand();
    antlr4::tree::TerminalNode *GIVING();
    std::vector<MultiplyGivingResultContext *> multiplyGivingResult();
    MultiplyGivingResultContext* multiplyGivingResult(size_t i);

   
  };

  MultiplyGivingContext* multiplyGiving();

  class  MultiplyGivingOperandContext : public antlr4::ParserRuleContext {
  public:
    MultiplyGivingOperandContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    LiteralContext *literal();

   
  };

  MultiplyGivingOperandContext* multiplyGivingOperand();

  class  MultiplyGivingResultContext : public antlr4::ParserRuleContext {
  public:
    MultiplyGivingResultContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    antlr4::tree::TerminalNode *ROUNDED();

   
  };

  MultiplyGivingResultContext* multiplyGivingResult();

  class  OpenStatementContext : public antlr4::ParserRuleContext {
  public:
    OpenStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OPEN();
    std::vector<OpenInputStatementContext *> openInputStatement();
    OpenInputStatementContext* openInputStatement(size_t i);
    std::vector<OpenOutputStatementContext *> openOutputStatement();
    OpenOutputStatementContext* openOutputStatement(size_t i);
    std::vector<OpenIOStatementContext *> openIOStatement();
    OpenIOStatementContext* openIOStatement(size_t i);
    std::vector<OpenExtendStatementContext *> openExtendStatement();
    OpenExtendStatementContext* openExtendStatement(size_t i);

   
  };

  OpenStatementContext* openStatement();

  class  OpenInputStatementContext : public antlr4::ParserRuleContext {
  public:
    OpenInputStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INPUT();
    std::vector<OpenInputContext *> openInput();
    OpenInputContext* openInput(size_t i);

   
  };

  OpenInputStatementContext* openInputStatement();

  class  OpenInputContext : public antlr4::ParserRuleContext {
  public:
    OpenInputContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    FileNameContext *fileName();
    antlr4::tree::TerminalNode *REVERSED();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *REWIND();
    antlr4::tree::TerminalNode *WITH();

   
  };

  OpenInputContext* openInput();

  class  OpenOutputStatementContext : public antlr4::ParserRuleContext {
  public:
    OpenOutputStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OUTPUT();
    std::vector<OpenOutputContext *> openOutput();
    OpenOutputContext* openOutput(size_t i);

   
  };

  OpenOutputStatementContext* openOutputStatement();

  class  OpenOutputContext : public antlr4::ParserRuleContext {
  public:
    OpenOutputContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    FileNameContext *fileName();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *REWIND();
    antlr4::tree::TerminalNode *WITH();

   
  };

  OpenOutputContext* openOutput();

  class  OpenIOStatementContext : public antlr4::ParserRuleContext {
  public:
    OpenIOStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *I_O();
    std::vector<FileNameContext *> fileName();
    FileNameContext* fileName(size_t i);

   
  };

  OpenIOStatementContext* openIOStatement();

  class  OpenExtendStatementContext : public antlr4::ParserRuleContext {
  public:
    OpenExtendStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *EXTEND();
    std::vector<FileNameContext *> fileName();
    FileNameContext* fileName(size_t i);

   
  };

  OpenExtendStatementContext* openExtendStatement();

  class  PerformStatementContext : public antlr4::ParserRuleContext {
  public:
    PerformStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PERFORM();
    PerformInlineStatementContext *performInlineStatement();
    PerformProcedureStatementContext *performProcedureStatement();

   
  };

  PerformStatementContext* performStatement();

  class  PerformInlineStatementContext : public antlr4::ParserRuleContext {
  public:
    PerformInlineStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *END_PERFORM();
    PerformTypeContext *performType();
    std::vector<StatementContext *> statement();
    StatementContext* statement(size_t i);

   
  };

  PerformInlineStatementContext* performInlineStatement();

  class  PerformProcedureStatementContext : public antlr4::ParserRuleContext {
  public:
    PerformProcedureStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<ProcedureNameContext *> procedureName();
    ProcedureNameContext* procedureName(size_t i);
    PerformTypeContext *performType();
    antlr4::tree::TerminalNode *THROUGH();
    antlr4::tree::TerminalNode *THRU();

   
  };

  PerformProcedureStatementContext* performProcedureStatement();

  class  PerformTypeContext : public antlr4::ParserRuleContext {
  public:
    PerformTypeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    PerformTimesContext *performTimes();
    PerformUntilContext *performUntil();
    PerformVaryingContext *performVarying();

   
  };

  PerformTypeContext* performType();

  class  PerformTimesContext : public antlr4::ParserRuleContext {
  public:
    PerformTimesContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TIMES();
    IdentifierContext *identifier();
    IntegerLiteralContext *integerLiteral();

   
  };

  PerformTimesContext* performTimes();

  class  PerformUntilContext : public antlr4::ParserRuleContext {
  public:
    PerformUntilContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *UNTIL();
    ConditionContext *condition();
    PerformTestClauseContext *performTestClause();

   
  };

  PerformUntilContext* performUntil();

  class  PerformVaryingContext : public antlr4::ParserRuleContext {
  public:
    PerformVaryingContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    PerformTestClauseContext *performTestClause();
    PerformVaryingClauseContext *performVaryingClause();

   
  };

  PerformVaryingContext* performVarying();

  class  PerformVaryingClauseContext : public antlr4::ParserRuleContext {
  public:
    PerformVaryingClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *VARYING();
    PerformVaryingPhraseContext *performVaryingPhrase();
    std::vector<PerformAfterContext *> performAfter();
    PerformAfterContext* performAfter(size_t i);

   
  };

  PerformVaryingClauseContext* performVaryingClause();

  class  PerformVaryingPhraseContext : public antlr4::ParserRuleContext {
  public:
    PerformVaryingPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    PerformFromContext *performFrom();
    PerformByContext *performBy();
    PerformUntilContext *performUntil();
    IdentifierContext *identifier();
    LiteralContext *literal();

   
  };

  PerformVaryingPhraseContext* performVaryingPhrase();

  class  PerformAfterContext : public antlr4::ParserRuleContext {
  public:
    PerformAfterContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *AFTER();
    PerformVaryingPhraseContext *performVaryingPhrase();

   
  };

  PerformAfterContext* performAfter();

  class  PerformFromContext : public antlr4::ParserRuleContext {
  public:
    PerformFromContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FROM();
    IdentifierContext *identifier();
    LiteralContext *literal();
    ArithmeticExpressionContext *arithmeticExpression();

   
  };

  PerformFromContext* performFrom();

  class  PerformByContext : public antlr4::ParserRuleContext {
  public:
    PerformByContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BY();
    IdentifierContext *identifier();
    LiteralContext *literal();
    ArithmeticExpressionContext *arithmeticExpression();

   
  };

  PerformByContext* performBy();

  class  PerformTestClauseContext : public antlr4::ParserRuleContext {
  public:
    PerformTestClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TEST();
    antlr4::tree::TerminalNode *BEFORE();
    antlr4::tree::TerminalNode *AFTER();
    antlr4::tree::TerminalNode *WITH();

   
  };

  PerformTestClauseContext* performTestClause();

  class  PurgeStatementContext : public antlr4::ParserRuleContext {
  public:
    PurgeStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PURGE();
    std::vector<CdNameContext *> cdName();
    CdNameContext* cdName(size_t i);

   
  };

  PurgeStatementContext* purgeStatement();

  class  ReadStatementContext : public antlr4::ParserRuleContext {
  public:
    ReadStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *READ();
    FileNameContext *fileName();
    antlr4::tree::TerminalNode *NEXT();
    antlr4::tree::TerminalNode *RECORD();
    ReadIntoContext *readInto();
    ReadWithContext *readWith();
    ReadKeyContext *readKey();
    InvalidKeyPhraseContext *invalidKeyPhrase();
    NotInvalidKeyPhraseContext *notInvalidKeyPhrase();
    AtEndPhraseContext *atEndPhrase();
    NotAtEndPhraseContext *notAtEndPhrase();
    antlr4::tree::TerminalNode *END_READ();

   
  };

  ReadStatementContext* readStatement();

  class  ReadIntoContext : public antlr4::ParserRuleContext {
  public:
    ReadIntoContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INTO();
    IdentifierContext *identifier();

   
  };

  ReadIntoContext* readInto();

  class  ReadWithContext : public antlr4::ParserRuleContext {
  public:
    ReadWithContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LOCK();
    antlr4::tree::TerminalNode *WAIT();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *KEPT();
    antlr4::tree::TerminalNode *NO();

   
  };

  ReadWithContext* readWith();

  class  ReadKeyContext : public antlr4::ParserRuleContext {
  public:
    ReadKeyContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *KEY();
    QualifiedDataNameContext *qualifiedDataName();
    antlr4::tree::TerminalNode *IS();

   
  };

  ReadKeyContext* readKey();

  class  ReceiveStatementContext : public antlr4::ParserRuleContext {
  public:
    ReceiveStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RECEIVE();
    ReceiveFromStatementContext *receiveFromStatement();
    ReceiveIntoStatementContext *receiveIntoStatement();
    OnExceptionClauseContext *onExceptionClause();
    NotOnExceptionClauseContext *notOnExceptionClause();
    antlr4::tree::TerminalNode *END_RECEIVE();

   
  };

  ReceiveStatementContext* receiveStatement();

  class  ReceiveFromStatementContext : public antlr4::ParserRuleContext {
  public:
    ReceiveFromStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    DataNameContext *dataName();
    antlr4::tree::TerminalNode *FROM();
    ReceiveFromContext *receiveFrom();
    std::vector<ReceiveBeforeContext *> receiveBefore();
    ReceiveBeforeContext* receiveBefore(size_t i);
    std::vector<ReceiveWithContext *> receiveWith();
    ReceiveWithContext* receiveWith(size_t i);
    std::vector<ReceiveThreadContext *> receiveThread();
    ReceiveThreadContext* receiveThread(size_t i);
    std::vector<ReceiveSizeContext *> receiveSize();
    ReceiveSizeContext* receiveSize(size_t i);
    std::vector<ReceiveStatusContext *> receiveStatus();
    ReceiveStatusContext* receiveStatus(size_t i);

   
  };

  ReceiveFromStatementContext* receiveFromStatement();

  class  ReceiveFromContext : public antlr4::ParserRuleContext {
  public:
    ReceiveFromContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *THREAD();
    DataNameContext *dataName();
    antlr4::tree::TerminalNode *LAST();
    antlr4::tree::TerminalNode *ANY();

   
  };

  ReceiveFromContext* receiveFrom();

  class  ReceiveIntoStatementContext : public antlr4::ParserRuleContext {
  public:
    ReceiveIntoStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    CdNameContext *cdName();
    IdentifierContext *identifier();
    antlr4::tree::TerminalNode *MESSAGE();
    antlr4::tree::TerminalNode *SEGMENT();
    antlr4::tree::TerminalNode *INTO();
    ReceiveNoDataContext *receiveNoData();
    ReceiveWithDataContext *receiveWithData();

   
  };

  ReceiveIntoStatementContext* receiveIntoStatement();

  class  ReceiveNoDataContext : public antlr4::ParserRuleContext {
  public:
    ReceiveNoDataContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *DATA();
    std::vector<StatementContext *> statement();
    StatementContext* statement(size_t i);

   
  };

  ReceiveNoDataContext* receiveNoData();

  class  ReceiveWithDataContext : public antlr4::ParserRuleContext {
  public:
    ReceiveWithDataContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *DATA();
    std::vector<StatementContext *> statement();
    StatementContext* statement(size_t i);

   
  };

  ReceiveWithDataContext* receiveWithData();

  class  ReceiveBeforeContext : public antlr4::ParserRuleContext {
  public:
    ReceiveBeforeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BEFORE();
    NumericLiteralContext *numericLiteral();
    IdentifierContext *identifier();
    antlr4::tree::TerminalNode *TIME();

   
  };

  ReceiveBeforeContext* receiveBefore();

  class  ReceiveWithContext : public antlr4::ParserRuleContext {
  public:
    ReceiveWithContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *WAIT();
    antlr4::tree::TerminalNode *WITH();

   
  };

  ReceiveWithContext* receiveWith();

  class  ReceiveThreadContext : public antlr4::ParserRuleContext {
  public:
    ReceiveThreadContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *THREAD();
    DataNameContext *dataName();
    antlr4::tree::TerminalNode *IN();

   
  };

  ReceiveThreadContext* receiveThread();

  class  ReceiveSizeContext : public antlr4::ParserRuleContext {
  public:
    ReceiveSizeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SIZE();
    NumericLiteralContext *numericLiteral();
    IdentifierContext *identifier();
    antlr4::tree::TerminalNode *IN();

   
  };

  ReceiveSizeContext* receiveSize();

  class  ReceiveStatusContext : public antlr4::ParserRuleContext {
  public:
    ReceiveStatusContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *STATUS();
    IdentifierContext *identifier();
    antlr4::tree::TerminalNode *IN();

   
  };

  ReceiveStatusContext* receiveStatus();

  class  ReleaseStatementContext : public antlr4::ParserRuleContext {
  public:
    ReleaseStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RELEASE();
    RecordNameContext *recordName();
    antlr4::tree::TerminalNode *FROM();
    QualifiedDataNameContext *qualifiedDataName();

   
  };

  ReleaseStatementContext* releaseStatement();

  class  ReturnStatementContext : public antlr4::ParserRuleContext {
  public:
    ReturnStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RETURN();
    FileNameContext *fileName();
    AtEndPhraseContext *atEndPhrase();
    antlr4::tree::TerminalNode *RECORD();
    ReturnIntoContext *returnInto();
    NotAtEndPhraseContext *notAtEndPhrase();
    antlr4::tree::TerminalNode *END_RETURN();

   
  };

  ReturnStatementContext* returnStatement();

  class  ReturnIntoContext : public antlr4::ParserRuleContext {
  public:
    ReturnIntoContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INTO();
    QualifiedDataNameContext *qualifiedDataName();

   
  };

  ReturnIntoContext* returnInto();

  class  RewriteStatementContext : public antlr4::ParserRuleContext {
  public:
    RewriteStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REWRITE();
    RecordNameContext *recordName();
    RewriteFromContext *rewriteFrom();
    InvalidKeyPhraseContext *invalidKeyPhrase();
    NotInvalidKeyPhraseContext *notInvalidKeyPhrase();
    antlr4::tree::TerminalNode *END_REWRITE();

   
  };

  RewriteStatementContext* rewriteStatement();

  class  RewriteFromContext : public antlr4::ParserRuleContext {
  public:
    RewriteFromContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FROM();
    IdentifierContext *identifier();

   
  };

  RewriteFromContext* rewriteFrom();

  class  SearchStatementContext : public antlr4::ParserRuleContext {
  public:
    SearchStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SEARCH();
    QualifiedDataNameContext *qualifiedDataName();
    antlr4::tree::TerminalNode *ALL();
    SearchVaryingContext *searchVarying();
    AtEndPhraseContext *atEndPhrase();
    std::vector<SearchWhenContext *> searchWhen();
    SearchWhenContext* searchWhen(size_t i);
    antlr4::tree::TerminalNode *END_SEARCH();

   
  };

  SearchStatementContext* searchStatement();

  class  SearchVaryingContext : public antlr4::ParserRuleContext {
  public:
    SearchVaryingContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *VARYING();
    QualifiedDataNameContext *qualifiedDataName();

   
  };

  SearchVaryingContext* searchVarying();

  class  SearchWhenContext : public antlr4::ParserRuleContext {
  public:
    SearchWhenContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WHEN();
    ConditionContext *condition();
    antlr4::tree::TerminalNode *NEXT();
    antlr4::tree::TerminalNode *SENTENCE();
    std::vector<StatementContext *> statement();
    StatementContext* statement(size_t i);

   
  };

  SearchWhenContext* searchWhen();

  class  SendStatementContext : public antlr4::ParserRuleContext {
  public:
    SendStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SEND();
    SendStatementSyncContext *sendStatementSync();
    SendStatementAsyncContext *sendStatementAsync();
    OnExceptionClauseContext *onExceptionClause();
    NotOnExceptionClauseContext *notOnExceptionClause();

   
  };

  SendStatementContext* sendStatement();

  class  SendStatementSyncContext : public antlr4::ParserRuleContext {
  public:
    SendStatementSyncContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    LiteralContext *literal();
    SendFromPhraseContext *sendFromPhrase();
    SendWithPhraseContext *sendWithPhrase();
    SendReplacingPhraseContext *sendReplacingPhrase();
    SendAdvancingPhraseContext *sendAdvancingPhrase();

   
  };

  SendStatementSyncContext* sendStatementSync();

  class  SendStatementAsyncContext : public antlr4::ParserRuleContext {
  public:
    SendStatementAsyncContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TO();
    IdentifierContext *identifier();
    antlr4::tree::TerminalNode *TOP();
    antlr4::tree::TerminalNode *BOTTOM();

   
  };

  SendStatementAsyncContext* sendStatementAsync();

  class  SendFromPhraseContext : public antlr4::ParserRuleContext {
  public:
    SendFromPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FROM();
    IdentifierContext *identifier();

   
  };

  SendFromPhraseContext* sendFromPhrase();

  class  SendWithPhraseContext : public antlr4::ParserRuleContext {
  public:
    SendWithPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *EGI();
    antlr4::tree::TerminalNode *EMI();
    antlr4::tree::TerminalNode *ESI();
    IdentifierContext *identifier();

   
  };

  SendWithPhraseContext* sendWithPhrase();

  class  SendReplacingPhraseContext : public antlr4::ParserRuleContext {
  public:
    SendReplacingPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REPLACING();
    antlr4::tree::TerminalNode *LINE();

   
  };

  SendReplacingPhraseContext* sendReplacingPhrase();

  class  SendAdvancingPhraseContext : public antlr4::ParserRuleContext {
  public:
    SendAdvancingPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BEFORE();
    antlr4::tree::TerminalNode *AFTER();
    SendAdvancingPageContext *sendAdvancingPage();
    SendAdvancingLinesContext *sendAdvancingLines();
    SendAdvancingMnemonicContext *sendAdvancingMnemonic();
    antlr4::tree::TerminalNode *ADVANCING();

   
  };

  SendAdvancingPhraseContext* sendAdvancingPhrase();

  class  SendAdvancingPageContext : public antlr4::ParserRuleContext {
  public:
    SendAdvancingPageContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PAGE();

   
  };

  SendAdvancingPageContext* sendAdvancingPage();

  class  SendAdvancingLinesContext : public antlr4::ParserRuleContext {
  public:
    SendAdvancingLinesContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    LiteralContext *literal();
    antlr4::tree::TerminalNode *LINE();
    antlr4::tree::TerminalNode *LINES();

   
  };

  SendAdvancingLinesContext* sendAdvancingLines();

  class  SendAdvancingMnemonicContext : public antlr4::ParserRuleContext {
  public:
    SendAdvancingMnemonicContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    MnemonicNameContext *mnemonicName();

   
  };

  SendAdvancingMnemonicContext* sendAdvancingMnemonic();

  class  SetStatementContext : public antlr4::ParserRuleContext {
  public:
    SetStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SET();
    SetUpDownByStatementContext *setUpDownByStatement();
    std::vector<SetToStatementContext *> setToStatement();
    SetToStatementContext* setToStatement(size_t i);

   
  };

  SetStatementContext* setStatement();

  class  SetToStatementContext : public antlr4::ParserRuleContext {
  public:
    SetToStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TO();
    std::vector<SetToContext *> setTo();
    SetToContext* setTo(size_t i);
    std::vector<SetToValueContext *> setToValue();
    SetToValueContext* setToValue(size_t i);

   
  };

  SetToStatementContext* setToStatement();

  class  SetUpDownByStatementContext : public antlr4::ParserRuleContext {
  public:
    SetUpDownByStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    SetByValueContext *setByValue();
    antlr4::tree::TerminalNode *UP();
    antlr4::tree::TerminalNode *BY();
    antlr4::tree::TerminalNode *DOWN();
    std::vector<SetToContext *> setTo();
    SetToContext* setTo(size_t i);

   
  };

  SetUpDownByStatementContext* setUpDownByStatement();

  class  SetToContext : public antlr4::ParserRuleContext {
  public:
    SetToContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();

   
  };

  SetToContext* setTo();

  class  SetToValueContext : public antlr4::ParserRuleContext {
  public:
    SetToValueContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *OFF();
    antlr4::tree::TerminalNode *ENTRY();
    IdentifierContext *identifier();
    LiteralContext *literal();

   
  };

  SetToValueContext* setToValue();

  class  SetByValueContext : public antlr4::ParserRuleContext {
  public:
    SetByValueContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    LiteralContext *literal();

   
  };

  SetByValueContext* setByValue();

  class  SortStatementContext : public antlr4::ParserRuleContext {
  public:
    SortStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SORT();
    FileNameContext *fileName();
    std::vector<SortOnKeyClauseContext *> sortOnKeyClause();
    SortOnKeyClauseContext* sortOnKeyClause(size_t i);
    SortDuplicatesPhraseContext *sortDuplicatesPhrase();
    SortCollatingSequencePhraseContext *sortCollatingSequencePhrase();
    SortInputProcedurePhraseContext *sortInputProcedurePhrase();
    std::vector<SortUsingContext *> sortUsing();
    SortUsingContext* sortUsing(size_t i);
    SortOutputProcedurePhraseContext *sortOutputProcedurePhrase();
    std::vector<SortGivingPhraseContext *> sortGivingPhrase();
    SortGivingPhraseContext* sortGivingPhrase(size_t i);

   
  };

  SortStatementContext* sortStatement();

  class  SortOnKeyClauseContext : public antlr4::ParserRuleContext {
  public:
    SortOnKeyClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ASCENDING();
    antlr4::tree::TerminalNode *DESCENDING();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *KEY();
    std::vector<QualifiedDataNameContext *> qualifiedDataName();
    QualifiedDataNameContext* qualifiedDataName(size_t i);

   
  };

  SortOnKeyClauseContext* sortOnKeyClause();

  class  SortDuplicatesPhraseContext : public antlr4::ParserRuleContext {
  public:
    SortDuplicatesPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DUPLICATES();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *IN();
    antlr4::tree::TerminalNode *ORDER();

   
  };

  SortDuplicatesPhraseContext* sortDuplicatesPhrase();

  class  SortCollatingSequencePhraseContext : public antlr4::ParserRuleContext {
  public:
    SortCollatingSequencePhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SEQUENCE();
    antlr4::tree::TerminalNode *COLLATING();
    antlr4::tree::TerminalNode *IS();
    std::vector<AlphabetNameContext *> alphabetName();
    AlphabetNameContext* alphabetName(size_t i);
    SortCollatingAlphanumericContext *sortCollatingAlphanumeric();
    SortCollatingNationalContext *sortCollatingNational();

   
  };

  SortCollatingSequencePhraseContext* sortCollatingSequencePhrase();

  class  SortCollatingAlphanumericContext : public antlr4::ParserRuleContext {
  public:
    SortCollatingAlphanumericContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALPHANUMERIC();
    antlr4::tree::TerminalNode *IS();
    AlphabetNameContext *alphabetName();
    antlr4::tree::TerminalNode *FOR();

   
  };

  SortCollatingAlphanumericContext* sortCollatingAlphanumeric();

  class  SortCollatingNationalContext : public antlr4::ParserRuleContext {
  public:
    SortCollatingNationalContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NATIONAL();
    AlphabetNameContext *alphabetName();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *IS();

   
  };

  SortCollatingNationalContext* sortCollatingNational();

  class  SortInputProcedurePhraseContext : public antlr4::ParserRuleContext {
  public:
    SortInputProcedurePhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INPUT();
    antlr4::tree::TerminalNode *PROCEDURE();
    ProcedureNameContext *procedureName();
    antlr4::tree::TerminalNode *IS();
    SortInputThroughContext *sortInputThrough();

   
  };

  SortInputProcedurePhraseContext* sortInputProcedurePhrase();

  class  SortInputThroughContext : public antlr4::ParserRuleContext {
  public:
    SortInputThroughContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ProcedureNameContext *procedureName();
    antlr4::tree::TerminalNode *THROUGH();
    antlr4::tree::TerminalNode *THRU();

   
  };

  SortInputThroughContext* sortInputThrough();

  class  SortUsingContext : public antlr4::ParserRuleContext {
  public:
    SortUsingContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *USING();
    std::vector<FileNameContext *> fileName();
    FileNameContext* fileName(size_t i);

   
  };

  SortUsingContext* sortUsing();

  class  SortOutputProcedurePhraseContext : public antlr4::ParserRuleContext {
  public:
    SortOutputProcedurePhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OUTPUT();
    antlr4::tree::TerminalNode *PROCEDURE();
    ProcedureNameContext *procedureName();
    antlr4::tree::TerminalNode *IS();
    SortOutputThroughContext *sortOutputThrough();

   
  };

  SortOutputProcedurePhraseContext* sortOutputProcedurePhrase();

  class  SortOutputThroughContext : public antlr4::ParserRuleContext {
  public:
    SortOutputThroughContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ProcedureNameContext *procedureName();
    antlr4::tree::TerminalNode *THROUGH();
    antlr4::tree::TerminalNode *THRU();

   
  };

  SortOutputThroughContext* sortOutputThrough();

  class  SortGivingPhraseContext : public antlr4::ParserRuleContext {
  public:
    SortGivingPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GIVING();
    std::vector<SortGivingContext *> sortGiving();
    SortGivingContext* sortGiving(size_t i);

   
  };

  SortGivingPhraseContext* sortGivingPhrase();

  class  SortGivingContext : public antlr4::ParserRuleContext {
  public:
    SortGivingContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    FileNameContext *fileName();
    antlr4::tree::TerminalNode *LOCK();
    antlr4::tree::TerminalNode *SAVE();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *REWIND();
    antlr4::tree::TerminalNode *CRUNCH();
    antlr4::tree::TerminalNode *RELEASE();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *REMOVE();

   
  };

  SortGivingContext* sortGiving();

  class  StartStatementContext : public antlr4::ParserRuleContext {
  public:
    StartStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *START();
    FileNameContext *fileName();
    StartKeyContext *startKey();
    InvalidKeyPhraseContext *invalidKeyPhrase();
    NotInvalidKeyPhraseContext *notInvalidKeyPhrase();
    antlr4::tree::TerminalNode *END_START();

   
  };

  StartStatementContext* startStatement();

  class  StartKeyContext : public antlr4::ParserRuleContext {
  public:
    StartKeyContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *KEY();
    QualifiedDataNameContext *qualifiedDataName();
    antlr4::tree::TerminalNode *EQUAL();
    antlr4::tree::TerminalNode *EQUALCHAR();
    antlr4::tree::TerminalNode *GREATER();
    antlr4::tree::TerminalNode *MORETHANCHAR();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *LESS();
    antlr4::tree::TerminalNode *LESSTHANCHAR();
    antlr4::tree::TerminalNode *OR();
    antlr4::tree::TerminalNode *MORETHANOREQUAL();
    antlr4::tree::TerminalNode *IS();
    antlr4::tree::TerminalNode *TO();
    antlr4::tree::TerminalNode *THAN();

   
  };

  StartKeyContext* startKey();

  class  StopStatementContext : public antlr4::ParserRuleContext {
  public:
    StopStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *STOP();
    antlr4::tree::TerminalNode *RUN();
    LiteralContext *literal();

   
  };

  StopStatementContext* stopStatement();

  class  StringStatementContext : public antlr4::ParserRuleContext {
  public:
    StringStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *STRING();
    StringIntoPhraseContext *stringIntoPhrase();
    std::vector<StringSendingPhraseContext *> stringSendingPhrase();
    StringSendingPhraseContext* stringSendingPhrase(size_t i);
    StringWithPointerPhraseContext *stringWithPointerPhrase();
    OnOverflowPhraseContext *onOverflowPhrase();
    NotOnOverflowPhraseContext *notOnOverflowPhrase();
    antlr4::tree::TerminalNode *END_STRING();

   
  };

  StringStatementContext* stringStatement();

  class  StringSendingPhraseContext : public antlr4::ParserRuleContext {
  public:
    StringSendingPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    StringDelimitedByPhraseContext *stringDelimitedByPhrase();
    StringForPhraseContext *stringForPhrase();
    std::vector<StringSendingContext *> stringSending();
    StringSendingContext* stringSending(size_t i);

   
  };

  StringSendingPhraseContext* stringSendingPhrase();

  class  StringSendingContext : public antlr4::ParserRuleContext {
  public:
    StringSendingContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    LiteralContext *literal();

   
  };

  StringSendingContext* stringSending();

  class  StringDelimitedByPhraseContext : public antlr4::ParserRuleContext {
  public:
    StringDelimitedByPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DELIMITED();
    antlr4::tree::TerminalNode *SIZE();
    IdentifierContext *identifier();
    LiteralContext *literal();
    antlr4::tree::TerminalNode *BY();

   
  };

  StringDelimitedByPhraseContext* stringDelimitedByPhrase();

  class  StringForPhraseContext : public antlr4::ParserRuleContext {
  public:
    StringForPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FOR();
    IdentifierContext *identifier();
    LiteralContext *literal();

   
  };

  StringForPhraseContext* stringForPhrase();

  class  StringIntoPhraseContext : public antlr4::ParserRuleContext {
  public:
    StringIntoPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INTO();
    IdentifierContext *identifier();

   
  };

  StringIntoPhraseContext* stringIntoPhrase();

  class  StringWithPointerPhraseContext : public antlr4::ParserRuleContext {
  public:
    StringWithPointerPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *POINTER();
    QualifiedDataNameContext *qualifiedDataName();
    antlr4::tree::TerminalNode *WITH();

   
  };

  StringWithPointerPhraseContext* stringWithPointerPhrase();

  class  SubtractStatementContext : public antlr4::ParserRuleContext {
  public:
    SubtractStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SUBTRACT();
    SubtractFromStatementContext *subtractFromStatement();
    SubtractFromGivingStatementContext *subtractFromGivingStatement();
    SubtractCorrespondingStatementContext *subtractCorrespondingStatement();
    OnSizeErrorPhraseContext *onSizeErrorPhrase();
    NotOnSizeErrorPhraseContext *notOnSizeErrorPhrase();
    antlr4::tree::TerminalNode *END_SUBTRACT();

   
  };

  SubtractStatementContext* subtractStatement();

  class  SubtractFromStatementContext : public antlr4::ParserRuleContext {
  public:
    SubtractFromStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FROM();
    std::vector<SubtractSubtrahendContext *> subtractSubtrahend();
    SubtractSubtrahendContext* subtractSubtrahend(size_t i);
    std::vector<SubtractMinuendContext *> subtractMinuend();
    SubtractMinuendContext* subtractMinuend(size_t i);

   
  };

  SubtractFromStatementContext* subtractFromStatement();

  class  SubtractFromGivingStatementContext : public antlr4::ParserRuleContext {
  public:
    SubtractFromGivingStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FROM();
    SubtractMinuendGivingContext *subtractMinuendGiving();
    antlr4::tree::TerminalNode *GIVING();
    std::vector<SubtractSubtrahendContext *> subtractSubtrahend();
    SubtractSubtrahendContext* subtractSubtrahend(size_t i);
    std::vector<SubtractGivingContext *> subtractGiving();
    SubtractGivingContext* subtractGiving(size_t i);

   
  };

  SubtractFromGivingStatementContext* subtractFromGivingStatement();

  class  SubtractCorrespondingStatementContext : public antlr4::ParserRuleContext {
  public:
    SubtractCorrespondingStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    QualifiedDataNameContext *qualifiedDataName();
    antlr4::tree::TerminalNode *FROM();
    SubtractMinuendCorrespondingContext *subtractMinuendCorresponding();
    antlr4::tree::TerminalNode *CORRESPONDING();
    antlr4::tree::TerminalNode *CORR();

   
  };

  SubtractCorrespondingStatementContext* subtractCorrespondingStatement();

  class  SubtractSubtrahendContext : public antlr4::ParserRuleContext {
  public:
    SubtractSubtrahendContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    LiteralContext *literal();

   
  };

  SubtractSubtrahendContext* subtractSubtrahend();

  class  SubtractMinuendContext : public antlr4::ParserRuleContext {
  public:
    SubtractMinuendContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    antlr4::tree::TerminalNode *ROUNDED();

   
  };

  SubtractMinuendContext* subtractMinuend();

  class  SubtractMinuendGivingContext : public antlr4::ParserRuleContext {
  public:
    SubtractMinuendGivingContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    LiteralContext *literal();

   
  };

  SubtractMinuendGivingContext* subtractMinuendGiving();

  class  SubtractGivingContext : public antlr4::ParserRuleContext {
  public:
    SubtractGivingContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    antlr4::tree::TerminalNode *ROUNDED();

   
  };

  SubtractGivingContext* subtractGiving();

  class  SubtractMinuendCorrespondingContext : public antlr4::ParserRuleContext {
  public:
    SubtractMinuendCorrespondingContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    QualifiedDataNameContext *qualifiedDataName();
    antlr4::tree::TerminalNode *ROUNDED();

   
  };

  SubtractMinuendCorrespondingContext* subtractMinuendCorresponding();

  class  TerminateStatementContext : public antlr4::ParserRuleContext {
  public:
    TerminateStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TERMINATE();
    ReportNameContext *reportName();

   
  };

  TerminateStatementContext* terminateStatement();

  class  UnstringStatementContext : public antlr4::ParserRuleContext {
  public:
    UnstringStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *UNSTRING();
    UnstringSendingPhraseContext *unstringSendingPhrase();
    UnstringIntoPhraseContext *unstringIntoPhrase();
    UnstringWithPointerPhraseContext *unstringWithPointerPhrase();
    UnstringTallyingPhraseContext *unstringTallyingPhrase();
    OnOverflowPhraseContext *onOverflowPhrase();
    NotOnOverflowPhraseContext *notOnOverflowPhrase();
    antlr4::tree::TerminalNode *END_UNSTRING();

   
  };

  UnstringStatementContext* unstringStatement();

  class  UnstringSendingPhraseContext : public antlr4::ParserRuleContext {
  public:
    UnstringSendingPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    UnstringDelimitedByPhraseContext *unstringDelimitedByPhrase();
    std::vector<UnstringOrAllPhraseContext *> unstringOrAllPhrase();
    UnstringOrAllPhraseContext* unstringOrAllPhrase(size_t i);

   
  };

  UnstringSendingPhraseContext* unstringSendingPhrase();

  class  UnstringDelimitedByPhraseContext : public antlr4::ParserRuleContext {
  public:
    UnstringDelimitedByPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DELIMITED();
    IdentifierContext *identifier();
    LiteralContext *literal();
    antlr4::tree::TerminalNode *BY();
    antlr4::tree::TerminalNode *ALL();

   
  };

  UnstringDelimitedByPhraseContext* unstringDelimitedByPhrase();

  class  UnstringOrAllPhraseContext : public antlr4::ParserRuleContext {
  public:
    UnstringOrAllPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OR();
    IdentifierContext *identifier();
    LiteralContext *literal();
    antlr4::tree::TerminalNode *ALL();

   
  };

  UnstringOrAllPhraseContext* unstringOrAllPhrase();

  class  UnstringIntoPhraseContext : public antlr4::ParserRuleContext {
  public:
    UnstringIntoPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INTO();
    std::vector<UnstringIntoContext *> unstringInto();
    UnstringIntoContext* unstringInto(size_t i);

   
  };

  UnstringIntoPhraseContext* unstringIntoPhrase();

  class  UnstringIntoContext : public antlr4::ParserRuleContext {
  public:
    UnstringIntoContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    UnstringDelimiterInContext *unstringDelimiterIn();
    UnstringCountInContext *unstringCountIn();

   
  };

  UnstringIntoContext* unstringInto();

  class  UnstringDelimiterInContext : public antlr4::ParserRuleContext {
  public:
    UnstringDelimiterInContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DELIMITER();
    IdentifierContext *identifier();
    antlr4::tree::TerminalNode *IN();

   
  };

  UnstringDelimiterInContext* unstringDelimiterIn();

  class  UnstringCountInContext : public antlr4::ParserRuleContext {
  public:
    UnstringCountInContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *COUNT();
    IdentifierContext *identifier();
    antlr4::tree::TerminalNode *IN();

   
  };

  UnstringCountInContext* unstringCountIn();

  class  UnstringWithPointerPhraseContext : public antlr4::ParserRuleContext {
  public:
    UnstringWithPointerPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *POINTER();
    QualifiedDataNameContext *qualifiedDataName();
    antlr4::tree::TerminalNode *WITH();

   
  };

  UnstringWithPointerPhraseContext* unstringWithPointerPhrase();

  class  UnstringTallyingPhraseContext : public antlr4::ParserRuleContext {
  public:
    UnstringTallyingPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TALLYING();
    QualifiedDataNameContext *qualifiedDataName();
    antlr4::tree::TerminalNode *IN();

   
  };

  UnstringTallyingPhraseContext* unstringTallyingPhrase();

  class  UseStatementContext : public antlr4::ParserRuleContext {
  public:
    UseStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *USE();
    UseAfterClauseContext *useAfterClause();
    UseDebugClauseContext *useDebugClause();

   
  };

  UseStatementContext* useStatement();

  class  UseAfterClauseContext : public antlr4::ParserRuleContext {
  public:
    UseAfterClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *AFTER();
    antlr4::tree::TerminalNode *PROCEDURE();
    UseAfterOnContext *useAfterOn();
    antlr4::tree::TerminalNode *EXCEPTION();
    antlr4::tree::TerminalNode *ERROR();
    antlr4::tree::TerminalNode *GLOBAL();
    antlr4::tree::TerminalNode *STANDARD();
    antlr4::tree::TerminalNode *ON();

   
  };

  UseAfterClauseContext* useAfterClause();

  class  UseAfterOnContext : public antlr4::ParserRuleContext {
  public:
    UseAfterOnContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INPUT();
    antlr4::tree::TerminalNode *OUTPUT();
    antlr4::tree::TerminalNode *I_O();
    antlr4::tree::TerminalNode *EXTEND();
    std::vector<FileNameContext *> fileName();
    FileNameContext* fileName(size_t i);

   
  };

  UseAfterOnContext* useAfterOn();

  class  UseDebugClauseContext : public antlr4::ParserRuleContext {
  public:
    UseDebugClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DEBUGGING();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *ON();
    std::vector<UseDebugOnContext *> useDebugOn();
    UseDebugOnContext* useDebugOn(size_t i);

   
  };

  UseDebugClauseContext* useDebugClause();

  class  UseDebugOnContext : public antlr4::ParserRuleContext {
  public:
    UseDebugOnContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *PROCEDURES();
    IdentifierContext *identifier();
    antlr4::tree::TerminalNode *REFERENCES();
    antlr4::tree::TerminalNode *OF();
    ProcedureNameContext *procedureName();
    FileNameContext *fileName();

   
  };

  UseDebugOnContext* useDebugOn();

  class  WriteStatementContext : public antlr4::ParserRuleContext {
  public:
    WriteStatementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WRITE();
    RecordNameContext *recordName();
    WriteFromPhraseContext *writeFromPhrase();
    WriteAdvancingPhraseContext *writeAdvancingPhrase();
    WriteAtEndOfPagePhraseContext *writeAtEndOfPagePhrase();
    WriteNotAtEndOfPagePhraseContext *writeNotAtEndOfPagePhrase();
    InvalidKeyPhraseContext *invalidKeyPhrase();
    NotInvalidKeyPhraseContext *notInvalidKeyPhrase();
    antlr4::tree::TerminalNode *END_WRITE();

   
  };

  WriteStatementContext* writeStatement();

  class  WriteFromPhraseContext : public antlr4::ParserRuleContext {
  public:
    WriteFromPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FROM();
    IdentifierContext *identifier();
    LiteralContext *literal();

   
  };

  WriteFromPhraseContext* writeFromPhrase();

  class  WriteAdvancingPhraseContext : public antlr4::ParserRuleContext {
  public:
    WriteAdvancingPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BEFORE();
    antlr4::tree::TerminalNode *AFTER();
    WriteAdvancingPageContext *writeAdvancingPage();
    WriteAdvancingLinesContext *writeAdvancingLines();
    WriteAdvancingMnemonicContext *writeAdvancingMnemonic();
    antlr4::tree::TerminalNode *ADVANCING();

   
  };

  WriteAdvancingPhraseContext* writeAdvancingPhrase();

  class  WriteAdvancingPageContext : public antlr4::ParserRuleContext {
  public:
    WriteAdvancingPageContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PAGE();

   
  };

  WriteAdvancingPageContext* writeAdvancingPage();

  class  WriteAdvancingLinesContext : public antlr4::ParserRuleContext {
  public:
    WriteAdvancingLinesContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    LiteralContext *literal();
    antlr4::tree::TerminalNode *LINE();
    antlr4::tree::TerminalNode *LINES();

   
  };

  WriteAdvancingLinesContext* writeAdvancingLines();

  class  WriteAdvancingMnemonicContext : public antlr4::ParserRuleContext {
  public:
    WriteAdvancingMnemonicContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    MnemonicNameContext *mnemonicName();

   
  };

  WriteAdvancingMnemonicContext* writeAdvancingMnemonic();

  class  WriteAtEndOfPagePhraseContext : public antlr4::ParserRuleContext {
  public:
    WriteAtEndOfPagePhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *END_OF_PAGE();
    antlr4::tree::TerminalNode *EOP();
    antlr4::tree::TerminalNode *AT();
    std::vector<StatementContext *> statement();
    StatementContext* statement(size_t i);

   
  };

  WriteAtEndOfPagePhraseContext* writeAtEndOfPagePhrase();

  class  WriteNotAtEndOfPagePhraseContext : public antlr4::ParserRuleContext {
  public:
    WriteNotAtEndOfPagePhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *END_OF_PAGE();
    antlr4::tree::TerminalNode *EOP();
    antlr4::tree::TerminalNode *AT();
    std::vector<StatementContext *> statement();
    StatementContext* statement(size_t i);

   
  };

  WriteNotAtEndOfPagePhraseContext* writeNotAtEndOfPagePhrase();

  class  AtEndPhraseContext : public antlr4::ParserRuleContext {
  public:
    AtEndPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *END();
    antlr4::tree::TerminalNode *AT();
    std::vector<StatementContext *> statement();
    StatementContext* statement(size_t i);

   
  };

  AtEndPhraseContext* atEndPhrase();

  class  NotAtEndPhraseContext : public antlr4::ParserRuleContext {
  public:
    NotAtEndPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *END();
    antlr4::tree::TerminalNode *AT();
    std::vector<StatementContext *> statement();
    StatementContext* statement(size_t i);

   
  };

  NotAtEndPhraseContext* notAtEndPhrase();

  class  InvalidKeyPhraseContext : public antlr4::ParserRuleContext {
  public:
    InvalidKeyPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INVALID();
    antlr4::tree::TerminalNode *KEY();
    std::vector<StatementContext *> statement();
    StatementContext* statement(size_t i);

   
  };

  InvalidKeyPhraseContext* invalidKeyPhrase();

  class  NotInvalidKeyPhraseContext : public antlr4::ParserRuleContext {
  public:
    NotInvalidKeyPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *INVALID();
    antlr4::tree::TerminalNode *KEY();
    std::vector<StatementContext *> statement();
    StatementContext* statement(size_t i);

   
  };

  NotInvalidKeyPhraseContext* notInvalidKeyPhrase();

  class  OnOverflowPhraseContext : public antlr4::ParserRuleContext {
  public:
    OnOverflowPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OVERFLOW();
    antlr4::tree::TerminalNode *ON();
    std::vector<StatementContext *> statement();
    StatementContext* statement(size_t i);

   
  };

  OnOverflowPhraseContext* onOverflowPhrase();

  class  NotOnOverflowPhraseContext : public antlr4::ParserRuleContext {
  public:
    NotOnOverflowPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *OVERFLOW();
    antlr4::tree::TerminalNode *ON();
    std::vector<StatementContext *> statement();
    StatementContext* statement(size_t i);

   
  };

  NotOnOverflowPhraseContext* notOnOverflowPhrase();

  class  OnSizeErrorPhraseContext : public antlr4::ParserRuleContext {
  public:
    OnSizeErrorPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SIZE();
    antlr4::tree::TerminalNode *ERROR();
    antlr4::tree::TerminalNode *ON();
    std::vector<StatementContext *> statement();
    StatementContext* statement(size_t i);

   
  };

  OnSizeErrorPhraseContext* onSizeErrorPhrase();

  class  NotOnSizeErrorPhraseContext : public antlr4::ParserRuleContext {
  public:
    NotOnSizeErrorPhraseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *SIZE();
    antlr4::tree::TerminalNode *ERROR();
    antlr4::tree::TerminalNode *ON();
    std::vector<StatementContext *> statement();
    StatementContext* statement(size_t i);

   
  };

  NotOnSizeErrorPhraseContext* notOnSizeErrorPhrase();

  class  OnExceptionClauseContext : public antlr4::ParserRuleContext {
  public:
    OnExceptionClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *EXCEPTION();
    antlr4::tree::TerminalNode *ON();
    std::vector<StatementContext *> statement();
    StatementContext* statement(size_t i);

   
  };

  OnExceptionClauseContext* onExceptionClause();

  class  NotOnExceptionClauseContext : public antlr4::ParserRuleContext {
  public:
    NotOnExceptionClauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *EXCEPTION();
    antlr4::tree::TerminalNode *ON();
    std::vector<StatementContext *> statement();
    StatementContext* statement(size_t i);

   
  };

  NotOnExceptionClauseContext* notOnExceptionClause();

  class  ArithmeticExpressionContext : public antlr4::ParserRuleContext {
  public:
    ArithmeticExpressionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    MultDivsContext *multDivs();
    std::vector<PlusMinusContext *> plusMinus();
    PlusMinusContext* plusMinus(size_t i);

   
  };

  ArithmeticExpressionContext* arithmeticExpression();

  class  PlusMinusContext : public antlr4::ParserRuleContext {
  public:
    PlusMinusContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    MultDivsContext *multDivs();
    antlr4::tree::TerminalNode *PLUSCHAR();
    antlr4::tree::TerminalNode *MINUSCHAR();

   
  };

  PlusMinusContext* plusMinus();

  class  MultDivsContext : public antlr4::ParserRuleContext {
  public:
    MultDivsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    PowersContext *powers();
    std::vector<MultDivContext *> multDiv();
    MultDivContext* multDiv(size_t i);

   
  };

  MultDivsContext* multDivs();

  class  MultDivContext : public antlr4::ParserRuleContext {
  public:
    MultDivContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    PowersContext *powers();
    antlr4::tree::TerminalNode *ASTERISKCHAR();
    antlr4::tree::TerminalNode *SLASHCHAR();

   
  };

  MultDivContext* multDiv();

  class  PowersContext : public antlr4::ParserRuleContext {
  public:
    PowersContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    BasisContext *basis();
    std::vector<PowerContext *> power();
    PowerContext* power(size_t i);
    antlr4::tree::TerminalNode *PLUSCHAR();
    antlr4::tree::TerminalNode *MINUSCHAR();

   
  };

  PowersContext* powers();

  class  PowerContext : public antlr4::ParserRuleContext {
  public:
    PowerContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DOUBLEASTERISKCHAR();
    BasisContext *basis();

   
  };

  PowerContext* power();

  class  BasisContext : public antlr4::ParserRuleContext {
  public:
    BasisContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LPARENCHAR();
    ArithmeticExpressionContext *arithmeticExpression();
    antlr4::tree::TerminalNode *RPARENCHAR();
    IdentifierContext *identifier();
    LiteralContext *literal();

   
  };

  BasisContext* basis();

  class  ConditionContext : public antlr4::ParserRuleContext {
  public:
    ConditionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    CombinableConditionContext *combinableCondition();
    std::vector<AndOrConditionContext *> andOrCondition();
    AndOrConditionContext* andOrCondition(size_t i);

   
  };

  ConditionContext* condition();

  class  AndOrConditionContext : public antlr4::ParserRuleContext {
  public:
    AndOrConditionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *AND();
    antlr4::tree::TerminalNode *OR();
    CombinableConditionContext *combinableCondition();
    std::vector<AbbreviationContext *> abbreviation();
    AbbreviationContext* abbreviation(size_t i);

   
  };

  AndOrConditionContext* andOrCondition();

  class  CombinableConditionContext : public antlr4::ParserRuleContext {
  public:
    CombinableConditionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    SimpleConditionContext *simpleCondition();
    antlr4::tree::TerminalNode *NOT();

   
  };

  CombinableConditionContext* combinableCondition();

  class  SimpleConditionContext : public antlr4::ParserRuleContext {
  public:
    SimpleConditionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LPARENCHAR();
    ConditionContext *condition();
    antlr4::tree::TerminalNode *RPARENCHAR();
    RelationConditionContext *relationCondition();
    ClassConditionContext *classCondition();
    ConditionNameReferenceContext *conditionNameReference();

   
  };

  SimpleConditionContext* simpleCondition();

  class  ClassConditionContext : public antlr4::ParserRuleContext {
  public:
    ClassConditionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    antlr4::tree::TerminalNode *NUMERIC();
    antlr4::tree::TerminalNode *ALPHABETIC();
    antlr4::tree::TerminalNode *ALPHABETIC_LOWER();
    antlr4::tree::TerminalNode *ALPHABETIC_UPPER();
    antlr4::tree::TerminalNode *DBCS();
    antlr4::tree::TerminalNode *KANJI();
    ClassNameContext *className();
    antlr4::tree::TerminalNode *IS();
    antlr4::tree::TerminalNode *NOT();

   
  };

  ClassConditionContext* classCondition();

  class  ConditionNameReferenceContext : public antlr4::ParserRuleContext {
  public:
    ConditionNameReferenceContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ConditionNameContext *conditionName();
    std::vector<InDataContext *> inData();
    InDataContext* inData(size_t i);
    InFileContext *inFile();
    std::vector<ConditionNameSubscriptReferenceContext *> conditionNameSubscriptReference();
    ConditionNameSubscriptReferenceContext* conditionNameSubscriptReference(size_t i);
    std::vector<InMnemonicContext *> inMnemonic();
    InMnemonicContext* inMnemonic(size_t i);

   
  };

  ConditionNameReferenceContext* conditionNameReference();

  class  ConditionNameSubscriptReferenceContext : public antlr4::ParserRuleContext {
  public:
    ConditionNameSubscriptReferenceContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LPARENCHAR();
    std::vector<Subscript_Context *> subscript_();
    Subscript_Context* subscript_(size_t i);
    antlr4::tree::TerminalNode *RPARENCHAR();
    std::vector<antlr4::tree::TerminalNode *> COMMACHAR();
    antlr4::tree::TerminalNode* COMMACHAR(size_t i);

   
  };

  ConditionNameSubscriptReferenceContext* conditionNameSubscriptReference();

  class  RelationConditionContext : public antlr4::ParserRuleContext {
  public:
    RelationConditionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    RelationSignConditionContext *relationSignCondition();
    RelationArithmeticComparisonContext *relationArithmeticComparison();
    RelationCombinedComparisonContext *relationCombinedComparison();

   
  };

  RelationConditionContext* relationCondition();

  class  RelationSignConditionContext : public antlr4::ParserRuleContext {
  public:
    RelationSignConditionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ArithmeticExpressionContext *arithmeticExpression();
    antlr4::tree::TerminalNode *POSITIVE();
    antlr4::tree::TerminalNode *NEGATIVE();
    antlr4::tree::TerminalNode *ZERO();
    antlr4::tree::TerminalNode *IS();
    antlr4::tree::TerminalNode *NOT();

   
  };

  RelationSignConditionContext* relationSignCondition();

  class  RelationArithmeticComparisonContext : public antlr4::ParserRuleContext {
  public:
    RelationArithmeticComparisonContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<ArithmeticExpressionContext *> arithmeticExpression();
    ArithmeticExpressionContext* arithmeticExpression(size_t i);
    RelationalOperatorContext *relationalOperator();

   
  };

  RelationArithmeticComparisonContext* relationArithmeticComparison();

  class  RelationCombinedComparisonContext : public antlr4::ParserRuleContext {
  public:
    RelationCombinedComparisonContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ArithmeticExpressionContext *arithmeticExpression();
    RelationalOperatorContext *relationalOperator();
    antlr4::tree::TerminalNode *LPARENCHAR();
    RelationCombinedConditionContext *relationCombinedCondition();
    antlr4::tree::TerminalNode *RPARENCHAR();

   
  };

  RelationCombinedComparisonContext* relationCombinedComparison();

  class  RelationCombinedConditionContext : public antlr4::ParserRuleContext {
  public:
    RelationCombinedConditionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<ArithmeticExpressionContext *> arithmeticExpression();
    ArithmeticExpressionContext* arithmeticExpression(size_t i);
    std::vector<antlr4::tree::TerminalNode *> AND();
    antlr4::tree::TerminalNode* AND(size_t i);
    std::vector<antlr4::tree::TerminalNode *> OR();
    antlr4::tree::TerminalNode* OR(size_t i);

   
  };

  RelationCombinedConditionContext* relationCombinedCondition();

  class  RelationalOperatorContext : public antlr4::ParserRuleContext {
  public:
    RelationalOperatorContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NOTEQUALCHAR();
    antlr4::tree::TerminalNode *GREATER();
    antlr4::tree::TerminalNode *OR();
    antlr4::tree::TerminalNode *EQUAL();
    antlr4::tree::TerminalNode *MORETHANOREQUAL();
    antlr4::tree::TerminalNode *LESS();
    antlr4::tree::TerminalNode *LESSTHANOREQUAL();
    antlr4::tree::TerminalNode *IS();
    antlr4::tree::TerminalNode *ARE();
    antlr4::tree::TerminalNode *MORETHANCHAR();
    antlr4::tree::TerminalNode *LESSTHANCHAR();
    antlr4::tree::TerminalNode *EQUALCHAR();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *THAN();
    antlr4::tree::TerminalNode *TO();

   
  };

  RelationalOperatorContext* relationalOperator();

  class  AbbreviationContext : public antlr4::ParserRuleContext {
  public:
    AbbreviationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ArithmeticExpressionContext *arithmeticExpression();
    antlr4::tree::TerminalNode *LPARENCHAR();
    AbbreviationContext *abbreviation();
    antlr4::tree::TerminalNode *RPARENCHAR();
    antlr4::tree::TerminalNode *NOT();
    RelationalOperatorContext *relationalOperator();

   
  };

  AbbreviationContext* abbreviation();

  class  IdentifierContext : public antlr4::ParserRuleContext {
  public:
    IdentifierContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    QualifiedDataNameContext *qualifiedDataName();
    TableCallContext *tableCall();
    FunctionCallContext *functionCall();
    SpecialRegisterContext *specialRegister();

   
  };

  IdentifierContext* identifier();

  class  TableCallContext : public antlr4::ParserRuleContext {
  public:
    TableCallContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    QualifiedDataNameContext *qualifiedDataName();
    std::vector<antlr4::tree::TerminalNode *> LPARENCHAR();
    antlr4::tree::TerminalNode* LPARENCHAR(size_t i);
    std::vector<Subscript_Context *> subscript_();
    Subscript_Context* subscript_(size_t i);
    std::vector<antlr4::tree::TerminalNode *> RPARENCHAR();
    antlr4::tree::TerminalNode* RPARENCHAR(size_t i);
    ReferenceModifierContext *referenceModifier();
    std::vector<antlr4::tree::TerminalNode *> COMMACHAR();
    antlr4::tree::TerminalNode* COMMACHAR(size_t i);

   
  };

  TableCallContext* tableCall();

  class  FunctionCallContext : public antlr4::ParserRuleContext {
  public:
    FunctionCallContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FUNCTION();
    FunctionNameContext *functionName();
    std::vector<antlr4::tree::TerminalNode *> LPARENCHAR();
    antlr4::tree::TerminalNode* LPARENCHAR(size_t i);
    std::vector<ArgumentContext *> argument();
    ArgumentContext* argument(size_t i);
    std::vector<antlr4::tree::TerminalNode *> RPARENCHAR();
    antlr4::tree::TerminalNode* RPARENCHAR(size_t i);
    ReferenceModifierContext *referenceModifier();
    std::vector<antlr4::tree::TerminalNode *> COMMACHAR();
    antlr4::tree::TerminalNode* COMMACHAR(size_t i);

   
  };

  FunctionCallContext* functionCall();

  class  ReferenceModifierContext : public antlr4::ParserRuleContext {
  public:
    ReferenceModifierContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LPARENCHAR();
    CharacterPositionContext *characterPosition();
    antlr4::tree::TerminalNode *COLONCHAR();
    antlr4::tree::TerminalNode *RPARENCHAR();
    LengthContext *length();

   
  };

  ReferenceModifierContext* referenceModifier();

  class  CharacterPositionContext : public antlr4::ParserRuleContext {
  public:
    CharacterPositionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ArithmeticExpressionContext *arithmeticExpression();

   
  };

  CharacterPositionContext* characterPosition();

  class  LengthContext : public antlr4::ParserRuleContext {
  public:
    LengthContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ArithmeticExpressionContext *arithmeticExpression();

   
  };

  LengthContext* length();

  class  Subscript_Context : public antlr4::ParserRuleContext {
  public:
    Subscript_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALL();
    IntegerLiteralContext *integerLiteral();
    QualifiedDataNameContext *qualifiedDataName();
    IndexNameContext *indexName();
    ArithmeticExpressionContext *arithmeticExpression();

   
  };

  Subscript_Context* subscript_();

  class  ArgumentContext : public antlr4::ParserRuleContext {
  public:
    ArgumentContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    LiteralContext *literal();
    IdentifierContext *identifier();
    QualifiedDataNameContext *qualifiedDataName();
    IntegerLiteralContext *integerLiteral();
    IndexNameContext *indexName();
    ArithmeticExpressionContext *arithmeticExpression();

   
  };

  ArgumentContext* argument();

  class  QualifiedDataNameContext : public antlr4::ParserRuleContext {
  public:
    QualifiedDataNameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    QualifiedDataNameFormat1Context *qualifiedDataNameFormat1();
    QualifiedDataNameFormat2Context *qualifiedDataNameFormat2();
    QualifiedDataNameFormat3Context *qualifiedDataNameFormat3();
    QualifiedDataNameFormat4Context *qualifiedDataNameFormat4();

   
  };

  QualifiedDataNameContext* qualifiedDataName();

  class  QualifiedDataNameFormat1Context : public antlr4::ParserRuleContext {
  public:
    QualifiedDataNameFormat1Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    DataNameContext *dataName();
    ConditionNameContext *conditionName();
    InFileContext *inFile();
    std::vector<QualifiedInDataContext *> qualifiedInData();
    QualifiedInDataContext* qualifiedInData(size_t i);

   
  };

  QualifiedDataNameFormat1Context* qualifiedDataNameFormat1();

  class  QualifiedDataNameFormat2Context : public antlr4::ParserRuleContext {
  public:
    QualifiedDataNameFormat2Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ParagraphNameContext *paragraphName();
    InSectionContext *inSection();

   
  };

  QualifiedDataNameFormat2Context* qualifiedDataNameFormat2();

  class  QualifiedDataNameFormat3Context : public antlr4::ParserRuleContext {
  public:
    QualifiedDataNameFormat3Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TextNameContext *textName();
    InLibraryContext *inLibrary();

   
  };

  QualifiedDataNameFormat3Context* qualifiedDataNameFormat3();

  class  QualifiedDataNameFormat4Context : public antlr4::ParserRuleContext {
  public:
    QualifiedDataNameFormat4Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LINAGE_COUNTER();
    InFileContext *inFile();

   
  };

  QualifiedDataNameFormat4Context* qualifiedDataNameFormat4();

  class  QualifiedInDataContext : public antlr4::ParserRuleContext {
  public:
    QualifiedInDataContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    InDataContext *inData();
    InTableContext *inTable();

   
  };

  QualifiedInDataContext* qualifiedInData();

  class  InDataContext : public antlr4::ParserRuleContext {
  public:
    InDataContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    DataNameContext *dataName();
    antlr4::tree::TerminalNode *IN();
    antlr4::tree::TerminalNode *OF();

   
  };

  InDataContext* inData();

  class  InFileContext : public antlr4::ParserRuleContext {
  public:
    InFileContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    FileNameContext *fileName();
    antlr4::tree::TerminalNode *IN();
    antlr4::tree::TerminalNode *OF();

   
  };

  InFileContext* inFile();

  class  InMnemonicContext : public antlr4::ParserRuleContext {
  public:
    InMnemonicContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    MnemonicNameContext *mnemonicName();
    antlr4::tree::TerminalNode *IN();
    antlr4::tree::TerminalNode *OF();

   
  };

  InMnemonicContext* inMnemonic();

  class  InSectionContext : public antlr4::ParserRuleContext {
  public:
    InSectionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    SectionNameContext *sectionName();
    antlr4::tree::TerminalNode *IN();
    antlr4::tree::TerminalNode *OF();

   
  };

  InSectionContext* inSection();

  class  InLibraryContext : public antlr4::ParserRuleContext {
  public:
    InLibraryContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    LibraryNameContext *libraryName();
    antlr4::tree::TerminalNode *IN();
    antlr4::tree::TerminalNode *OF();

   
  };

  InLibraryContext* inLibrary();

  class  InTableContext : public antlr4::ParserRuleContext {
  public:
    InTableContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TableCallContext *tableCall();
    antlr4::tree::TerminalNode *IN();
    antlr4::tree::TerminalNode *OF();

   
  };

  InTableContext* inTable();

  class  AlphabetNameContext : public antlr4::ParserRuleContext {
  public:
    AlphabetNameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    CobolWordContext *cobolWord();

   
  };

  AlphabetNameContext* alphabetName();

  class  AssignmentNameContext : public antlr4::ParserRuleContext {
  public:
    AssignmentNameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    SystemNameContext *systemName();

   
  };

  AssignmentNameContext* assignmentName();

  class  BasisNameContext : public antlr4::ParserRuleContext {
  public:
    BasisNameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ProgramNameContext *programName();

   
  };

  BasisNameContext* basisName();

  class  CdNameContext : public antlr4::ParserRuleContext {
  public:
    CdNameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    CobolWordContext *cobolWord();

   
  };

  CdNameContext* cdName();

  class  ClassNameContext : public antlr4::ParserRuleContext {
  public:
    ClassNameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    CobolWordContext *cobolWord();

   
  };

  ClassNameContext* className();

  class  ComputerNameContext : public antlr4::ParserRuleContext {
  public:
    ComputerNameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    SystemNameContext *systemName();

   
  };

  ComputerNameContext* computerName();

  class  ConditionNameContext : public antlr4::ParserRuleContext {
  public:
    ConditionNameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    CobolWordContext *cobolWord();

   
  };

  ConditionNameContext* conditionName();

  class  DataNameContext : public antlr4::ParserRuleContext {
  public:
    DataNameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    CobolWordContext *cobolWord();

   
  };

  DataNameContext* dataName();

  class  DataDescNameContext : public antlr4::ParserRuleContext {
  public:
    DataDescNameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FILLER();
    antlr4::tree::TerminalNode *CURSOR();
    DataNameContext *dataName();

   
  };

  DataDescNameContext* dataDescName();

  class  EnvironmentNameContext : public antlr4::ParserRuleContext {
  public:
    EnvironmentNameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    SystemNameContext *systemName();

   
  };

  EnvironmentNameContext* environmentName();

  class  FileNameContext : public antlr4::ParserRuleContext {
  public:
    FileNameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    CobolWordContext *cobolWord();

   
  };

  FileNameContext* fileName();

  class  FunctionNameContext : public antlr4::ParserRuleContext {
  public:
    FunctionNameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INTEGER();
    antlr4::tree::TerminalNode *LENGTH();
    antlr4::tree::TerminalNode *RANDOM();
    antlr4::tree::TerminalNode *SUM();
    antlr4::tree::TerminalNode *WHEN_COMPILED();
    CobolWordContext *cobolWord();

   
  };

  FunctionNameContext* functionName();

  class  IndexNameContext : public antlr4::ParserRuleContext {
  public:
    IndexNameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    CobolWordContext *cobolWord();

   
  };

  IndexNameContext* indexName();

  class  LanguageNameContext : public antlr4::ParserRuleContext {
  public:
    LanguageNameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    SystemNameContext *systemName();

   
  };

  LanguageNameContext* languageName();

  class  LibraryNameContext : public antlr4::ParserRuleContext {
  public:
    LibraryNameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    CobolWordContext *cobolWord();

   
  };

  LibraryNameContext* libraryName();

  class  LocalNameContext : public antlr4::ParserRuleContext {
  public:
    LocalNameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    CobolWordContext *cobolWord();

   
  };

  LocalNameContext* localName();

  class  MnemonicNameContext : public antlr4::ParserRuleContext {
  public:
    MnemonicNameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    CobolWordContext *cobolWord();

   
  };

  MnemonicNameContext* mnemonicName();

  class  ParagraphNameContext : public antlr4::ParserRuleContext {
  public:
    ParagraphNameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    CobolWordContext *cobolWord();
    IntegerLiteralContext *integerLiteral();

   
  };

  ParagraphNameContext* paragraphName();

  class  ProcedureNameContext : public antlr4::ParserRuleContext {
  public:
    ProcedureNameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ParagraphNameContext *paragraphName();
    InSectionContext *inSection();
    SectionNameContext *sectionName();

   
  };

  ProcedureNameContext* procedureName();

  class  ProgramNameContext : public antlr4::ParserRuleContext {
  public:
    ProgramNameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NONNUMERICLITERAL();
    CobolWordContext *cobolWord();

   
  };

  ProgramNameContext* programName();

  class  RecordNameContext : public antlr4::ParserRuleContext {
  public:
    RecordNameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    QualifiedDataNameContext *qualifiedDataName();

   
  };

  RecordNameContext* recordName();

  class  ReportNameContext : public antlr4::ParserRuleContext {
  public:
    ReportNameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    QualifiedDataNameContext *qualifiedDataName();

   
  };

  ReportNameContext* reportName();

  class  RoutineNameContext : public antlr4::ParserRuleContext {
  public:
    RoutineNameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    CobolWordContext *cobolWord();

   
  };

  RoutineNameContext* routineName();

  class  ScreenNameContext : public antlr4::ParserRuleContext {
  public:
    ScreenNameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    CobolWordContext *cobolWord();

   
  };

  ScreenNameContext* screenName();

  class  SectionNameContext : public antlr4::ParserRuleContext {
  public:
    SectionNameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    CobolWordContext *cobolWord();
    IntegerLiteralContext *integerLiteral();

   
  };

  SectionNameContext* sectionName();

  class  SystemNameContext : public antlr4::ParserRuleContext {
  public:
    SystemNameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    CobolWordContext *cobolWord();

   
  };

  SystemNameContext* systemName();

  class  SymbolicCharacterContext : public antlr4::ParserRuleContext {
  public:
    SymbolicCharacterContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    CobolWordContext *cobolWord();

   
  };

  SymbolicCharacterContext* symbolicCharacter();

  class  TextNameContext : public antlr4::ParserRuleContext {
  public:
    TextNameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    CobolWordContext *cobolWord();

   
  };

  TextNameContext* textName();

  class  CobolWordContext : public antlr4::ParserRuleContext {
  public:
    CobolWordContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *IDENTIFIER();
    antlr4::tree::TerminalNode *COBOL();
    antlr4::tree::TerminalNode *PROGRAM();
    antlr4::tree::TerminalNode *ABORT();
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *ASCII();
    antlr4::tree::TerminalNode *ASSOCIATED_DATA();
    antlr4::tree::TerminalNode *ASSOCIATED_DATA_LENGTH();
    antlr4::tree::TerminalNode *ATTRIBUTE();
    antlr4::tree::TerminalNode *AUTO();
    antlr4::tree::TerminalNode *AUTO_SKIP();
    antlr4::tree::TerminalNode *BACKGROUND_COLOR();
    antlr4::tree::TerminalNode *BACKGROUND_COLOUR();
    antlr4::tree::TerminalNode *BEEP();
    antlr4::tree::TerminalNode *BELL();
    antlr4::tree::TerminalNode *BINARY();
    antlr4::tree::TerminalNode *BIT();
    antlr4::tree::TerminalNode *BLINK();
    antlr4::tree::TerminalNode *BOUNDS();
    antlr4::tree::TerminalNode *CAPABLE();
    antlr4::tree::TerminalNode *CCSVERSION();
    antlr4::tree::TerminalNode *CHANGED();
    antlr4::tree::TerminalNode *CHANNEL();
    antlr4::tree::TerminalNode *CLOSE_DISPOSITION();
    antlr4::tree::TerminalNode *COMMITMENT();
    antlr4::tree::TerminalNode *CONTROL_POINT();
    antlr4::tree::TerminalNode *CONVENTION();
    antlr4::tree::TerminalNode *CRUNCH();
    antlr4::tree::TerminalNode *CURSOR();
    antlr4::tree::TerminalNode *DEFAULT();
    antlr4::tree::TerminalNode *DEFAULT_DISPLAY();
    antlr4::tree::TerminalNode *DEFINITION();
    antlr4::tree::TerminalNode *DFHRESP();
    antlr4::tree::TerminalNode *DFHVALUE();
    antlr4::tree::TerminalNode *DISK();
    antlr4::tree::TerminalNode *DONTCARE();
    antlr4::tree::TerminalNode *DOUBLE();
    antlr4::tree::TerminalNode *EBCDIC();
    antlr4::tree::TerminalNode *EMPTY_CHECK();
    antlr4::tree::TerminalNode *ENTER();
    antlr4::tree::TerminalNode *ENTRY_PROCEDURE();
    antlr4::tree::TerminalNode *EOL();
    antlr4::tree::TerminalNode *EOS();
    antlr4::tree::TerminalNode *ERASE();
    antlr4::tree::TerminalNode *ESCAPE();
    antlr4::tree::TerminalNode *EVENT();
    antlr4::tree::TerminalNode *EXCLUSIVE();
    antlr4::tree::TerminalNode *EXPORT();
    antlr4::tree::TerminalNode *EXTENDED();
    antlr4::tree::TerminalNode *FOREGROUND_COLOR();
    antlr4::tree::TerminalNode *FOREGROUND_COLOUR();
    antlr4::tree::TerminalNode *FULL();
    antlr4::tree::TerminalNode *FUNCTIONNAME();
    antlr4::tree::TerminalNode *FUNCTION_POINTER();
    antlr4::tree::TerminalNode *GRID();
    antlr4::tree::TerminalNode *HIGHLIGHT();
    antlr4::tree::TerminalNode *IMPLICIT();
    antlr4::tree::TerminalNode *IMPORT();
    antlr4::tree::TerminalNode *INTEGER();
    antlr4::tree::TerminalNode *KEPT();
    antlr4::tree::TerminalNode *KEYBOARD();
    antlr4::tree::TerminalNode *LANGUAGE();
    antlr4::tree::TerminalNode *LB();
    antlr4::tree::TerminalNode *LD();
    antlr4::tree::TerminalNode *LEFTLINE();
    antlr4::tree::TerminalNode *LENGTH_CHECK();
    antlr4::tree::TerminalNode *LIBACCESS();
    antlr4::tree::TerminalNode *LIBPARAMETER();
    antlr4::tree::TerminalNode *LIBRARY();
    antlr4::tree::TerminalNode *LIST();
    antlr4::tree::TerminalNode *LOCAL();
    antlr4::tree::TerminalNode *LONG_DATE();
    antlr4::tree::TerminalNode *LONG_TIME();
    antlr4::tree::TerminalNode *LOWER();
    antlr4::tree::TerminalNode *LOWLIGHT();
    antlr4::tree::TerminalNode *MMDDYYYY();
    antlr4::tree::TerminalNode *NAMED();
    antlr4::tree::TerminalNode *NATIONAL();
    antlr4::tree::TerminalNode *NATIONAL_EDITED();
    antlr4::tree::TerminalNode *NETWORK();
    antlr4::tree::TerminalNode *NO_ECHO();
    antlr4::tree::TerminalNode *NUMERIC_DATE();
    antlr4::tree::TerminalNode *NUMERIC_TIME();
    antlr4::tree::TerminalNode *ODT();
    antlr4::tree::TerminalNode *ORDERLY();
    antlr4::tree::TerminalNode *OVERLINE();
    antlr4::tree::TerminalNode *OWN();
    antlr4::tree::TerminalNode *PASSWORD();
    antlr4::tree::TerminalNode *PORT();
    antlr4::tree::TerminalNode *PRINTER();
    antlr4::tree::TerminalNode *PRIVATE();
    antlr4::tree::TerminalNode *PROCESS();
    antlr4::tree::TerminalNode *PROMPT();
    antlr4::tree::TerminalNode *READER();
    antlr4::tree::TerminalNode *REAL();
    antlr4::tree::TerminalNode *RECEIVED();
    antlr4::tree::TerminalNode *RECURSIVE();
    antlr4::tree::TerminalNode *REF();
    antlr4::tree::TerminalNode *REMOTE();
    antlr4::tree::TerminalNode *REMOVE();
    antlr4::tree::TerminalNode *REQUIRED();
    antlr4::tree::TerminalNode *REVERSE_VIDEO();
    antlr4::tree::TerminalNode *SAVE();
    antlr4::tree::TerminalNode *SECURE();
    antlr4::tree::TerminalNode *SHARED();
    antlr4::tree::TerminalNode *SHAREDBYALL();
    antlr4::tree::TerminalNode *SHAREDBYRUNUNIT();
    antlr4::tree::TerminalNode *SHARING();
    antlr4::tree::TerminalNode *SHORT_DATE();
    antlr4::tree::TerminalNode *SYMBOL();
    antlr4::tree::TerminalNode *TASK();
    antlr4::tree::TerminalNode *THREAD();
    antlr4::tree::TerminalNode *THREAD_LOCAL();
    antlr4::tree::TerminalNode *TIMER();
    antlr4::tree::TerminalNode *TODAYS_DATE();
    antlr4::tree::TerminalNode *TODAYS_NAME();
    antlr4::tree::TerminalNode *TRUNCATED();
    antlr4::tree::TerminalNode *TYPEDEF();
    antlr4::tree::TerminalNode *UNDERLINE();
    antlr4::tree::TerminalNode *VIRTUAL();
    antlr4::tree::TerminalNode *WAIT();
    antlr4::tree::TerminalNode *YEAR();
    antlr4::tree::TerminalNode *YYYYMMDD();
    antlr4::tree::TerminalNode *YYYYDDD();
    antlr4::tree::TerminalNode *ZERO_FILL();

   
  };

  CobolWordContext* cobolWord();

  class  LiteralContext : public antlr4::ParserRuleContext {
  public:
    LiteralContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NONNUMERICLITERAL();
    FigurativeConstantContext *figurativeConstant();
    NumericLiteralContext *numericLiteral();
    BooleanLiteralContext *booleanLiteral();
    CicsDfhRespLiteralContext *cicsDfhRespLiteral();
    CicsDfhValueLiteralContext *cicsDfhValueLiteral();

   
  };

  LiteralContext* literal();

  class  BooleanLiteralContext : public antlr4::ParserRuleContext {
  public:
    BooleanLiteralContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TRUE();
    antlr4::tree::TerminalNode *FALSE();

   
  };

  BooleanLiteralContext* booleanLiteral();

  class  NumericLiteralContext : public antlr4::ParserRuleContext {
  public:
    NumericLiteralContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NUMERICLITERAL();
    antlr4::tree::TerminalNode *ZERO();
    IntegerLiteralContext *integerLiteral();

   
  };

  NumericLiteralContext* numericLiteral();

  class  IntegerLiteralContext : public antlr4::ParserRuleContext {
  public:
    IntegerLiteralContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INTEGERLITERAL();
    antlr4::tree::TerminalNode *LEVEL_NUMBER_66();
    antlr4::tree::TerminalNode *LEVEL_NUMBER_77();
    antlr4::tree::TerminalNode *LEVEL_NUMBER_88();

   
  };

  IntegerLiteralContext* integerLiteral();

  class  CicsDfhRespLiteralContext : public antlr4::ParserRuleContext {
  public:
    CicsDfhRespLiteralContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DFHRESP();
    antlr4::tree::TerminalNode *LPARENCHAR();
    antlr4::tree::TerminalNode *RPARENCHAR();
    CobolWordContext *cobolWord();
    LiteralContext *literal();

   
  };

  CicsDfhRespLiteralContext* cicsDfhRespLiteral();

  class  CicsDfhValueLiteralContext : public antlr4::ParserRuleContext {
  public:
    CicsDfhValueLiteralContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DFHVALUE();
    antlr4::tree::TerminalNode *LPARENCHAR();
    antlr4::tree::TerminalNode *RPARENCHAR();
    CobolWordContext *cobolWord();
    LiteralContext *literal();

   
  };

  CicsDfhValueLiteralContext* cicsDfhValueLiteral();

  class  FigurativeConstantContext : public antlr4::ParserRuleContext {
  public:
    FigurativeConstantContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALL();
    LiteralContext *literal();
    antlr4::tree::TerminalNode *HIGH_VALUE();
    antlr4::tree::TerminalNode *HIGH_VALUES();
    antlr4::tree::TerminalNode *LOW_VALUE();
    antlr4::tree::TerminalNode *LOW_VALUES();
    antlr4::tree::TerminalNode *NULL_();
    antlr4::tree::TerminalNode *NULLS();
    antlr4::tree::TerminalNode *QUOTE();
    antlr4::tree::TerminalNode *QUOTES();
    antlr4::tree::TerminalNode *SPACE();
    antlr4::tree::TerminalNode *SPACES();
    antlr4::tree::TerminalNode *ZERO();
    antlr4::tree::TerminalNode *ZEROS();
    antlr4::tree::TerminalNode *ZEROES();

   
  };

  FigurativeConstantContext* figurativeConstant();

  class  SpecialRegisterContext : public antlr4::ParserRuleContext {
  public:
    SpecialRegisterContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ADDRESS();
    antlr4::tree::TerminalNode *OF();
    IdentifierContext *identifier();
    antlr4::tree::TerminalNode *DATE();
    antlr4::tree::TerminalNode *DAY();
    antlr4::tree::TerminalNode *DAY_OF_WEEK();
    antlr4::tree::TerminalNode *DEBUG_CONTENTS();
    antlr4::tree::TerminalNode *DEBUG_ITEM();
    antlr4::tree::TerminalNode *DEBUG_LINE();
    antlr4::tree::TerminalNode *DEBUG_NAME();
    antlr4::tree::TerminalNode *DEBUG_SUB_1();
    antlr4::tree::TerminalNode *DEBUG_SUB_2();
    antlr4::tree::TerminalNode *DEBUG_SUB_3();
    antlr4::tree::TerminalNode *LENGTH();
    antlr4::tree::TerminalNode *LINAGE_COUNTER();
    antlr4::tree::TerminalNode *LINE_COUNTER();
    antlr4::tree::TerminalNode *PAGE_COUNTER();
    antlr4::tree::TerminalNode *RETURN_CODE();
    antlr4::tree::TerminalNode *SHIFT_IN();
    antlr4::tree::TerminalNode *SHIFT_OUT();
    antlr4::tree::TerminalNode *SORT_CONTROL();
    antlr4::tree::TerminalNode *SORT_CORE_SIZE();
    antlr4::tree::TerminalNode *SORT_FILE_SIZE();
    antlr4::tree::TerminalNode *SORT_MESSAGE();
    antlr4::tree::TerminalNode *SORT_MODE_SIZE();
    antlr4::tree::TerminalNode *SORT_RETURN();
    antlr4::tree::TerminalNode *TALLY();
    antlr4::tree::TerminalNode *TIME();
    antlr4::tree::TerminalNode *WHEN_COMPILED();

   
  };

  SpecialRegisterContext* specialRegister();

  class  CommentEntryContext : public antlr4::ParserRuleContext {
  public:
    CommentEntryContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<antlr4::tree::TerminalNode *> COMMENTENTRYLINE();
    antlr4::tree::TerminalNode* COMMENTENTRYLINE(size_t i);

   
  };

  CommentEntryContext* commentEntry();


  bool sempred(antlr4::RuleContext *_localctx, size_t ruleIndex, size_t predicateIndex) override;

  bool copyDirectiveLiteralSempred(CopyDirectiveLiteralContext *_localctx, size_t predicateIndex);
  bool copyDirectiveDigitsSempred(CopyDirectiveDigitsContext *_localctx, size_t predicateIndex);
  bool copyDirectiveFilenameSempred(CopyDirectiveFilenameContext *_localctx, size_t predicateIndex);
  bool copyDirectivePseudoTextSempred(CopyDirectivePseudoTextContext *_localctx, size_t predicateIndex);
  bool copyDirectivePseudoDelimiterSempred(CopyDirectivePseudoDelimiterContext *_localctx, size_t predicateIndex);

  // By default the static state used to implement the parser is lazily initialized during the first
  // call to the constructor. You can call this function if you wish to initialize the static state
  // ahead of time.
  static void initialize();

private:
};

}  // namespace graphit::antlr::cobol85
