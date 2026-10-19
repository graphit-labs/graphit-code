#include "PostgreSQLParserBase.h"

// Generated from PostgreSQLParser.g4 by ANTLR 4.13.2

#pragma once


#include "antlr4-runtime.h"


namespace graphit::antlr::postgresql {


class  PostgreSQLParser : public PostgreSQLParserBase {
public:
  enum {
    Dollar = 1, OPEN_PAREN = 2, CLOSE_PAREN = 3, OPEN_BRACKET = 4, CLOSE_BRACKET = 5, 
    COMMA = 6, SEMI = 7, COLON = 8, STAR = 9, EQUAL = 10, DOT = 11, PLUS = 12, 
    MINUS = 13, SLASH = 14, CARET = 15, LT = 16, GT = 17, LESS_LESS = 18, 
    GREATER_GREATER = 19, COLON_EQUALS = 20, LESS_EQUALS = 21, EQUALS_GREATER = 22, 
    GREATER_EQUALS = 23, DOT_DOT = 24, NOT_EQUALS = 25, TYPECAST = 26, PERCENT = 27, 
    PARAM = 28, Operator = 29, JSON = 30, JSON_ARRAY = 31, JSON_ARRAYAGG = 32, 
    JSON_EXISTS = 33, JSON_OBJECT = 34, JSON_OBJECTAGG = 35, JSON_QUERY = 36, 
    JSON_SCALAR = 37, JSON_SERIALIZE = 38, JSON_TABLE = 39, JSON_VALUE = 40, 
    MERGE_ACTION = 41, SYSTEM_USER = 42, ABSENT = 43, ASENSITIVE = 44, ATOMIC = 45, 
    BREADTH = 46, COMPRESSION = 47, CONDITIONAL = 48, DEPTH = 49, EMPTY_P = 50, 
    FINALIZE = 51, INDENT = 52, KEEP = 53, KEYS = 54, NESTED = 55, OMIT = 56, 
    PARAMETER = 57, PATH = 58, PLAN = 59, QUOTES = 60, SCALAR = 61, SOURCE = 62, 
    STRING_P = 63, TARGET = 64, UNCONDITIONAL = 65, PERIOD = 66, FORMAT_LA = 67, 
    ALL = 68, ANALYSE = 69, ANALYZE = 70, AND = 71, ANY = 72, ARRAY = 73, 
    AS = 74, ASC = 75, ASYMMETRIC = 76, BOTH = 77, CASE = 78, CAST = 79, 
    CHECK = 80, COLLATE = 81, COLUMN = 82, CONSTRAINT = 83, CREATE = 84, 
    CURRENT_CATALOG = 85, CURRENT_DATE = 86, CURRENT_ROLE = 87, CURRENT_TIME = 88, 
    CURRENT_TIMESTAMP = 89, CURRENT_USER = 90, DEFAULT = 91, DEFERRABLE = 92, 
    DESC = 93, DISTINCT = 94, DO = 95, ELSE = 96, EXCEPT = 97, FALSE_P = 98, 
    FETCH = 99, FOR = 100, FOREIGN = 101, FROM = 102, GRANT = 103, GROUP_P = 104, 
    HAVING = 105, IN_P = 106, INITIALLY = 107, INTERSECT = 108, INTO = 109, 
    LATERAL_P = 110, LEADING = 111, LIMIT = 112, LOCALTIME = 113, LOCALTIMESTAMP = 114, 
    NOT = 115, NULL_P = 116, OFFSET = 117, ON = 118, ONLY = 119, OR = 120, 
    ORDER = 121, PLACING = 122, PRIMARY = 123, REFERENCES = 124, RETURNING = 125, 
    SELECT = 126, SESSION_USER = 127, SOME = 128, SYMMETRIC = 129, TABLE = 130, 
    THEN = 131, TO = 132, TRAILING = 133, TRUE_P = 134, UNION = 135, UNIQUE = 136, 
    USER = 137, USING = 138, VARIADIC = 139, WHEN = 140, WHERE = 141, WINDOW = 142, 
    WITH = 143, AUTHORIZATION = 144, BINARY = 145, COLLATION = 146, CONCURRENTLY = 147, 
    CROSS = 148, CURRENT_SCHEMA = 149, FREEZE = 150, FULL = 151, ILIKE = 152, 
    INNER_P = 153, IS = 154, ISNULL = 155, JOIN = 156, LEFT = 157, LIKE = 158, 
    NATURAL = 159, NOTNULL = 160, OUTER_P = 161, OVER = 162, OVERLAPS = 163, 
    RIGHT = 164, SIMILAR = 165, VERBOSE = 166, ABORT_P = 167, ABSOLUTE_P = 168, 
    ACCESS = 169, ACTION = 170, ADD_P = 171, ADMIN = 172, AFTER = 173, AGGREGATE = 174, 
    ALSO = 175, ALTER = 176, ALWAYS = 177, ASSERTION = 178, ASSIGNMENT = 179, 
    AT = 180, ATTRIBUTE = 181, BACKWARD = 182, BEFORE = 183, BEGIN_P = 184, 
    BY = 185, CACHE = 186, CALLED = 187, CASCADE = 188, CASCADED = 189, 
    CATALOG = 190, CHAIN = 191, CHARACTERISTICS = 192, CHECKPOINT = 193, 
    CLASS = 194, CLOSE = 195, CLUSTER = 196, COMMENT = 197, COMMENTS = 198, 
    COMMIT = 199, COMMITTED = 200, CONFIGURATION = 201, CONNECTION = 202, 
    CONSTRAINTS = 203, CONTENT_P = 204, CONTINUE_P = 205, CONVERSION_P = 206, 
    COPY = 207, COST = 208, CSV = 209, CURSOR = 210, CYCLE = 211, DATA_P = 212, 
    DATABASE = 213, DAY_P = 214, DEALLOCATE = 215, DECLARE = 216, DEFAULTS = 217, 
    DEFERRED = 218, DEFINER = 219, DELETE_P = 220, DELIMITER = 221, DELIMITERS = 222, 
    DICTIONARY = 223, DISABLE_P = 224, DISCARD = 225, DOCUMENT_P = 226, 
    DOMAIN_P = 227, DOUBLE_P = 228, DROP = 229, EACH = 230, ENABLE_P = 231, 
    ENCODING = 232, ENCRYPTED = 233, ENUM_P = 234, ESCAPE = 235, EVENT = 236, 
    EXCLUDE = 237, EXCLUDING = 238, EXCLUSIVE = 239, EXECUTE = 240, EXPLAIN = 241, 
    EXTENSION = 242, EXTERNAL = 243, FAMILY = 244, FIRST_P = 245, FOLLOWING = 246, 
    FORCE = 247, FORWARD = 248, FUNCTION = 249, FUNCTIONS = 250, GLOBAL = 251, 
    GRANTED = 252, HANDLER = 253, HEADER_P = 254, HOLD = 255, HOUR_P = 256, 
    IDENTITY_P = 257, IF_P = 258, IMMEDIATE = 259, IMMUTABLE = 260, IMPLICIT_P = 261, 
    INCLUDING = 262, INCREMENT = 263, INDEX = 264, INDEXES = 265, INHERIT = 266, 
    INHERITS = 267, INLINE_P = 268, INSENSITIVE = 269, INSERT = 270, INSTEAD = 271, 
    INVOKER = 272, ISOLATION = 273, KEY = 274, LABEL = 275, LANGUAGE = 276, 
    LARGE_P = 277, LAST_P = 278, LEAKPROOF = 279, LEVEL = 280, LISTEN = 281, 
    LOAD = 282, LOCAL = 283, LOCATION = 284, LOCK_P = 285, MAPPING = 286, 
    MATCH = 287, MATCHED = 288, MATERIALIZED = 289, MAXVALUE = 290, MERGE = 291, 
    MINUTE_P = 292, MINVALUE = 293, MODE = 294, MONTH_P = 295, MOVE = 296, 
    NAME_P = 297, NAMES = 298, NEXT = 299, NO = 300, NOTHING = 301, NOTIFY = 302, 
    NOWAIT = 303, NULLS_P = 304, OBJECT_P = 305, OF = 306, OFF = 307, OIDS = 308, 
    OPERATOR = 309, OPTION = 310, OPTIONS = 311, OWNED = 312, OWNER = 313, 
    PARSER = 314, PARTIAL = 315, PARTITION = 316, PASSING = 317, PASSWORD = 318, 
    PLANS = 319, PRECEDING = 320, PREPARE = 321, PREPARED = 322, PRESERVE = 323, 
    PRIOR = 324, PRIVILEGES = 325, PROCEDURAL = 326, PROCEDURE = 327, PROGRAM = 328, 
    QUOTE = 329, RANGE = 330, READ = 331, REASSIGN = 332, RECHECK = 333, 
    RECURSIVE = 334, REF = 335, REFRESH = 336, REINDEX = 337, RELATIVE_P = 338, 
    RELEASE = 339, RENAME = 340, REPEATABLE = 341, REPLACE = 342, REPLICA = 343, 
    RESET = 344, RESTART = 345, RESTRICT = 346, RETURNS = 347, REVOKE = 348, 
    ROLE = 349, ROLLBACK = 350, ROWS = 351, RULE = 352, SAVEPOINT = 353, 
    SCHEMA = 354, SCROLL = 355, SEARCH = 356, SECOND_P = 357, SECURITY = 358, 
    SEQUENCE = 359, SEQUENCES = 360, SERIALIZABLE = 361, SERVER = 362, SESSION = 363, 
    SET = 364, SHARE = 365, SHOW = 366, SIMPLE = 367, SNAPSHOT = 368, STABLE = 369, 
    STANDALONE_P = 370, START = 371, STATEMENT = 372, STATISTICS = 373, 
    STDIN = 374, STDOUT = 375, STORAGE = 376, STRICT_P = 377, STRIP_P = 378, 
    SYSID = 379, SYSTEM_P = 380, TABLES = 381, TABLESPACE = 382, TEMP = 383, 
    TEMPLATE = 384, TEMPORARY = 385, TEXT_P = 386, TRANSACTION = 387, TRIGGER = 388, 
    TRUNCATE = 389, TRUSTED = 390, TYPE_P = 391, TYPES_P = 392, UNBOUNDED = 393, 
    UNCOMMITTED = 394, UNENCRYPTED = 395, UNKNOWN = 396, UNLISTEN = 397, 
    UNLOGGED = 398, UNTIL = 399, UPDATE = 400, VACUUM = 401, VALID = 402, 
    VALIDATE = 403, VALIDATOR = 404, VARYING = 405, VERSION_P = 406, VIEW = 407, 
    VOLATILE = 408, WHITESPACE_P = 409, WITHOUT = 410, WORK = 411, WRAPPER = 412, 
    WRITE = 413, XML_P = 414, YEAR_P = 415, YES_P = 416, ZONE = 417, BETWEEN = 418, 
    BIGINT = 419, BIT = 420, BOOLEAN_P = 421, CHAR_P = 422, CHARACTER = 423, 
    COALESCE = 424, DEC = 425, DECIMAL_P = 426, EXISTS = 427, EXTRACT = 428, 
    FLOAT_P = 429, GREATEST = 430, INOUT = 431, INT_P = 432, INTEGER = 433, 
    INTERVAL = 434, LEAST = 435, NATIONAL = 436, NCHAR = 437, NONE = 438, 
    NULLIF = 439, NUMERIC = 440, OVERLAY = 441, POSITION = 442, PRECISION = 443, 
    REAL = 444, ROW = 445, SETOF = 446, SMALLINT = 447, SUBSTRING = 448, 
    TIME = 449, TIMESTAMP = 450, TREAT = 451, TRIM = 452, VALUES = 453, 
    VARCHAR = 454, XMLATTRIBUTES = 455, XMLCOMMENT = 456, XMLAGG = 457, 
    XML_IS_WELL_FORMED = 458, XML_IS_WELL_FORMED_DOCUMENT = 459, XML_IS_WELL_FORMED_CONTENT = 460, 
    XPATH = 461, XPATH_EXISTS = 462, XMLCONCAT = 463, XMLELEMENT = 464, 
    XMLEXISTS = 465, XMLFOREST = 466, XMLPARSE = 467, XMLPI = 468, XMLROOT = 469, 
    XMLSERIALIZE = 470, CALL = 471, CURRENT_P = 472, ATTACH = 473, DETACH = 474, 
    EXPRESSION = 475, GENERATED = 476, LOGGED = 477, STORED = 478, INCLUDE = 479, 
    ROUTINE = 480, TRANSFORM = 481, IMPORT_P = 482, POLICY = 483, METHOD = 484, 
    REFERENCING = 485, NEW = 486, OLD = 487, VALUE_P = 488, SUBSCRIPTION = 489, 
    PUBLICATION = 490, OUT_P = 491, END_P = 492, ROUTINES = 493, SCHEMAS = 494, 
    PROCEDURES = 495, INPUT_P = 496, SUPPORT = 497, PARALLEL = 498, SQL_P = 499, 
    DEPENDS = 500, OVERRIDING = 501, CONFLICT = 502, SKIP_P = 503, LOCKED = 504, 
    TIES = 505, ROLLUP = 506, CUBE = 507, GROUPING = 508, SETS = 509, TABLESAMPLE = 510, 
    ORDINALITY = 511, XMLTABLE = 512, COLUMNS = 513, XMLNAMESPACES = 514, 
    ROWTYPE = 515, NORMALIZED = 516, WITHIN = 517, FILTER = 518, GROUPS = 519, 
    OTHERS = 520, NFC = 521, NFD = 522, NFKC = 523, NFKD = 524, UESCAPE = 525, 
    VIEWS = 526, NORMALIZE = 527, DUMP = 528, ERROR = 529, USE_VARIABLE = 530, 
    USE_COLUMN = 531, CONSTANT = 532, PERFORM = 533, GET = 534, DIAGNOSTICS = 535, 
    STACKED = 536, ELSIF = 537, WHILE = 538, FOREACH = 539, SLICE = 540, 
    EXIT = 541, RETURN = 542, RAISE = 543, SQLSTATE = 544, DEBUG = 545, 
    INFO = 546, NOTICE = 547, WARNING = 548, EXCEPTION = 549, ASSERT = 550, 
    LOOP = 551, OPEN = 552, FORMAT = 553, Identifier = 554, QuotedIdentifier = 555, 
    UnterminatedQuotedIdentifier = 556, InvalidQuotedIdentifier = 557, InvalidUnterminatedQuotedIdentifier = 558, 
    UnicodeQuotedIdentifier = 559, UnterminatedUnicodeQuotedIdentifier = 560, 
    InvalidUnicodeQuotedIdentifier = 561, InvalidUnterminatedUnicodeQuotedIdentifier = 562, 
    StringConstant = 563, UnterminatedStringConstant = 564, UnicodeEscapeStringConstant = 565, 
    UnterminatedUnicodeEscapeStringConstant = 566, BeginDollarStringConstant = 567, 
    BinaryStringConstant = 568, UnterminatedBinaryStringConstant = 569, 
    InvalidBinaryStringConstant = 570, InvalidUnterminatedBinaryStringConstant = 571, 
    HexadecimalStringConstant = 572, UnterminatedHexadecimalStringConstant = 573, 
    InvalidHexadecimalStringConstant = 574, InvalidUnterminatedHexadecimalStringConstant = 575, 
    Integral = 576, BinaryIntegral = 577, OctalIntegral = 578, HexadecimalIntegral = 579, 
    NumericFail = 580, Numeric = 581, PLSQLVARIABLENAME = 582, PLSQLIDENTIFIER = 583, 
    Whitespace = 584, Newline = 585, LineComment = 586, BlockComment = 587, 
    UnterminatedBlockComment = 588, ErrorCharacter = 589, EscapeStringConstant = 590, 
    UnterminatedEscapeStringConstant = 591, InvalidEscapeStringConstant = 592, 
    InvalidUnterminatedEscapeStringConstant = 593, AfterEscapeStringConstantMode_NotContinued = 594, 
    AfterEscapeStringConstantWithNewlineMode_NotContinued = 595, DollarText = 596, 
    EndDollarStringConstant = 597, MetaCommand = 598, AfterEscapeStringConstantWithNewlineMode_Continued = 599
  };

  enum {
    RuleRoot = 0, RuleStmtblock = 1, RuleStmtmulti = 2, RuleStmt = 3, RuleCallstmt = 4, 
    RuleCreaterolestmt = 5, RuleWith_ = 6, RuleOptrolelist = 7, RuleAlteroptrolelist = 8, 
    RuleAlteroptroleelem = 9, RuleCreateoptroleelem = 10, RuleCreateuserstmt = 11, 
    RuleAlterrolestmt = 12, RuleIn_database_ = 13, RuleAlterrolesetstmt = 14, 
    RuleDroprolestmt = 15, RuleCreategroupstmt = 16, RuleAltergroupstmt = 17, 
    RuleAdd_drop = 18, RuleCreateschemastmt = 19, RuleOptschemaname = 20, 
    RuleOptschemaeltlist = 21, RuleSchema_stmt = 22, RuleVariablesetstmt = 23, 
    RuleSet_rest = 24, RuleGeneric_set = 25, RuleSet_rest_more = 26, RuleVar_name = 27, 
    RuleVar_list = 28, RuleVar_value = 29, RuleIso_level = 30, RuleBoolean_or_string_ = 31, 
    RuleZone_value = 32, RuleEncoding_ = 33, RuleNonreservedword_or_sconst = 34, 
    RuleVariableresetstmt = 35, RuleReset_rest = 36, RuleGeneric_reset = 37, 
    RuleSetresetclause = 38, RuleFunctionsetresetclause = 39, RuleVariableshowstmt = 40, 
    RuleConstraintssetstmt = 41, RuleConstraints_set_list = 42, RuleConstraints_set_mode = 43, 
    RuleCheckpointstmt = 44, RuleDiscardstmt = 45, RuleAltertablestmt = 46, 
    RuleAlter_table_cmds = 47, RulePartition_cmd = 48, RuleIndex_partition_cmd = 49, 
    RuleAlter_table_cmd = 50, RuleAlter_column_default = 51, RuleDrop_behavior_ = 52, 
    RuleCollate_clause_ = 53, RuleAlter_using = 54, RuleReplica_identity = 55, 
    RuleReloptions = 56, RuleReloptions_ = 57, RuleReloption_list = 58, 
    RuleReloption_elem = 59, RuleAlter_identity_column_option_list = 60, 
    RuleAlter_identity_column_option = 61, RulePartitionboundspec = 62, 
    RuleHash_partbound_elem = 63, RuleHash_partbound = 64, RuleAltercompositetypestmt = 65, 
    RuleAlter_type_cmds = 66, RuleAlter_type_cmd = 67, RuleCloseportalstmt = 68, 
    RuleCopystmt = 69, RuleCopy_from = 70, RuleProgram_ = 71, RuleCopy_file_name = 72, 
    RuleCopy_options = 73, RuleCopy_opt_list = 74, RuleCopy_opt_item = 75, 
    RuleBinary_ = 76, RuleCopy_delimiter = 77, RuleUsing_ = 78, RuleCopy_generic_opt_list = 79, 
    RuleCopy_generic_opt_elem = 80, RuleCopy_generic_opt_arg = 81, RuleCopy_generic_opt_arg_list = 82, 
    RuleCopy_generic_opt_arg_list_item = 83, RuleCreatestmt = 84, RuleOpttemp = 85, 
    RuleOpttableelementlist = 86, RuleOpttypedtableelementlist = 87, RuleTableelementlist = 88, 
    RuleTypedtableelementlist = 89, RuleTableelement = 90, RuleTypedtableelement = 91, 
    RuleColumnDef = 92, RuleColumnOptions = 93, RuleColquallist = 94, RuleColconstraint = 95, 
    RuleColconstraintelem = 96, RuleGenerated_when = 97, RuleConstraintattr = 98, 
    RuleTablelikeclause = 99, RuleTablelikeoptionlist = 100, RuleTablelikeoption = 101, 
    RuleTableconstraint = 102, RuleConstraintelem = 103, RuleNo_inherit_ = 104, 
    RuleColumn_list_ = 105, RuleColumnlist = 106, RuleColumnElem = 107, 
    RuleC_include_ = 108, RuleKey_match = 109, RuleExclusionconstraintlist = 110, 
    RuleExclusionconstraintelem = 111, RuleExclusionwhereclause = 112, RuleKey_actions = 113, 
    RuleKey_update = 114, RuleKey_delete = 115, RuleKey_action = 116, RuleOptinherit = 117, 
    RuleOptpartitionspec = 118, RulePartitionspec = 119, RulePart_params = 120, 
    RulePart_elem = 121, RuleTable_access_method_clause = 122, RuleOptwith = 123, 
    RuleOncommitoption = 124, RuleOpttablespace = 125, RuleOptconstablespace = 126, 
    RuleExistingindex = 127, RuleCreatestatsstmt = 128, RuleAlterstatsstmt = 129, 
    RuleCreateasstmt = 130, RuleCreate_as_target = 131, RuleWith_data_ = 132, 
    RuleCreatematviewstmt = 133, RuleCreate_mv_target = 134, RuleOptnolog = 135, 
    RuleRefreshmatviewstmt = 136, RuleCreateseqstmt = 137, RuleAlterseqstmt = 138, 
    RuleOptseqoptlist = 139, RuleOptparenthesizedseqoptlist = 140, RuleSeqoptlist = 141, 
    RuleSeqoptelem = 142, RuleBy_ = 143, RuleNumericonly = 144, RuleNumericonly_list = 145, 
    RuleCreateplangstmt = 146, RuleTrusted_ = 147, RuleHandler_name = 148, 
    RuleInline_handler_ = 149, RuleValidator_clause = 150, RuleValidator_ = 151, 
    RuleProcedural_ = 152, RuleCreatetablespacestmt = 153, RuleOpttablespaceowner = 154, 
    RuleDroptablespacestmt = 155, RuleCreateextensionstmt = 156, RuleCreate_extension_opt_list = 157, 
    RuleCreate_extension_opt_item = 158, RuleAlterextensionstmt = 159, RuleAlter_extension_opt_list = 160, 
    RuleAlter_extension_opt_item = 161, RuleAlterextensioncontentsstmt = 162, 
    RuleCreatefdwstmt = 163, RuleFdw_option = 164, RuleFdw_options = 165, 
    RuleFdw_options_ = 166, RuleAlterfdwstmt = 167, RuleCreate_generic_options = 168, 
    RuleGeneric_option_list = 169, RuleAlter_generic_options = 170, RuleAlter_generic_option_list = 171, 
    RuleAlter_generic_option_elem = 172, RuleGeneric_option_elem = 173, 
    RuleGeneric_option_name = 174, RuleGeneric_option_arg = 175, RuleCreateforeignserverstmt = 176, 
    RuleType_ = 177, RuleForeign_server_version = 178, RuleForeign_server_version_ = 179, 
    RuleAlterforeignserverstmt = 180, RuleCreateforeigntablestmt = 181, 
    RuleImportforeignschemastmt = 182, RuleImport_qualification_type = 183, 
    RuleImport_qualification = 184, RuleCreateusermappingstmt = 185, RuleAuth_ident = 186, 
    RuleDropusermappingstmt = 187, RuleAlterusermappingstmt = 188, RuleCreatepolicystmt = 189, 
    RuleAlterpolicystmt = 190, RuleRowsecurityoptionalexpr = 191, RuleRowsecurityoptionalwithcheck = 192, 
    RuleRowsecuritydefaulttorole = 193, RuleRowsecurityoptionaltorole = 194, 
    RuleRowsecuritydefaultpermissive = 195, RuleRowsecuritydefaultforcmd = 196, 
    RuleRow_security_cmd = 197, RuleCreateamstmt = 198, RuleAm_type = 199, 
    RuleCreatetrigstmt = 200, RuleTriggeractiontime = 201, RuleTriggerevents = 202, 
    RuleTriggeroneevent = 203, RuleTriggerreferencing = 204, RuleTriggertransitions = 205, 
    RuleTriggertransition = 206, RuleTransitionoldornew = 207, RuleTransitionrowortable = 208, 
    RuleTransitionrelname = 209, RuleTriggerforspec = 210, RuleTriggerforopteach = 211, 
    RuleTriggerfortype = 212, RuleTriggerwhen = 213, RuleFunction_or_procedure = 214, 
    RuleTriggerfuncargs = 215, RuleTriggerfuncarg = 216, RuleOptconstrfromtable = 217, 
    RuleConstraintattributespec = 218, RuleConstraintattributeElem = 219, 
    RuleCreateeventtrigstmt = 220, RuleEvent_trigger_when_list = 221, RuleEvent_trigger_when_item = 222, 
    RuleEvent_trigger_value_list = 223, RuleAltereventtrigstmt = 224, RuleEnable_trigger = 225, 
    RuleCreateassertionstmt = 226, RuleDefinestmt = 227, RuleDefinition = 228, 
    RuleDef_list = 229, RuleDef_elem = 230, RuleDef_arg = 231, RuleOld_aggr_definition = 232, 
    RuleOld_aggr_list = 233, RuleOld_aggr_elem = 234, RuleEnum_val_list_ = 235, 
    RuleEnum_val_list = 236, RuleAlterenumstmt = 237, RuleIf_not_exists_ = 238, 
    RuleCreateopclassstmt = 239, RuleOpclass_item_list = 240, RuleOpclass_item = 241, 
    RuleDefault_ = 242, RuleOpfamily_ = 243, RuleOpclass_purpose = 244, 
    RuleRecheck_ = 245, RuleCreateopfamilystmt = 246, RuleAlteropfamilystmt = 247, 
    RuleOpclass_drop_list = 248, RuleOpclass_drop = 249, RuleDropopclassstmt = 250, 
    RuleDropopfamilystmt = 251, RuleDropownedstmt = 252, RuleReassignownedstmt = 253, 
    RuleDropstmt = 254, RuleObject_type_any_name = 255, RuleObject_type_name = 256, 
    RuleDrop_type_name = 257, RuleObject_type_name_on_any_name = 258, RuleAny_name_list_ = 259, 
    RuleAny_name = 260, RuleAttrs = 261, RuleType_name_list = 262, RuleTruncatestmt = 263, 
    RuleRestart_seqs_ = 264, RuleCommentstmt = 265, RuleComment_text = 266, 
    RuleSeclabelstmt = 267, RuleProvider_ = 268, RuleSecurity_label = 269, 
    RuleFetchstmt = 270, RuleFetch_args = 271, RuleFrom_in = 272, RuleFrom_in_ = 273, 
    RuleGrantstmt = 274, RuleRevokestmt = 275, RulePrivileges = 276, RulePrivilege_list = 277, 
    RulePrivilege = 278, RulePrivilege_target = 279, RuleGrantee_list = 280, 
    RuleGrantee = 281, RuleGrant_grant_option_ = 282, RuleGrantrolestmt = 283, 
    RuleRevokerolestmt = 284, RuleGrant_admin_option_ = 285, RuleGranted_by_ = 286, 
    RuleAlterdefaultprivilegesstmt = 287, RuleDefacloptionlist = 288, RuleDefacloption = 289, 
    RuleDefaclaction = 290, RuleDefacl_privilege_target = 291, RuleIndexstmt = 292, 
    RuleUnique_ = 293, RuleNulls_distinct = 294, RuleSingle_name_ = 295, 
    RuleConcurrently_ = 296, RuleIndex_name_ = 297, RuleAccess_method_clause = 298, 
    RuleIndex_params = 299, RuleIndex_elem_options = 300, RuleIndex_elem = 301, 
    RuleInclude_ = 302, RuleIndex_including_params = 303, RuleCollate_ = 304, 
    RuleClass_ = 305, RuleAsc_desc_ = 306, RuleNulls_order_ = 307, RuleCreatefunctionstmt = 308, 
    RuleOr_replace_ = 309, RuleFunc_args = 310, RuleFunc_args_list = 311, 
    RuleFunction_with_argtypes_list = 312, RuleFunction_with_argtypes = 313, 
    RuleFunc_args_with_defaults = 314, RuleFunc_args_with_defaults_list = 315, 
    RuleFunc_arg = 316, RuleArg_class = 317, RuleParam_name = 318, RuleFunc_return = 319, 
    RuleFunc_type = 320, RuleFunc_arg_with_default = 321, RuleAggr_arg = 322, 
    RuleAggr_args = 323, RuleAggr_args_list = 324, RuleAggregate_with_argtypes = 325, 
    RuleAggregate_with_argtypes_list = 326, RuleCreatefunc_opt_list = 327, 
    RuleCommon_func_opt_item = 328, RuleCreatefunc_opt_item = 329, RuleFunc_as = 330, 
    RuleTransform_type_list = 331, RuleDefinition_ = 332, RuleTable_func_column = 333, 
    RuleTable_func_column_list = 334, RuleAlterfunctionstmt = 335, RuleAlterfunc_opt_list = 336, 
    RuleRestrict_ = 337, RuleRemovefuncstmt = 338, RuleRemoveaggrstmt = 339, 
    RuleRemoveoperstmt = 340, RuleOper_argtypes = 341, RuleAny_operator = 342, 
    RuleOperator_with_argtypes_list = 343, RuleOperator_with_argtypes = 344, 
    RuleDostmt = 345, RuleDostmt_opt_list = 346, RuleDostmt_opt_item = 347, 
    RuleCreatecaststmt = 348, RuleCast_context = 349, RuleDropcaststmt = 350, 
    RuleIf_exists_ = 351, RuleCreatetransformstmt = 352, RuleTransform_element_list = 353, 
    RuleDroptransformstmt = 354, RuleReindexstmt = 355, RuleReindex_target_relation = 356, 
    RuleReindex_target_all = 357, RuleReindex_option_list = 358, RuleAltertblspcstmt = 359, 
    RuleRenamestmt = 360, RuleColumn_ = 361, RuleSet_data_ = 362, RuleAlterobjectdependsstmt = 363, 
    RuleNo_ = 364, RuleAlterobjectschemastmt = 365, RuleAlteroperatorstmt = 366, 
    RuleOperator_def_list = 367, RuleOperator_def_elem = 368, RuleOperator_def_arg = 369, 
    RuleAltertypestmt = 370, RuleAlterownerstmt = 371, RuleCreatepublicationstmt = 372, 
    RulePublication_for_tables_ = 373, RulePublication_for_tables = 374, 
    RuleAlterpublicationstmt = 375, RuleCreatesubscriptionstmt = 376, RulePublication_name_list = 377, 
    RulePublication_name_item = 378, RuleAltersubscriptionstmt = 379, RuleDropsubscriptionstmt = 380, 
    RuleRulestmt = 381, RuleRuleactionlist = 382, RuleRuleactionmulti = 383, 
    RuleRuleactionstmt = 384, RuleRuleactionstmtOrEmpty = 385, RuleEvent = 386, 
    RuleInstead_ = 387, RuleNotifystmt = 388, RuleNotify_payload = 389, 
    RuleListenstmt = 390, RuleUnlistenstmt = 391, RuleTransactionstmt = 392, 
    RuleTransaction_ = 393, RuleTransaction_mode_item = 394, RuleTransaction_mode_list = 395, 
    RuleTransaction_mode_list_or_empty = 396, RuleTransaction_chain_ = 397, 
    RuleViewstmt = 398, RuleCheck_option_ = 399, RuleLoadstmt = 400, RuleCreatedbstmt = 401, 
    RuleCreatedb_opt_list = 402, RuleCreatedb_opt_items = 403, RuleCreatedb_opt_item = 404, 
    RuleCreatedb_opt_name = 405, RuleEqual_ = 406, RuleAlterdatabasestmt = 407, 
    RuleAlterdatabasesetstmt = 408, RuleDropdbstmt = 409, RuleDrop_option_list = 410, 
    RuleDrop_option = 411, RuleAltercollationstmt = 412, RuleAltersystemstmt = 413, 
    RuleCreatedomainstmt = 414, RuleAlterdomainstmt = 415, RuleAs_ = 416, 
    RuleAltertsdictionarystmt = 417, RuleAltertsconfigurationstmt = 418, 
    RuleAny_with = 419, RuleCreateconversionstmt = 420, RuleClusterstmt = 421, 
    RuleCluster_index_specification = 422, RuleVacuumstmt = 423, RuleAnalyzestmt = 424, 
    RuleUtility_option_list = 425, RuleVac_analyze_option_list = 426, RuleAnalyze_keyword = 427, 
    RuleUtility_option_elem = 428, RuleUtility_option_name = 429, RuleUtility_option_arg = 430, 
    RuleVac_analyze_option_elem = 431, RuleVac_analyze_option_name = 432, 
    RuleVac_analyze_option_arg = 433, RuleAnalyze_ = 434, RuleVerbose_ = 435, 
    RuleFull_ = 436, RuleFreeze_ = 437, RuleName_list_ = 438, RuleVacuum_relation = 439, 
    RuleVacuum_relation_list = 440, RuleVacuum_relation_list_ = 441, RuleExplainstmt = 442, 
    RuleExplainablestmt = 443, RuleExplain_option_list = 444, RuleExplain_option_elem = 445, 
    RuleExplain_option_name = 446, RuleExplain_option_arg = 447, RulePreparestmt = 448, 
    RulePrep_type_clause = 449, RulePreparablestmt = 450, RuleExecutestmt = 451, 
    RuleExecute_param_clause = 452, RuleDeallocatestmt = 453, RuleInsertstmt = 454, 
    RuleInsert_target = 455, RuleInsert_rest = 456, RuleOverride_kind = 457, 
    RuleInsert_column_list = 458, RuleInsert_column_item = 459, RuleOn_conflict_ = 460, 
    RuleConf_expr_ = 461, RuleReturning_clause = 462, RuleMergestmt = 463, 
    RuleMerge_insert_clause = 464, RuleMerge_update_clause = 465, RuleMerge_delete_clause = 466, 
    RuleDeletestmt = 467, RuleUsing_clause = 468, RuleLockstmt = 469, RuleLock_ = 470, 
    RuleLock_type = 471, RuleNowait_ = 472, RuleNowait_or_skip_ = 473, RuleUpdatestmt = 474, 
    RuleSet_clause_list = 475, RuleSet_clause = 476, RuleSet_target = 477, 
    RuleSet_target_list = 478, RuleDeclarecursorstmt = 479, RuleCursor_name = 480, 
    RuleCursor_options = 481, RuleHold_ = 482, RuleSelectstmt = 483, RuleSelect_with_parens = 484, 
    RuleSelect_no_parens = 485, RuleSelect_clause = 486, RuleSimple_select_intersect = 487, 
    RuleSimple_select_pramary = 488, RuleWith_clause = 489, RuleCte_list = 490, 
    RuleCommon_table_expr = 491, RuleMaterialized_ = 492, RuleWith_clause_ = 493, 
    RuleInto_clause = 494, RuleStrict_ = 495, RuleOpttempTableName = 496, 
    RuleTable_ = 497, RuleAll_or_distinct = 498, RuleDistinct_clause = 499, 
    RuleAll_clause_ = 500, RuleSort_clause_ = 501, RuleSort_clause = 502, 
    RuleSortby_list = 503, RuleSortby = 504, RuleSelect_limit = 505, RuleSelect_limit_ = 506, 
    RuleLimit_clause = 507, RuleOffset_clause = 508, RuleSelect_limit_value = 509, 
    RuleSelect_offset_value = 510, RuleSelect_fetch_first_value = 511, RuleI_or_f_const = 512, 
    RuleRow_or_rows = 513, RuleFirst_or_next = 514, RuleGroup_clause = 515, 
    RuleGroup_by_list = 516, RuleGroup_by_item = 517, RuleEmpty_grouping_set = 518, 
    RuleRollup_clause = 519, RuleCube_clause = 520, RuleGrouping_sets_clause = 521, 
    RuleHaving_clause = 522, RuleFor_locking_clause = 523, RuleFor_locking_clause_ = 524, 
    RuleFor_locking_items = 525, RuleFor_locking_item = 526, RuleFor_locking_strength = 527, 
    RuleLocked_rels_list = 528, RuleValues_clause = 529, RuleFrom_clause = 530, 
    RuleFrom_list = 531, RuleTable_ref = 532, RuleAlias_clause = 533, RuleFunc_alias_clause = 534, 
    RuleJoin_type = 535, RuleJoin_qual = 536, RuleRelation_expr = 537, RuleRelation_expr_list = 538, 
    RuleRelation_expr_opt_alias = 539, RuleTablesample_clause = 540, RuleRepeatable_clause_ = 541, 
    RuleFunc_table = 542, RuleRowsfrom_item = 543, RuleRowsfrom_list = 544, 
    RuleCol_def_list_ = 545, RuleOrdinality_ = 546, RuleWhere_clause = 547, 
    RuleWhere_or_current_clause = 548, RuleOpttablefuncelementlist = 549, 
    RuleTablefuncelementlist = 550, RuleTablefuncelement = 551, RuleXmltable = 552, 
    RuleXmltable_column_list = 553, RuleXmltable_column_el = 554, RuleXmltable_column_option_list = 555, 
    RuleXmltable_column_option_el = 556, RuleXml_namespace_list = 557, RuleXml_namespace_el = 558, 
    RuleTypename = 559, RuleOpt_array_bounds = 560, RuleSimpletypename = 561, 
    RuleConsttypename = 562, RuleGenerictype = 563, RuleType_modifiers_ = 564, 
    RuleNumeric = 565, RuleFloat_ = 566, RuleBit = 567, RuleConstbit = 568, 
    RuleBitwithlength = 569, RuleBitwithoutlength = 570, RuleCharacter = 571, 
    RuleConstcharacter = 572, RuleCharacter_c = 573, RuleVarying_ = 574, 
    RuleConstdatetime = 575, RuleConstinterval = 576, RuleTimezone_ = 577, 
    RuleInterval_ = 578, RuleInterval_second = 579, RuleJsonType = 580, 
    RuleEscape_ = 581, RuleA_expr = 582, RuleA_expr_qual = 583, RuleA_expr_lessless = 584, 
    RuleA_expr_or = 585, RuleA_expr_and = 586, RuleA_expr_between = 587, 
    RuleA_expr_in = 588, RuleA_expr_unary_not = 589, RuleA_expr_isnull = 590, 
    RuleA_expr_is_not = 591, RuleA_expr_compare = 592, RuleA_expr_like = 593, 
    RuleA_expr_qual_op = 594, RuleA_expr_unary_qualop = 595, RuleA_expr_add = 596, 
    RuleA_expr_mul = 597, RuleA_expr_caret = 598, RuleA_expr_unary_sign = 599, 
    RuleA_expr_at_time_zone = 600, RuleA_expr_collate = 601, RuleA_expr_typecast = 602, 
    RuleB_expr = 603, RuleC_expr = 604, RulePlsqlvariablename = 605, RuleFunc_application = 606, 
    RuleFunc_expr = 607, RuleFunc_expr_windowless = 608, RuleFunc_expr_common_subexpr = 609, 
    RuleXml_root_version = 610, RuleXml_root_standalone_ = 611, RuleXml_attributes = 612, 
    RuleXml_attribute_list = 613, RuleXml_attribute_el = 614, RuleDocument_or_content = 615, 
    RuleXml_whitespace_option = 616, RuleXmlexists_argument = 617, RuleXml_passing_mech = 618, 
    RuleWithin_group_clause = 619, RuleFilter_clause = 620, RuleWindow_clause = 621, 
    RuleWindow_definition_list = 622, RuleWindow_definition = 623, RuleOver_clause = 624, 
    RuleWindow_specification = 625, RuleExisting_window_name_ = 626, RulePartition_clause_ = 627, 
    RuleFrame_clause_ = 628, RuleFrame_extent = 629, RuleFrame_bound = 630, 
    RuleWindow_exclusion_clause_ = 631, RuleRow = 632, RuleExplicit_row = 633, 
    RuleImplicit_row = 634, RuleSub_type = 635, RuleAll_op = 636, RuleMathop = 637, 
    RuleQual_op = 638, RuleQual_all_op = 639, RuleSubquery_Op = 640, RuleExpr_list = 641, 
    RuleFunc_arg_list = 642, RuleFunc_arg_expr = 643, RuleType_list = 644, 
    RuleArray_expr = 645, RuleArray_expr_list = 646, RuleExtract_list = 647, 
    RuleExtract_arg = 648, RuleUnicode_normal_form = 649, RuleOverlay_list = 650, 
    RulePosition_list = 651, RuleSubstr_list = 652, RuleTrim_list = 653, 
    RuleIn_expr = 654, RuleCase_expr = 655, RuleWhen_clause_list = 656, 
    RuleWhen_clause = 657, RuleCase_default = 658, RuleCase_arg = 659, RuleColumnref = 660, 
    RuleIndirection_el = 661, RuleSlice_bound_ = 662, RuleIndirection = 663, 
    RuleOpt_indirection = 664, RuleJson_passing_clause = 665, RuleJson_arguments = 666, 
    RuleJson_argument = 667, RuleJson_wrapper_behavior = 668, RuleJson_behavior = 669, 
    RuleJson_behavior_type = 670, RuleJson_behavior_clause = 671, RuleJson_on_error_clause = 672, 
    RuleJson_value_expr = 673, RuleJson_format_clause = 674, RuleJson_quotes_clause = 675, 
    RuleJson_returning_clause = 676, RuleJson_predicate_type_constraint = 677, 
    RuleJson_key_uniqueness_constraint = 678, RuleJson_name_and_value_list = 679, 
    RuleJson_name_and_value = 680, RuleJson_object_constructor_null_clause = 681, 
    RuleJson_array_constructor_null_clause = 682, RuleJson_value_expr_list = 683, 
    RuleJson_aggregate_func = 684, RuleJson_array_aggregate_order_by_clause = 685, 
    RuleTarget_list_ = 686, RuleTarget_list = 687, RuleTarget_el = 688, 
    RuleQualified_name_list = 689, RuleQualified_name = 690, RuleName_list = 691, 
    RuleName = 692, RuleAttr_name = 693, RuleFile_name = 694, RuleFunc_name = 695, 
    RuleAexprconst = 696, RuleXconst = 697, RuleBconst = 698, RuleFconst = 699, 
    RuleIconst = 700, RuleSconst = 701, RuleAnysconst = 702, RuleUescape_ = 703, 
    RuleSignediconst = 704, RuleRoleid = 705, RuleRolespec = 706, RuleRole_list = 707, 
    RuleColid = 708, RuleType_function_name = 709, RuleNonreservedword = 710, 
    RuleColLabel = 711, RuleBareColLabel = 712, RuleUnreserved_keyword = 713, 
    RuleCol_name_keyword = 714, RuleType_func_name_keyword = 715, RuleReserved_keyword = 716, 
    RuleBare_label_keyword = 717, RuleAny_identifier = 718, RuleIdentifier = 719
  };

  explicit PostgreSQLParser(antlr4::TokenStream *input);

  PostgreSQLParser(antlr4::TokenStream *input, const antlr4::atn::ParserATNSimulatorOptions &options);

  ~PostgreSQLParser() override;

  std::string getGrammarFileName() const override;

  const antlr4::atn::ATN& getATN() const override;

  const std::vector<std::string>& getRuleNames() const override;

  const antlr4::dfa::Vocabulary& getVocabulary() const override;

  antlr4::atn::SerializedATNView getSerializedATN() const override;


  class RootContext;
  class StmtblockContext;
  class StmtmultiContext;
  class StmtContext;
  class CallstmtContext;
  class CreaterolestmtContext;
  class With_Context;
  class OptrolelistContext;
  class AlteroptrolelistContext;
  class AlteroptroleelemContext;
  class CreateoptroleelemContext;
  class CreateuserstmtContext;
  class AlterrolestmtContext;
  class In_database_Context;
  class AlterrolesetstmtContext;
  class DroprolestmtContext;
  class CreategroupstmtContext;
  class AltergroupstmtContext;
  class Add_dropContext;
  class CreateschemastmtContext;
  class OptschemanameContext;
  class OptschemaeltlistContext;
  class Schema_stmtContext;
  class VariablesetstmtContext;
  class Set_restContext;
  class Generic_setContext;
  class Set_rest_moreContext;
  class Var_nameContext;
  class Var_listContext;
  class Var_valueContext;
  class Iso_levelContext;
  class Boolean_or_string_Context;
  class Zone_valueContext;
  class Encoding_Context;
  class Nonreservedword_or_sconstContext;
  class VariableresetstmtContext;
  class Reset_restContext;
  class Generic_resetContext;
  class SetresetclauseContext;
  class FunctionsetresetclauseContext;
  class VariableshowstmtContext;
  class ConstraintssetstmtContext;
  class Constraints_set_listContext;
  class Constraints_set_modeContext;
  class CheckpointstmtContext;
  class DiscardstmtContext;
  class AltertablestmtContext;
  class Alter_table_cmdsContext;
  class Partition_cmdContext;
  class Index_partition_cmdContext;
  class Alter_table_cmdContext;
  class Alter_column_defaultContext;
  class Drop_behavior_Context;
  class Collate_clause_Context;
  class Alter_usingContext;
  class Replica_identityContext;
  class ReloptionsContext;
  class Reloptions_Context;
  class Reloption_listContext;
  class Reloption_elemContext;
  class Alter_identity_column_option_listContext;
  class Alter_identity_column_optionContext;
  class PartitionboundspecContext;
  class Hash_partbound_elemContext;
  class Hash_partboundContext;
  class AltercompositetypestmtContext;
  class Alter_type_cmdsContext;
  class Alter_type_cmdContext;
  class CloseportalstmtContext;
  class CopystmtContext;
  class Copy_fromContext;
  class Program_Context;
  class Copy_file_nameContext;
  class Copy_optionsContext;
  class Copy_opt_listContext;
  class Copy_opt_itemContext;
  class Binary_Context;
  class Copy_delimiterContext;
  class Using_Context;
  class Copy_generic_opt_listContext;
  class Copy_generic_opt_elemContext;
  class Copy_generic_opt_argContext;
  class Copy_generic_opt_arg_listContext;
  class Copy_generic_opt_arg_list_itemContext;
  class CreatestmtContext;
  class OpttempContext;
  class OpttableelementlistContext;
  class OpttypedtableelementlistContext;
  class TableelementlistContext;
  class TypedtableelementlistContext;
  class TableelementContext;
  class TypedtableelementContext;
  class ColumnDefContext;
  class ColumnOptionsContext;
  class ColquallistContext;
  class ColconstraintContext;
  class ColconstraintelemContext;
  class Generated_whenContext;
  class ConstraintattrContext;
  class TablelikeclauseContext;
  class TablelikeoptionlistContext;
  class TablelikeoptionContext;
  class TableconstraintContext;
  class ConstraintelemContext;
  class No_inherit_Context;
  class Column_list_Context;
  class ColumnlistContext;
  class ColumnElemContext;
  class C_include_Context;
  class Key_matchContext;
  class ExclusionconstraintlistContext;
  class ExclusionconstraintelemContext;
  class ExclusionwhereclauseContext;
  class Key_actionsContext;
  class Key_updateContext;
  class Key_deleteContext;
  class Key_actionContext;
  class OptinheritContext;
  class OptpartitionspecContext;
  class PartitionspecContext;
  class Part_paramsContext;
  class Part_elemContext;
  class Table_access_method_clauseContext;
  class OptwithContext;
  class OncommitoptionContext;
  class OpttablespaceContext;
  class OptconstablespaceContext;
  class ExistingindexContext;
  class CreatestatsstmtContext;
  class AlterstatsstmtContext;
  class CreateasstmtContext;
  class Create_as_targetContext;
  class With_data_Context;
  class CreatematviewstmtContext;
  class Create_mv_targetContext;
  class OptnologContext;
  class RefreshmatviewstmtContext;
  class CreateseqstmtContext;
  class AlterseqstmtContext;
  class OptseqoptlistContext;
  class OptparenthesizedseqoptlistContext;
  class SeqoptlistContext;
  class SeqoptelemContext;
  class By_Context;
  class NumericonlyContext;
  class Numericonly_listContext;
  class CreateplangstmtContext;
  class Trusted_Context;
  class Handler_nameContext;
  class Inline_handler_Context;
  class Validator_clauseContext;
  class Validator_Context;
  class Procedural_Context;
  class CreatetablespacestmtContext;
  class OpttablespaceownerContext;
  class DroptablespacestmtContext;
  class CreateextensionstmtContext;
  class Create_extension_opt_listContext;
  class Create_extension_opt_itemContext;
  class AlterextensionstmtContext;
  class Alter_extension_opt_listContext;
  class Alter_extension_opt_itemContext;
  class AlterextensioncontentsstmtContext;
  class CreatefdwstmtContext;
  class Fdw_optionContext;
  class Fdw_optionsContext;
  class Fdw_options_Context;
  class AlterfdwstmtContext;
  class Create_generic_optionsContext;
  class Generic_option_listContext;
  class Alter_generic_optionsContext;
  class Alter_generic_option_listContext;
  class Alter_generic_option_elemContext;
  class Generic_option_elemContext;
  class Generic_option_nameContext;
  class Generic_option_argContext;
  class CreateforeignserverstmtContext;
  class Type_Context;
  class Foreign_server_versionContext;
  class Foreign_server_version_Context;
  class AlterforeignserverstmtContext;
  class CreateforeigntablestmtContext;
  class ImportforeignschemastmtContext;
  class Import_qualification_typeContext;
  class Import_qualificationContext;
  class CreateusermappingstmtContext;
  class Auth_identContext;
  class DropusermappingstmtContext;
  class AlterusermappingstmtContext;
  class CreatepolicystmtContext;
  class AlterpolicystmtContext;
  class RowsecurityoptionalexprContext;
  class RowsecurityoptionalwithcheckContext;
  class RowsecuritydefaulttoroleContext;
  class RowsecurityoptionaltoroleContext;
  class RowsecuritydefaultpermissiveContext;
  class RowsecuritydefaultforcmdContext;
  class Row_security_cmdContext;
  class CreateamstmtContext;
  class Am_typeContext;
  class CreatetrigstmtContext;
  class TriggeractiontimeContext;
  class TriggereventsContext;
  class TriggeroneeventContext;
  class TriggerreferencingContext;
  class TriggertransitionsContext;
  class TriggertransitionContext;
  class TransitionoldornewContext;
  class TransitionrowortableContext;
  class TransitionrelnameContext;
  class TriggerforspecContext;
  class TriggerforopteachContext;
  class TriggerfortypeContext;
  class TriggerwhenContext;
  class Function_or_procedureContext;
  class TriggerfuncargsContext;
  class TriggerfuncargContext;
  class OptconstrfromtableContext;
  class ConstraintattributespecContext;
  class ConstraintattributeElemContext;
  class CreateeventtrigstmtContext;
  class Event_trigger_when_listContext;
  class Event_trigger_when_itemContext;
  class Event_trigger_value_listContext;
  class AltereventtrigstmtContext;
  class Enable_triggerContext;
  class CreateassertionstmtContext;
  class DefinestmtContext;
  class DefinitionContext;
  class Def_listContext;
  class Def_elemContext;
  class Def_argContext;
  class Old_aggr_definitionContext;
  class Old_aggr_listContext;
  class Old_aggr_elemContext;
  class Enum_val_list_Context;
  class Enum_val_listContext;
  class AlterenumstmtContext;
  class If_not_exists_Context;
  class CreateopclassstmtContext;
  class Opclass_item_listContext;
  class Opclass_itemContext;
  class Default_Context;
  class Opfamily_Context;
  class Opclass_purposeContext;
  class Recheck_Context;
  class CreateopfamilystmtContext;
  class AlteropfamilystmtContext;
  class Opclass_drop_listContext;
  class Opclass_dropContext;
  class DropopclassstmtContext;
  class DropopfamilystmtContext;
  class DropownedstmtContext;
  class ReassignownedstmtContext;
  class DropstmtContext;
  class Object_type_any_nameContext;
  class Object_type_nameContext;
  class Drop_type_nameContext;
  class Object_type_name_on_any_nameContext;
  class Any_name_list_Context;
  class Any_nameContext;
  class AttrsContext;
  class Type_name_listContext;
  class TruncatestmtContext;
  class Restart_seqs_Context;
  class CommentstmtContext;
  class Comment_textContext;
  class SeclabelstmtContext;
  class Provider_Context;
  class Security_labelContext;
  class FetchstmtContext;
  class Fetch_argsContext;
  class From_inContext;
  class From_in_Context;
  class GrantstmtContext;
  class RevokestmtContext;
  class PrivilegesContext;
  class Privilege_listContext;
  class PrivilegeContext;
  class Privilege_targetContext;
  class Grantee_listContext;
  class GranteeContext;
  class Grant_grant_option_Context;
  class GrantrolestmtContext;
  class RevokerolestmtContext;
  class Grant_admin_option_Context;
  class Granted_by_Context;
  class AlterdefaultprivilegesstmtContext;
  class DefacloptionlistContext;
  class DefacloptionContext;
  class DefaclactionContext;
  class Defacl_privilege_targetContext;
  class IndexstmtContext;
  class Unique_Context;
  class Nulls_distinctContext;
  class Single_name_Context;
  class Concurrently_Context;
  class Index_name_Context;
  class Access_method_clauseContext;
  class Index_paramsContext;
  class Index_elem_optionsContext;
  class Index_elemContext;
  class Include_Context;
  class Index_including_paramsContext;
  class Collate_Context;
  class Class_Context;
  class Asc_desc_Context;
  class Nulls_order_Context;
  class CreatefunctionstmtContext;
  class Or_replace_Context;
  class Func_argsContext;
  class Func_args_listContext;
  class Function_with_argtypes_listContext;
  class Function_with_argtypesContext;
  class Func_args_with_defaultsContext;
  class Func_args_with_defaults_listContext;
  class Func_argContext;
  class Arg_classContext;
  class Param_nameContext;
  class Func_returnContext;
  class Func_typeContext;
  class Func_arg_with_defaultContext;
  class Aggr_argContext;
  class Aggr_argsContext;
  class Aggr_args_listContext;
  class Aggregate_with_argtypesContext;
  class Aggregate_with_argtypes_listContext;
  class Createfunc_opt_listContext;
  class Common_func_opt_itemContext;
  class Createfunc_opt_itemContext;
  class Func_asContext;
  class Transform_type_listContext;
  class Definition_Context;
  class Table_func_columnContext;
  class Table_func_column_listContext;
  class AlterfunctionstmtContext;
  class Alterfunc_opt_listContext;
  class Restrict_Context;
  class RemovefuncstmtContext;
  class RemoveaggrstmtContext;
  class RemoveoperstmtContext;
  class Oper_argtypesContext;
  class Any_operatorContext;
  class Operator_with_argtypes_listContext;
  class Operator_with_argtypesContext;
  class DostmtContext;
  class Dostmt_opt_listContext;
  class Dostmt_opt_itemContext;
  class CreatecaststmtContext;
  class Cast_contextContext;
  class DropcaststmtContext;
  class If_exists_Context;
  class CreatetransformstmtContext;
  class Transform_element_listContext;
  class DroptransformstmtContext;
  class ReindexstmtContext;
  class Reindex_target_relationContext;
  class Reindex_target_allContext;
  class Reindex_option_listContext;
  class AltertblspcstmtContext;
  class RenamestmtContext;
  class Column_Context;
  class Set_data_Context;
  class AlterobjectdependsstmtContext;
  class No_Context;
  class AlterobjectschemastmtContext;
  class AlteroperatorstmtContext;
  class Operator_def_listContext;
  class Operator_def_elemContext;
  class Operator_def_argContext;
  class AltertypestmtContext;
  class AlterownerstmtContext;
  class CreatepublicationstmtContext;
  class Publication_for_tables_Context;
  class Publication_for_tablesContext;
  class AlterpublicationstmtContext;
  class CreatesubscriptionstmtContext;
  class Publication_name_listContext;
  class Publication_name_itemContext;
  class AltersubscriptionstmtContext;
  class DropsubscriptionstmtContext;
  class RulestmtContext;
  class RuleactionlistContext;
  class RuleactionmultiContext;
  class RuleactionstmtContext;
  class RuleactionstmtOrEmptyContext;
  class EventContext;
  class Instead_Context;
  class NotifystmtContext;
  class Notify_payloadContext;
  class ListenstmtContext;
  class UnlistenstmtContext;
  class TransactionstmtContext;
  class Transaction_Context;
  class Transaction_mode_itemContext;
  class Transaction_mode_listContext;
  class Transaction_mode_list_or_emptyContext;
  class Transaction_chain_Context;
  class ViewstmtContext;
  class Check_option_Context;
  class LoadstmtContext;
  class CreatedbstmtContext;
  class Createdb_opt_listContext;
  class Createdb_opt_itemsContext;
  class Createdb_opt_itemContext;
  class Createdb_opt_nameContext;
  class Equal_Context;
  class AlterdatabasestmtContext;
  class AlterdatabasesetstmtContext;
  class DropdbstmtContext;
  class Drop_option_listContext;
  class Drop_optionContext;
  class AltercollationstmtContext;
  class AltersystemstmtContext;
  class CreatedomainstmtContext;
  class AlterdomainstmtContext;
  class As_Context;
  class AltertsdictionarystmtContext;
  class AltertsconfigurationstmtContext;
  class Any_withContext;
  class CreateconversionstmtContext;
  class ClusterstmtContext;
  class Cluster_index_specificationContext;
  class VacuumstmtContext;
  class AnalyzestmtContext;
  class Utility_option_listContext;
  class Vac_analyze_option_listContext;
  class Analyze_keywordContext;
  class Utility_option_elemContext;
  class Utility_option_nameContext;
  class Utility_option_argContext;
  class Vac_analyze_option_elemContext;
  class Vac_analyze_option_nameContext;
  class Vac_analyze_option_argContext;
  class Analyze_Context;
  class Verbose_Context;
  class Full_Context;
  class Freeze_Context;
  class Name_list_Context;
  class Vacuum_relationContext;
  class Vacuum_relation_listContext;
  class Vacuum_relation_list_Context;
  class ExplainstmtContext;
  class ExplainablestmtContext;
  class Explain_option_listContext;
  class Explain_option_elemContext;
  class Explain_option_nameContext;
  class Explain_option_argContext;
  class PreparestmtContext;
  class Prep_type_clauseContext;
  class PreparablestmtContext;
  class ExecutestmtContext;
  class Execute_param_clauseContext;
  class DeallocatestmtContext;
  class InsertstmtContext;
  class Insert_targetContext;
  class Insert_restContext;
  class Override_kindContext;
  class Insert_column_listContext;
  class Insert_column_itemContext;
  class On_conflict_Context;
  class Conf_expr_Context;
  class Returning_clauseContext;
  class MergestmtContext;
  class Merge_insert_clauseContext;
  class Merge_update_clauseContext;
  class Merge_delete_clauseContext;
  class DeletestmtContext;
  class Using_clauseContext;
  class LockstmtContext;
  class Lock_Context;
  class Lock_typeContext;
  class Nowait_Context;
  class Nowait_or_skip_Context;
  class UpdatestmtContext;
  class Set_clause_listContext;
  class Set_clauseContext;
  class Set_targetContext;
  class Set_target_listContext;
  class DeclarecursorstmtContext;
  class Cursor_nameContext;
  class Cursor_optionsContext;
  class Hold_Context;
  class SelectstmtContext;
  class Select_with_parensContext;
  class Select_no_parensContext;
  class Select_clauseContext;
  class Simple_select_intersectContext;
  class Simple_select_pramaryContext;
  class With_clauseContext;
  class Cte_listContext;
  class Common_table_exprContext;
  class Materialized_Context;
  class With_clause_Context;
  class Into_clauseContext;
  class Strict_Context;
  class OpttempTableNameContext;
  class Table_Context;
  class All_or_distinctContext;
  class Distinct_clauseContext;
  class All_clause_Context;
  class Sort_clause_Context;
  class Sort_clauseContext;
  class Sortby_listContext;
  class SortbyContext;
  class Select_limitContext;
  class Select_limit_Context;
  class Limit_clauseContext;
  class Offset_clauseContext;
  class Select_limit_valueContext;
  class Select_offset_valueContext;
  class Select_fetch_first_valueContext;
  class I_or_f_constContext;
  class Row_or_rowsContext;
  class First_or_nextContext;
  class Group_clauseContext;
  class Group_by_listContext;
  class Group_by_itemContext;
  class Empty_grouping_setContext;
  class Rollup_clauseContext;
  class Cube_clauseContext;
  class Grouping_sets_clauseContext;
  class Having_clauseContext;
  class For_locking_clauseContext;
  class For_locking_clause_Context;
  class For_locking_itemsContext;
  class For_locking_itemContext;
  class For_locking_strengthContext;
  class Locked_rels_listContext;
  class Values_clauseContext;
  class From_clauseContext;
  class From_listContext;
  class Table_refContext;
  class Alias_clauseContext;
  class Func_alias_clauseContext;
  class Join_typeContext;
  class Join_qualContext;
  class Relation_exprContext;
  class Relation_expr_listContext;
  class Relation_expr_opt_aliasContext;
  class Tablesample_clauseContext;
  class Repeatable_clause_Context;
  class Func_tableContext;
  class Rowsfrom_itemContext;
  class Rowsfrom_listContext;
  class Col_def_list_Context;
  class Ordinality_Context;
  class Where_clauseContext;
  class Where_or_current_clauseContext;
  class OpttablefuncelementlistContext;
  class TablefuncelementlistContext;
  class TablefuncelementContext;
  class XmltableContext;
  class Xmltable_column_listContext;
  class Xmltable_column_elContext;
  class Xmltable_column_option_listContext;
  class Xmltable_column_option_elContext;
  class Xml_namespace_listContext;
  class Xml_namespace_elContext;
  class TypenameContext;
  class Opt_array_boundsContext;
  class SimpletypenameContext;
  class ConsttypenameContext;
  class GenerictypeContext;
  class Type_modifiers_Context;
  class NumericContext;
  class Float_Context;
  class BitContext;
  class ConstbitContext;
  class BitwithlengthContext;
  class BitwithoutlengthContext;
  class CharacterContext;
  class ConstcharacterContext;
  class Character_cContext;
  class Varying_Context;
  class ConstdatetimeContext;
  class ConstintervalContext;
  class Timezone_Context;
  class Interval_Context;
  class Interval_secondContext;
  class JsonTypeContext;
  class Escape_Context;
  class A_exprContext;
  class A_expr_qualContext;
  class A_expr_lesslessContext;
  class A_expr_orContext;
  class A_expr_andContext;
  class A_expr_betweenContext;
  class A_expr_inContext;
  class A_expr_unary_notContext;
  class A_expr_isnullContext;
  class A_expr_is_notContext;
  class A_expr_compareContext;
  class A_expr_likeContext;
  class A_expr_qual_opContext;
  class A_expr_unary_qualopContext;
  class A_expr_addContext;
  class A_expr_mulContext;
  class A_expr_caretContext;
  class A_expr_unary_signContext;
  class A_expr_at_time_zoneContext;
  class A_expr_collateContext;
  class A_expr_typecastContext;
  class B_exprContext;
  class C_exprContext;
  class PlsqlvariablenameContext;
  class Func_applicationContext;
  class Func_exprContext;
  class Func_expr_windowlessContext;
  class Func_expr_common_subexprContext;
  class Xml_root_versionContext;
  class Xml_root_standalone_Context;
  class Xml_attributesContext;
  class Xml_attribute_listContext;
  class Xml_attribute_elContext;
  class Document_or_contentContext;
  class Xml_whitespace_optionContext;
  class Xmlexists_argumentContext;
  class Xml_passing_mechContext;
  class Within_group_clauseContext;
  class Filter_clauseContext;
  class Window_clauseContext;
  class Window_definition_listContext;
  class Window_definitionContext;
  class Over_clauseContext;
  class Window_specificationContext;
  class Existing_window_name_Context;
  class Partition_clause_Context;
  class Frame_clause_Context;
  class Frame_extentContext;
  class Frame_boundContext;
  class Window_exclusion_clause_Context;
  class RowContext;
  class Explicit_rowContext;
  class Implicit_rowContext;
  class Sub_typeContext;
  class All_opContext;
  class MathopContext;
  class Qual_opContext;
  class Qual_all_opContext;
  class Subquery_OpContext;
  class Expr_listContext;
  class Func_arg_listContext;
  class Func_arg_exprContext;
  class Type_listContext;
  class Array_exprContext;
  class Array_expr_listContext;
  class Extract_listContext;
  class Extract_argContext;
  class Unicode_normal_formContext;
  class Overlay_listContext;
  class Position_listContext;
  class Substr_listContext;
  class Trim_listContext;
  class In_exprContext;
  class Case_exprContext;
  class When_clause_listContext;
  class When_clauseContext;
  class Case_defaultContext;
  class Case_argContext;
  class ColumnrefContext;
  class Indirection_elContext;
  class Slice_bound_Context;
  class IndirectionContext;
  class Opt_indirectionContext;
  class Json_passing_clauseContext;
  class Json_argumentsContext;
  class Json_argumentContext;
  class Json_wrapper_behaviorContext;
  class Json_behaviorContext;
  class Json_behavior_typeContext;
  class Json_behavior_clauseContext;
  class Json_on_error_clauseContext;
  class Json_value_exprContext;
  class Json_format_clauseContext;
  class Json_quotes_clauseContext;
  class Json_returning_clauseContext;
  class Json_predicate_type_constraintContext;
  class Json_key_uniqueness_constraintContext;
  class Json_name_and_value_listContext;
  class Json_name_and_valueContext;
  class Json_object_constructor_null_clauseContext;
  class Json_array_constructor_null_clauseContext;
  class Json_value_expr_listContext;
  class Json_aggregate_funcContext;
  class Json_array_aggregate_order_by_clauseContext;
  class Target_list_Context;
  class Target_listContext;
  class Target_elContext;
  class Qualified_name_listContext;
  class Qualified_nameContext;
  class Name_listContext;
  class NameContext;
  class Attr_nameContext;
  class File_nameContext;
  class Func_nameContext;
  class AexprconstContext;
  class XconstContext;
  class BconstContext;
  class FconstContext;
  class IconstContext;
  class SconstContext;
  class AnysconstContext;
  class Uescape_Context;
  class SignediconstContext;
  class RoleidContext;
  class RolespecContext;
  class Role_listContext;
  class ColidContext;
  class Type_function_nameContext;
  class NonreservedwordContext;
  class ColLabelContext;
  class BareColLabelContext;
  class Unreserved_keywordContext;
  class Col_name_keywordContext;
  class Type_func_name_keywordContext;
  class Reserved_keywordContext;
  class Bare_label_keywordContext;
  class Any_identifierContext;
  class IdentifierContext; 

  class  RootContext : public antlr4::ParserRuleContext {
  public:
    RootContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    StmtblockContext *stmtblock();
    antlr4::tree::TerminalNode *EOF();

   
  };

  RootContext* root();

  class  StmtblockContext : public antlr4::ParserRuleContext {
  public:
    StmtblockContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    StmtmultiContext *stmtmulti();

   
  };

  StmtblockContext* stmtblock();

  class  StmtmultiContext : public antlr4::ParserRuleContext {
  public:
    StmtmultiContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<StmtContext *> stmt();
    StmtContext* stmt(size_t i);
    std::vector<antlr4::tree::TerminalNode *> SEMI();
    antlr4::tree::TerminalNode* SEMI(size_t i);

   
  };

  StmtmultiContext* stmtmulti();

  class  StmtContext : public antlr4::ParserRuleContext {
  public:
    StmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    AltereventtrigstmtContext *altereventtrigstmt();
    AltercollationstmtContext *altercollationstmt();
    AlterdatabasestmtContext *alterdatabasestmt();
    AlterdatabasesetstmtContext *alterdatabasesetstmt();
    AlterdefaultprivilegesstmtContext *alterdefaultprivilegesstmt();
    AlterdomainstmtContext *alterdomainstmt();
    AlterenumstmtContext *alterenumstmt();
    AlterextensionstmtContext *alterextensionstmt();
    AlterextensioncontentsstmtContext *alterextensioncontentsstmt();
    AlterfdwstmtContext *alterfdwstmt();
    AlterforeignserverstmtContext *alterforeignserverstmt();
    AlterfunctionstmtContext *alterfunctionstmt();
    AltergroupstmtContext *altergroupstmt();
    AlterobjectdependsstmtContext *alterobjectdependsstmt();
    AlterobjectschemastmtContext *alterobjectschemastmt();
    AlterownerstmtContext *alterownerstmt();
    AlteroperatorstmtContext *alteroperatorstmt();
    AltertypestmtContext *altertypestmt();
    AlterpolicystmtContext *alterpolicystmt();
    AlterseqstmtContext *alterseqstmt();
    AltersystemstmtContext *altersystemstmt();
    AltertablestmtContext *altertablestmt();
    AltertblspcstmtContext *altertblspcstmt();
    AltercompositetypestmtContext *altercompositetypestmt();
    AlterpublicationstmtContext *alterpublicationstmt();
    AlterrolesetstmtContext *alterrolesetstmt();
    AlterrolestmtContext *alterrolestmt();
    AltersubscriptionstmtContext *altersubscriptionstmt();
    AlterstatsstmtContext *alterstatsstmt();
    AltertsconfigurationstmtContext *altertsconfigurationstmt();
    AltertsdictionarystmtContext *altertsdictionarystmt();
    AlterusermappingstmtContext *alterusermappingstmt();
    AnalyzestmtContext *analyzestmt();
    CallstmtContext *callstmt();
    CheckpointstmtContext *checkpointstmt();
    CloseportalstmtContext *closeportalstmt();
    ClusterstmtContext *clusterstmt();
    CommentstmtContext *commentstmt();
    ConstraintssetstmtContext *constraintssetstmt();
    CopystmtContext *copystmt();
    CreateamstmtContext *createamstmt();
    CreateasstmtContext *createasstmt();
    CreateassertionstmtContext *createassertionstmt();
    CreatecaststmtContext *createcaststmt();
    CreateconversionstmtContext *createconversionstmt();
    CreatedomainstmtContext *createdomainstmt();
    CreateextensionstmtContext *createextensionstmt();
    CreatefdwstmtContext *createfdwstmt();
    CreateforeignserverstmtContext *createforeignserverstmt();
    CreateforeigntablestmtContext *createforeigntablestmt();
    CreatefunctionstmtContext *createfunctionstmt();
    CreategroupstmtContext *creategroupstmt();
    CreatematviewstmtContext *creatematviewstmt();
    CreateopclassstmtContext *createopclassstmt();
    CreateopfamilystmtContext *createopfamilystmt();
    CreatepublicationstmtContext *createpublicationstmt();
    AlteropfamilystmtContext *alteropfamilystmt();
    CreatepolicystmtContext *createpolicystmt();
    CreateplangstmtContext *createplangstmt();
    CreateschemastmtContext *createschemastmt();
    CreateseqstmtContext *createseqstmt();
    CreatestmtContext *createstmt();
    CreatesubscriptionstmtContext *createsubscriptionstmt();
    CreatestatsstmtContext *createstatsstmt();
    CreatetablespacestmtContext *createtablespacestmt();
    CreatetransformstmtContext *createtransformstmt();
    CreatetrigstmtContext *createtrigstmt();
    CreateeventtrigstmtContext *createeventtrigstmt();
    CreaterolestmtContext *createrolestmt();
    CreateuserstmtContext *createuserstmt();
    CreateusermappingstmtContext *createusermappingstmt();
    CreatedbstmtContext *createdbstmt();
    DeallocatestmtContext *deallocatestmt();
    DeclarecursorstmtContext *declarecursorstmt();
    DefinestmtContext *definestmt();
    DeletestmtContext *deletestmt();
    DiscardstmtContext *discardstmt();
    DostmtContext *dostmt();
    DropcaststmtContext *dropcaststmt();
    DropopclassstmtContext *dropopclassstmt();
    DropopfamilystmtContext *dropopfamilystmt();
    DropownedstmtContext *dropownedstmt();
    DropstmtContext *dropstmt();
    DropsubscriptionstmtContext *dropsubscriptionstmt();
    DroptablespacestmtContext *droptablespacestmt();
    DroptransformstmtContext *droptransformstmt();
    DroprolestmtContext *droprolestmt();
    DropusermappingstmtContext *dropusermappingstmt();
    DropdbstmtContext *dropdbstmt();
    ExecutestmtContext *executestmt();
    ExplainstmtContext *explainstmt();
    FetchstmtContext *fetchstmt();
    GrantstmtContext *grantstmt();
    GrantrolestmtContext *grantrolestmt();
    ImportforeignschemastmtContext *importforeignschemastmt();
    IndexstmtContext *indexstmt();
    InsertstmtContext *insertstmt();
    MergestmtContext *mergestmt();
    ListenstmtContext *listenstmt();
    RefreshmatviewstmtContext *refreshmatviewstmt();
    LoadstmtContext *loadstmt();
    LockstmtContext *lockstmt();
    NotifystmtContext *notifystmt();
    PreparestmtContext *preparestmt();
    ReassignownedstmtContext *reassignownedstmt();
    ReindexstmtContext *reindexstmt();
    RemoveaggrstmtContext *removeaggrstmt();
    RemovefuncstmtContext *removefuncstmt();
    RemoveoperstmtContext *removeoperstmt();
    RenamestmtContext *renamestmt();
    RevokestmtContext *revokestmt();
    RevokerolestmtContext *revokerolestmt();
    RulestmtContext *rulestmt();
    SeclabelstmtContext *seclabelstmt();
    SelectstmtContext *selectstmt();
    TransactionstmtContext *transactionstmt();
    TruncatestmtContext *truncatestmt();
    UnlistenstmtContext *unlistenstmt();
    UpdatestmtContext *updatestmt();
    VacuumstmtContext *vacuumstmt();
    VariableresetstmtContext *variableresetstmt();
    VariablesetstmtContext *variablesetstmt();
    VariableshowstmtContext *variableshowstmt();
    ViewstmtContext *viewstmt();

   
  };

  StmtContext* stmt();

  class  CallstmtContext : public antlr4::ParserRuleContext {
  public:
    CallstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CALL();
    Func_applicationContext *func_application();

   
  };

  CallstmtContext* callstmt();

  class  CreaterolestmtContext : public antlr4::ParserRuleContext {
  public:
    CreaterolestmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *ROLE();
    RoleidContext *roleid();
    OptrolelistContext *optrolelist();
    With_Context *with_();

   
  };

  CreaterolestmtContext* createrolestmt();

  class  With_Context : public antlr4::ParserRuleContext {
  public:
    With_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WITH();

   
  };

  With_Context* with_();

  class  OptrolelistContext : public antlr4::ParserRuleContext {
  public:
    OptrolelistContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<CreateoptroleelemContext *> createoptroleelem();
    CreateoptroleelemContext* createoptroleelem(size_t i);

   
  };

  OptrolelistContext* optrolelist();

  class  AlteroptrolelistContext : public antlr4::ParserRuleContext {
  public:
    AlteroptrolelistContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<AlteroptroleelemContext *> alteroptroleelem();
    AlteroptroleelemContext* alteroptroleelem(size_t i);

   
  };

  AlteroptrolelistContext* alteroptrolelist();

  class  AlteroptroleelemContext : public antlr4::ParserRuleContext {
  public:
    AlteroptroleelemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PASSWORD();
    SconstContext *sconst();
    antlr4::tree::TerminalNode *NULL_P();
    antlr4::tree::TerminalNode *ENCRYPTED();
    antlr4::tree::TerminalNode *UNENCRYPTED();
    antlr4::tree::TerminalNode *INHERIT();
    antlr4::tree::TerminalNode *CONNECTION();
    antlr4::tree::TerminalNode *LIMIT();
    SignediconstContext *signediconst();
    antlr4::tree::TerminalNode *VALID();
    antlr4::tree::TerminalNode *UNTIL();
    antlr4::tree::TerminalNode *USER();
    Role_listContext *role_list();
    IdentifierContext *identifier();

   
  };

  AlteroptroleelemContext* alteroptroleelem();

  class  CreateoptroleelemContext : public antlr4::ParserRuleContext {
  public:
    CreateoptroleelemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    AlteroptroleelemContext *alteroptroleelem();
    antlr4::tree::TerminalNode *SYSID();
    IconstContext *iconst();
    antlr4::tree::TerminalNode *ADMIN();
    Role_listContext *role_list();
    antlr4::tree::TerminalNode *ROLE();
    antlr4::tree::TerminalNode *IN_P();
    antlr4::tree::TerminalNode *GROUP_P();

   
  };

  CreateoptroleelemContext* createoptroleelem();

  class  CreateuserstmtContext : public antlr4::ParserRuleContext {
  public:
    CreateuserstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *USER();
    RoleidContext *roleid();
    OptrolelistContext *optrolelist();
    With_Context *with_();

   
  };

  CreateuserstmtContext* createuserstmt();

  class  AlterrolestmtContext : public antlr4::ParserRuleContext {
  public:
    AlterrolestmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    RolespecContext *rolespec();
    AlteroptrolelistContext *alteroptrolelist();
    antlr4::tree::TerminalNode *ROLE();
    antlr4::tree::TerminalNode *USER();
    With_Context *with_();

   
  };

  AlterrolestmtContext* alterrolestmt();

  class  In_database_Context : public antlr4::ParserRuleContext {
  public:
    In_database_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *IN_P();
    antlr4::tree::TerminalNode *DATABASE();
    NameContext *name();

   
  };

  In_database_Context* in_database_();

  class  AlterrolesetstmtContext : public antlr4::ParserRuleContext {
  public:
    AlterrolesetstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    RolespecContext *rolespec();
    SetresetclauseContext *setresetclause();
    antlr4::tree::TerminalNode *ROLE();
    antlr4::tree::TerminalNode *USER();
    antlr4::tree::TerminalNode *ALL();
    In_database_Context *in_database_();

   
  };

  AlterrolesetstmtContext* alterrolesetstmt();

  class  DroprolestmtContext : public antlr4::ParserRuleContext {
  public:
    DroprolestmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DROP();
    Role_listContext *role_list();
    antlr4::tree::TerminalNode *ROLE();
    antlr4::tree::TerminalNode *USER();
    antlr4::tree::TerminalNode *GROUP_P();
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *EXISTS();

   
  };

  DroprolestmtContext* droprolestmt();

  class  CreategroupstmtContext : public antlr4::ParserRuleContext {
  public:
    CreategroupstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *GROUP_P();
    RoleidContext *roleid();
    OptrolelistContext *optrolelist();
    With_Context *with_();

   
  };

  CreategroupstmtContext* creategroupstmt();

  class  AltergroupstmtContext : public antlr4::ParserRuleContext {
  public:
    AltergroupstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *GROUP_P();
    RolespecContext *rolespec();
    Add_dropContext *add_drop();
    antlr4::tree::TerminalNode *USER();
    Role_listContext *role_list();

   
  };

  AltergroupstmtContext* altergroupstmt();

  class  Add_dropContext : public antlr4::ParserRuleContext {
  public:
    Add_dropContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ADD_P();
    antlr4::tree::TerminalNode *DROP();

   
  };

  Add_dropContext* add_drop();

  class  CreateschemastmtContext : public antlr4::ParserRuleContext {
  public:
    CreateschemastmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *SCHEMA();
    OptschemaeltlistContext *optschemaeltlist();
    antlr4::tree::TerminalNode *AUTHORIZATION();
    RolespecContext *rolespec();
    ColidContext *colid();
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *EXISTS();
    OptschemanameContext *optschemaname();

   
  };

  CreateschemastmtContext* createschemastmt();

  class  OptschemanameContext : public antlr4::ParserRuleContext {
  public:
    OptschemanameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ColidContext *colid();

   
  };

  OptschemanameContext* optschemaname();

  class  OptschemaeltlistContext : public antlr4::ParserRuleContext {
  public:
    OptschemaeltlistContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Schema_stmtContext *> schema_stmt();
    Schema_stmtContext* schema_stmt(size_t i);

   
  };

  OptschemaeltlistContext* optschemaeltlist();

  class  Schema_stmtContext : public antlr4::ParserRuleContext {
  public:
    Schema_stmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    CreatestmtContext *createstmt();
    IndexstmtContext *indexstmt();
    CreateseqstmtContext *createseqstmt();
    CreatetrigstmtContext *createtrigstmt();
    GrantstmtContext *grantstmt();
    ViewstmtContext *viewstmt();

   
  };

  Schema_stmtContext* schema_stmt();

  class  VariablesetstmtContext : public antlr4::ParserRuleContext {
  public:
    VariablesetstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SET();
    Set_restContext *set_rest();
    antlr4::tree::TerminalNode *LOCAL();
    antlr4::tree::TerminalNode *SESSION();

   
  };

  VariablesetstmtContext* variablesetstmt();

  class  Set_restContext : public antlr4::ParserRuleContext {
  public:
    Set_restContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TRANSACTION();
    Transaction_mode_listContext *transaction_mode_list();
    antlr4::tree::TerminalNode *SESSION();
    antlr4::tree::TerminalNode *CHARACTERISTICS();
    antlr4::tree::TerminalNode *AS();
    Set_rest_moreContext *set_rest_more();

   
  };

  Set_restContext* set_rest();

  class  Generic_setContext : public antlr4::ParserRuleContext {
  public:
    Generic_setContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Var_nameContext *var_name();
    antlr4::tree::TerminalNode *TO();
    antlr4::tree::TerminalNode *EQUAL();
    Var_listContext *var_list();
    antlr4::tree::TerminalNode *DEFAULT();

   
  };

  Generic_setContext* generic_set();

  class  Set_rest_moreContext : public antlr4::ParserRuleContext {
  public:
    Set_rest_moreContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Generic_setContext *generic_set();
    Var_nameContext *var_name();
    antlr4::tree::TerminalNode *FROM();
    antlr4::tree::TerminalNode *CURRENT_P();
    antlr4::tree::TerminalNode *TIME();
    antlr4::tree::TerminalNode *ZONE();
    Zone_valueContext *zone_value();
    antlr4::tree::TerminalNode *CATALOG();
    SconstContext *sconst();
    antlr4::tree::TerminalNode *SCHEMA();
    antlr4::tree::TerminalNode *NAMES();
    Encoding_Context *encoding_();
    antlr4::tree::TerminalNode *ROLE();
    Nonreservedword_or_sconstContext *nonreservedword_or_sconst();
    antlr4::tree::TerminalNode *SESSION();
    antlr4::tree::TerminalNode *AUTHORIZATION();
    antlr4::tree::TerminalNode *XML_P();
    antlr4::tree::TerminalNode *OPTION();
    Document_or_contentContext *document_or_content();
    antlr4::tree::TerminalNode *TRANSACTION();
    antlr4::tree::TerminalNode *SNAPSHOT();

   
  };

  Set_rest_moreContext* set_rest_more();

  class  Var_nameContext : public antlr4::ParserRuleContext {
  public:
    Var_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<ColidContext *> colid();
    ColidContext* colid(size_t i);
    std::vector<antlr4::tree::TerminalNode *> DOT();
    antlr4::tree::TerminalNode* DOT(size_t i);

   
  };

  Var_nameContext* var_name();

  class  Var_listContext : public antlr4::ParserRuleContext {
  public:
    Var_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Var_valueContext *> var_value();
    Var_valueContext* var_value(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Var_listContext* var_list();

  class  Var_valueContext : public antlr4::ParserRuleContext {
  public:
    Var_valueContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Boolean_or_string_Context *boolean_or_string_();
    NumericonlyContext *numericonly();

   
  };

  Var_valueContext* var_value();

  class  Iso_levelContext : public antlr4::ParserRuleContext {
  public:
    Iso_levelContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *READ();
    antlr4::tree::TerminalNode *UNCOMMITTED();
    antlr4::tree::TerminalNode *COMMITTED();
    antlr4::tree::TerminalNode *REPEATABLE();
    antlr4::tree::TerminalNode *SERIALIZABLE();

   
  };

  Iso_levelContext* iso_level();

  class  Boolean_or_string_Context : public antlr4::ParserRuleContext {
  public:
    Boolean_or_string_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TRUE_P();
    antlr4::tree::TerminalNode *FALSE_P();
    antlr4::tree::TerminalNode *ON();
    Nonreservedword_or_sconstContext *nonreservedword_or_sconst();

   
  };

  Boolean_or_string_Context* boolean_or_string_();

  class  Zone_valueContext : public antlr4::ParserRuleContext {
  public:
    Zone_valueContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    SconstContext *sconst();
    IdentifierContext *identifier();
    ConstintervalContext *constinterval();
    Interval_Context *interval_();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    IconstContext *iconst();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    NumericonlyContext *numericonly();
    antlr4::tree::TerminalNode *DEFAULT();
    antlr4::tree::TerminalNode *LOCAL();

   
  };

  Zone_valueContext* zone_value();

  class  Encoding_Context : public antlr4::ParserRuleContext {
  public:
    Encoding_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    SconstContext *sconst();
    antlr4::tree::TerminalNode *DEFAULT();

   
  };

  Encoding_Context* encoding_();

  class  Nonreservedword_or_sconstContext : public antlr4::ParserRuleContext {
  public:
    Nonreservedword_or_sconstContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    NonreservedwordContext *nonreservedword();
    SconstContext *sconst();

   
  };

  Nonreservedword_or_sconstContext* nonreservedword_or_sconst();

  class  VariableresetstmtContext : public antlr4::ParserRuleContext {
  public:
    VariableresetstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RESET();
    Reset_restContext *reset_rest();

   
  };

  VariableresetstmtContext* variableresetstmt();

  class  Reset_restContext : public antlr4::ParserRuleContext {
  public:
    Reset_restContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Generic_resetContext *generic_reset();
    antlr4::tree::TerminalNode *TIME();
    antlr4::tree::TerminalNode *ZONE();
    antlr4::tree::TerminalNode *TRANSACTION();
    antlr4::tree::TerminalNode *ISOLATION();
    antlr4::tree::TerminalNode *LEVEL();
    antlr4::tree::TerminalNode *SESSION();
    antlr4::tree::TerminalNode *AUTHORIZATION();

   
  };

  Reset_restContext* reset_rest();

  class  Generic_resetContext : public antlr4::ParserRuleContext {
  public:
    Generic_resetContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Var_nameContext *var_name();
    antlr4::tree::TerminalNode *ALL();

   
  };

  Generic_resetContext* generic_reset();

  class  SetresetclauseContext : public antlr4::ParserRuleContext {
  public:
    SetresetclauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SET();
    Set_restContext *set_rest();
    VariableresetstmtContext *variableresetstmt();

   
  };

  SetresetclauseContext* setresetclause();

  class  FunctionsetresetclauseContext : public antlr4::ParserRuleContext {
  public:
    FunctionsetresetclauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SET();
    Set_rest_moreContext *set_rest_more();
    VariableresetstmtContext *variableresetstmt();

   
  };

  FunctionsetresetclauseContext* functionsetresetclause();

  class  VariableshowstmtContext : public antlr4::ParserRuleContext {
  public:
    VariableshowstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SHOW();
    Var_nameContext *var_name();
    antlr4::tree::TerminalNode *TIME();
    antlr4::tree::TerminalNode *ZONE();
    antlr4::tree::TerminalNode *TRANSACTION();
    antlr4::tree::TerminalNode *ISOLATION();
    antlr4::tree::TerminalNode *LEVEL();
    antlr4::tree::TerminalNode *SESSION();
    antlr4::tree::TerminalNode *AUTHORIZATION();
    antlr4::tree::TerminalNode *ALL();

   
  };

  VariableshowstmtContext* variableshowstmt();

  class  ConstraintssetstmtContext : public antlr4::ParserRuleContext {
  public:
    ConstraintssetstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SET();
    antlr4::tree::TerminalNode *CONSTRAINTS();
    Constraints_set_listContext *constraints_set_list();
    Constraints_set_modeContext *constraints_set_mode();

   
  };

  ConstraintssetstmtContext* constraintssetstmt();

  class  Constraints_set_listContext : public antlr4::ParserRuleContext {
  public:
    Constraints_set_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALL();
    Qualified_name_listContext *qualified_name_list();

   
  };

  Constraints_set_listContext* constraints_set_list();

  class  Constraints_set_modeContext : public antlr4::ParserRuleContext {
  public:
    Constraints_set_modeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DEFERRED();
    antlr4::tree::TerminalNode *IMMEDIATE();

   
  };

  Constraints_set_modeContext* constraints_set_mode();

  class  CheckpointstmtContext : public antlr4::ParserRuleContext {
  public:
    CheckpointstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CHECKPOINT();

   
  };

  CheckpointstmtContext* checkpointstmt();

  class  DiscardstmtContext : public antlr4::ParserRuleContext {
  public:
    DiscardstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DISCARD();
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *TEMP();
    antlr4::tree::TerminalNode *TEMPORARY();
    antlr4::tree::TerminalNode *PLANS();
    antlr4::tree::TerminalNode *SEQUENCES();

   
  };

  DiscardstmtContext* discardstmt();

  class  AltertablestmtContext : public antlr4::ParserRuleContext {
  public:
    AltertablestmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *TABLE();
    Relation_exprContext *relation_expr();
    Alter_table_cmdsContext *alter_table_cmds();
    Partition_cmdContext *partition_cmd();
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *EXISTS();
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *IN_P();
    std::vector<antlr4::tree::TerminalNode *> TABLESPACE();
    antlr4::tree::TerminalNode* TABLESPACE(size_t i);
    std::vector<NameContext *> name();
    NameContext* name(size_t i);
    antlr4::tree::TerminalNode *SET();
    antlr4::tree::TerminalNode *OWNED();
    antlr4::tree::TerminalNode *BY();
    Role_listContext *role_list();
    Nowait_Context *nowait_();
    antlr4::tree::TerminalNode *INDEX();
    Qualified_nameContext *qualified_name();
    Index_partition_cmdContext *index_partition_cmd();
    antlr4::tree::TerminalNode *SEQUENCE();
    antlr4::tree::TerminalNode *VIEW();
    antlr4::tree::TerminalNode *MATERIALIZED();
    antlr4::tree::TerminalNode *FOREIGN();

   
  };

  AltertablestmtContext* altertablestmt();

  class  Alter_table_cmdsContext : public antlr4::ParserRuleContext {
  public:
    Alter_table_cmdsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Alter_table_cmdContext *> alter_table_cmd();
    Alter_table_cmdContext* alter_table_cmd(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Alter_table_cmdsContext* alter_table_cmds();

  class  Partition_cmdContext : public antlr4::ParserRuleContext {
  public:
    Partition_cmdContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ATTACH();
    antlr4::tree::TerminalNode *PARTITION();
    Qualified_nameContext *qualified_name();
    PartitionboundspecContext *partitionboundspec();
    antlr4::tree::TerminalNode *DETACH();

   
  };

  Partition_cmdContext* partition_cmd();

  class  Index_partition_cmdContext : public antlr4::ParserRuleContext {
  public:
    Index_partition_cmdContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ATTACH();
    antlr4::tree::TerminalNode *PARTITION();
    Qualified_nameContext *qualified_name();

   
  };

  Index_partition_cmdContext* index_partition_cmd();

  class  Alter_table_cmdContext : public antlr4::ParserRuleContext {
  public:
    Alter_table_cmdContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ADD_P();
    ColumnDefContext *columnDef();
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *EXISTS();
    antlr4::tree::TerminalNode *COLUMN();
    antlr4::tree::TerminalNode *ALTER();
    std::vector<ColidContext *> colid();
    ColidContext* colid(size_t i);
    Alter_column_defaultContext *alter_column_default();
    Column_Context *column_();
    antlr4::tree::TerminalNode *DROP();
    antlr4::tree::TerminalNode *NULL_P();
    antlr4::tree::TerminalNode *SET();
    antlr4::tree::TerminalNode *EXPRESSION();
    antlr4::tree::TerminalNode *STATISTICS();
    SignediconstContext *signediconst();
    IconstContext *iconst();
    ReloptionsContext *reloptions();
    antlr4::tree::TerminalNode *RESET();
    antlr4::tree::TerminalNode *STORAGE();
    antlr4::tree::TerminalNode *GENERATED();
    Generated_whenContext *generated_when();
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *IDENTITY_P();
    OptparenthesizedseqoptlistContext *optparenthesizedseqoptlist();
    Alter_identity_column_option_listContext *alter_identity_column_option_list();
    Drop_behavior_Context *drop_behavior_();
    antlr4::tree::TerminalNode *TYPE_P();
    TypenameContext *typename_();
    Set_data_Context *set_data_();
    Collate_clause_Context *collate_clause_();
    Alter_usingContext *alter_using();
    Alter_generic_optionsContext *alter_generic_options();
    TableconstraintContext *tableconstraint();
    antlr4::tree::TerminalNode *CONSTRAINT();
    NameContext *name();
    ConstraintattributespecContext *constraintattributespec();
    antlr4::tree::TerminalNode *VALIDATE();
    antlr4::tree::TerminalNode *WITHOUT();
    antlr4::tree::TerminalNode *OIDS();
    antlr4::tree::TerminalNode *CLUSTER();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *LOGGED();
    antlr4::tree::TerminalNode *UNLOGGED();
    antlr4::tree::TerminalNode *ENABLE_P();
    antlr4::tree::TerminalNode *TRIGGER();
    antlr4::tree::TerminalNode *ALWAYS();
    antlr4::tree::TerminalNode *REPLICA();
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *USER();
    antlr4::tree::TerminalNode *DISABLE_P();
    antlr4::tree::TerminalNode *RULE();
    antlr4::tree::TerminalNode *INHERIT();
    Qualified_nameContext *qualified_name();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *OF();
    Any_nameContext *any_name();
    antlr4::tree::TerminalNode *OWNER();
    antlr4::tree::TerminalNode *TO();
    RolespecContext *rolespec();
    antlr4::tree::TerminalNode *TABLESPACE();
    Replica_identityContext *replica_identity();
    antlr4::tree::TerminalNode *ROW();
    antlr4::tree::TerminalNode *LEVEL();
    antlr4::tree::TerminalNode *SECURITY();
    antlr4::tree::TerminalNode *FORCE();

   
  };

  Alter_table_cmdContext* alter_table_cmd();

  class  Alter_column_defaultContext : public antlr4::ParserRuleContext {
  public:
    Alter_column_defaultContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SET();
    antlr4::tree::TerminalNode *DEFAULT();
    A_exprContext *a_expr();
    antlr4::tree::TerminalNode *DROP();

   
  };

  Alter_column_defaultContext* alter_column_default();

  class  Drop_behavior_Context : public antlr4::ParserRuleContext {
  public:
    Drop_behavior_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CASCADE();
    antlr4::tree::TerminalNode *RESTRICT();

   
  };

  Drop_behavior_Context* drop_behavior_();

  class  Collate_clause_Context : public antlr4::ParserRuleContext {
  public:
    Collate_clause_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *COLLATE();
    Any_nameContext *any_name();

   
  };

  Collate_clause_Context* collate_clause_();

  class  Alter_usingContext : public antlr4::ParserRuleContext {
  public:
    Alter_usingContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *USING();
    A_exprContext *a_expr();

   
  };

  Alter_usingContext* alter_using();

  class  Replica_identityContext : public antlr4::ParserRuleContext {
  public:
    Replica_identityContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NOTHING();
    antlr4::tree::TerminalNode *FULL();
    antlr4::tree::TerminalNode *DEFAULT();
    antlr4::tree::TerminalNode *USING();
    antlr4::tree::TerminalNode *INDEX();
    NameContext *name();

   
  };

  Replica_identityContext* replica_identity();

  class  ReloptionsContext : public antlr4::ParserRuleContext {
  public:
    ReloptionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Reloption_listContext *reloption_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  ReloptionsContext* reloptions();

  class  Reloptions_Context : public antlr4::ParserRuleContext {
  public:
    Reloptions_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WITH();
    ReloptionsContext *reloptions();

   
  };

  Reloptions_Context* reloptions_();

  class  Reloption_listContext : public antlr4::ParserRuleContext {
  public:
    Reloption_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Reloption_elemContext *> reloption_elem();
    Reloption_elemContext* reloption_elem(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Reloption_listContext* reloption_list();

  class  Reloption_elemContext : public antlr4::ParserRuleContext {
  public:
    Reloption_elemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<ColLabelContext *> colLabel();
    ColLabelContext* colLabel(size_t i);
    antlr4::tree::TerminalNode *EQUAL();
    Def_argContext *def_arg();
    antlr4::tree::TerminalNode *DOT();

   
  };

  Reloption_elemContext* reloption_elem();

  class  Alter_identity_column_option_listContext : public antlr4::ParserRuleContext {
  public:
    Alter_identity_column_option_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Alter_identity_column_optionContext *> alter_identity_column_option();
    Alter_identity_column_optionContext* alter_identity_column_option(size_t i);

   
  };

  Alter_identity_column_option_listContext* alter_identity_column_option_list();

  class  Alter_identity_column_optionContext : public antlr4::ParserRuleContext {
  public:
    Alter_identity_column_optionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RESTART();
    NumericonlyContext *numericonly();
    With_Context *with_();
    antlr4::tree::TerminalNode *SET();
    SeqoptelemContext *seqoptelem();
    antlr4::tree::TerminalNode *GENERATED();
    Generated_whenContext *generated_when();

   
  };

  Alter_identity_column_optionContext* alter_identity_column_option();

  class  PartitionboundspecContext : public antlr4::ParserRuleContext {
  public:
    PartitionboundspecContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *VALUES();
    antlr4::tree::TerminalNode *WITH();
    std::vector<antlr4::tree::TerminalNode *> OPEN_PAREN();
    antlr4::tree::TerminalNode* OPEN_PAREN(size_t i);
    Hash_partboundContext *hash_partbound();
    std::vector<antlr4::tree::TerminalNode *> CLOSE_PAREN();
    antlr4::tree::TerminalNode* CLOSE_PAREN(size_t i);
    antlr4::tree::TerminalNode *IN_P();
    std::vector<Expr_listContext *> expr_list();
    Expr_listContext* expr_list(size_t i);
    antlr4::tree::TerminalNode *FROM();
    antlr4::tree::TerminalNode *TO();
    antlr4::tree::TerminalNode *DEFAULT();

   
  };

  PartitionboundspecContext* partitionboundspec();

  class  Hash_partbound_elemContext : public antlr4::ParserRuleContext {
  public:
    Hash_partbound_elemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    NonreservedwordContext *nonreservedword();
    IconstContext *iconst();

   
  };

  Hash_partbound_elemContext* hash_partbound_elem();

  class  Hash_partboundContext : public antlr4::ParserRuleContext {
  public:
    Hash_partboundContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Hash_partbound_elemContext *> hash_partbound_elem();
    Hash_partbound_elemContext* hash_partbound_elem(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Hash_partboundContext* hash_partbound();

  class  AltercompositetypestmtContext : public antlr4::ParserRuleContext {
  public:
    AltercompositetypestmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *TYPE_P();
    Any_nameContext *any_name();
    Alter_type_cmdsContext *alter_type_cmds();

   
  };

  AltercompositetypestmtContext* altercompositetypestmt();

  class  Alter_type_cmdsContext : public antlr4::ParserRuleContext {
  public:
    Alter_type_cmdsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Alter_type_cmdContext *> alter_type_cmd();
    Alter_type_cmdContext* alter_type_cmd(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Alter_type_cmdsContext* alter_type_cmds();

  class  Alter_type_cmdContext : public antlr4::ParserRuleContext {
  public:
    Alter_type_cmdContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ADD_P();
    antlr4::tree::TerminalNode *ATTRIBUTE();
    TablefuncelementContext *tablefuncelement();
    Drop_behavior_Context *drop_behavior_();
    antlr4::tree::TerminalNode *DROP();
    ColidContext *colid();
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *EXISTS();
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *TYPE_P();
    TypenameContext *typename_();
    Set_data_Context *set_data_();
    Collate_clause_Context *collate_clause_();

   
  };

  Alter_type_cmdContext* alter_type_cmd();

  class  CloseportalstmtContext : public antlr4::ParserRuleContext {
  public:
    CloseportalstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CLOSE();
    Cursor_nameContext *cursor_name();
    antlr4::tree::TerminalNode *ALL();

   
  };

  CloseportalstmtContext* closeportalstmt();

  class  CopystmtContext : public antlr4::ParserRuleContext {
  public:
    CopystmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *COPY();
    Qualified_nameContext *qualified_name();
    Copy_fromContext *copy_from();
    Copy_file_nameContext *copy_file_name();
    Copy_optionsContext *copy_options();
    Binary_Context *binary_();
    Column_list_Context *column_list_();
    Program_Context *program_();
    Copy_delimiterContext *copy_delimiter();
    With_Context *with_();
    Where_clauseContext *where_clause();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    PreparablestmtContext *preparablestmt();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    antlr4::tree::TerminalNode *TO();

   
  };

  CopystmtContext* copystmt();

  class  Copy_fromContext : public antlr4::ParserRuleContext {
  public:
    Copy_fromContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FROM();
    antlr4::tree::TerminalNode *TO();

   
  };

  Copy_fromContext* copy_from();

  class  Program_Context : public antlr4::ParserRuleContext {
  public:
    Program_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PROGRAM();

   
  };

  Program_Context* program_();

  class  Copy_file_nameContext : public antlr4::ParserRuleContext {
  public:
    Copy_file_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    SconstContext *sconst();
    antlr4::tree::TerminalNode *STDIN();
    antlr4::tree::TerminalNode *STDOUT();

   
  };

  Copy_file_nameContext* copy_file_name();

  class  Copy_optionsContext : public antlr4::ParserRuleContext {
  public:
    Copy_optionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Copy_opt_listContext *copy_opt_list();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Copy_generic_opt_listContext *copy_generic_opt_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Copy_optionsContext* copy_options();

  class  Copy_opt_listContext : public antlr4::ParserRuleContext {
  public:
    Copy_opt_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Copy_opt_itemContext *> copy_opt_item();
    Copy_opt_itemContext* copy_opt_item(size_t i);

   
  };

  Copy_opt_listContext* copy_opt_list();

  class  Copy_opt_itemContext : public antlr4::ParserRuleContext {
  public:
    Copy_opt_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BINARY();
    antlr4::tree::TerminalNode *FREEZE();
    antlr4::tree::TerminalNode *DELIMITER();
    SconstContext *sconst();
    As_Context *as_();
    antlr4::tree::TerminalNode *NULL_P();
    antlr4::tree::TerminalNode *CSV();
    antlr4::tree::TerminalNode *HEADER_P();
    antlr4::tree::TerminalNode *QUOTE();
    antlr4::tree::TerminalNode *ESCAPE();
    antlr4::tree::TerminalNode *FORCE();
    ColumnlistContext *columnlist();
    antlr4::tree::TerminalNode *STAR();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *ENCODING();

   
  };

  Copy_opt_itemContext* copy_opt_item();

  class  Binary_Context : public antlr4::ParserRuleContext {
  public:
    Binary_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BINARY();

   
  };

  Binary_Context* binary_();

  class  Copy_delimiterContext : public antlr4::ParserRuleContext {
  public:
    Copy_delimiterContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DELIMITERS();
    SconstContext *sconst();
    Using_Context *using_();

   
  };

  Copy_delimiterContext* copy_delimiter();

  class  Using_Context : public antlr4::ParserRuleContext {
  public:
    Using_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *USING();

   
  };

  Using_Context* using_();

  class  Copy_generic_opt_listContext : public antlr4::ParserRuleContext {
  public:
    Copy_generic_opt_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Copy_generic_opt_elemContext *> copy_generic_opt_elem();
    Copy_generic_opt_elemContext* copy_generic_opt_elem(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Copy_generic_opt_listContext* copy_generic_opt_list();

  class  Copy_generic_opt_elemContext : public antlr4::ParserRuleContext {
  public:
    Copy_generic_opt_elemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ColLabelContext *colLabel();
    Copy_generic_opt_argContext *copy_generic_opt_arg();

   
  };

  Copy_generic_opt_elemContext* copy_generic_opt_elem();

  class  Copy_generic_opt_argContext : public antlr4::ParserRuleContext {
  public:
    Copy_generic_opt_argContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Boolean_or_string_Context *boolean_or_string_();
    NumericonlyContext *numericonly();
    antlr4::tree::TerminalNode *STAR();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Copy_generic_opt_arg_listContext *copy_generic_opt_arg_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Copy_generic_opt_argContext* copy_generic_opt_arg();

  class  Copy_generic_opt_arg_listContext : public antlr4::ParserRuleContext {
  public:
    Copy_generic_opt_arg_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Copy_generic_opt_arg_list_itemContext *> copy_generic_opt_arg_list_item();
    Copy_generic_opt_arg_list_itemContext* copy_generic_opt_arg_list_item(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Copy_generic_opt_arg_listContext* copy_generic_opt_arg_list();

  class  Copy_generic_opt_arg_list_itemContext : public antlr4::ParserRuleContext {
  public:
    Copy_generic_opt_arg_list_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Boolean_or_string_Context *boolean_or_string_();

   
  };

  Copy_generic_opt_arg_list_itemContext* copy_generic_opt_arg_list_item();

  class  CreatestmtContext : public antlr4::ParserRuleContext {
  public:
    CreatestmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *TABLE();
    std::vector<Qualified_nameContext *> qualified_name();
    Qualified_nameContext* qualified_name(size_t i);
    antlr4::tree::TerminalNode *OPEN_PAREN();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    antlr4::tree::TerminalNode *OF();
    Any_nameContext *any_name();
    antlr4::tree::TerminalNode *PARTITION();
    PartitionboundspecContext *partitionboundspec();
    OpttempContext *opttemp();
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *EXISTS();
    OpttableelementlistContext *opttableelementlist();
    OptinheritContext *optinherit();
    OptpartitionspecContext *optpartitionspec();
    Table_access_method_clauseContext *table_access_method_clause();
    OptwithContext *optwith();
    OncommitoptionContext *oncommitoption();
    OpttablespaceContext *opttablespace();
    OpttypedtableelementlistContext *opttypedtableelementlist();

   
  };

  CreatestmtContext* createstmt();

  class  OpttempContext : public antlr4::ParserRuleContext {
  public:
    OpttempContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TEMPORARY();
    antlr4::tree::TerminalNode *TEMP();
    antlr4::tree::TerminalNode *LOCAL();
    antlr4::tree::TerminalNode *GLOBAL();
    antlr4::tree::TerminalNode *UNLOGGED();

   
  };

  OpttempContext* opttemp();

  class  OpttableelementlistContext : public antlr4::ParserRuleContext {
  public:
    OpttableelementlistContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TableelementlistContext *tableelementlist();

   
  };

  OpttableelementlistContext* opttableelementlist();

  class  OpttypedtableelementlistContext : public antlr4::ParserRuleContext {
  public:
    OpttypedtableelementlistContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OPEN_PAREN();
    TypedtableelementlistContext *typedtableelementlist();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  OpttypedtableelementlistContext* opttypedtableelementlist();

  class  TableelementlistContext : public antlr4::ParserRuleContext {
  public:
    TableelementlistContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<TableelementContext *> tableelement();
    TableelementContext* tableelement(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  TableelementlistContext* tableelementlist();

  class  TypedtableelementlistContext : public antlr4::ParserRuleContext {
  public:
    TypedtableelementlistContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<TypedtableelementContext *> typedtableelement();
    TypedtableelementContext* typedtableelement(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  TypedtableelementlistContext* typedtableelementlist();

  class  TableelementContext : public antlr4::ParserRuleContext {
  public:
    TableelementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TableconstraintContext *tableconstraint();
    TablelikeclauseContext *tablelikeclause();
    ColumnDefContext *columnDef();

   
  };

  TableelementContext* tableelement();

  class  TypedtableelementContext : public antlr4::ParserRuleContext {
  public:
    TypedtableelementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ColumnOptionsContext *columnOptions();
    TableconstraintContext *tableconstraint();

   
  };

  TypedtableelementContext* typedtableelement();

  class  ColumnDefContext : public antlr4::ParserRuleContext {
  public:
    ColumnDefContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ColidContext *colid();
    TypenameContext *typename_();
    ColquallistContext *colquallist();
    Create_generic_optionsContext *create_generic_options();

   
  };

  ColumnDefContext* columnDef();

  class  ColumnOptionsContext : public antlr4::ParserRuleContext {
  public:
    ColumnOptionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ColidContext *colid();
    ColquallistContext *colquallist();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *OPTIONS();

   
  };

  ColumnOptionsContext* columnOptions();

  class  ColquallistContext : public antlr4::ParserRuleContext {
  public:
    ColquallistContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<ColconstraintContext *> colconstraint();
    ColconstraintContext* colconstraint(size_t i);

   
  };

  ColquallistContext* colquallist();

  class  ColconstraintContext : public antlr4::ParserRuleContext {
  public:
    ColconstraintContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CONSTRAINT();
    NameContext *name();
    ColconstraintelemContext *colconstraintelem();
    ConstraintattrContext *constraintattr();
    antlr4::tree::TerminalNode *COLLATE();
    Any_nameContext *any_name();

   
  };

  ColconstraintContext* colconstraint();

  class  ColconstraintelemContext : public antlr4::ParserRuleContext {
  public:
    ColconstraintelemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *NULL_P();
    antlr4::tree::TerminalNode *UNIQUE();
    Definition_Context *definition_();
    OptconstablespaceContext *optconstablespace();
    antlr4::tree::TerminalNode *PRIMARY();
    antlr4::tree::TerminalNode *KEY();
    antlr4::tree::TerminalNode *CHECK();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    A_exprContext *a_expr();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    No_inherit_Context *no_inherit_();
    antlr4::tree::TerminalNode *DEFAULT();
    B_exprContext *b_expr();
    antlr4::tree::TerminalNode *GENERATED();
    Generated_whenContext *generated_when();
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *IDENTITY_P();
    antlr4::tree::TerminalNode *STORED();
    OptparenthesizedseqoptlistContext *optparenthesizedseqoptlist();
    antlr4::tree::TerminalNode *REFERENCES();
    Qualified_nameContext *qualified_name();
    Column_list_Context *column_list_();
    Key_matchContext *key_match();
    Key_actionsContext *key_actions();

   
  };

  ColconstraintelemContext* colconstraintelem();

  class  Generated_whenContext : public antlr4::ParserRuleContext {
  public:
    Generated_whenContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALWAYS();
    antlr4::tree::TerminalNode *BY();
    antlr4::tree::TerminalNode *DEFAULT();

   
  };

  Generated_whenContext* generated_when();

  class  ConstraintattrContext : public antlr4::ParserRuleContext {
  public:
    ConstraintattrContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DEFERRABLE();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *INITIALLY();
    antlr4::tree::TerminalNode *DEFERRED();
    antlr4::tree::TerminalNode *IMMEDIATE();

   
  };

  ConstraintattrContext* constraintattr();

  class  TablelikeclauseContext : public antlr4::ParserRuleContext {
  public:
    TablelikeclauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LIKE();
    Qualified_nameContext *qualified_name();
    TablelikeoptionlistContext *tablelikeoptionlist();

   
  };

  TablelikeclauseContext* tablelikeclause();

  class  TablelikeoptionlistContext : public antlr4::ParserRuleContext {
  public:
    TablelikeoptionlistContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<TablelikeoptionContext *> tablelikeoption();
    TablelikeoptionContext* tablelikeoption(size_t i);
    std::vector<antlr4::tree::TerminalNode *> INCLUDING();
    antlr4::tree::TerminalNode* INCLUDING(size_t i);
    std::vector<antlr4::tree::TerminalNode *> EXCLUDING();
    antlr4::tree::TerminalNode* EXCLUDING(size_t i);

   
  };

  TablelikeoptionlistContext* tablelikeoptionlist();

  class  TablelikeoptionContext : public antlr4::ParserRuleContext {
  public:
    TablelikeoptionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *COMMENTS();
    antlr4::tree::TerminalNode *CONSTRAINTS();
    antlr4::tree::TerminalNode *DEFAULTS();
    antlr4::tree::TerminalNode *IDENTITY_P();
    antlr4::tree::TerminalNode *GENERATED();
    antlr4::tree::TerminalNode *INDEXES();
    antlr4::tree::TerminalNode *STATISTICS();
    antlr4::tree::TerminalNode *STORAGE();
    antlr4::tree::TerminalNode *ALL();

   
  };

  TablelikeoptionContext* tablelikeoption();

  class  TableconstraintContext : public antlr4::ParserRuleContext {
  public:
    TableconstraintContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CONSTRAINT();
    NameContext *name();
    ConstraintelemContext *constraintelem();

   
  };

  TableconstraintContext* tableconstraint();

  class  ConstraintelemContext : public antlr4::ParserRuleContext {
  public:
    ConstraintelemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CHECK();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    A_exprContext *a_expr();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    ConstraintattributespecContext *constraintattributespec();
    antlr4::tree::TerminalNode *UNIQUE();
    ColumnlistContext *columnlist();
    ExistingindexContext *existingindex();
    C_include_Context *c_include_();
    Definition_Context *definition_();
    OptconstablespaceContext *optconstablespace();
    antlr4::tree::TerminalNode *PRIMARY();
    antlr4::tree::TerminalNode *KEY();
    antlr4::tree::TerminalNode *EXCLUDE();
    ExclusionconstraintlistContext *exclusionconstraintlist();
    Access_method_clauseContext *access_method_clause();
    ExclusionwhereclauseContext *exclusionwhereclause();
    antlr4::tree::TerminalNode *FOREIGN();
    antlr4::tree::TerminalNode *REFERENCES();
    Qualified_nameContext *qualified_name();
    Column_list_Context *column_list_();
    Key_matchContext *key_match();
    Key_actionsContext *key_actions();

   
  };

  ConstraintelemContext* constraintelem();

  class  No_inherit_Context : public antlr4::ParserRuleContext {
  public:
    No_inherit_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *INHERIT();

   
  };

  No_inherit_Context* no_inherit_();

  class  Column_list_Context : public antlr4::ParserRuleContext {
  public:
    Column_list_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OPEN_PAREN();
    ColumnlistContext *columnlist();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Column_list_Context* column_list_();

  class  ColumnlistContext : public antlr4::ParserRuleContext {
  public:
    ColumnlistContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<ColumnElemContext *> columnElem();
    ColumnElemContext* columnElem(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  ColumnlistContext* columnlist();

  class  ColumnElemContext : public antlr4::ParserRuleContext {
  public:
    ColumnElemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ColidContext *colid();

   
  };

  ColumnElemContext* columnElem();

  class  C_include_Context : public antlr4::ParserRuleContext {
  public:
    C_include_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INCLUDE();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    ColumnlistContext *columnlist();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  C_include_Context* c_include_();

  class  Key_matchContext : public antlr4::ParserRuleContext {
  public:
    Key_matchContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *MATCH();
    antlr4::tree::TerminalNode *FULL();
    antlr4::tree::TerminalNode *PARTIAL();
    antlr4::tree::TerminalNode *SIMPLE();

   
  };

  Key_matchContext* key_match();

  class  ExclusionconstraintlistContext : public antlr4::ParserRuleContext {
  public:
    ExclusionconstraintlistContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<ExclusionconstraintelemContext *> exclusionconstraintelem();
    ExclusionconstraintelemContext* exclusionconstraintelem(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  ExclusionconstraintlistContext* exclusionconstraintlist();

  class  ExclusionconstraintelemContext : public antlr4::ParserRuleContext {
  public:
    ExclusionconstraintelemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Index_elemContext *index_elem();
    antlr4::tree::TerminalNode *WITH();
    Any_operatorContext *any_operator();
    antlr4::tree::TerminalNode *OPERATOR();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  ExclusionconstraintelemContext* exclusionconstraintelem();

  class  ExclusionwhereclauseContext : public antlr4::ParserRuleContext {
  public:
    ExclusionwhereclauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WHERE();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    A_exprContext *a_expr();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  ExclusionwhereclauseContext* exclusionwhereclause();

  class  Key_actionsContext : public antlr4::ParserRuleContext {
  public:
    Key_actionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Key_updateContext *key_update();
    Key_deleteContext *key_delete();

   
  };

  Key_actionsContext* key_actions();

  class  Key_updateContext : public antlr4::ParserRuleContext {
  public:
    Key_updateContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *UPDATE();
    Key_actionContext *key_action();

   
  };

  Key_updateContext* key_update();

  class  Key_deleteContext : public antlr4::ParserRuleContext {
  public:
    Key_deleteContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *DELETE_P();
    Key_actionContext *key_action();

   
  };

  Key_deleteContext* key_delete();

  class  Key_actionContext : public antlr4::ParserRuleContext {
  public:
    Key_actionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *ACTION();
    antlr4::tree::TerminalNode *RESTRICT();
    antlr4::tree::TerminalNode *CASCADE();
    antlr4::tree::TerminalNode *SET();
    antlr4::tree::TerminalNode *NULL_P();
    antlr4::tree::TerminalNode *DEFAULT();

   
  };

  Key_actionContext* key_action();

  class  OptinheritContext : public antlr4::ParserRuleContext {
  public:
    OptinheritContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INHERITS();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Qualified_name_listContext *qualified_name_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  OptinheritContext* optinherit();

  class  OptpartitionspecContext : public antlr4::ParserRuleContext {
  public:
    OptpartitionspecContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    PartitionspecContext *partitionspec();

   
  };

  OptpartitionspecContext* optpartitionspec();

  class  PartitionspecContext : public antlr4::ParserRuleContext {
  public:
    PartitionspecContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PARTITION();
    antlr4::tree::TerminalNode *BY();
    ColidContext *colid();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Part_paramsContext *part_params();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  PartitionspecContext* partitionspec();

  class  Part_paramsContext : public antlr4::ParserRuleContext {
  public:
    Part_paramsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Part_elemContext *> part_elem();
    Part_elemContext* part_elem(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Part_paramsContext* part_params();

  class  Part_elemContext : public antlr4::ParserRuleContext {
  public:
    Part_elemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ColidContext *colid();
    Collate_Context *collate_();
    Class_Context *class_();
    Func_expr_windowlessContext *func_expr_windowless();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    A_exprContext *a_expr();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Part_elemContext* part_elem();

  class  Table_access_method_clauseContext : public antlr4::ParserRuleContext {
  public:
    Table_access_method_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *USING();
    NameContext *name();

   
  };

  Table_access_method_clauseContext* table_access_method_clause();

  class  OptwithContext : public antlr4::ParserRuleContext {
  public:
    OptwithContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WITH();
    ReloptionsContext *reloptions();
    antlr4::tree::TerminalNode *WITHOUT();
    antlr4::tree::TerminalNode *OIDS();

   
  };

  OptwithContext* optwith();

  class  OncommitoptionContext : public antlr4::ParserRuleContext {
  public:
    OncommitoptionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *COMMIT();
    antlr4::tree::TerminalNode *DROP();
    antlr4::tree::TerminalNode *DELETE_P();
    antlr4::tree::TerminalNode *ROWS();
    antlr4::tree::TerminalNode *PRESERVE();

   
  };

  OncommitoptionContext* oncommitoption();

  class  OpttablespaceContext : public antlr4::ParserRuleContext {
  public:
    OpttablespaceContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TABLESPACE();
    NameContext *name();

   
  };

  OpttablespaceContext* opttablespace();

  class  OptconstablespaceContext : public antlr4::ParserRuleContext {
  public:
    OptconstablespaceContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *USING();
    antlr4::tree::TerminalNode *INDEX();
    antlr4::tree::TerminalNode *TABLESPACE();
    NameContext *name();

   
  };

  OptconstablespaceContext* optconstablespace();

  class  ExistingindexContext : public antlr4::ParserRuleContext {
  public:
    ExistingindexContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *USING();
    antlr4::tree::TerminalNode *INDEX();
    NameContext *name();

   
  };

  ExistingindexContext* existingindex();

  class  CreatestatsstmtContext : public antlr4::ParserRuleContext {
  public:
    CreatestatsstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *STATISTICS();
    Any_nameContext *any_name();
    antlr4::tree::TerminalNode *ON();
    Expr_listContext *expr_list();
    antlr4::tree::TerminalNode *FROM();
    From_listContext *from_list();
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *EXISTS();
    Name_list_Context *name_list_();

   
  };

  CreatestatsstmtContext* createstatsstmt();

  class  AlterstatsstmtContext : public antlr4::ParserRuleContext {
  public:
    AlterstatsstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    std::vector<antlr4::tree::TerminalNode *> STATISTICS();
    antlr4::tree::TerminalNode* STATISTICS(size_t i);
    Any_nameContext *any_name();
    antlr4::tree::TerminalNode *SET();
    SignediconstContext *signediconst();
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *EXISTS();

   
  };

  AlterstatsstmtContext* alterstatsstmt();

  class  CreateasstmtContext : public antlr4::ParserRuleContext {
  public:
    CreateasstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *TABLE();
    Create_as_targetContext *create_as_target();
    antlr4::tree::TerminalNode *AS();
    SelectstmtContext *selectstmt();
    OpttempContext *opttemp();
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *EXISTS();
    With_data_Context *with_data_();

   
  };

  CreateasstmtContext* createasstmt();

  class  Create_as_targetContext : public antlr4::ParserRuleContext {
  public:
    Create_as_targetContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Qualified_nameContext *qualified_name();
    Column_list_Context *column_list_();
    Table_access_method_clauseContext *table_access_method_clause();
    OptwithContext *optwith();
    OncommitoptionContext *oncommitoption();
    OpttablespaceContext *opttablespace();

   
  };

  Create_as_targetContext* create_as_target();

  class  With_data_Context : public antlr4::ParserRuleContext {
  public:
    With_data_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *DATA_P();
    antlr4::tree::TerminalNode *NO();

   
  };

  With_data_Context* with_data_();

  class  CreatematviewstmtContext : public antlr4::ParserRuleContext {
  public:
    CreatematviewstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *MATERIALIZED();
    antlr4::tree::TerminalNode *VIEW();
    Create_mv_targetContext *create_mv_target();
    antlr4::tree::TerminalNode *AS();
    SelectstmtContext *selectstmt();
    OptnologContext *optnolog();
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *EXISTS();
    With_data_Context *with_data_();

   
  };

  CreatematviewstmtContext* creatematviewstmt();

  class  Create_mv_targetContext : public antlr4::ParserRuleContext {
  public:
    Create_mv_targetContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Qualified_nameContext *qualified_name();
    Column_list_Context *column_list_();
    Table_access_method_clauseContext *table_access_method_clause();
    Reloptions_Context *reloptions_();
    OpttablespaceContext *opttablespace();

   
  };

  Create_mv_targetContext* create_mv_target();

  class  OptnologContext : public antlr4::ParserRuleContext {
  public:
    OptnologContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *UNLOGGED();

   
  };

  OptnologContext* optnolog();

  class  RefreshmatviewstmtContext : public antlr4::ParserRuleContext {
  public:
    RefreshmatviewstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REFRESH();
    antlr4::tree::TerminalNode *MATERIALIZED();
    antlr4::tree::TerminalNode *VIEW();
    Qualified_nameContext *qualified_name();
    Concurrently_Context *concurrently_();
    With_data_Context *with_data_();

   
  };

  RefreshmatviewstmtContext* refreshmatviewstmt();

  class  CreateseqstmtContext : public antlr4::ParserRuleContext {
  public:
    CreateseqstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *SEQUENCE();
    Qualified_nameContext *qualified_name();
    OpttempContext *opttemp();
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *EXISTS();
    OptseqoptlistContext *optseqoptlist();

   
  };

  CreateseqstmtContext* createseqstmt();

  class  AlterseqstmtContext : public antlr4::ParserRuleContext {
  public:
    AlterseqstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *SEQUENCE();
    Qualified_nameContext *qualified_name();
    SeqoptlistContext *seqoptlist();
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *EXISTS();

   
  };

  AlterseqstmtContext* alterseqstmt();

  class  OptseqoptlistContext : public antlr4::ParserRuleContext {
  public:
    OptseqoptlistContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    SeqoptlistContext *seqoptlist();

   
  };

  OptseqoptlistContext* optseqoptlist();

  class  OptparenthesizedseqoptlistContext : public antlr4::ParserRuleContext {
  public:
    OptparenthesizedseqoptlistContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OPEN_PAREN();
    SeqoptlistContext *seqoptlist();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  OptparenthesizedseqoptlistContext* optparenthesizedseqoptlist();

  class  SeqoptlistContext : public antlr4::ParserRuleContext {
  public:
    SeqoptlistContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<SeqoptelemContext *> seqoptelem();
    SeqoptelemContext* seqoptelem(size_t i);

   
  };

  SeqoptlistContext* seqoptlist();

  class  SeqoptelemContext : public antlr4::ParserRuleContext {
  public:
    SeqoptelemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *AS();
    SimpletypenameContext *simpletypename();
    antlr4::tree::TerminalNode *CACHE();
    NumericonlyContext *numericonly();
    antlr4::tree::TerminalNode *CYCLE();
    antlr4::tree::TerminalNode *INCREMENT();
    By_Context *by_();
    antlr4::tree::TerminalNode *MAXVALUE();
    antlr4::tree::TerminalNode *MINVALUE();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *OWNED();
    antlr4::tree::TerminalNode *BY();
    Any_nameContext *any_name();
    antlr4::tree::TerminalNode *SEQUENCE();
    antlr4::tree::TerminalNode *NAME_P();
    antlr4::tree::TerminalNode *START();
    With_Context *with_();
    antlr4::tree::TerminalNode *RESTART();

   
  };

  SeqoptelemContext* seqoptelem();

  class  By_Context : public antlr4::ParserRuleContext {
  public:
    By_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BY();

   
  };

  By_Context* by_();

  class  NumericonlyContext : public antlr4::ParserRuleContext {
  public:
    NumericonlyContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    FconstContext *fconst();
    antlr4::tree::TerminalNode *PLUS();
    antlr4::tree::TerminalNode *MINUS();
    SignediconstContext *signediconst();

   
  };

  NumericonlyContext* numericonly();

  class  Numericonly_listContext : public antlr4::ParserRuleContext {
  public:
    Numericonly_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<NumericonlyContext *> numericonly();
    NumericonlyContext* numericonly(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Numericonly_listContext* numericonly_list();

  class  CreateplangstmtContext : public antlr4::ParserRuleContext {
  public:
    CreateplangstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *LANGUAGE();
    NameContext *name();
    Or_replace_Context *or_replace_();
    Trusted_Context *trusted_();
    Procedural_Context *procedural_();
    antlr4::tree::TerminalNode *HANDLER();
    Handler_nameContext *handler_name();
    Inline_handler_Context *inline_handler_();
    Validator_Context *validator_();

   
  };

  CreateplangstmtContext* createplangstmt();

  class  Trusted_Context : public antlr4::ParserRuleContext {
  public:
    Trusted_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TRUSTED();

   
  };

  Trusted_Context* trusted_();

  class  Handler_nameContext : public antlr4::ParserRuleContext {
  public:
    Handler_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    NameContext *name();
    AttrsContext *attrs();

   
  };

  Handler_nameContext* handler_name();

  class  Inline_handler_Context : public antlr4::ParserRuleContext {
  public:
    Inline_handler_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INLINE_P();
    Handler_nameContext *handler_name();

   
  };

  Inline_handler_Context* inline_handler_();

  class  Validator_clauseContext : public antlr4::ParserRuleContext {
  public:
    Validator_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *VALIDATOR();
    Handler_nameContext *handler_name();
    antlr4::tree::TerminalNode *NO();

   
  };

  Validator_clauseContext* validator_clause();

  class  Validator_Context : public antlr4::ParserRuleContext {
  public:
    Validator_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Validator_clauseContext *validator_clause();

   
  };

  Validator_Context* validator_();

  class  Procedural_Context : public antlr4::ParserRuleContext {
  public:
    Procedural_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PROCEDURAL();

   
  };

  Procedural_Context* procedural_();

  class  CreatetablespacestmtContext : public antlr4::ParserRuleContext {
  public:
    CreatetablespacestmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *TABLESPACE();
    NameContext *name();
    antlr4::tree::TerminalNode *LOCATION();
    SconstContext *sconst();
    OpttablespaceownerContext *opttablespaceowner();
    Reloptions_Context *reloptions_();

   
  };

  CreatetablespacestmtContext* createtablespacestmt();

  class  OpttablespaceownerContext : public antlr4::ParserRuleContext {
  public:
    OpttablespaceownerContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OWNER();
    RolespecContext *rolespec();

   
  };

  OpttablespaceownerContext* opttablespaceowner();

  class  DroptablespacestmtContext : public antlr4::ParserRuleContext {
  public:
    DroptablespacestmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DROP();
    antlr4::tree::TerminalNode *TABLESPACE();
    NameContext *name();
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *EXISTS();

   
  };

  DroptablespacestmtContext* droptablespacestmt();

  class  CreateextensionstmtContext : public antlr4::ParserRuleContext {
  public:
    CreateextensionstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *EXTENSION();
    NameContext *name();
    Create_extension_opt_listContext *create_extension_opt_list();
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *EXISTS();
    With_Context *with_();

   
  };

  CreateextensionstmtContext* createextensionstmt();

  class  Create_extension_opt_listContext : public antlr4::ParserRuleContext {
  public:
    Create_extension_opt_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Create_extension_opt_itemContext *> create_extension_opt_item();
    Create_extension_opt_itemContext* create_extension_opt_item(size_t i);

   
  };

  Create_extension_opt_listContext* create_extension_opt_list();

  class  Create_extension_opt_itemContext : public antlr4::ParserRuleContext {
  public:
    Create_extension_opt_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SCHEMA();
    NameContext *name();
    antlr4::tree::TerminalNode *VERSION_P();
    Nonreservedword_or_sconstContext *nonreservedword_or_sconst();
    antlr4::tree::TerminalNode *FROM();
    antlr4::tree::TerminalNode *CASCADE();

   
  };

  Create_extension_opt_itemContext* create_extension_opt_item();

  class  AlterextensionstmtContext : public antlr4::ParserRuleContext {
  public:
    AlterextensionstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *EXTENSION();
    NameContext *name();
    antlr4::tree::TerminalNode *UPDATE();
    Alter_extension_opt_listContext *alter_extension_opt_list();

   
  };

  AlterextensionstmtContext* alterextensionstmt();

  class  Alter_extension_opt_listContext : public antlr4::ParserRuleContext {
  public:
    Alter_extension_opt_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Alter_extension_opt_itemContext *> alter_extension_opt_item();
    Alter_extension_opt_itemContext* alter_extension_opt_item(size_t i);

   
  };

  Alter_extension_opt_listContext* alter_extension_opt_list();

  class  Alter_extension_opt_itemContext : public antlr4::ParserRuleContext {
  public:
    Alter_extension_opt_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TO();
    Nonreservedword_or_sconstContext *nonreservedword_or_sconst();

   
  };

  Alter_extension_opt_itemContext* alter_extension_opt_item();

  class  AlterextensioncontentsstmtContext : public antlr4::ParserRuleContext {
  public:
    AlterextensioncontentsstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *EXTENSION();
    std::vector<NameContext *> name();
    NameContext* name(size_t i);
    Add_dropContext *add_drop();
    Object_type_nameContext *object_type_name();
    Object_type_any_nameContext *object_type_any_name();
    Any_nameContext *any_name();
    antlr4::tree::TerminalNode *AGGREGATE();
    Aggregate_with_argtypesContext *aggregate_with_argtypes();
    antlr4::tree::TerminalNode *CAST();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    std::vector<TypenameContext *> typename_();
    TypenameContext* typename_(size_t i);
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    antlr4::tree::TerminalNode *DOMAIN_P();
    antlr4::tree::TerminalNode *FUNCTION();
    Function_with_argtypesContext *function_with_argtypes();
    antlr4::tree::TerminalNode *OPERATOR();
    Operator_with_argtypesContext *operator_with_argtypes();
    antlr4::tree::TerminalNode *CLASS();
    antlr4::tree::TerminalNode *USING();
    antlr4::tree::TerminalNode *FAMILY();
    antlr4::tree::TerminalNode *PROCEDURE();
    antlr4::tree::TerminalNode *ROUTINE();
    antlr4::tree::TerminalNode *TRANSFORM();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *LANGUAGE();
    antlr4::tree::TerminalNode *TYPE_P();

   
  };

  AlterextensioncontentsstmtContext* alterextensioncontentsstmt();

  class  CreatefdwstmtContext : public antlr4::ParserRuleContext {
  public:
    CreatefdwstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *FOREIGN();
    antlr4::tree::TerminalNode *DATA_P();
    antlr4::tree::TerminalNode *WRAPPER();
    NameContext *name();
    Fdw_options_Context *fdw_options_();
    Create_generic_optionsContext *create_generic_options();

   
  };

  CreatefdwstmtContext* createfdwstmt();

  class  Fdw_optionContext : public antlr4::ParserRuleContext {
  public:
    Fdw_optionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *HANDLER();
    Handler_nameContext *handler_name();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *VALIDATOR();

   
  };

  Fdw_optionContext* fdw_option();

  class  Fdw_optionsContext : public antlr4::ParserRuleContext {
  public:
    Fdw_optionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Fdw_optionContext *> fdw_option();
    Fdw_optionContext* fdw_option(size_t i);

   
  };

  Fdw_optionsContext* fdw_options();

  class  Fdw_options_Context : public antlr4::ParserRuleContext {
  public:
    Fdw_options_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Fdw_optionsContext *fdw_options();

   
  };

  Fdw_options_Context* fdw_options_();

  class  AlterfdwstmtContext : public antlr4::ParserRuleContext {
  public:
    AlterfdwstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *FOREIGN();
    antlr4::tree::TerminalNode *DATA_P();
    antlr4::tree::TerminalNode *WRAPPER();
    NameContext *name();
    Alter_generic_optionsContext *alter_generic_options();
    Fdw_options_Context *fdw_options_();
    Fdw_optionsContext *fdw_options();

   
  };

  AlterfdwstmtContext* alterfdwstmt();

  class  Create_generic_optionsContext : public antlr4::ParserRuleContext {
  public:
    Create_generic_optionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OPTIONS();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Generic_option_listContext *generic_option_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Create_generic_optionsContext* create_generic_options();

  class  Generic_option_listContext : public antlr4::ParserRuleContext {
  public:
    Generic_option_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Generic_option_elemContext *> generic_option_elem();
    Generic_option_elemContext* generic_option_elem(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Generic_option_listContext* generic_option_list();

  class  Alter_generic_optionsContext : public antlr4::ParserRuleContext {
  public:
    Alter_generic_optionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OPTIONS();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Alter_generic_option_listContext *alter_generic_option_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Alter_generic_optionsContext* alter_generic_options();

  class  Alter_generic_option_listContext : public antlr4::ParserRuleContext {
  public:
    Alter_generic_option_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Alter_generic_option_elemContext *> alter_generic_option_elem();
    Alter_generic_option_elemContext* alter_generic_option_elem(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Alter_generic_option_listContext* alter_generic_option_list();

  class  Alter_generic_option_elemContext : public antlr4::ParserRuleContext {
  public:
    Alter_generic_option_elemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Generic_option_elemContext *generic_option_elem();
    antlr4::tree::TerminalNode *SET();
    antlr4::tree::TerminalNode *ADD_P();
    antlr4::tree::TerminalNode *DROP();
    Generic_option_nameContext *generic_option_name();

   
  };

  Alter_generic_option_elemContext* alter_generic_option_elem();

  class  Generic_option_elemContext : public antlr4::ParserRuleContext {
  public:
    Generic_option_elemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Generic_option_nameContext *generic_option_name();
    Generic_option_argContext *generic_option_arg();

   
  };

  Generic_option_elemContext* generic_option_elem();

  class  Generic_option_nameContext : public antlr4::ParserRuleContext {
  public:
    Generic_option_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ColLabelContext *colLabel();

   
  };

  Generic_option_nameContext* generic_option_name();

  class  Generic_option_argContext : public antlr4::ParserRuleContext {
  public:
    Generic_option_argContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    SconstContext *sconst();

   
  };

  Generic_option_argContext* generic_option_arg();

  class  CreateforeignserverstmtContext : public antlr4::ParserRuleContext {
  public:
    CreateforeignserverstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *SERVER();
    std::vector<NameContext *> name();
    NameContext* name(size_t i);
    antlr4::tree::TerminalNode *FOREIGN();
    antlr4::tree::TerminalNode *DATA_P();
    antlr4::tree::TerminalNode *WRAPPER();
    Type_Context *type_();
    Foreign_server_version_Context *foreign_server_version_();
    Create_generic_optionsContext *create_generic_options();
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *EXISTS();

   
  };

  CreateforeignserverstmtContext* createforeignserverstmt();

  class  Type_Context : public antlr4::ParserRuleContext {
  public:
    Type_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TYPE_P();
    SconstContext *sconst();

   
  };

  Type_Context* type_();

  class  Foreign_server_versionContext : public antlr4::ParserRuleContext {
  public:
    Foreign_server_versionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *VERSION_P();
    SconstContext *sconst();
    antlr4::tree::TerminalNode *NULL_P();

   
  };

  Foreign_server_versionContext* foreign_server_version();

  class  Foreign_server_version_Context : public antlr4::ParserRuleContext {
  public:
    Foreign_server_version_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Foreign_server_versionContext *foreign_server_version();

   
  };

  Foreign_server_version_Context* foreign_server_version_();

  class  AlterforeignserverstmtContext : public antlr4::ParserRuleContext {
  public:
    AlterforeignserverstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *SERVER();
    NameContext *name();
    Alter_generic_optionsContext *alter_generic_options();
    Foreign_server_versionContext *foreign_server_version();

   
  };

  AlterforeignserverstmtContext* alterforeignserverstmt();

  class  CreateforeigntablestmtContext : public antlr4::ParserRuleContext {
  public:
    CreateforeigntablestmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *FOREIGN();
    antlr4::tree::TerminalNode *TABLE();
    std::vector<Qualified_nameContext *> qualified_name();
    Qualified_nameContext* qualified_name(size_t i);
    antlr4::tree::TerminalNode *OPEN_PAREN();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    antlr4::tree::TerminalNode *SERVER();
    NameContext *name();
    OpttableelementlistContext *opttableelementlist();
    OptinheritContext *optinherit();
    Create_generic_optionsContext *create_generic_options();
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *EXISTS();
    antlr4::tree::TerminalNode *PARTITION();
    antlr4::tree::TerminalNode *OF();
    PartitionboundspecContext *partitionboundspec();
    OpttypedtableelementlistContext *opttypedtableelementlist();

   
  };

  CreateforeigntablestmtContext* createforeigntablestmt();

  class  ImportforeignschemastmtContext : public antlr4::ParserRuleContext {
  public:
    ImportforeignschemastmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *IMPORT_P();
    antlr4::tree::TerminalNode *FOREIGN();
    antlr4::tree::TerminalNode *SCHEMA();
    std::vector<NameContext *> name();
    NameContext* name(size_t i);
    antlr4::tree::TerminalNode *FROM();
    antlr4::tree::TerminalNode *SERVER();
    antlr4::tree::TerminalNode *INTO();
    Import_qualificationContext *import_qualification();
    Create_generic_optionsContext *create_generic_options();

   
  };

  ImportforeignschemastmtContext* importforeignschemastmt();

  class  Import_qualification_typeContext : public antlr4::ParserRuleContext {
  public:
    Import_qualification_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LIMIT();
    antlr4::tree::TerminalNode *TO();
    antlr4::tree::TerminalNode *EXCEPT();

   
  };

  Import_qualification_typeContext* import_qualification_type();

  class  Import_qualificationContext : public antlr4::ParserRuleContext {
  public:
    Import_qualificationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Import_qualification_typeContext *import_qualification_type();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Relation_expr_listContext *relation_expr_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Import_qualificationContext* import_qualification();

  class  CreateusermappingstmtContext : public antlr4::ParserRuleContext {
  public:
    CreateusermappingstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *USER();
    antlr4::tree::TerminalNode *MAPPING();
    antlr4::tree::TerminalNode *FOR();
    Auth_identContext *auth_ident();
    antlr4::tree::TerminalNode *SERVER();
    NameContext *name();
    Create_generic_optionsContext *create_generic_options();
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *EXISTS();

   
  };

  CreateusermappingstmtContext* createusermappingstmt();

  class  Auth_identContext : public antlr4::ParserRuleContext {
  public:
    Auth_identContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    RolespecContext *rolespec();
    antlr4::tree::TerminalNode *USER();

   
  };

  Auth_identContext* auth_ident();

  class  DropusermappingstmtContext : public antlr4::ParserRuleContext {
  public:
    DropusermappingstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DROP();
    antlr4::tree::TerminalNode *USER();
    antlr4::tree::TerminalNode *MAPPING();
    antlr4::tree::TerminalNode *FOR();
    Auth_identContext *auth_ident();
    antlr4::tree::TerminalNode *SERVER();
    NameContext *name();
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *EXISTS();

   
  };

  DropusermappingstmtContext* dropusermappingstmt();

  class  AlterusermappingstmtContext : public antlr4::ParserRuleContext {
  public:
    AlterusermappingstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *USER();
    antlr4::tree::TerminalNode *MAPPING();
    antlr4::tree::TerminalNode *FOR();
    Auth_identContext *auth_ident();
    antlr4::tree::TerminalNode *SERVER();
    NameContext *name();
    Alter_generic_optionsContext *alter_generic_options();

   
  };

  AlterusermappingstmtContext* alterusermappingstmt();

  class  CreatepolicystmtContext : public antlr4::ParserRuleContext {
  public:
    CreatepolicystmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *POLICY();
    NameContext *name();
    antlr4::tree::TerminalNode *ON();
    Qualified_nameContext *qualified_name();
    RowsecuritydefaultpermissiveContext *rowsecuritydefaultpermissive();
    RowsecuritydefaultforcmdContext *rowsecuritydefaultforcmd();
    RowsecuritydefaulttoroleContext *rowsecuritydefaulttorole();
    RowsecurityoptionalexprContext *rowsecurityoptionalexpr();
    RowsecurityoptionalwithcheckContext *rowsecurityoptionalwithcheck();

   
  };

  CreatepolicystmtContext* createpolicystmt();

  class  AlterpolicystmtContext : public antlr4::ParserRuleContext {
  public:
    AlterpolicystmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *POLICY();
    NameContext *name();
    antlr4::tree::TerminalNode *ON();
    Qualified_nameContext *qualified_name();
    RowsecurityoptionaltoroleContext *rowsecurityoptionaltorole();
    RowsecurityoptionalexprContext *rowsecurityoptionalexpr();
    RowsecurityoptionalwithcheckContext *rowsecurityoptionalwithcheck();

   
  };

  AlterpolicystmtContext* alterpolicystmt();

  class  RowsecurityoptionalexprContext : public antlr4::ParserRuleContext {
  public:
    RowsecurityoptionalexprContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *USING();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    A_exprContext *a_expr();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  RowsecurityoptionalexprContext* rowsecurityoptionalexpr();

  class  RowsecurityoptionalwithcheckContext : public antlr4::ParserRuleContext {
  public:
    RowsecurityoptionalwithcheckContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *CHECK();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    A_exprContext *a_expr();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  RowsecurityoptionalwithcheckContext* rowsecurityoptionalwithcheck();

  class  RowsecuritydefaulttoroleContext : public antlr4::ParserRuleContext {
  public:
    RowsecuritydefaulttoroleContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TO();
    Role_listContext *role_list();

   
  };

  RowsecuritydefaulttoroleContext* rowsecuritydefaulttorole();

  class  RowsecurityoptionaltoroleContext : public antlr4::ParserRuleContext {
  public:
    RowsecurityoptionaltoroleContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TO();
    Role_listContext *role_list();

   
  };

  RowsecurityoptionaltoroleContext* rowsecurityoptionaltorole();

  class  RowsecuritydefaultpermissiveContext : public antlr4::ParserRuleContext {
  public:
    RowsecuritydefaultpermissiveContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *AS();
    IdentifierContext *identifier();

   
  };

  RowsecuritydefaultpermissiveContext* rowsecuritydefaultpermissive();

  class  RowsecuritydefaultforcmdContext : public antlr4::ParserRuleContext {
  public:
    RowsecuritydefaultforcmdContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FOR();
    Row_security_cmdContext *row_security_cmd();

   
  };

  RowsecuritydefaultforcmdContext* rowsecuritydefaultforcmd();

  class  Row_security_cmdContext : public antlr4::ParserRuleContext {
  public:
    Row_security_cmdContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *SELECT();
    antlr4::tree::TerminalNode *INSERT();
    antlr4::tree::TerminalNode *UPDATE();
    antlr4::tree::TerminalNode *DELETE_P();

   
  };

  Row_security_cmdContext* row_security_cmd();

  class  CreateamstmtContext : public antlr4::ParserRuleContext {
  public:
    CreateamstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *ACCESS();
    antlr4::tree::TerminalNode *METHOD();
    NameContext *name();
    antlr4::tree::TerminalNode *TYPE_P();
    Am_typeContext *am_type();
    antlr4::tree::TerminalNode *HANDLER();
    Handler_nameContext *handler_name();

   
  };

  CreateamstmtContext* createamstmt();

  class  Am_typeContext : public antlr4::ParserRuleContext {
  public:
    Am_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INDEX();
    antlr4::tree::TerminalNode *TABLE();

   
  };

  Am_typeContext* am_type();

  class  CreatetrigstmtContext : public antlr4::ParserRuleContext {
  public:
    CreatetrigstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *TRIGGER();
    NameContext *name();
    TriggeractiontimeContext *triggeractiontime();
    TriggereventsContext *triggerevents();
    antlr4::tree::TerminalNode *ON();
    Qualified_nameContext *qualified_name();
    antlr4::tree::TerminalNode *EXECUTE();
    Function_or_procedureContext *function_or_procedure();
    Func_nameContext *func_name();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    TriggerfuncargsContext *triggerfuncargs();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    TriggerreferencingContext *triggerreferencing();
    TriggerforspecContext *triggerforspec();
    TriggerwhenContext *triggerwhen();
    antlr4::tree::TerminalNode *CONSTRAINT();
    antlr4::tree::TerminalNode *AFTER();
    ConstraintattributespecContext *constraintattributespec();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *EACH();
    antlr4::tree::TerminalNode *ROW();
    OptconstrfromtableContext *optconstrfromtable();

   
  };

  CreatetrigstmtContext* createtrigstmt();

  class  TriggeractiontimeContext : public antlr4::ParserRuleContext {
  public:
    TriggeractiontimeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BEFORE();
    antlr4::tree::TerminalNode *AFTER();
    antlr4::tree::TerminalNode *INSTEAD();
    antlr4::tree::TerminalNode *OF();

   
  };

  TriggeractiontimeContext* triggeractiontime();

  class  TriggereventsContext : public antlr4::ParserRuleContext {
  public:
    TriggereventsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<TriggeroneeventContext *> triggeroneevent();
    TriggeroneeventContext* triggeroneevent(size_t i);
    std::vector<antlr4::tree::TerminalNode *> OR();
    antlr4::tree::TerminalNode* OR(size_t i);

   
  };

  TriggereventsContext* triggerevents();

  class  TriggeroneeventContext : public antlr4::ParserRuleContext {
  public:
    TriggeroneeventContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INSERT();
    antlr4::tree::TerminalNode *DELETE_P();
    antlr4::tree::TerminalNode *UPDATE();
    antlr4::tree::TerminalNode *OF();
    ColumnlistContext *columnlist();
    antlr4::tree::TerminalNode *TRUNCATE();

   
  };

  TriggeroneeventContext* triggeroneevent();

  class  TriggerreferencingContext : public antlr4::ParserRuleContext {
  public:
    TriggerreferencingContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REFERENCING();
    TriggertransitionsContext *triggertransitions();

   
  };

  TriggerreferencingContext* triggerreferencing();

  class  TriggertransitionsContext : public antlr4::ParserRuleContext {
  public:
    TriggertransitionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<TriggertransitionContext *> triggertransition();
    TriggertransitionContext* triggertransition(size_t i);

   
  };

  TriggertransitionsContext* triggertransitions();

  class  TriggertransitionContext : public antlr4::ParserRuleContext {
  public:
    TriggertransitionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TransitionoldornewContext *transitionoldornew();
    TransitionrowortableContext *transitionrowortable();
    TransitionrelnameContext *transitionrelname();
    As_Context *as_();

   
  };

  TriggertransitionContext* triggertransition();

  class  TransitionoldornewContext : public antlr4::ParserRuleContext {
  public:
    TransitionoldornewContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NEW();
    antlr4::tree::TerminalNode *OLD();

   
  };

  TransitionoldornewContext* transitionoldornew();

  class  TransitionrowortableContext : public antlr4::ParserRuleContext {
  public:
    TransitionrowortableContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TABLE();
    antlr4::tree::TerminalNode *ROW();

   
  };

  TransitionrowortableContext* transitionrowortable();

  class  TransitionrelnameContext : public antlr4::ParserRuleContext {
  public:
    TransitionrelnameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ColidContext *colid();

   
  };

  TransitionrelnameContext* transitionrelname();

  class  TriggerforspecContext : public antlr4::ParserRuleContext {
  public:
    TriggerforspecContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FOR();
    TriggerfortypeContext *triggerfortype();
    TriggerforopteachContext *triggerforopteach();

   
  };

  TriggerforspecContext* triggerforspec();

  class  TriggerforopteachContext : public antlr4::ParserRuleContext {
  public:
    TriggerforopteachContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *EACH();

   
  };

  TriggerforopteachContext* triggerforopteach();

  class  TriggerfortypeContext : public antlr4::ParserRuleContext {
  public:
    TriggerfortypeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ROW();
    antlr4::tree::TerminalNode *STATEMENT();

   
  };

  TriggerfortypeContext* triggerfortype();

  class  TriggerwhenContext : public antlr4::ParserRuleContext {
  public:
    TriggerwhenContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WHEN();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    A_exprContext *a_expr();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  TriggerwhenContext* triggerwhen();

  class  Function_or_procedureContext : public antlr4::ParserRuleContext {
  public:
    Function_or_procedureContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FUNCTION();
    antlr4::tree::TerminalNode *PROCEDURE();

   
  };

  Function_or_procedureContext* function_or_procedure();

  class  TriggerfuncargsContext : public antlr4::ParserRuleContext {
  public:
    TriggerfuncargsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<TriggerfuncargContext *> triggerfuncarg();
    TriggerfuncargContext* triggerfuncarg(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  TriggerfuncargsContext* triggerfuncargs();

  class  TriggerfuncargContext : public antlr4::ParserRuleContext {
  public:
    TriggerfuncargContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IconstContext *iconst();
    FconstContext *fconst();
    SconstContext *sconst();
    ColLabelContext *colLabel();

   
  };

  TriggerfuncargContext* triggerfuncarg();

  class  OptconstrfromtableContext : public antlr4::ParserRuleContext {
  public:
    OptconstrfromtableContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FROM();
    Qualified_nameContext *qualified_name();

   
  };

  OptconstrfromtableContext* optconstrfromtable();

  class  ConstraintattributespecContext : public antlr4::ParserRuleContext {
  public:
    ConstraintattributespecContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<ConstraintattributeElemContext *> constraintattributeElem();
    ConstraintattributeElemContext* constraintattributeElem(size_t i);

   
  };

  ConstraintattributespecContext* constraintattributespec();

  class  ConstraintattributeElemContext : public antlr4::ParserRuleContext {
  public:
    ConstraintattributeElemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *DEFERRABLE();
    antlr4::tree::TerminalNode *INITIALLY();
    antlr4::tree::TerminalNode *IMMEDIATE();
    antlr4::tree::TerminalNode *DEFERRED();
    antlr4::tree::TerminalNode *VALID();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *INHERIT();

   
  };

  ConstraintattributeElemContext* constraintattributeElem();

  class  CreateeventtrigstmtContext : public antlr4::ParserRuleContext {
  public:
    CreateeventtrigstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *EVENT();
    antlr4::tree::TerminalNode *TRIGGER();
    NameContext *name();
    antlr4::tree::TerminalNode *ON();
    ColLabelContext *colLabel();
    antlr4::tree::TerminalNode *EXECUTE();
    Function_or_procedureContext *function_or_procedure();
    Func_nameContext *func_name();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    antlr4::tree::TerminalNode *WHEN();
    Event_trigger_when_listContext *event_trigger_when_list();

   
  };

  CreateeventtrigstmtContext* createeventtrigstmt();

  class  Event_trigger_when_listContext : public antlr4::ParserRuleContext {
  public:
    Event_trigger_when_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Event_trigger_when_itemContext *> event_trigger_when_item();
    Event_trigger_when_itemContext* event_trigger_when_item(size_t i);
    std::vector<antlr4::tree::TerminalNode *> AND();
    antlr4::tree::TerminalNode* AND(size_t i);

   
  };

  Event_trigger_when_listContext* event_trigger_when_list();

  class  Event_trigger_when_itemContext : public antlr4::ParserRuleContext {
  public:
    Event_trigger_when_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ColidContext *colid();
    antlr4::tree::TerminalNode *IN_P();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Event_trigger_value_listContext *event_trigger_value_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Event_trigger_when_itemContext* event_trigger_when_item();

  class  Event_trigger_value_listContext : public antlr4::ParserRuleContext {
  public:
    Event_trigger_value_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<SconstContext *> sconst();
    SconstContext* sconst(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Event_trigger_value_listContext* event_trigger_value_list();

  class  AltereventtrigstmtContext : public antlr4::ParserRuleContext {
  public:
    AltereventtrigstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *EVENT();
    antlr4::tree::TerminalNode *TRIGGER();
    NameContext *name();
    Enable_triggerContext *enable_trigger();

   
  };

  AltereventtrigstmtContext* altereventtrigstmt();

  class  Enable_triggerContext : public antlr4::ParserRuleContext {
  public:
    Enable_triggerContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ENABLE_P();
    antlr4::tree::TerminalNode *REPLICA();
    antlr4::tree::TerminalNode *ALWAYS();
    antlr4::tree::TerminalNode *DISABLE_P();

   
  };

  Enable_triggerContext* enable_trigger();

  class  CreateassertionstmtContext : public antlr4::ParserRuleContext {
  public:
    CreateassertionstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *ASSERTION();
    Any_nameContext *any_name();
    antlr4::tree::TerminalNode *CHECK();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    A_exprContext *a_expr();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    ConstraintattributespecContext *constraintattributespec();

   
  };

  CreateassertionstmtContext* createassertionstmt();

  class  DefinestmtContext : public antlr4::ParserRuleContext {
  public:
    DefinestmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *AGGREGATE();
    Func_nameContext *func_name();
    Aggr_argsContext *aggr_args();
    DefinitionContext *definition();
    Or_replace_Context *or_replace_();
    Old_aggr_definitionContext *old_aggr_definition();
    antlr4::tree::TerminalNode *OPERATOR();
    Any_operatorContext *any_operator();
    antlr4::tree::TerminalNode *TYPE_P();
    std::vector<Any_nameContext *> any_name();
    Any_nameContext* any_name(size_t i);
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    OpttablefuncelementlistContext *opttablefuncelementlist();
    antlr4::tree::TerminalNode *ENUM_P();
    Enum_val_list_Context *enum_val_list_();
    antlr4::tree::TerminalNode *RANGE();
    antlr4::tree::TerminalNode *TEXT_P();
    antlr4::tree::TerminalNode *SEARCH();
    antlr4::tree::TerminalNode *PARSER();
    antlr4::tree::TerminalNode *DICTIONARY();
    antlr4::tree::TerminalNode *TEMPLATE();
    antlr4::tree::TerminalNode *CONFIGURATION();
    antlr4::tree::TerminalNode *COLLATION();
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *EXISTS();
    antlr4::tree::TerminalNode *FROM();

   
  };

  DefinestmtContext* definestmt();

  class  DefinitionContext : public antlr4::ParserRuleContext {
  public:
    DefinitionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Def_listContext *def_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  DefinitionContext* definition();

  class  Def_listContext : public antlr4::ParserRuleContext {
  public:
    Def_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Def_elemContext *> def_elem();
    Def_elemContext* def_elem(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Def_listContext* def_list();

  class  Def_elemContext : public antlr4::ParserRuleContext {
  public:
    Def_elemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ColLabelContext *colLabel();
    antlr4::tree::TerminalNode *EQUAL();
    Def_argContext *def_arg();

   
  };

  Def_elemContext* def_elem();

  class  Def_argContext : public antlr4::ParserRuleContext {
  public:
    Def_argContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Func_typeContext *func_type();
    Reserved_keywordContext *reserved_keyword();
    Qual_all_opContext *qual_all_op();
    NumericonlyContext *numericonly();
    SconstContext *sconst();
    antlr4::tree::TerminalNode *NONE();

   
  };

  Def_argContext* def_arg();

  class  Old_aggr_definitionContext : public antlr4::ParserRuleContext {
  public:
    Old_aggr_definitionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Old_aggr_listContext *old_aggr_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Old_aggr_definitionContext* old_aggr_definition();

  class  Old_aggr_listContext : public antlr4::ParserRuleContext {
  public:
    Old_aggr_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Old_aggr_elemContext *> old_aggr_elem();
    Old_aggr_elemContext* old_aggr_elem(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Old_aggr_listContext* old_aggr_list();

  class  Old_aggr_elemContext : public antlr4::ParserRuleContext {
  public:
    Old_aggr_elemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    antlr4::tree::TerminalNode *EQUAL();
    Def_argContext *def_arg();

   
  };

  Old_aggr_elemContext* old_aggr_elem();

  class  Enum_val_list_Context : public antlr4::ParserRuleContext {
  public:
    Enum_val_list_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Enum_val_listContext *enum_val_list();

   
  };

  Enum_val_list_Context* enum_val_list_();

  class  Enum_val_listContext : public antlr4::ParserRuleContext {
  public:
    Enum_val_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<SconstContext *> sconst();
    SconstContext* sconst(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Enum_val_listContext* enum_val_list();

  class  AlterenumstmtContext : public antlr4::ParserRuleContext {
  public:
    AlterenumstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *TYPE_P();
    Any_nameContext *any_name();
    antlr4::tree::TerminalNode *ADD_P();
    antlr4::tree::TerminalNode *VALUE_P();
    std::vector<SconstContext *> sconst();
    SconstContext* sconst(size_t i);
    If_not_exists_Context *if_not_exists_();
    antlr4::tree::TerminalNode *BEFORE();
    antlr4::tree::TerminalNode *AFTER();
    antlr4::tree::TerminalNode *RENAME();
    antlr4::tree::TerminalNode *TO();

   
  };

  AlterenumstmtContext* alterenumstmt();

  class  If_not_exists_Context : public antlr4::ParserRuleContext {
  public:
    If_not_exists_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *EXISTS();

   
  };

  If_not_exists_Context* if_not_exists_();

  class  CreateopclassstmtContext : public antlr4::ParserRuleContext {
  public:
    CreateopclassstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *OPERATOR();
    antlr4::tree::TerminalNode *CLASS();
    Any_nameContext *any_name();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *TYPE_P();
    TypenameContext *typename_();
    antlr4::tree::TerminalNode *USING();
    NameContext *name();
    antlr4::tree::TerminalNode *AS();
    Opclass_item_listContext *opclass_item_list();
    Default_Context *default_();
    Opfamily_Context *opfamily_();

   
  };

  CreateopclassstmtContext* createopclassstmt();

  class  Opclass_item_listContext : public antlr4::ParserRuleContext {
  public:
    Opclass_item_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Opclass_itemContext *> opclass_item();
    Opclass_itemContext* opclass_item(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Opclass_item_listContext* opclass_item_list();

  class  Opclass_itemContext : public antlr4::ParserRuleContext {
  public:
    Opclass_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OPERATOR();
    IconstContext *iconst();
    Any_operatorContext *any_operator();
    Opclass_purposeContext *opclass_purpose();
    Recheck_Context *recheck_();
    Operator_with_argtypesContext *operator_with_argtypes();
    antlr4::tree::TerminalNode *FUNCTION();
    Function_with_argtypesContext *function_with_argtypes();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Type_listContext *type_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    antlr4::tree::TerminalNode *STORAGE();
    TypenameContext *typename_();

   
  };

  Opclass_itemContext* opclass_item();

  class  Default_Context : public antlr4::ParserRuleContext {
  public:
    Default_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DEFAULT();

   
  };

  Default_Context* default_();

  class  Opfamily_Context : public antlr4::ParserRuleContext {
  public:
    Opfamily_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FAMILY();
    Any_nameContext *any_name();

   
  };

  Opfamily_Context* opfamily_();

  class  Opclass_purposeContext : public antlr4::ParserRuleContext {
  public:
    Opclass_purposeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *SEARCH();
    antlr4::tree::TerminalNode *ORDER();
    antlr4::tree::TerminalNode *BY();
    Any_nameContext *any_name();

   
  };

  Opclass_purposeContext* opclass_purpose();

  class  Recheck_Context : public antlr4::ParserRuleContext {
  public:
    Recheck_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RECHECK();

   
  };

  Recheck_Context* recheck_();

  class  CreateopfamilystmtContext : public antlr4::ParserRuleContext {
  public:
    CreateopfamilystmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *OPERATOR();
    antlr4::tree::TerminalNode *FAMILY();
    Any_nameContext *any_name();
    antlr4::tree::TerminalNode *USING();
    NameContext *name();

   
  };

  CreateopfamilystmtContext* createopfamilystmt();

  class  AlteropfamilystmtContext : public antlr4::ParserRuleContext {
  public:
    AlteropfamilystmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *OPERATOR();
    antlr4::tree::TerminalNode *FAMILY();
    Any_nameContext *any_name();
    antlr4::tree::TerminalNode *USING();
    NameContext *name();
    antlr4::tree::TerminalNode *ADD_P();
    Opclass_item_listContext *opclass_item_list();
    antlr4::tree::TerminalNode *DROP();
    Opclass_drop_listContext *opclass_drop_list();

   
  };

  AlteropfamilystmtContext* alteropfamilystmt();

  class  Opclass_drop_listContext : public antlr4::ParserRuleContext {
  public:
    Opclass_drop_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Opclass_dropContext *> opclass_drop();
    Opclass_dropContext* opclass_drop(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Opclass_drop_listContext* opclass_drop_list();

  class  Opclass_dropContext : public antlr4::ParserRuleContext {
  public:
    Opclass_dropContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OPERATOR();
    IconstContext *iconst();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Type_listContext *type_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    antlr4::tree::TerminalNode *FUNCTION();

   
  };

  Opclass_dropContext* opclass_drop();

  class  DropopclassstmtContext : public antlr4::ParserRuleContext {
  public:
    DropopclassstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DROP();
    antlr4::tree::TerminalNode *OPERATOR();
    antlr4::tree::TerminalNode *CLASS();
    Any_nameContext *any_name();
    antlr4::tree::TerminalNode *USING();
    NameContext *name();
    Drop_behavior_Context *drop_behavior_();
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *EXISTS();

   
  };

  DropopclassstmtContext* dropopclassstmt();

  class  DropopfamilystmtContext : public antlr4::ParserRuleContext {
  public:
    DropopfamilystmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DROP();
    antlr4::tree::TerminalNode *OPERATOR();
    antlr4::tree::TerminalNode *FAMILY();
    Any_nameContext *any_name();
    antlr4::tree::TerminalNode *USING();
    NameContext *name();
    Drop_behavior_Context *drop_behavior_();
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *EXISTS();

   
  };

  DropopfamilystmtContext* dropopfamilystmt();

  class  DropownedstmtContext : public antlr4::ParserRuleContext {
  public:
    DropownedstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DROP();
    antlr4::tree::TerminalNode *OWNED();
    antlr4::tree::TerminalNode *BY();
    Role_listContext *role_list();
    Drop_behavior_Context *drop_behavior_();

   
  };

  DropownedstmtContext* dropownedstmt();

  class  ReassignownedstmtContext : public antlr4::ParserRuleContext {
  public:
    ReassignownedstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REASSIGN();
    antlr4::tree::TerminalNode *OWNED();
    antlr4::tree::TerminalNode *BY();
    Role_listContext *role_list();
    antlr4::tree::TerminalNode *TO();
    RolespecContext *rolespec();

   
  };

  ReassignownedstmtContext* reassignownedstmt();

  class  DropstmtContext : public antlr4::ParserRuleContext {
  public:
    DropstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DROP();
    Object_type_any_nameContext *object_type_any_name();
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *EXISTS();
    Any_name_list_Context *any_name_list_();
    Drop_behavior_Context *drop_behavior_();
    Drop_type_nameContext *drop_type_name();
    Name_listContext *name_list();
    Object_type_name_on_any_nameContext *object_type_name_on_any_name();
    NameContext *name();
    antlr4::tree::TerminalNode *ON();
    Any_nameContext *any_name();
    antlr4::tree::TerminalNode *TYPE_P();
    Type_name_listContext *type_name_list();
    antlr4::tree::TerminalNode *DOMAIN_P();
    antlr4::tree::TerminalNode *INDEX();
    antlr4::tree::TerminalNode *CONCURRENTLY();

   
  };

  DropstmtContext* dropstmt();

  class  Object_type_any_nameContext : public antlr4::ParserRuleContext {
  public:
    Object_type_any_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TABLE();
    antlr4::tree::TerminalNode *SEQUENCE();
    antlr4::tree::TerminalNode *VIEW();
    antlr4::tree::TerminalNode *MATERIALIZED();
    antlr4::tree::TerminalNode *INDEX();
    antlr4::tree::TerminalNode *FOREIGN();
    antlr4::tree::TerminalNode *COLLATION();
    antlr4::tree::TerminalNode *CONVERSION_P();
    antlr4::tree::TerminalNode *STATISTICS();
    antlr4::tree::TerminalNode *TEXT_P();
    antlr4::tree::TerminalNode *SEARCH();
    antlr4::tree::TerminalNode *PARSER();
    antlr4::tree::TerminalNode *DICTIONARY();
    antlr4::tree::TerminalNode *TEMPLATE();
    antlr4::tree::TerminalNode *CONFIGURATION();

   
  };

  Object_type_any_nameContext* object_type_any_name();

  class  Object_type_nameContext : public antlr4::ParserRuleContext {
  public:
    Object_type_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Drop_type_nameContext *drop_type_name();
    antlr4::tree::TerminalNode *DATABASE();
    antlr4::tree::TerminalNode *ROLE();
    antlr4::tree::TerminalNode *SUBSCRIPTION();
    antlr4::tree::TerminalNode *TABLESPACE();

   
  };

  Object_type_nameContext* object_type_name();

  class  Drop_type_nameContext : public antlr4::ParserRuleContext {
  public:
    Drop_type_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ACCESS();
    antlr4::tree::TerminalNode *METHOD();
    antlr4::tree::TerminalNode *EVENT();
    antlr4::tree::TerminalNode *TRIGGER();
    antlr4::tree::TerminalNode *EXTENSION();
    antlr4::tree::TerminalNode *FOREIGN();
    antlr4::tree::TerminalNode *DATA_P();
    antlr4::tree::TerminalNode *WRAPPER();
    antlr4::tree::TerminalNode *LANGUAGE();
    Procedural_Context *procedural_();
    antlr4::tree::TerminalNode *PUBLICATION();
    antlr4::tree::TerminalNode *SCHEMA();
    antlr4::tree::TerminalNode *SERVER();

   
  };

  Drop_type_nameContext* drop_type_name();

  class  Object_type_name_on_any_nameContext : public antlr4::ParserRuleContext {
  public:
    Object_type_name_on_any_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *POLICY();
    antlr4::tree::TerminalNode *RULE();
    antlr4::tree::TerminalNode *TRIGGER();

   
  };

  Object_type_name_on_any_nameContext* object_type_name_on_any_name();

  class  Any_name_list_Context : public antlr4::ParserRuleContext {
  public:
    Any_name_list_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Any_nameContext *> any_name();
    Any_nameContext* any_name(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Any_name_list_Context* any_name_list_();

  class  Any_nameContext : public antlr4::ParserRuleContext {
  public:
    Any_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ColidContext *colid();
    AttrsContext *attrs();

   
  };

  Any_nameContext* any_name();

  class  AttrsContext : public antlr4::ParserRuleContext {
  public:
    AttrsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<antlr4::tree::TerminalNode *> DOT();
    antlr4::tree::TerminalNode* DOT(size_t i);
    std::vector<Attr_nameContext *> attr_name();
    Attr_nameContext* attr_name(size_t i);

   
  };

  AttrsContext* attrs();

  class  Type_name_listContext : public antlr4::ParserRuleContext {
  public:
    Type_name_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<TypenameContext *> typename_();
    TypenameContext* typename_(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Type_name_listContext* type_name_list();

  class  TruncatestmtContext : public antlr4::ParserRuleContext {
  public:
    TruncatestmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TRUNCATE();
    Relation_expr_listContext *relation_expr_list();
    Table_Context *table_();
    Restart_seqs_Context *restart_seqs_();
    Drop_behavior_Context *drop_behavior_();

   
  };

  TruncatestmtContext* truncatestmt();

  class  Restart_seqs_Context : public antlr4::ParserRuleContext {
  public:
    Restart_seqs_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CONTINUE_P();
    antlr4::tree::TerminalNode *IDENTITY_P();
    antlr4::tree::TerminalNode *RESTART();

   
  };

  Restart_seqs_Context* restart_seqs_();

  class  CommentstmtContext : public antlr4::ParserRuleContext {
  public:
    CommentstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *COMMENT();
    std::vector<antlr4::tree::TerminalNode *> ON();
    antlr4::tree::TerminalNode* ON(size_t i);
    Object_type_any_nameContext *object_type_any_name();
    Any_nameContext *any_name();
    antlr4::tree::TerminalNode *IS();
    Comment_textContext *comment_text();
    antlr4::tree::TerminalNode *COLUMN();
    Object_type_nameContext *object_type_name();
    NameContext *name();
    antlr4::tree::TerminalNode *TYPE_P();
    std::vector<TypenameContext *> typename_();
    TypenameContext* typename_(size_t i);
    antlr4::tree::TerminalNode *DOMAIN_P();
    antlr4::tree::TerminalNode *AGGREGATE();
    Aggregate_with_argtypesContext *aggregate_with_argtypes();
    antlr4::tree::TerminalNode *FUNCTION();
    Function_with_argtypesContext *function_with_argtypes();
    antlr4::tree::TerminalNode *OPERATOR();
    Operator_with_argtypesContext *operator_with_argtypes();
    antlr4::tree::TerminalNode *CONSTRAINT();
    Object_type_name_on_any_nameContext *object_type_name_on_any_name();
    antlr4::tree::TerminalNode *PROCEDURE();
    antlr4::tree::TerminalNode *ROUTINE();
    antlr4::tree::TerminalNode *TRANSFORM();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *LANGUAGE();
    antlr4::tree::TerminalNode *CLASS();
    antlr4::tree::TerminalNode *USING();
    antlr4::tree::TerminalNode *FAMILY();
    antlr4::tree::TerminalNode *LARGE_P();
    antlr4::tree::TerminalNode *OBJECT_P();
    NumericonlyContext *numericonly();
    antlr4::tree::TerminalNode *CAST();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  CommentstmtContext* commentstmt();

  class  Comment_textContext : public antlr4::ParserRuleContext {
  public:
    Comment_textContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    SconstContext *sconst();
    antlr4::tree::TerminalNode *NULL_P();

   
  };

  Comment_textContext* comment_text();

  class  SeclabelstmtContext : public antlr4::ParserRuleContext {
  public:
    SeclabelstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SECURITY();
    antlr4::tree::TerminalNode *LABEL();
    antlr4::tree::TerminalNode *ON();
    Object_type_any_nameContext *object_type_any_name();
    Any_nameContext *any_name();
    antlr4::tree::TerminalNode *IS();
    Security_labelContext *security_label();
    Provider_Context *provider_();
    antlr4::tree::TerminalNode *COLUMN();
    Object_type_nameContext *object_type_name();
    NameContext *name();
    antlr4::tree::TerminalNode *TYPE_P();
    TypenameContext *typename_();
    antlr4::tree::TerminalNode *DOMAIN_P();
    antlr4::tree::TerminalNode *AGGREGATE();
    Aggregate_with_argtypesContext *aggregate_with_argtypes();
    antlr4::tree::TerminalNode *FUNCTION();
    Function_with_argtypesContext *function_with_argtypes();
    antlr4::tree::TerminalNode *LARGE_P();
    antlr4::tree::TerminalNode *OBJECT_P();
    NumericonlyContext *numericonly();
    antlr4::tree::TerminalNode *PROCEDURE();
    antlr4::tree::TerminalNode *ROUTINE();

   
  };

  SeclabelstmtContext* seclabelstmt();

  class  Provider_Context : public antlr4::ParserRuleContext {
  public:
    Provider_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FOR();
    Nonreservedword_or_sconstContext *nonreservedword_or_sconst();

   
  };

  Provider_Context* provider_();

  class  Security_labelContext : public antlr4::ParserRuleContext {
  public:
    Security_labelContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    SconstContext *sconst();
    antlr4::tree::TerminalNode *NULL_P();

   
  };

  Security_labelContext* security_label();

  class  FetchstmtContext : public antlr4::ParserRuleContext {
  public:
    FetchstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FETCH();
    Fetch_argsContext *fetch_args();
    antlr4::tree::TerminalNode *MOVE();

   
  };

  FetchstmtContext* fetchstmt();

  class  Fetch_argsContext : public antlr4::ParserRuleContext {
  public:
    Fetch_argsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Cursor_nameContext *cursor_name();
    From_inContext *from_in();
    antlr4::tree::TerminalNode *NEXT();
    From_in_Context *from_in_();
    antlr4::tree::TerminalNode *PRIOR();
    antlr4::tree::TerminalNode *FIRST_P();
    antlr4::tree::TerminalNode *LAST_P();
    antlr4::tree::TerminalNode *ABSOLUTE_P();
    SignediconstContext *signediconst();
    antlr4::tree::TerminalNode *RELATIVE_P();
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *FORWARD();
    antlr4::tree::TerminalNode *BACKWARD();

   
  };

  Fetch_argsContext* fetch_args();

  class  From_inContext : public antlr4::ParserRuleContext {
  public:
    From_inContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FROM();
    antlr4::tree::TerminalNode *IN_P();

   
  };

  From_inContext* from_in();

  class  From_in_Context : public antlr4::ParserRuleContext {
  public:
    From_in_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    From_inContext *from_in();

   
  };

  From_in_Context* from_in_();

  class  GrantstmtContext : public antlr4::ParserRuleContext {
  public:
    GrantstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GRANT();
    PrivilegesContext *privileges();
    antlr4::tree::TerminalNode *ON();
    Privilege_targetContext *privilege_target();
    antlr4::tree::TerminalNode *TO();
    Grantee_listContext *grantee_list();
    Grant_grant_option_Context *grant_grant_option_();

   
  };

  GrantstmtContext* grantstmt();

  class  RevokestmtContext : public antlr4::ParserRuleContext {
  public:
    RevokestmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REVOKE();
    PrivilegesContext *privileges();
    antlr4::tree::TerminalNode *ON();
    Privilege_targetContext *privilege_target();
    antlr4::tree::TerminalNode *FROM();
    Grantee_listContext *grantee_list();
    Drop_behavior_Context *drop_behavior_();
    antlr4::tree::TerminalNode *GRANT();
    antlr4::tree::TerminalNode *OPTION();
    antlr4::tree::TerminalNode *FOR();

   
  };

  RevokestmtContext* revokestmt();

  class  PrivilegesContext : public antlr4::ParserRuleContext {
  public:
    PrivilegesContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Privilege_listContext *privilege_list();
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *PRIVILEGES();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    ColumnlistContext *columnlist();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  PrivilegesContext* privileges();

  class  Privilege_listContext : public antlr4::ParserRuleContext {
  public:
    Privilege_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<PrivilegeContext *> privilege();
    PrivilegeContext* privilege(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Privilege_listContext* privilege_list();

  class  PrivilegeContext : public antlr4::ParserRuleContext {
  public:
    PrivilegeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SELECT();
    Column_list_Context *column_list_();
    antlr4::tree::TerminalNode *REFERENCES();
    antlr4::tree::TerminalNode *CREATE();
    ColidContext *colid();

   
  };

  PrivilegeContext* privilege();

  class  Privilege_targetContext : public antlr4::ParserRuleContext {
  public:
    Privilege_targetContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Qualified_name_listContext *qualified_name_list();
    antlr4::tree::TerminalNode *TABLE();
    antlr4::tree::TerminalNode *SEQUENCE();
    antlr4::tree::TerminalNode *FOREIGN();
    antlr4::tree::TerminalNode *DATA_P();
    antlr4::tree::TerminalNode *WRAPPER();
    Name_listContext *name_list();
    antlr4::tree::TerminalNode *SERVER();
    antlr4::tree::TerminalNode *FUNCTION();
    Function_with_argtypes_listContext *function_with_argtypes_list();
    antlr4::tree::TerminalNode *PROCEDURE();
    antlr4::tree::TerminalNode *ROUTINE();
    antlr4::tree::TerminalNode *DATABASE();
    antlr4::tree::TerminalNode *DOMAIN_P();
    Any_name_list_Context *any_name_list_();
    antlr4::tree::TerminalNode *LANGUAGE();
    antlr4::tree::TerminalNode *LARGE_P();
    antlr4::tree::TerminalNode *OBJECT_P();
    Numericonly_listContext *numericonly_list();
    antlr4::tree::TerminalNode *SCHEMA();
    antlr4::tree::TerminalNode *TABLESPACE();
    antlr4::tree::TerminalNode *TYPE_P();
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *TABLES();
    antlr4::tree::TerminalNode *IN_P();
    antlr4::tree::TerminalNode *SEQUENCES();
    antlr4::tree::TerminalNode *FUNCTIONS();
    antlr4::tree::TerminalNode *PROCEDURES();
    antlr4::tree::TerminalNode *ROUTINES();

   
  };

  Privilege_targetContext* privilege_target();

  class  Grantee_listContext : public antlr4::ParserRuleContext {
  public:
    Grantee_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<GranteeContext *> grantee();
    GranteeContext* grantee(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Grantee_listContext* grantee_list();

  class  GranteeContext : public antlr4::ParserRuleContext {
  public:
    GranteeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    RolespecContext *rolespec();
    antlr4::tree::TerminalNode *GROUP_P();

   
  };

  GranteeContext* grantee();

  class  Grant_grant_option_Context : public antlr4::ParserRuleContext {
  public:
    Grant_grant_option_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *GRANT();
    antlr4::tree::TerminalNode *OPTION();

   
  };

  Grant_grant_option_Context* grant_grant_option_();

  class  GrantrolestmtContext : public antlr4::ParserRuleContext {
  public:
    GrantrolestmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GRANT();
    Privilege_listContext *privilege_list();
    antlr4::tree::TerminalNode *TO();
    Role_listContext *role_list();
    Grant_admin_option_Context *grant_admin_option_();
    Granted_by_Context *granted_by_();

   
  };

  GrantrolestmtContext* grantrolestmt();

  class  RevokerolestmtContext : public antlr4::ParserRuleContext {
  public:
    RevokerolestmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REVOKE();
    Privilege_listContext *privilege_list();
    antlr4::tree::TerminalNode *FROM();
    Role_listContext *role_list();
    Granted_by_Context *granted_by_();
    Drop_behavior_Context *drop_behavior_();
    antlr4::tree::TerminalNode *ADMIN();
    antlr4::tree::TerminalNode *OPTION();
    antlr4::tree::TerminalNode *FOR();

   
  };

  RevokerolestmtContext* revokerolestmt();

  class  Grant_admin_option_Context : public antlr4::ParserRuleContext {
  public:
    Grant_admin_option_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *ADMIN();
    antlr4::tree::TerminalNode *OPTION();

   
  };

  Grant_admin_option_Context* grant_admin_option_();

  class  Granted_by_Context : public antlr4::ParserRuleContext {
  public:
    Granted_by_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GRANTED();
    antlr4::tree::TerminalNode *BY();
    RolespecContext *rolespec();

   
  };

  Granted_by_Context* granted_by_();

  class  AlterdefaultprivilegesstmtContext : public antlr4::ParserRuleContext {
  public:
    AlterdefaultprivilegesstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *DEFAULT();
    antlr4::tree::TerminalNode *PRIVILEGES();
    DefacloptionlistContext *defacloptionlist();
    DefaclactionContext *defaclaction();

   
  };

  AlterdefaultprivilegesstmtContext* alterdefaultprivilegesstmt();

  class  DefacloptionlistContext : public antlr4::ParserRuleContext {
  public:
    DefacloptionlistContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<DefacloptionContext *> defacloption();
    DefacloptionContext* defacloption(size_t i);

   
  };

  DefacloptionlistContext* defacloptionlist();

  class  DefacloptionContext : public antlr4::ParserRuleContext {
  public:
    DefacloptionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *IN_P();
    antlr4::tree::TerminalNode *SCHEMA();
    Name_listContext *name_list();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *ROLE();
    Role_listContext *role_list();
    antlr4::tree::TerminalNode *USER();

   
  };

  DefacloptionContext* defacloption();

  class  DefaclactionContext : public antlr4::ParserRuleContext {
  public:
    DefaclactionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GRANT();
    PrivilegesContext *privileges();
    antlr4::tree::TerminalNode *ON();
    Defacl_privilege_targetContext *defacl_privilege_target();
    antlr4::tree::TerminalNode *TO();
    Grantee_listContext *grantee_list();
    Grant_grant_option_Context *grant_grant_option_();
    antlr4::tree::TerminalNode *REVOKE();
    antlr4::tree::TerminalNode *FROM();
    Drop_behavior_Context *drop_behavior_();
    antlr4::tree::TerminalNode *OPTION();
    antlr4::tree::TerminalNode *FOR();

   
  };

  DefaclactionContext* defaclaction();

  class  Defacl_privilege_targetContext : public antlr4::ParserRuleContext {
  public:
    Defacl_privilege_targetContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TABLES();
    antlr4::tree::TerminalNode *FUNCTIONS();
    antlr4::tree::TerminalNode *ROUTINES();
    antlr4::tree::TerminalNode *SEQUENCES();
    antlr4::tree::TerminalNode *TYPES_P();
    antlr4::tree::TerminalNode *SCHEMAS();

   
  };

  Defacl_privilege_targetContext* defacl_privilege_target();

  class  IndexstmtContext : public antlr4::ParserRuleContext {
  public:
    IndexstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *INDEX();
    antlr4::tree::TerminalNode *ON();
    Relation_exprContext *relation_expr();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Index_paramsContext *index_params();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    Unique_Context *unique_();
    Concurrently_Context *concurrently_();
    Index_name_Context *index_name_();
    Access_method_clauseContext *access_method_clause();
    Include_Context *include_();
    Nulls_distinctContext *nulls_distinct();
    Reloptions_Context *reloptions_();
    OpttablespaceContext *opttablespace();
    Where_clauseContext *where_clause();
    If_not_exists_Context *if_not_exists_();
    NameContext *name();

   
  };

  IndexstmtContext* indexstmt();

  class  Unique_Context : public antlr4::ParserRuleContext {
  public:
    Unique_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *UNIQUE();

   
  };

  Unique_Context* unique_();

  class  Nulls_distinctContext : public antlr4::ParserRuleContext {
  public:
    Nulls_distinctContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NULLS_P();
    antlr4::tree::TerminalNode *DISTINCT();
    antlr4::tree::TerminalNode *NOT();

   
  };

  Nulls_distinctContext* nulls_distinct();

  class  Single_name_Context : public antlr4::ParserRuleContext {
  public:
    Single_name_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ColidContext *colid();

   
  };

  Single_name_Context* single_name_();

  class  Concurrently_Context : public antlr4::ParserRuleContext {
  public:
    Concurrently_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CONCURRENTLY();

   
  };

  Concurrently_Context* concurrently_();

  class  Index_name_Context : public antlr4::ParserRuleContext {
  public:
    Index_name_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    NameContext *name();

   
  };

  Index_name_Context* index_name_();

  class  Access_method_clauseContext : public antlr4::ParserRuleContext {
  public:
    Access_method_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *USING();
    NameContext *name();

   
  };

  Access_method_clauseContext* access_method_clause();

  class  Index_paramsContext : public antlr4::ParserRuleContext {
  public:
    Index_paramsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Index_elemContext *> index_elem();
    Index_elemContext* index_elem(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Index_paramsContext* index_params();

  class  Index_elem_optionsContext : public antlr4::ParserRuleContext {
  public:
    Index_elem_optionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Collate_Context *collate_();
    Class_Context *class_();
    Asc_desc_Context *asc_desc_();
    Nulls_order_Context *nulls_order_();
    Any_nameContext *any_name();
    ReloptionsContext *reloptions();

   
  };

  Index_elem_optionsContext* index_elem_options();

  class  Index_elemContext : public antlr4::ParserRuleContext {
  public:
    Index_elemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ColidContext *colid();
    Index_elem_optionsContext *index_elem_options();
    Func_expr_windowlessContext *func_expr_windowless();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    A_exprContext *a_expr();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Index_elemContext* index_elem();

  class  Include_Context : public antlr4::ParserRuleContext {
  public:
    Include_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INCLUDE();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Index_including_paramsContext *index_including_params();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Include_Context* include_();

  class  Index_including_paramsContext : public antlr4::ParserRuleContext {
  public:
    Index_including_paramsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Index_elemContext *> index_elem();
    Index_elemContext* index_elem(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Index_including_paramsContext* index_including_params();

  class  Collate_Context : public antlr4::ParserRuleContext {
  public:
    Collate_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *COLLATE();
    Any_nameContext *any_name();

   
  };

  Collate_Context* collate_();

  class  Class_Context : public antlr4::ParserRuleContext {
  public:
    Class_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Any_nameContext *any_name();

   
  };

  Class_Context* class_();

  class  Asc_desc_Context : public antlr4::ParserRuleContext {
  public:
    Asc_desc_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ASC();
    antlr4::tree::TerminalNode *DESC();

   
  };

  Asc_desc_Context* asc_desc_();

  class  Nulls_order_Context : public antlr4::ParserRuleContext {
  public:
    Nulls_order_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NULLS_P();
    antlr4::tree::TerminalNode *FIRST_P();
    antlr4::tree::TerminalNode *LAST_P();

   
  };

  Nulls_order_Context* nulls_order_();

  class  CreatefunctionstmtContext : public antlr4::ParserRuleContext {
  public:
    CreatefunctionstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    Func_nameContext *func_name();
    Func_args_with_defaultsContext *func_args_with_defaults();
    Createfunc_opt_listContext *createfunc_opt_list();
    antlr4::tree::TerminalNode *FUNCTION();
    antlr4::tree::TerminalNode *PROCEDURE();
    Or_replace_Context *or_replace_();
    antlr4::tree::TerminalNode *RETURNS();
    Func_returnContext *func_return();
    antlr4::tree::TerminalNode *TABLE();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Table_func_column_listContext *table_func_column_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  CreatefunctionstmtContext* createfunctionstmt();

  class  Or_replace_Context : public antlr4::ParserRuleContext {
  public:
    Or_replace_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OR();
    antlr4::tree::TerminalNode *REPLACE();

   
  };

  Or_replace_Context* or_replace_();

  class  Func_argsContext : public antlr4::ParserRuleContext {
  public:
    Func_argsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OPEN_PAREN();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    Func_args_listContext *func_args_list();

   
  };

  Func_argsContext* func_args();

  class  Func_args_listContext : public antlr4::ParserRuleContext {
  public:
    Func_args_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Func_argContext *> func_arg();
    Func_argContext* func_arg(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Func_args_listContext* func_args_list();

  class  Function_with_argtypes_listContext : public antlr4::ParserRuleContext {
  public:
    Function_with_argtypes_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Function_with_argtypesContext *> function_with_argtypes();
    Function_with_argtypesContext* function_with_argtypes(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Function_with_argtypes_listContext* function_with_argtypes_list();

  class  Function_with_argtypesContext : public antlr4::ParserRuleContext {
  public:
    Function_with_argtypesContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Func_nameContext *func_name();
    Func_argsContext *func_args();
    Type_func_name_keywordContext *type_func_name_keyword();
    ColidContext *colid();
    IndirectionContext *indirection();

   
  };

  Function_with_argtypesContext* function_with_argtypes();

  class  Func_args_with_defaultsContext : public antlr4::ParserRuleContext {
  public:
    Func_args_with_defaultsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OPEN_PAREN();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    Func_args_with_defaults_listContext *func_args_with_defaults_list();

   
  };

  Func_args_with_defaultsContext* func_args_with_defaults();

  class  Func_args_with_defaults_listContext : public antlr4::ParserRuleContext {
  public:
    Func_args_with_defaults_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Func_arg_with_defaultContext *> func_arg_with_default();
    Func_arg_with_defaultContext* func_arg_with_default(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Func_args_with_defaults_listContext* func_args_with_defaults_list();

  class  Func_argContext : public antlr4::ParserRuleContext {
  public:
    Func_argContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Arg_classContext *arg_class();
    Func_typeContext *func_type();
    Param_nameContext *param_name();

   
  };

  Func_argContext* func_arg();

  class  Arg_classContext : public antlr4::ParserRuleContext {
  public:
    Arg_classContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *IN_P();
    antlr4::tree::TerminalNode *OUT_P();
    antlr4::tree::TerminalNode *INOUT();
    antlr4::tree::TerminalNode *VARIADIC();

   
  };

  Arg_classContext* arg_class();

  class  Param_nameContext : public antlr4::ParserRuleContext {
  public:
    Param_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Type_function_nameContext *type_function_name();

   
  };

  Param_nameContext* param_name();

  class  Func_returnContext : public antlr4::ParserRuleContext {
  public:
    Func_returnContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Func_typeContext *func_type();

   
  };

  Func_returnContext* func_return();

  class  Func_typeContext : public antlr4::ParserRuleContext {
  public:
    Func_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TypenameContext *typename_();
    Type_function_nameContext *type_function_name();
    AttrsContext *attrs();
    antlr4::tree::TerminalNode *PERCENT();
    antlr4::tree::TerminalNode *TYPE_P();
    antlr4::tree::TerminalNode *SETOF();

   
  };

  Func_typeContext* func_type();

  class  Func_arg_with_defaultContext : public antlr4::ParserRuleContext {
  public:
    Func_arg_with_defaultContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Func_argContext *func_arg();
    A_exprContext *a_expr();
    antlr4::tree::TerminalNode *DEFAULT();
    antlr4::tree::TerminalNode *EQUAL();

   
  };

  Func_arg_with_defaultContext* func_arg_with_default();

  class  Aggr_argContext : public antlr4::ParserRuleContext {
  public:
    Aggr_argContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Func_argContext *func_arg();

   
  };

  Aggr_argContext* aggr_arg();

  class  Aggr_argsContext : public antlr4::ParserRuleContext {
  public:
    Aggr_argsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OPEN_PAREN();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    antlr4::tree::TerminalNode *STAR();
    std::vector<Aggr_args_listContext *> aggr_args_list();
    Aggr_args_listContext* aggr_args_list(size_t i);
    antlr4::tree::TerminalNode *ORDER();
    antlr4::tree::TerminalNode *BY();

   
  };

  Aggr_argsContext* aggr_args();

  class  Aggr_args_listContext : public antlr4::ParserRuleContext {
  public:
    Aggr_args_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Aggr_argContext *> aggr_arg();
    Aggr_argContext* aggr_arg(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Aggr_args_listContext* aggr_args_list();

  class  Aggregate_with_argtypesContext : public antlr4::ParserRuleContext {
  public:
    Aggregate_with_argtypesContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Func_nameContext *func_name();
    Aggr_argsContext *aggr_args();

   
  };

  Aggregate_with_argtypesContext* aggregate_with_argtypes();

  class  Aggregate_with_argtypes_listContext : public antlr4::ParserRuleContext {
  public:
    Aggregate_with_argtypes_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Aggregate_with_argtypesContext *> aggregate_with_argtypes();
    Aggregate_with_argtypesContext* aggregate_with_argtypes(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Aggregate_with_argtypes_listContext* aggregate_with_argtypes_list();

  class  Createfunc_opt_listContext : public antlr4::ParserRuleContext {
  public:
    Createfunc_opt_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Createfunc_opt_itemContext *> createfunc_opt_item();
    Createfunc_opt_itemContext* createfunc_opt_item(size_t i);

   
  };

  Createfunc_opt_listContext* createfunc_opt_list();

  class  Common_func_opt_itemContext : public antlr4::ParserRuleContext {
  public:
    Common_func_opt_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CALLED();
    antlr4::tree::TerminalNode *ON();
    std::vector<antlr4::tree::TerminalNode *> NULL_P();
    antlr4::tree::TerminalNode* NULL_P(size_t i);
    antlr4::tree::TerminalNode *INPUT_P();
    antlr4::tree::TerminalNode *RETURNS();
    antlr4::tree::TerminalNode *STRICT_P();
    antlr4::tree::TerminalNode *IMMUTABLE();
    antlr4::tree::TerminalNode *STABLE();
    antlr4::tree::TerminalNode *VOLATILE();
    antlr4::tree::TerminalNode *EXTERNAL();
    antlr4::tree::TerminalNode *SECURITY();
    antlr4::tree::TerminalNode *DEFINER();
    antlr4::tree::TerminalNode *INVOKER();
    antlr4::tree::TerminalNode *LEAKPROOF();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *COST();
    NumericonlyContext *numericonly();
    antlr4::tree::TerminalNode *ROWS();
    antlr4::tree::TerminalNode *SUPPORT();
    Any_nameContext *any_name();
    FunctionsetresetclauseContext *functionsetresetclause();
    antlr4::tree::TerminalNode *PARALLEL();
    ColidContext *colid();

   
  };

  Common_func_opt_itemContext* common_func_opt_item();

  class  Createfunc_opt_itemContext : public antlr4::ParserRuleContext {
  public:
    Createfunc_opt_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *AS();
    Func_asContext *func_as();
    antlr4::tree::TerminalNode *LANGUAGE();
    Nonreservedword_or_sconstContext *nonreservedword_or_sconst();
    antlr4::tree::TerminalNode *TRANSFORM();
    Transform_type_listContext *transform_type_list();
    antlr4::tree::TerminalNode *WINDOW();
    Common_func_opt_itemContext *common_func_opt_item();

   
  };

  Createfunc_opt_itemContext* createfunc_opt_item();

  class  Func_asContext : public antlr4::ParserRuleContext {
  public:
    PostgreSQLParser::SconstContext *def = nullptr;
    Func_asContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<SconstContext *> sconst();
    SconstContext* sconst(size_t i);
    antlr4::tree::TerminalNode *COMMA();

   
  };

  Func_asContext* func_as();

  class  Transform_type_listContext : public antlr4::ParserRuleContext {
  public:
    Transform_type_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<antlr4::tree::TerminalNode *> FOR();
    antlr4::tree::TerminalNode* FOR(size_t i);
    std::vector<antlr4::tree::TerminalNode *> TYPE_P();
    antlr4::tree::TerminalNode* TYPE_P(size_t i);
    std::vector<TypenameContext *> typename_();
    TypenameContext* typename_(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Transform_type_listContext* transform_type_list();

  class  Definition_Context : public antlr4::ParserRuleContext {
  public:
    Definition_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WITH();
    DefinitionContext *definition();

   
  };

  Definition_Context* definition_();

  class  Table_func_columnContext : public antlr4::ParserRuleContext {
  public:
    Table_func_columnContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Param_nameContext *param_name();
    Func_typeContext *func_type();

   
  };

  Table_func_columnContext* table_func_column();

  class  Table_func_column_listContext : public antlr4::ParserRuleContext {
  public:
    Table_func_column_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Table_func_columnContext *> table_func_column();
    Table_func_columnContext* table_func_column(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Table_func_column_listContext* table_func_column_list();

  class  AlterfunctionstmtContext : public antlr4::ParserRuleContext {
  public:
    AlterfunctionstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    Function_with_argtypesContext *function_with_argtypes();
    Alterfunc_opt_listContext *alterfunc_opt_list();
    antlr4::tree::TerminalNode *FUNCTION();
    antlr4::tree::TerminalNode *PROCEDURE();
    antlr4::tree::TerminalNode *ROUTINE();
    Restrict_Context *restrict_();

   
  };

  AlterfunctionstmtContext* alterfunctionstmt();

  class  Alterfunc_opt_listContext : public antlr4::ParserRuleContext {
  public:
    Alterfunc_opt_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Common_func_opt_itemContext *> common_func_opt_item();
    Common_func_opt_itemContext* common_func_opt_item(size_t i);

   
  };

  Alterfunc_opt_listContext* alterfunc_opt_list();

  class  Restrict_Context : public antlr4::ParserRuleContext {
  public:
    Restrict_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RESTRICT();

   
  };

  Restrict_Context* restrict_();

  class  RemovefuncstmtContext : public antlr4::ParserRuleContext {
  public:
    RemovefuncstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DROP();
    antlr4::tree::TerminalNode *FUNCTION();
    Function_with_argtypes_listContext *function_with_argtypes_list();
    Drop_behavior_Context *drop_behavior_();
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *EXISTS();
    antlr4::tree::TerminalNode *PROCEDURE();
    antlr4::tree::TerminalNode *ROUTINE();

   
  };

  RemovefuncstmtContext* removefuncstmt();

  class  RemoveaggrstmtContext : public antlr4::ParserRuleContext {
  public:
    RemoveaggrstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DROP();
    antlr4::tree::TerminalNode *AGGREGATE();
    Aggregate_with_argtypes_listContext *aggregate_with_argtypes_list();
    Drop_behavior_Context *drop_behavior_();
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *EXISTS();

   
  };

  RemoveaggrstmtContext* removeaggrstmt();

  class  RemoveoperstmtContext : public antlr4::ParserRuleContext {
  public:
    RemoveoperstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DROP();
    antlr4::tree::TerminalNode *OPERATOR();
    Operator_with_argtypes_listContext *operator_with_argtypes_list();
    Drop_behavior_Context *drop_behavior_();
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *EXISTS();

   
  };

  RemoveoperstmtContext* removeoperstmt();

  class  Oper_argtypesContext : public antlr4::ParserRuleContext {
  public:
    Oper_argtypesContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OPEN_PAREN();
    std::vector<TypenameContext *> typename_();
    TypenameContext* typename_(size_t i);
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    antlr4::tree::TerminalNode *COMMA();
    antlr4::tree::TerminalNode *NONE();

   
  };

  Oper_argtypesContext* oper_argtypes();

  class  Any_operatorContext : public antlr4::ParserRuleContext {
  public:
    Any_operatorContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    All_opContext *all_op();
    std::vector<ColidContext *> colid();
    ColidContext* colid(size_t i);
    std::vector<antlr4::tree::TerminalNode *> DOT();
    antlr4::tree::TerminalNode* DOT(size_t i);

   
  };

  Any_operatorContext* any_operator();

  class  Operator_with_argtypes_listContext : public antlr4::ParserRuleContext {
  public:
    Operator_with_argtypes_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Operator_with_argtypesContext *> operator_with_argtypes();
    Operator_with_argtypesContext* operator_with_argtypes(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Operator_with_argtypes_listContext* operator_with_argtypes_list();

  class  Operator_with_argtypesContext : public antlr4::ParserRuleContext {
  public:
    Operator_with_argtypesContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Any_operatorContext *any_operator();
    Oper_argtypesContext *oper_argtypes();

   
  };

  Operator_with_argtypesContext* operator_with_argtypes();

  class  DostmtContext : public antlr4::ParserRuleContext {
  public:
    DostmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DO();
    Dostmt_opt_listContext *dostmt_opt_list();

   
  };

  DostmtContext* dostmt();

  class  Dostmt_opt_listContext : public antlr4::ParserRuleContext {
  public:
    Dostmt_opt_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Dostmt_opt_itemContext *> dostmt_opt_item();
    Dostmt_opt_itemContext* dostmt_opt_item(size_t i);

   
  };

  Dostmt_opt_listContext* dostmt_opt_list();

  class  Dostmt_opt_itemContext : public antlr4::ParserRuleContext {
  public:
    Dostmt_opt_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    SconstContext *sconst();
    antlr4::tree::TerminalNode *LANGUAGE();
    Nonreservedword_or_sconstContext *nonreservedword_or_sconst();

   
  };

  Dostmt_opt_itemContext* dostmt_opt_item();

  class  CreatecaststmtContext : public antlr4::ParserRuleContext {
  public:
    CreatecaststmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *CAST();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    std::vector<TypenameContext *> typename_();
    TypenameContext* typename_(size_t i);
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *FUNCTION();
    Function_with_argtypesContext *function_with_argtypes();
    Cast_contextContext *cast_context();
    antlr4::tree::TerminalNode *WITHOUT();
    antlr4::tree::TerminalNode *INOUT();

   
  };

  CreatecaststmtContext* createcaststmt();

  class  Cast_contextContext : public antlr4::ParserRuleContext {
  public:
    Cast_contextContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *IMPLICIT_P();
    antlr4::tree::TerminalNode *ASSIGNMENT();

   
  };

  Cast_contextContext* cast_context();

  class  DropcaststmtContext : public antlr4::ParserRuleContext {
  public:
    DropcaststmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DROP();
    antlr4::tree::TerminalNode *CAST();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    std::vector<TypenameContext *> typename_();
    TypenameContext* typename_(size_t i);
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    If_exists_Context *if_exists_();
    Drop_behavior_Context *drop_behavior_();

   
  };

  DropcaststmtContext* dropcaststmt();

  class  If_exists_Context : public antlr4::ParserRuleContext {
  public:
    If_exists_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *EXISTS();

   
  };

  If_exists_Context* if_exists_();

  class  CreatetransformstmtContext : public antlr4::ParserRuleContext {
  public:
    CreatetransformstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *TRANSFORM();
    antlr4::tree::TerminalNode *FOR();
    TypenameContext *typename_();
    antlr4::tree::TerminalNode *LANGUAGE();
    NameContext *name();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Transform_element_listContext *transform_element_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    Or_replace_Context *or_replace_();

   
  };

  CreatetransformstmtContext* createtransformstmt();

  class  Transform_element_listContext : public antlr4::ParserRuleContext {
  public:
    Transform_element_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FROM();
    std::vector<antlr4::tree::TerminalNode *> SQL_P();
    antlr4::tree::TerminalNode* SQL_P(size_t i);
    std::vector<antlr4::tree::TerminalNode *> WITH();
    antlr4::tree::TerminalNode* WITH(size_t i);
    std::vector<antlr4::tree::TerminalNode *> FUNCTION();
    antlr4::tree::TerminalNode* FUNCTION(size_t i);
    std::vector<Function_with_argtypesContext *> function_with_argtypes();
    Function_with_argtypesContext* function_with_argtypes(size_t i);
    antlr4::tree::TerminalNode *COMMA();
    antlr4::tree::TerminalNode *TO();

   
  };

  Transform_element_listContext* transform_element_list();

  class  DroptransformstmtContext : public antlr4::ParserRuleContext {
  public:
    DroptransformstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DROP();
    antlr4::tree::TerminalNode *TRANSFORM();
    antlr4::tree::TerminalNode *FOR();
    TypenameContext *typename_();
    antlr4::tree::TerminalNode *LANGUAGE();
    NameContext *name();
    If_exists_Context *if_exists_();
    Drop_behavior_Context *drop_behavior_();

   
  };

  DroptransformstmtContext* droptransformstmt();

  class  ReindexstmtContext : public antlr4::ParserRuleContext {
  public:
    ReindexstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REINDEX();
    Reindex_target_relationContext *reindex_target_relation();
    Qualified_nameContext *qualified_name();
    Reindex_option_listContext *reindex_option_list();
    Concurrently_Context *concurrently_();
    antlr4::tree::TerminalNode *SCHEMA();
    NameContext *name();
    Reindex_target_allContext *reindex_target_all();
    Single_name_Context *single_name_();

   
  };

  ReindexstmtContext* reindexstmt();

  class  Reindex_target_relationContext : public antlr4::ParserRuleContext {
  public:
    Reindex_target_relationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INDEX();
    antlr4::tree::TerminalNode *TABLE();

   
  };

  Reindex_target_relationContext* reindex_target_relation();

  class  Reindex_target_allContext : public antlr4::ParserRuleContext {
  public:
    Reindex_target_allContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SYSTEM_P();
    antlr4::tree::TerminalNode *DATABASE();

   
  };

  Reindex_target_allContext* reindex_target_all();

  class  Reindex_option_listContext : public antlr4::ParserRuleContext {
  public:
    Reindex_option_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Utility_option_listContext *utility_option_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Reindex_option_listContext* reindex_option_list();

  class  AltertblspcstmtContext : public antlr4::ParserRuleContext {
  public:
    AltertblspcstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *TABLESPACE();
    NameContext *name();
    antlr4::tree::TerminalNode *SET();
    ReloptionsContext *reloptions();
    antlr4::tree::TerminalNode *RESET();

   
  };

  AltertblspcstmtContext* altertblspcstmt();

  class  RenamestmtContext : public antlr4::ParserRuleContext {
  public:
    RenamestmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *AGGREGATE();
    Aggregate_with_argtypesContext *aggregate_with_argtypes();
    antlr4::tree::TerminalNode *RENAME();
    antlr4::tree::TerminalNode *TO();
    std::vector<NameContext *> name();
    NameContext* name(size_t i);
    antlr4::tree::TerminalNode *COLLATION();
    Any_nameContext *any_name();
    antlr4::tree::TerminalNode *CONVERSION_P();
    antlr4::tree::TerminalNode *DATABASE();
    antlr4::tree::TerminalNode *DOMAIN_P();
    antlr4::tree::TerminalNode *CONSTRAINT();
    antlr4::tree::TerminalNode *FOREIGN();
    antlr4::tree::TerminalNode *DATA_P();
    antlr4::tree::TerminalNode *WRAPPER();
    antlr4::tree::TerminalNode *FUNCTION();
    Function_with_argtypesContext *function_with_argtypes();
    antlr4::tree::TerminalNode *GROUP_P();
    std::vector<RoleidContext *> roleid();
    RoleidContext* roleid(size_t i);
    antlr4::tree::TerminalNode *LANGUAGE();
    Procedural_Context *procedural_();
    antlr4::tree::TerminalNode *OPERATOR();
    antlr4::tree::TerminalNode *CLASS();
    antlr4::tree::TerminalNode *USING();
    antlr4::tree::TerminalNode *FAMILY();
    antlr4::tree::TerminalNode *POLICY();
    antlr4::tree::TerminalNode *ON();
    Qualified_nameContext *qualified_name();
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *EXISTS();
    antlr4::tree::TerminalNode *PROCEDURE();
    antlr4::tree::TerminalNode *PUBLICATION();
    antlr4::tree::TerminalNode *ROUTINE();
    antlr4::tree::TerminalNode *SCHEMA();
    antlr4::tree::TerminalNode *SERVER();
    antlr4::tree::TerminalNode *SUBSCRIPTION();
    antlr4::tree::TerminalNode *TABLE();
    Relation_exprContext *relation_expr();
    antlr4::tree::TerminalNode *SEQUENCE();
    antlr4::tree::TerminalNode *VIEW();
    antlr4::tree::TerminalNode *MATERIALIZED();
    antlr4::tree::TerminalNode *INDEX();
    Column_Context *column_();
    antlr4::tree::TerminalNode *RULE();
    antlr4::tree::TerminalNode *TRIGGER();
    antlr4::tree::TerminalNode *EVENT();
    antlr4::tree::TerminalNode *ROLE();
    antlr4::tree::TerminalNode *USER();
    antlr4::tree::TerminalNode *TABLESPACE();
    antlr4::tree::TerminalNode *STATISTICS();
    antlr4::tree::TerminalNode *TEXT_P();
    antlr4::tree::TerminalNode *SEARCH();
    antlr4::tree::TerminalNode *PARSER();
    antlr4::tree::TerminalNode *DICTIONARY();
    antlr4::tree::TerminalNode *TEMPLATE();
    antlr4::tree::TerminalNode *CONFIGURATION();
    antlr4::tree::TerminalNode *TYPE_P();
    antlr4::tree::TerminalNode *ATTRIBUTE();
    Drop_behavior_Context *drop_behavior_();

   
  };

  RenamestmtContext* renamestmt();

  class  Column_Context : public antlr4::ParserRuleContext {
  public:
    Column_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *COLUMN();

   
  };

  Column_Context* column_();

  class  Set_data_Context : public antlr4::ParserRuleContext {
  public:
    Set_data_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SET();
    antlr4::tree::TerminalNode *DATA_P();

   
  };

  Set_data_Context* set_data_();

  class  AlterobjectdependsstmtContext : public antlr4::ParserRuleContext {
  public:
    AlterobjectdependsstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *FUNCTION();
    Function_with_argtypesContext *function_with_argtypes();
    antlr4::tree::TerminalNode *DEPENDS();
    std::vector<antlr4::tree::TerminalNode *> ON();
    antlr4::tree::TerminalNode* ON(size_t i);
    antlr4::tree::TerminalNode *EXTENSION();
    std::vector<NameContext *> name();
    NameContext* name(size_t i);
    No_Context *no_();
    antlr4::tree::TerminalNode *PROCEDURE();
    antlr4::tree::TerminalNode *ROUTINE();
    antlr4::tree::TerminalNode *TRIGGER();
    Qualified_nameContext *qualified_name();
    antlr4::tree::TerminalNode *MATERIALIZED();
    antlr4::tree::TerminalNode *VIEW();
    antlr4::tree::TerminalNode *INDEX();

   
  };

  AlterobjectdependsstmtContext* alterobjectdependsstmt();

  class  No_Context : public antlr4::ParserRuleContext {
  public:
    No_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NO();

   
  };

  No_Context* no_();

  class  AlterobjectschemastmtContext : public antlr4::ParserRuleContext {
  public:
    AlterobjectschemastmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *AGGREGATE();
    Aggregate_with_argtypesContext *aggregate_with_argtypes();
    antlr4::tree::TerminalNode *SET();
    antlr4::tree::TerminalNode *SCHEMA();
    std::vector<NameContext *> name();
    NameContext* name(size_t i);
    antlr4::tree::TerminalNode *COLLATION();
    Any_nameContext *any_name();
    antlr4::tree::TerminalNode *CONVERSION_P();
    antlr4::tree::TerminalNode *DOMAIN_P();
    antlr4::tree::TerminalNode *EXTENSION();
    antlr4::tree::TerminalNode *FUNCTION();
    Function_with_argtypesContext *function_with_argtypes();
    antlr4::tree::TerminalNode *OPERATOR();
    Operator_with_argtypesContext *operator_with_argtypes();
    antlr4::tree::TerminalNode *CLASS();
    antlr4::tree::TerminalNode *USING();
    antlr4::tree::TerminalNode *FAMILY();
    antlr4::tree::TerminalNode *PROCEDURE();
    antlr4::tree::TerminalNode *ROUTINE();
    antlr4::tree::TerminalNode *TABLE();
    Relation_exprContext *relation_expr();
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *EXISTS();
    antlr4::tree::TerminalNode *STATISTICS();
    antlr4::tree::TerminalNode *TEXT_P();
    antlr4::tree::TerminalNode *SEARCH();
    antlr4::tree::TerminalNode *PARSER();
    antlr4::tree::TerminalNode *DICTIONARY();
    antlr4::tree::TerminalNode *TEMPLATE();
    antlr4::tree::TerminalNode *CONFIGURATION();
    antlr4::tree::TerminalNode *SEQUENCE();
    Qualified_nameContext *qualified_name();
    antlr4::tree::TerminalNode *VIEW();
    antlr4::tree::TerminalNode *MATERIALIZED();
    antlr4::tree::TerminalNode *FOREIGN();
    antlr4::tree::TerminalNode *TYPE_P();

   
  };

  AlterobjectschemastmtContext* alterobjectschemastmt();

  class  AlteroperatorstmtContext : public antlr4::ParserRuleContext {
  public:
    AlteroperatorstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *OPERATOR();
    Operator_with_argtypesContext *operator_with_argtypes();
    antlr4::tree::TerminalNode *SET();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Operator_def_listContext *operator_def_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  AlteroperatorstmtContext* alteroperatorstmt();

  class  Operator_def_listContext : public antlr4::ParserRuleContext {
  public:
    Operator_def_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Operator_def_elemContext *> operator_def_elem();
    Operator_def_elemContext* operator_def_elem(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Operator_def_listContext* operator_def_list();

  class  Operator_def_elemContext : public antlr4::ParserRuleContext {
  public:
    Operator_def_elemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ColLabelContext *colLabel();
    antlr4::tree::TerminalNode *EQUAL();
    antlr4::tree::TerminalNode *NONE();
    Operator_def_argContext *operator_def_arg();

   
  };

  Operator_def_elemContext* operator_def_elem();

  class  Operator_def_argContext : public antlr4::ParserRuleContext {
  public:
    Operator_def_argContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Func_typeContext *func_type();
    Reserved_keywordContext *reserved_keyword();
    Qual_all_opContext *qual_all_op();
    NumericonlyContext *numericonly();
    SconstContext *sconst();

   
  };

  Operator_def_argContext* operator_def_arg();

  class  AltertypestmtContext : public antlr4::ParserRuleContext {
  public:
    AltertypestmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *TYPE_P();
    Any_nameContext *any_name();
    antlr4::tree::TerminalNode *SET();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Operator_def_listContext *operator_def_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  AltertypestmtContext* altertypestmt();

  class  AlterownerstmtContext : public antlr4::ParserRuleContext {
  public:
    AlterownerstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *AGGREGATE();
    Aggregate_with_argtypesContext *aggregate_with_argtypes();
    antlr4::tree::TerminalNode *OWNER();
    antlr4::tree::TerminalNode *TO();
    RolespecContext *rolespec();
    antlr4::tree::TerminalNode *COLLATION();
    Any_nameContext *any_name();
    antlr4::tree::TerminalNode *CONVERSION_P();
    antlr4::tree::TerminalNode *DATABASE();
    NameContext *name();
    antlr4::tree::TerminalNode *DOMAIN_P();
    antlr4::tree::TerminalNode *FUNCTION();
    Function_with_argtypesContext *function_with_argtypes();
    antlr4::tree::TerminalNode *LANGUAGE();
    Procedural_Context *procedural_();
    antlr4::tree::TerminalNode *LARGE_P();
    antlr4::tree::TerminalNode *OBJECT_P();
    NumericonlyContext *numericonly();
    antlr4::tree::TerminalNode *OPERATOR();
    Operator_with_argtypesContext *operator_with_argtypes();
    antlr4::tree::TerminalNode *CLASS();
    antlr4::tree::TerminalNode *USING();
    antlr4::tree::TerminalNode *FAMILY();
    antlr4::tree::TerminalNode *PROCEDURE();
    antlr4::tree::TerminalNode *ROUTINE();
    antlr4::tree::TerminalNode *SCHEMA();
    antlr4::tree::TerminalNode *TYPE_P();
    antlr4::tree::TerminalNode *TABLESPACE();
    antlr4::tree::TerminalNode *STATISTICS();
    antlr4::tree::TerminalNode *TEXT_P();
    antlr4::tree::TerminalNode *SEARCH();
    antlr4::tree::TerminalNode *DICTIONARY();
    antlr4::tree::TerminalNode *CONFIGURATION();
    antlr4::tree::TerminalNode *FOREIGN();
    antlr4::tree::TerminalNode *DATA_P();
    antlr4::tree::TerminalNode *WRAPPER();
    antlr4::tree::TerminalNode *SERVER();
    antlr4::tree::TerminalNode *EVENT();
    antlr4::tree::TerminalNode *TRIGGER();
    antlr4::tree::TerminalNode *PUBLICATION();
    antlr4::tree::TerminalNode *SUBSCRIPTION();

   
  };

  AlterownerstmtContext* alterownerstmt();

  class  CreatepublicationstmtContext : public antlr4::ParserRuleContext {
  public:
    CreatepublicationstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *PUBLICATION();
    NameContext *name();
    Publication_for_tables_Context *publication_for_tables_();
    Definition_Context *definition_();

   
  };

  CreatepublicationstmtContext* createpublicationstmt();

  class  Publication_for_tables_Context : public antlr4::ParserRuleContext {
  public:
    Publication_for_tables_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Publication_for_tablesContext *publication_for_tables();

   
  };

  Publication_for_tables_Context* publication_for_tables_();

  class  Publication_for_tablesContext : public antlr4::ParserRuleContext {
  public:
    Publication_for_tablesContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *TABLE();
    Relation_expr_listContext *relation_expr_list();
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *TABLES();

   
  };

  Publication_for_tablesContext* publication_for_tables();

  class  AlterpublicationstmtContext : public antlr4::ParserRuleContext {
  public:
    AlterpublicationstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *PUBLICATION();
    NameContext *name();
    antlr4::tree::TerminalNode *SET();
    DefinitionContext *definition();
    antlr4::tree::TerminalNode *ADD_P();
    antlr4::tree::TerminalNode *TABLE();
    Relation_expr_listContext *relation_expr_list();
    antlr4::tree::TerminalNode *DROP();

   
  };

  AlterpublicationstmtContext* alterpublicationstmt();

  class  CreatesubscriptionstmtContext : public antlr4::ParserRuleContext {
  public:
    CreatesubscriptionstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *SUBSCRIPTION();
    NameContext *name();
    antlr4::tree::TerminalNode *CONNECTION();
    SconstContext *sconst();
    antlr4::tree::TerminalNode *PUBLICATION();
    Publication_name_listContext *publication_name_list();
    Definition_Context *definition_();

   
  };

  CreatesubscriptionstmtContext* createsubscriptionstmt();

  class  Publication_name_listContext : public antlr4::ParserRuleContext {
  public:
    Publication_name_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Publication_name_itemContext *> publication_name_item();
    Publication_name_itemContext* publication_name_item(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Publication_name_listContext* publication_name_list();

  class  Publication_name_itemContext : public antlr4::ParserRuleContext {
  public:
    Publication_name_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ColLabelContext *colLabel();

   
  };

  Publication_name_itemContext* publication_name_item();

  class  AltersubscriptionstmtContext : public antlr4::ParserRuleContext {
  public:
    AltersubscriptionstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *SUBSCRIPTION();
    NameContext *name();
    antlr4::tree::TerminalNode *SET();
    DefinitionContext *definition();
    antlr4::tree::TerminalNode *CONNECTION();
    SconstContext *sconst();
    antlr4::tree::TerminalNode *REFRESH();
    antlr4::tree::TerminalNode *PUBLICATION();
    Definition_Context *definition_();
    Publication_name_listContext *publication_name_list();
    antlr4::tree::TerminalNode *ENABLE_P();
    antlr4::tree::TerminalNode *DISABLE_P();

   
  };

  AltersubscriptionstmtContext* altersubscriptionstmt();

  class  DropsubscriptionstmtContext : public antlr4::ParserRuleContext {
  public:
    DropsubscriptionstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DROP();
    antlr4::tree::TerminalNode *SUBSCRIPTION();
    NameContext *name();
    Drop_behavior_Context *drop_behavior_();
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *EXISTS();

   
  };

  DropsubscriptionstmtContext* dropsubscriptionstmt();

  class  RulestmtContext : public antlr4::ParserRuleContext {
  public:
    RulestmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *RULE();
    NameContext *name();
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *ON();
    EventContext *event();
    antlr4::tree::TerminalNode *TO();
    Qualified_nameContext *qualified_name();
    antlr4::tree::TerminalNode *DO();
    RuleactionlistContext *ruleactionlist();
    Or_replace_Context *or_replace_();
    Where_clauseContext *where_clause();
    Instead_Context *instead_();

   
  };

  RulestmtContext* rulestmt();

  class  RuleactionlistContext : public antlr4::ParserRuleContext {
  public:
    RuleactionlistContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NOTHING();
    RuleactionstmtContext *ruleactionstmt();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    RuleactionmultiContext *ruleactionmulti();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  RuleactionlistContext* ruleactionlist();

  class  RuleactionmultiContext : public antlr4::ParserRuleContext {
  public:
    RuleactionmultiContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<RuleactionstmtOrEmptyContext *> ruleactionstmtOrEmpty();
    RuleactionstmtOrEmptyContext* ruleactionstmtOrEmpty(size_t i);
    std::vector<antlr4::tree::TerminalNode *> SEMI();
    antlr4::tree::TerminalNode* SEMI(size_t i);

   
  };

  RuleactionmultiContext* ruleactionmulti();

  class  RuleactionstmtContext : public antlr4::ParserRuleContext {
  public:
    RuleactionstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    SelectstmtContext *selectstmt();
    InsertstmtContext *insertstmt();
    UpdatestmtContext *updatestmt();
    DeletestmtContext *deletestmt();
    NotifystmtContext *notifystmt();

   
  };

  RuleactionstmtContext* ruleactionstmt();

  class  RuleactionstmtOrEmptyContext : public antlr4::ParserRuleContext {
  public:
    RuleactionstmtOrEmptyContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    RuleactionstmtContext *ruleactionstmt();

   
  };

  RuleactionstmtOrEmptyContext* ruleactionstmtOrEmpty();

  class  EventContext : public antlr4::ParserRuleContext {
  public:
    EventContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SELECT();
    antlr4::tree::TerminalNode *UPDATE();
    antlr4::tree::TerminalNode *DELETE_P();
    antlr4::tree::TerminalNode *INSERT();

   
  };

  EventContext* event();

  class  Instead_Context : public antlr4::ParserRuleContext {
  public:
    Instead_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INSTEAD();
    antlr4::tree::TerminalNode *ALSO();

   
  };

  Instead_Context* instead_();

  class  NotifystmtContext : public antlr4::ParserRuleContext {
  public:
    NotifystmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NOTIFY();
    ColidContext *colid();
    Notify_payloadContext *notify_payload();

   
  };

  NotifystmtContext* notifystmt();

  class  Notify_payloadContext : public antlr4::ParserRuleContext {
  public:
    Notify_payloadContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *COMMA();
    SconstContext *sconst();

   
  };

  Notify_payloadContext* notify_payload();

  class  ListenstmtContext : public antlr4::ParserRuleContext {
  public:
    ListenstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LISTEN();
    ColidContext *colid();

   
  };

  ListenstmtContext* listenstmt();

  class  UnlistenstmtContext : public antlr4::ParserRuleContext {
  public:
    UnlistenstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *UNLISTEN();
    ColidContext *colid();
    antlr4::tree::TerminalNode *STAR();

   
  };

  UnlistenstmtContext* unlistenstmt();

  class  TransactionstmtContext : public antlr4::ParserRuleContext {
  public:
    TransactionstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ABORT_P();
    Transaction_Context *transaction_();
    Transaction_chain_Context *transaction_chain_();
    antlr4::tree::TerminalNode *BEGIN_P();
    Transaction_mode_list_or_emptyContext *transaction_mode_list_or_empty();
    antlr4::tree::TerminalNode *START();
    antlr4::tree::TerminalNode *TRANSACTION();
    antlr4::tree::TerminalNode *COMMIT();
    antlr4::tree::TerminalNode *END_P();
    antlr4::tree::TerminalNode *ROLLBACK();
    antlr4::tree::TerminalNode *SAVEPOINT();
    ColidContext *colid();
    antlr4::tree::TerminalNode *RELEASE();
    antlr4::tree::TerminalNode *TO();
    antlr4::tree::TerminalNode *PREPARE();
    SconstContext *sconst();
    antlr4::tree::TerminalNode *PREPARED();

   
  };

  TransactionstmtContext* transactionstmt();

  class  Transaction_Context : public antlr4::ParserRuleContext {
  public:
    Transaction_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WORK();
    antlr4::tree::TerminalNode *TRANSACTION();

   
  };

  Transaction_Context* transaction_();

  class  Transaction_mode_itemContext : public antlr4::ParserRuleContext {
  public:
    Transaction_mode_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ISOLATION();
    antlr4::tree::TerminalNode *LEVEL();
    Iso_levelContext *iso_level();
    antlr4::tree::TerminalNode *READ();
    antlr4::tree::TerminalNode *ONLY();
    antlr4::tree::TerminalNode *WRITE();
    antlr4::tree::TerminalNode *DEFERRABLE();
    antlr4::tree::TerminalNode *NOT();

   
  };

  Transaction_mode_itemContext* transaction_mode_item();

  class  Transaction_mode_listContext : public antlr4::ParserRuleContext {
  public:
    Transaction_mode_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Transaction_mode_itemContext *> transaction_mode_item();
    Transaction_mode_itemContext* transaction_mode_item(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Transaction_mode_listContext* transaction_mode_list();

  class  Transaction_mode_list_or_emptyContext : public antlr4::ParserRuleContext {
  public:
    Transaction_mode_list_or_emptyContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Transaction_mode_listContext *transaction_mode_list();

   
  };

  Transaction_mode_list_or_emptyContext* transaction_mode_list_or_empty();

  class  Transaction_chain_Context : public antlr4::ParserRuleContext {
  public:
    Transaction_chain_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *AND();
    antlr4::tree::TerminalNode *CHAIN();
    antlr4::tree::TerminalNode *NO();

   
  };

  Transaction_chain_Context* transaction_chain_();

  class  ViewstmtContext : public antlr4::ParserRuleContext {
  public:
    ViewstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *AS();
    SelectstmtContext *selectstmt();
    antlr4::tree::TerminalNode *VIEW();
    Qualified_nameContext *qualified_name();
    antlr4::tree::TerminalNode *RECURSIVE();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    ColumnlistContext *columnlist();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    antlr4::tree::TerminalNode *OR();
    antlr4::tree::TerminalNode *REPLACE();
    OpttempContext *opttemp();
    Check_option_Context *check_option_();
    Column_list_Context *column_list_();
    Reloptions_Context *reloptions_();

   
  };

  ViewstmtContext* viewstmt();

  class  Check_option_Context : public antlr4::ParserRuleContext {
  public:
    Check_option_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *CHECK();
    antlr4::tree::TerminalNode *OPTION();
    antlr4::tree::TerminalNode *CASCADED();
    antlr4::tree::TerminalNode *LOCAL();

   
  };

  Check_option_Context* check_option_();

  class  LoadstmtContext : public antlr4::ParserRuleContext {
  public:
    LoadstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LOAD();
    File_nameContext *file_name();

   
  };

  LoadstmtContext* loadstmt();

  class  CreatedbstmtContext : public antlr4::ParserRuleContext {
  public:
    CreatedbstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *DATABASE();
    NameContext *name();
    With_Context *with_();
    Createdb_opt_listContext *createdb_opt_list();

   
  };

  CreatedbstmtContext* createdbstmt();

  class  Createdb_opt_listContext : public antlr4::ParserRuleContext {
  public:
    Createdb_opt_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Createdb_opt_itemsContext *createdb_opt_items();

   
  };

  Createdb_opt_listContext* createdb_opt_list();

  class  Createdb_opt_itemsContext : public antlr4::ParserRuleContext {
  public:
    Createdb_opt_itemsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Createdb_opt_itemContext *> createdb_opt_item();
    Createdb_opt_itemContext* createdb_opt_item(size_t i);

   
  };

  Createdb_opt_itemsContext* createdb_opt_items();

  class  Createdb_opt_itemContext : public antlr4::ParserRuleContext {
  public:
    Createdb_opt_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Createdb_opt_nameContext *createdb_opt_name();
    SignediconstContext *signediconst();
    Boolean_or_string_Context *boolean_or_string_();
    antlr4::tree::TerminalNode *DEFAULT();
    Equal_Context *equal_();

   
  };

  Createdb_opt_itemContext* createdb_opt_item();

  class  Createdb_opt_nameContext : public antlr4::ParserRuleContext {
  public:
    Createdb_opt_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    antlr4::tree::TerminalNode *CONNECTION();
    antlr4::tree::TerminalNode *LIMIT();
    antlr4::tree::TerminalNode *ENCODING();
    antlr4::tree::TerminalNode *LOCATION();
    antlr4::tree::TerminalNode *OWNER();
    antlr4::tree::TerminalNode *TABLESPACE();
    antlr4::tree::TerminalNode *TEMPLATE();

   
  };

  Createdb_opt_nameContext* createdb_opt_name();

  class  Equal_Context : public antlr4::ParserRuleContext {
  public:
    Equal_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *EQUAL();

   
  };

  Equal_Context* equal_();

  class  AlterdatabasestmtContext : public antlr4::ParserRuleContext {
  public:
    AlterdatabasestmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *DATABASE();
    std::vector<NameContext *> name();
    NameContext* name(size_t i);
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *SET();
    antlr4::tree::TerminalNode *TABLESPACE();
    Createdb_opt_listContext *createdb_opt_list();

   
  };

  AlterdatabasestmtContext* alterdatabasestmt();

  class  AlterdatabasesetstmtContext : public antlr4::ParserRuleContext {
  public:
    AlterdatabasesetstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *DATABASE();
    NameContext *name();
    SetresetclauseContext *setresetclause();

   
  };

  AlterdatabasesetstmtContext* alterdatabasesetstmt();

  class  DropdbstmtContext : public antlr4::ParserRuleContext {
  public:
    DropdbstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DROP();
    antlr4::tree::TerminalNode *DATABASE();
    NameContext *name();
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *EXISTS();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Drop_option_listContext *drop_option_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    With_Context *with_();

   
  };

  DropdbstmtContext* dropdbstmt();

  class  Drop_option_listContext : public antlr4::ParserRuleContext {
  public:
    Drop_option_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Drop_optionContext *> drop_option();
    Drop_optionContext* drop_option(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Drop_option_listContext* drop_option_list();

  class  Drop_optionContext : public antlr4::ParserRuleContext {
  public:
    Drop_optionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FORCE();

   
  };

  Drop_optionContext* drop_option();

  class  AltercollationstmtContext : public antlr4::ParserRuleContext {
  public:
    AltercollationstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *COLLATION();
    Any_nameContext *any_name();
    antlr4::tree::TerminalNode *REFRESH();
    antlr4::tree::TerminalNode *VERSION_P();

   
  };

  AltercollationstmtContext* altercollationstmt();

  class  AltersystemstmtContext : public antlr4::ParserRuleContext {
  public:
    AltersystemstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *SYSTEM_P();
    Generic_setContext *generic_set();
    antlr4::tree::TerminalNode *SET();
    antlr4::tree::TerminalNode *RESET();

   
  };

  AltersystemstmtContext* altersystemstmt();

  class  CreatedomainstmtContext : public antlr4::ParserRuleContext {
  public:
    CreatedomainstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *DOMAIN_P();
    Any_nameContext *any_name();
    TypenameContext *typename_();
    ColquallistContext *colquallist();
    As_Context *as_();

   
  };

  CreatedomainstmtContext* createdomainstmt();

  class  AlterdomainstmtContext : public antlr4::ParserRuleContext {
  public:
    AlterdomainstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *DOMAIN_P();
    Any_nameContext *any_name();
    Alter_column_defaultContext *alter_column_default();
    antlr4::tree::TerminalNode *DROP();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *NULL_P();
    antlr4::tree::TerminalNode *SET();
    antlr4::tree::TerminalNode *ADD_P();
    TableconstraintContext *tableconstraint();
    antlr4::tree::TerminalNode *CONSTRAINT();
    NameContext *name();
    antlr4::tree::TerminalNode *VALIDATE();
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *EXISTS();
    Drop_behavior_Context *drop_behavior_();

   
  };

  AlterdomainstmtContext* alterdomainstmt();

  class  As_Context : public antlr4::ParserRuleContext {
  public:
    As_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *AS();

   
  };

  As_Context* as_();

  class  AltertsdictionarystmtContext : public antlr4::ParserRuleContext {
  public:
    AltertsdictionarystmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *TEXT_P();
    antlr4::tree::TerminalNode *SEARCH();
    antlr4::tree::TerminalNode *DICTIONARY();
    Any_nameContext *any_name();
    DefinitionContext *definition();

   
  };

  AltertsdictionarystmtContext* altertsdictionarystmt();

  class  AltertsconfigurationstmtContext : public antlr4::ParserRuleContext {
  public:
    AltertsconfigurationstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<antlr4::tree::TerminalNode *> ALTER();
    antlr4::tree::TerminalNode* ALTER(size_t i);
    antlr4::tree::TerminalNode *TEXT_P();
    antlr4::tree::TerminalNode *SEARCH();
    antlr4::tree::TerminalNode *CONFIGURATION();
    std::vector<Any_nameContext *> any_name();
    Any_nameContext* any_name(size_t i);
    antlr4::tree::TerminalNode *ADD_P();
    antlr4::tree::TerminalNode *MAPPING();
    antlr4::tree::TerminalNode *FOR();
    Name_listContext *name_list();
    Any_withContext *any_with();
    Any_name_list_Context *any_name_list_();
    antlr4::tree::TerminalNode *REPLACE();
    antlr4::tree::TerminalNode *DROP();
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *EXISTS();

   
  };

  AltertsconfigurationstmtContext* altertsconfigurationstmt();

  class  Any_withContext : public antlr4::ParserRuleContext {
  public:
    Any_withContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WITH();

   
  };

  Any_withContext* any_with();

  class  CreateconversionstmtContext : public antlr4::ParserRuleContext {
  public:
    CreateconversionstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *CONVERSION_P();
    std::vector<Any_nameContext *> any_name();
    Any_nameContext* any_name(size_t i);
    antlr4::tree::TerminalNode *FOR();
    std::vector<SconstContext *> sconst();
    SconstContext* sconst(size_t i);
    antlr4::tree::TerminalNode *TO();
    antlr4::tree::TerminalNode *FROM();
    Default_Context *default_();

   
  };

  CreateconversionstmtContext* createconversionstmt();

  class  ClusterstmtContext : public antlr4::ParserRuleContext {
  public:
    ClusterstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CLUSTER();
    Qualified_nameContext *qualified_name();
    Verbose_Context *verbose_();
    Cluster_index_specificationContext *cluster_index_specification();
    NameContext *name();
    antlr4::tree::TerminalNode *ON();

   
  };

  ClusterstmtContext* clusterstmt();

  class  Cluster_index_specificationContext : public antlr4::ParserRuleContext {
  public:
    Cluster_index_specificationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *USING();
    NameContext *name();

   
  };

  Cluster_index_specificationContext* cluster_index_specification();

  class  VacuumstmtContext : public antlr4::ParserRuleContext {
  public:
    VacuumstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *VACUUM();
    Full_Context *full_();
    Freeze_Context *freeze_();
    Verbose_Context *verbose_();
    Analyze_Context *analyze_();
    Vacuum_relation_list_Context *vacuum_relation_list_();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Vac_analyze_option_listContext *vac_analyze_option_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  VacuumstmtContext* vacuumstmt();

  class  AnalyzestmtContext : public antlr4::ParserRuleContext {
  public:
    AnalyzestmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Analyze_keywordContext *analyze_keyword();
    Verbose_Context *verbose_();
    Vacuum_relation_list_Context *vacuum_relation_list_();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Vac_analyze_option_listContext *vac_analyze_option_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  AnalyzestmtContext* analyzestmt();

  class  Utility_option_listContext : public antlr4::ParserRuleContext {
  public:
    Utility_option_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Utility_option_elemContext *> utility_option_elem();
    Utility_option_elemContext* utility_option_elem(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Utility_option_listContext* utility_option_list();

  class  Vac_analyze_option_listContext : public antlr4::ParserRuleContext {
  public:
    Vac_analyze_option_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Vac_analyze_option_elemContext *> vac_analyze_option_elem();
    Vac_analyze_option_elemContext* vac_analyze_option_elem(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Vac_analyze_option_listContext* vac_analyze_option_list();

  class  Analyze_keywordContext : public antlr4::ParserRuleContext {
  public:
    Analyze_keywordContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ANALYZE();
    antlr4::tree::TerminalNode *ANALYSE();

   
  };

  Analyze_keywordContext* analyze_keyword();

  class  Utility_option_elemContext : public antlr4::ParserRuleContext {
  public:
    Utility_option_elemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Utility_option_nameContext *utility_option_name();
    Utility_option_argContext *utility_option_arg();

   
  };

  Utility_option_elemContext* utility_option_elem();

  class  Utility_option_nameContext : public antlr4::ParserRuleContext {
  public:
    Utility_option_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    NonreservedwordContext *nonreservedword();
    Analyze_keywordContext *analyze_keyword();
    antlr4::tree::TerminalNode *FORMAT_LA();

   
  };

  Utility_option_nameContext* utility_option_name();

  class  Utility_option_argContext : public antlr4::ParserRuleContext {
  public:
    Utility_option_argContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Boolean_or_string_Context *boolean_or_string_();
    NumericonlyContext *numericonly();

   
  };

  Utility_option_argContext* utility_option_arg();

  class  Vac_analyze_option_elemContext : public antlr4::ParserRuleContext {
  public:
    Vac_analyze_option_elemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Vac_analyze_option_nameContext *vac_analyze_option_name();
    Vac_analyze_option_argContext *vac_analyze_option_arg();

   
  };

  Vac_analyze_option_elemContext* vac_analyze_option_elem();

  class  Vac_analyze_option_nameContext : public antlr4::ParserRuleContext {
  public:
    Vac_analyze_option_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    NonreservedwordContext *nonreservedword();
    Analyze_keywordContext *analyze_keyword();

   
  };

  Vac_analyze_option_nameContext* vac_analyze_option_name();

  class  Vac_analyze_option_argContext : public antlr4::ParserRuleContext {
  public:
    Vac_analyze_option_argContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Boolean_or_string_Context *boolean_or_string_();
    NumericonlyContext *numericonly();

   
  };

  Vac_analyze_option_argContext* vac_analyze_option_arg();

  class  Analyze_Context : public antlr4::ParserRuleContext {
  public:
    Analyze_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Analyze_keywordContext *analyze_keyword();

   
  };

  Analyze_Context* analyze_();

  class  Verbose_Context : public antlr4::ParserRuleContext {
  public:
    Verbose_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *VERBOSE();

   
  };

  Verbose_Context* verbose_();

  class  Full_Context : public antlr4::ParserRuleContext {
  public:
    Full_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FULL();

   
  };

  Full_Context* full_();

  class  Freeze_Context : public antlr4::ParserRuleContext {
  public:
    Freeze_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FREEZE();

   
  };

  Freeze_Context* freeze_();

  class  Name_list_Context : public antlr4::ParserRuleContext {
  public:
    Name_list_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Name_listContext *name_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Name_list_Context* name_list_();

  class  Vacuum_relationContext : public antlr4::ParserRuleContext {
  public:
    Vacuum_relationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Qualified_nameContext *qualified_name();
    Name_list_Context *name_list_();

   
  };

  Vacuum_relationContext* vacuum_relation();

  class  Vacuum_relation_listContext : public antlr4::ParserRuleContext {
  public:
    Vacuum_relation_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Vacuum_relationContext *> vacuum_relation();
    Vacuum_relationContext* vacuum_relation(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Vacuum_relation_listContext* vacuum_relation_list();

  class  Vacuum_relation_list_Context : public antlr4::ParserRuleContext {
  public:
    Vacuum_relation_list_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Vacuum_relation_listContext *vacuum_relation_list();

   
  };

  Vacuum_relation_list_Context* vacuum_relation_list_();

  class  ExplainstmtContext : public antlr4::ParserRuleContext {
  public:
    ExplainstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *EXPLAIN();
    ExplainablestmtContext *explainablestmt();
    Analyze_keywordContext *analyze_keyword();
    Verbose_Context *verbose_();
    antlr4::tree::TerminalNode *VERBOSE();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Explain_option_listContext *explain_option_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  ExplainstmtContext* explainstmt();

  class  ExplainablestmtContext : public antlr4::ParserRuleContext {
  public:
    ExplainablestmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    SelectstmtContext *selectstmt();
    InsertstmtContext *insertstmt();
    UpdatestmtContext *updatestmt();
    DeletestmtContext *deletestmt();
    DeclarecursorstmtContext *declarecursorstmt();
    CreateasstmtContext *createasstmt();
    CreatematviewstmtContext *creatematviewstmt();
    RefreshmatviewstmtContext *refreshmatviewstmt();
    ExecutestmtContext *executestmt();

   
  };

  ExplainablestmtContext* explainablestmt();

  class  Explain_option_listContext : public antlr4::ParserRuleContext {
  public:
    Explain_option_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Explain_option_elemContext *> explain_option_elem();
    Explain_option_elemContext* explain_option_elem(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Explain_option_listContext* explain_option_list();

  class  Explain_option_elemContext : public antlr4::ParserRuleContext {
  public:
    Explain_option_elemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Explain_option_nameContext *explain_option_name();
    Explain_option_argContext *explain_option_arg();

   
  };

  Explain_option_elemContext* explain_option_elem();

  class  Explain_option_nameContext : public antlr4::ParserRuleContext {
  public:
    Explain_option_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    NonreservedwordContext *nonreservedword();
    Analyze_keywordContext *analyze_keyword();

   
  };

  Explain_option_nameContext* explain_option_name();

  class  Explain_option_argContext : public antlr4::ParserRuleContext {
  public:
    Explain_option_argContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Boolean_or_string_Context *boolean_or_string_();
    NumericonlyContext *numericonly();

   
  };

  Explain_option_argContext* explain_option_arg();

  class  PreparestmtContext : public antlr4::ParserRuleContext {
  public:
    PreparestmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PREPARE();
    NameContext *name();
    antlr4::tree::TerminalNode *AS();
    PreparablestmtContext *preparablestmt();
    Prep_type_clauseContext *prep_type_clause();

   
  };

  PreparestmtContext* preparestmt();

  class  Prep_type_clauseContext : public antlr4::ParserRuleContext {
  public:
    Prep_type_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Type_listContext *type_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Prep_type_clauseContext* prep_type_clause();

  class  PreparablestmtContext : public antlr4::ParserRuleContext {
  public:
    PreparablestmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    SelectstmtContext *selectstmt();
    InsertstmtContext *insertstmt();
    UpdatestmtContext *updatestmt();
    DeletestmtContext *deletestmt();

   
  };

  PreparablestmtContext* preparablestmt();

  class  ExecutestmtContext : public antlr4::ParserRuleContext {
  public:
    ExecutestmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *EXECUTE();
    NameContext *name();
    Execute_param_clauseContext *execute_param_clause();
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *TABLE();
    Create_as_targetContext *create_as_target();
    antlr4::tree::TerminalNode *AS();
    OpttempContext *opttemp();
    With_data_Context *with_data_();
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *EXISTS();

   
  };

  ExecutestmtContext* executestmt();

  class  Execute_param_clauseContext : public antlr4::ParserRuleContext {
  public:
    Execute_param_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Expr_listContext *expr_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Execute_param_clauseContext* execute_param_clause();

  class  DeallocatestmtContext : public antlr4::ParserRuleContext {
  public:
    DeallocatestmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DEALLOCATE();
    NameContext *name();
    antlr4::tree::TerminalNode *PREPARE();
    antlr4::tree::TerminalNode *ALL();

   
  };

  DeallocatestmtContext* deallocatestmt();

  class  InsertstmtContext : public antlr4::ParserRuleContext {
  public:
    InsertstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INSERT();
    antlr4::tree::TerminalNode *INTO();
    Insert_targetContext *insert_target();
    Insert_restContext *insert_rest();
    With_clause_Context *with_clause_();
    On_conflict_Context *on_conflict_();
    Returning_clauseContext *returning_clause();

   
  };

  InsertstmtContext* insertstmt();

  class  Insert_targetContext : public antlr4::ParserRuleContext {
  public:
    Insert_targetContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Qualified_nameContext *qualified_name();
    antlr4::tree::TerminalNode *AS();
    ColidContext *colid();

   
  };

  Insert_targetContext* insert_target();

  class  Insert_restContext : public antlr4::ParserRuleContext {
  public:
    Insert_restContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    SelectstmtContext *selectstmt();
    antlr4::tree::TerminalNode *OVERRIDING();
    Override_kindContext *override_kind();
    antlr4::tree::TerminalNode *VALUE_P();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Insert_column_listContext *insert_column_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    antlr4::tree::TerminalNode *DEFAULT();
    antlr4::tree::TerminalNode *VALUES();

   
  };

  Insert_restContext* insert_rest();

  class  Override_kindContext : public antlr4::ParserRuleContext {
  public:
    Override_kindContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *USER();
    antlr4::tree::TerminalNode *SYSTEM_P();

   
  };

  Override_kindContext* override_kind();

  class  Insert_column_listContext : public antlr4::ParserRuleContext {
  public:
    Insert_column_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Insert_column_itemContext *> insert_column_item();
    Insert_column_itemContext* insert_column_item(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Insert_column_listContext* insert_column_list();

  class  Insert_column_itemContext : public antlr4::ParserRuleContext {
  public:
    Insert_column_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ColidContext *colid();
    Opt_indirectionContext *opt_indirection();

   
  };

  Insert_column_itemContext* insert_column_item();

  class  On_conflict_Context : public antlr4::ParserRuleContext {
  public:
    On_conflict_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *CONFLICT();
    antlr4::tree::TerminalNode *DO();
    antlr4::tree::TerminalNode *UPDATE();
    antlr4::tree::TerminalNode *SET();
    Set_clause_listContext *set_clause_list();
    antlr4::tree::TerminalNode *NOTHING();
    Conf_expr_Context *conf_expr_();
    Where_clauseContext *where_clause();

   
  };

  On_conflict_Context* on_conflict_();

  class  Conf_expr_Context : public antlr4::ParserRuleContext {
  public:
    Conf_expr_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Index_paramsContext *index_params();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    Where_clauseContext *where_clause();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *CONSTRAINT();
    NameContext *name();

   
  };

  Conf_expr_Context* conf_expr_();

  class  Returning_clauseContext : public antlr4::ParserRuleContext {
  public:
    Returning_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RETURNING();
    Target_listContext *target_list();

   
  };

  Returning_clauseContext* returning_clause();

  class  MergestmtContext : public antlr4::ParserRuleContext {
  public:
    MergestmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *MERGE();
    std::vector<Qualified_nameContext *> qualified_name();
    Qualified_nameContext* qualified_name(size_t i);
    antlr4::tree::TerminalNode *USING();
    antlr4::tree::TerminalNode *ON();
    A_exprContext *a_expr();
    Select_with_parensContext *select_with_parens();
    Merge_insert_clauseContext *merge_insert_clause();
    Merge_update_clauseContext *merge_update_clause();
    antlr4::tree::TerminalNode *INTO();
    std::vector<Alias_clauseContext *> alias_clause();
    Alias_clauseContext* alias_clause(size_t i);
    Merge_delete_clauseContext *merge_delete_clause();

   
  };

  MergestmtContext* mergestmt();

  class  Merge_insert_clauseContext : public antlr4::ParserRuleContext {
  public:
    Merge_insert_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WHEN();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *MATCHED();
    antlr4::tree::TerminalNode *INSERT();
    Values_clauseContext *values_clause();
    antlr4::tree::TerminalNode *AND();
    A_exprContext *a_expr();
    antlr4::tree::TerminalNode *THEN();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Insert_column_listContext *insert_column_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Merge_insert_clauseContext* merge_insert_clause();

  class  Merge_update_clauseContext : public antlr4::ParserRuleContext {
  public:
    Merge_update_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WHEN();
    antlr4::tree::TerminalNode *MATCHED();
    antlr4::tree::TerminalNode *UPDATE();
    antlr4::tree::TerminalNode *SET();
    Set_clause_listContext *set_clause_list();
    antlr4::tree::TerminalNode *AND();
    A_exprContext *a_expr();
    antlr4::tree::TerminalNode *THEN();

   
  };

  Merge_update_clauseContext* merge_update_clause();

  class  Merge_delete_clauseContext : public antlr4::ParserRuleContext {
  public:
    Merge_delete_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WHEN();
    antlr4::tree::TerminalNode *MATCHED();
    antlr4::tree::TerminalNode *DELETE_P();
    antlr4::tree::TerminalNode *THEN();

   
  };

  Merge_delete_clauseContext* merge_delete_clause();

  class  DeletestmtContext : public antlr4::ParserRuleContext {
  public:
    DeletestmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DELETE_P();
    antlr4::tree::TerminalNode *FROM();
    Relation_expr_opt_aliasContext *relation_expr_opt_alias();
    With_clause_Context *with_clause_();
    Using_clauseContext *using_clause();
    Where_or_current_clauseContext *where_or_current_clause();
    Returning_clauseContext *returning_clause();

   
  };

  DeletestmtContext* deletestmt();

  class  Using_clauseContext : public antlr4::ParserRuleContext {
  public:
    Using_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *USING();
    From_listContext *from_list();

   
  };

  Using_clauseContext* using_clause();

  class  LockstmtContext : public antlr4::ParserRuleContext {
  public:
    LockstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LOCK_P();
    Relation_expr_listContext *relation_expr_list();
    Table_Context *table_();
    Lock_Context *lock_();
    Nowait_Context *nowait_();

   
  };

  LockstmtContext* lockstmt();

  class  Lock_Context : public antlr4::ParserRuleContext {
  public:
    Lock_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *IN_P();
    Lock_typeContext *lock_type();
    antlr4::tree::TerminalNode *MODE();

   
  };

  Lock_Context* lock_();

  class  Lock_typeContext : public antlr4::ParserRuleContext {
  public:
    Lock_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ACCESS();
    antlr4::tree::TerminalNode *SHARE();
    antlr4::tree::TerminalNode *EXCLUSIVE();
    antlr4::tree::TerminalNode *ROW();
    antlr4::tree::TerminalNode *UPDATE();

   
  };

  Lock_typeContext* lock_type();

  class  Nowait_Context : public antlr4::ParserRuleContext {
  public:
    Nowait_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NOWAIT();

   
  };

  Nowait_Context* nowait_();

  class  Nowait_or_skip_Context : public antlr4::ParserRuleContext {
  public:
    Nowait_or_skip_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NOWAIT();
    antlr4::tree::TerminalNode *SKIP_P();
    antlr4::tree::TerminalNode *LOCKED();

   
  };

  Nowait_or_skip_Context* nowait_or_skip_();

  class  UpdatestmtContext : public antlr4::ParserRuleContext {
  public:
    UpdatestmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *UPDATE();
    Relation_expr_opt_aliasContext *relation_expr_opt_alias();
    antlr4::tree::TerminalNode *SET();
    Set_clause_listContext *set_clause_list();
    With_clause_Context *with_clause_();
    From_clauseContext *from_clause();
    Where_or_current_clauseContext *where_or_current_clause();
    Returning_clauseContext *returning_clause();

   
  };

  UpdatestmtContext* updatestmt();

  class  Set_clause_listContext : public antlr4::ParserRuleContext {
  public:
    Set_clause_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Set_clauseContext *> set_clause();
    Set_clauseContext* set_clause(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Set_clause_listContext* set_clause_list();

  class  Set_clauseContext : public antlr4::ParserRuleContext {
  public:
    Set_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Set_targetContext *set_target();
    antlr4::tree::TerminalNode *EQUAL();
    A_exprContext *a_expr();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Set_target_listContext *set_target_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Set_clauseContext* set_clause();

  class  Set_targetContext : public antlr4::ParserRuleContext {
  public:
    Set_targetContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ColidContext *colid();
    Opt_indirectionContext *opt_indirection();

   
  };

  Set_targetContext* set_target();

  class  Set_target_listContext : public antlr4::ParserRuleContext {
  public:
    Set_target_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Set_targetContext *> set_target();
    Set_targetContext* set_target(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Set_target_listContext* set_target_list();

  class  DeclarecursorstmtContext : public antlr4::ParserRuleContext {
  public:
    DeclarecursorstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DECLARE();
    Cursor_nameContext *cursor_name();
    Cursor_optionsContext *cursor_options();
    antlr4::tree::TerminalNode *CURSOR();
    antlr4::tree::TerminalNode *FOR();
    SelectstmtContext *selectstmt();
    Hold_Context *hold_();

   
  };

  DeclarecursorstmtContext* declarecursorstmt();

  class  Cursor_nameContext : public antlr4::ParserRuleContext {
  public:
    Cursor_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    NameContext *name();

   
  };

  Cursor_nameContext* cursor_name();

  class  Cursor_optionsContext : public antlr4::ParserRuleContext {
  public:
    Cursor_optionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<antlr4::tree::TerminalNode *> NO();
    antlr4::tree::TerminalNode* NO(size_t i);
    std::vector<antlr4::tree::TerminalNode *> SCROLL();
    antlr4::tree::TerminalNode* SCROLL(size_t i);
    std::vector<antlr4::tree::TerminalNode *> BINARY();
    antlr4::tree::TerminalNode* BINARY(size_t i);
    std::vector<antlr4::tree::TerminalNode *> INSENSITIVE();
    antlr4::tree::TerminalNode* INSENSITIVE(size_t i);

   
  };

  Cursor_optionsContext* cursor_options();

  class  Hold_Context : public antlr4::ParserRuleContext {
  public:
    Hold_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *HOLD();
    antlr4::tree::TerminalNode *WITHOUT();

   
  };

  Hold_Context* hold_();

  class  SelectstmtContext : public antlr4::ParserRuleContext {
  public:
    SelectstmtContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Select_no_parensContext *select_no_parens();
    Select_with_parensContext *select_with_parens();

   
  };

  SelectstmtContext* selectstmt();

  class  Select_with_parensContext : public antlr4::ParserRuleContext {
  public:
    Select_with_parensContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Select_no_parensContext *select_no_parens();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    Select_with_parensContext *select_with_parens();

   
  };

  Select_with_parensContext* select_with_parens();

  class  Select_no_parensContext : public antlr4::ParserRuleContext {
  public:
    Select_no_parensContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Select_clauseContext *select_clause();
    Sort_clause_Context *sort_clause_();
    For_locking_clauseContext *for_locking_clause();
    Select_limitContext *select_limit();
    Select_limit_Context *select_limit_();
    For_locking_clause_Context *for_locking_clause_();
    With_clauseContext *with_clause();

   
  };

  Select_no_parensContext* select_no_parens();

  class  Select_clauseContext : public antlr4::ParserRuleContext {
  public:
    Select_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Simple_select_intersectContext *> simple_select_intersect();
    Simple_select_intersectContext* simple_select_intersect(size_t i);
    std::vector<antlr4::tree::TerminalNode *> UNION();
    antlr4::tree::TerminalNode* UNION(size_t i);
    std::vector<antlr4::tree::TerminalNode *> EXCEPT();
    antlr4::tree::TerminalNode* EXCEPT(size_t i);
    std::vector<All_or_distinctContext *> all_or_distinct();
    All_or_distinctContext* all_or_distinct(size_t i);

   
  };

  Select_clauseContext* select_clause();

  class  Simple_select_intersectContext : public antlr4::ParserRuleContext {
  public:
    Simple_select_intersectContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Simple_select_pramaryContext *> simple_select_pramary();
    Simple_select_pramaryContext* simple_select_pramary(size_t i);
    std::vector<antlr4::tree::TerminalNode *> INTERSECT();
    antlr4::tree::TerminalNode* INTERSECT(size_t i);
    std::vector<All_or_distinctContext *> all_or_distinct();
    All_or_distinctContext* all_or_distinct(size_t i);

   
  };

  Simple_select_intersectContext* simple_select_intersect();

  class  Simple_select_pramaryContext : public antlr4::ParserRuleContext {
  public:
    Simple_select_pramaryContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SELECT();
    Distinct_clauseContext *distinct_clause();
    Target_listContext *target_list();
    All_clause_Context *all_clause_();
    Target_list_Context *target_list_();
    Into_clauseContext *into_clause();
    From_clauseContext *from_clause();
    Where_clauseContext *where_clause();
    Group_clauseContext *group_clause();
    Having_clauseContext *having_clause();
    Window_clauseContext *window_clause();
    Values_clauseContext *values_clause();
    antlr4::tree::TerminalNode *TABLE();
    Relation_exprContext *relation_expr();
    Select_with_parensContext *select_with_parens();

   
  };

  Simple_select_pramaryContext* simple_select_pramary();

  class  With_clauseContext : public antlr4::ParserRuleContext {
  public:
    With_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WITH();
    Cte_listContext *cte_list();
    antlr4::tree::TerminalNode *RECURSIVE();

   
  };

  With_clauseContext* with_clause();

  class  Cte_listContext : public antlr4::ParserRuleContext {
  public:
    Cte_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Common_table_exprContext *> common_table_expr();
    Common_table_exprContext* common_table_expr(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Cte_listContext* cte_list();

  class  Common_table_exprContext : public antlr4::ParserRuleContext {
  public:
    Common_table_exprContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    NameContext *name();
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    PreparablestmtContext *preparablestmt();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    Name_list_Context *name_list_();
    Materialized_Context *materialized_();

   
  };

  Common_table_exprContext* common_table_expr();

  class  Materialized_Context : public antlr4::ParserRuleContext {
  public:
    Materialized_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *MATERIALIZED();
    antlr4::tree::TerminalNode *NOT();

   
  };

  Materialized_Context* materialized_();

  class  With_clause_Context : public antlr4::ParserRuleContext {
  public:
    With_clause_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    With_clauseContext *with_clause();

   
  };

  With_clause_Context* with_clause_();

  class  Into_clauseContext : public antlr4::ParserRuleContext {
  public:
    Into_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INTO();
    OpttempTableNameContext *opttempTableName();

   
  };

  Into_clauseContext* into_clause();

  class  Strict_Context : public antlr4::ParserRuleContext {
  public:
    Strict_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *STRICT_P();

   
  };

  Strict_Context* strict_();

  class  OpttempTableNameContext : public antlr4::ParserRuleContext {
  public:
    OpttempTableNameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Qualified_nameContext *qualified_name();
    antlr4::tree::TerminalNode *TEMPORARY();
    antlr4::tree::TerminalNode *TEMP();
    Table_Context *table_();
    antlr4::tree::TerminalNode *LOCAL();
    antlr4::tree::TerminalNode *GLOBAL();
    antlr4::tree::TerminalNode *UNLOGGED();
    antlr4::tree::TerminalNode *TABLE();

   
  };

  OpttempTableNameContext* opttempTableName();

  class  Table_Context : public antlr4::ParserRuleContext {
  public:
    Table_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TABLE();

   
  };

  Table_Context* table_();

  class  All_or_distinctContext : public antlr4::ParserRuleContext {
  public:
    All_or_distinctContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *DISTINCT();

   
  };

  All_or_distinctContext* all_or_distinct();

  class  Distinct_clauseContext : public antlr4::ParserRuleContext {
  public:
    Distinct_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DISTINCT();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Expr_listContext *expr_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Distinct_clauseContext* distinct_clause();

  class  All_clause_Context : public antlr4::ParserRuleContext {
  public:
    All_clause_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALL();

   
  };

  All_clause_Context* all_clause_();

  class  Sort_clause_Context : public antlr4::ParserRuleContext {
  public:
    Sort_clause_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Sort_clauseContext *sort_clause();

   
  };

  Sort_clause_Context* sort_clause_();

  class  Sort_clauseContext : public antlr4::ParserRuleContext {
  public:
    Sort_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ORDER();
    antlr4::tree::TerminalNode *BY();
    Sortby_listContext *sortby_list();

   
  };

  Sort_clauseContext* sort_clause();

  class  Sortby_listContext : public antlr4::ParserRuleContext {
  public:
    Sortby_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<SortbyContext *> sortby();
    SortbyContext* sortby(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Sortby_listContext* sortby_list();

  class  SortbyContext : public antlr4::ParserRuleContext {
  public:
    SortbyContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    A_exprContext *a_expr();
    antlr4::tree::TerminalNode *USING();
    Qual_all_opContext *qual_all_op();
    Nulls_order_Context *nulls_order_();
    Asc_desc_Context *asc_desc_();

   
  };

  SortbyContext* sortby();

  class  Select_limitContext : public antlr4::ParserRuleContext {
  public:
    Select_limitContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Limit_clauseContext *limit_clause();
    Offset_clauseContext *offset_clause();

   
  };

  Select_limitContext* select_limit();

  class  Select_limit_Context : public antlr4::ParserRuleContext {
  public:
    Select_limit_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Select_limitContext *select_limit();

   
  };

  Select_limit_Context* select_limit_();

  class  Limit_clauseContext : public antlr4::ParserRuleContext {
  public:
    Limit_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LIMIT();
    Select_limit_valueContext *select_limit_value();
    antlr4::tree::TerminalNode *COMMA();
    Select_offset_valueContext *select_offset_value();
    antlr4::tree::TerminalNode *FETCH();
    First_or_nextContext *first_or_next();
    Select_fetch_first_valueContext *select_fetch_first_value();
    Row_or_rowsContext *row_or_rows();
    antlr4::tree::TerminalNode *ONLY();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *TIES();

   
  };

  Limit_clauseContext* limit_clause();

  class  Offset_clauseContext : public antlr4::ParserRuleContext {
  public:
    Offset_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OFFSET();
    Select_offset_valueContext *select_offset_value();
    Select_fetch_first_valueContext *select_fetch_first_value();
    Row_or_rowsContext *row_or_rows();

   
  };

  Offset_clauseContext* offset_clause();

  class  Select_limit_valueContext : public antlr4::ParserRuleContext {
  public:
    Select_limit_valueContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    A_exprContext *a_expr();
    antlr4::tree::TerminalNode *ALL();

   
  };

  Select_limit_valueContext* select_limit_value();

  class  Select_offset_valueContext : public antlr4::ParserRuleContext {
  public:
    Select_offset_valueContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    A_exprContext *a_expr();

   
  };

  Select_offset_valueContext* select_offset_value();

  class  Select_fetch_first_valueContext : public antlr4::ParserRuleContext {
  public:
    Select_fetch_first_valueContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    C_exprContext *c_expr();
    antlr4::tree::TerminalNode *PLUS();
    I_or_f_constContext *i_or_f_const();
    antlr4::tree::TerminalNode *MINUS();

   
  };

  Select_fetch_first_valueContext* select_fetch_first_value();

  class  I_or_f_constContext : public antlr4::ParserRuleContext {
  public:
    I_or_f_constContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IconstContext *iconst();
    FconstContext *fconst();

   
  };

  I_or_f_constContext* i_or_f_const();

  class  Row_or_rowsContext : public antlr4::ParserRuleContext {
  public:
    Row_or_rowsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ROW();
    antlr4::tree::TerminalNode *ROWS();

   
  };

  Row_or_rowsContext* row_or_rows();

  class  First_or_nextContext : public antlr4::ParserRuleContext {
  public:
    First_or_nextContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FIRST_P();
    antlr4::tree::TerminalNode *NEXT();

   
  };

  First_or_nextContext* first_or_next();

  class  Group_clauseContext : public antlr4::ParserRuleContext {
  public:
    Group_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GROUP_P();
    antlr4::tree::TerminalNode *BY();
    Group_by_listContext *group_by_list();

   
  };

  Group_clauseContext* group_clause();

  class  Group_by_listContext : public antlr4::ParserRuleContext {
  public:
    Group_by_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Group_by_itemContext *> group_by_item();
    Group_by_itemContext* group_by_item(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Group_by_listContext* group_by_list();

  class  Group_by_itemContext : public antlr4::ParserRuleContext {
  public:
    Group_by_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Empty_grouping_setContext *empty_grouping_set();
    Cube_clauseContext *cube_clause();
    Rollup_clauseContext *rollup_clause();
    Grouping_sets_clauseContext *grouping_sets_clause();
    A_exprContext *a_expr();

   
  };

  Group_by_itemContext* group_by_item();

  class  Empty_grouping_setContext : public antlr4::ParserRuleContext {
  public:
    Empty_grouping_setContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OPEN_PAREN();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Empty_grouping_setContext* empty_grouping_set();

  class  Rollup_clauseContext : public antlr4::ParserRuleContext {
  public:
    Rollup_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ROLLUP();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Expr_listContext *expr_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Rollup_clauseContext* rollup_clause();

  class  Cube_clauseContext : public antlr4::ParserRuleContext {
  public:
    Cube_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CUBE();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Expr_listContext *expr_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Cube_clauseContext* cube_clause();

  class  Grouping_sets_clauseContext : public antlr4::ParserRuleContext {
  public:
    Grouping_sets_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GROUPING();
    antlr4::tree::TerminalNode *SETS();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Group_by_listContext *group_by_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Grouping_sets_clauseContext* grouping_sets_clause();

  class  Having_clauseContext : public antlr4::ParserRuleContext {
  public:
    Having_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *HAVING();
    A_exprContext *a_expr();

   
  };

  Having_clauseContext* having_clause();

  class  For_locking_clauseContext : public antlr4::ParserRuleContext {
  public:
    For_locking_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    For_locking_itemsContext *for_locking_items();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *READ();
    antlr4::tree::TerminalNode *ONLY();

   
  };

  For_locking_clauseContext* for_locking_clause();

  class  For_locking_clause_Context : public antlr4::ParserRuleContext {
  public:
    For_locking_clause_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    For_locking_clauseContext *for_locking_clause();

   
  };

  For_locking_clause_Context* for_locking_clause_();

  class  For_locking_itemsContext : public antlr4::ParserRuleContext {
  public:
    For_locking_itemsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<For_locking_itemContext *> for_locking_item();
    For_locking_itemContext* for_locking_item(size_t i);

   
  };

  For_locking_itemsContext* for_locking_items();

  class  For_locking_itemContext : public antlr4::ParserRuleContext {
  public:
    For_locking_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    For_locking_strengthContext *for_locking_strength();
    Locked_rels_listContext *locked_rels_list();
    Nowait_or_skip_Context *nowait_or_skip_();

   
  };

  For_locking_itemContext* for_locking_item();

  class  For_locking_strengthContext : public antlr4::ParserRuleContext {
  public:
    For_locking_strengthContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *UPDATE();
    antlr4::tree::TerminalNode *SHARE();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *KEY();

   
  };

  For_locking_strengthContext* for_locking_strength();

  class  Locked_rels_listContext : public antlr4::ParserRuleContext {
  public:
    Locked_rels_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OF();
    Qualified_name_listContext *qualified_name_list();

   
  };

  Locked_rels_listContext* locked_rels_list();

  class  Values_clauseContext : public antlr4::ParserRuleContext {
  public:
    Values_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *VALUES();
    std::vector<antlr4::tree::TerminalNode *> OPEN_PAREN();
    antlr4::tree::TerminalNode* OPEN_PAREN(size_t i);
    std::vector<Expr_listContext *> expr_list();
    Expr_listContext* expr_list(size_t i);
    std::vector<antlr4::tree::TerminalNode *> CLOSE_PAREN();
    antlr4::tree::TerminalNode* CLOSE_PAREN(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Values_clauseContext* values_clause();

  class  From_clauseContext : public antlr4::ParserRuleContext {
  public:
    From_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FROM();
    From_listContext *from_list();

   
  };

  From_clauseContext* from_clause();

  class  From_listContext : public antlr4::ParserRuleContext {
  public:
    From_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Table_refContext *> table_ref();
    Table_refContext* table_ref(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  From_listContext* from_list();

  class  Table_refContext : public antlr4::ParserRuleContext {
  public:
    Table_refContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Relation_exprContext *relation_expr();
    Func_tableContext *func_table();
    XmltableContext *xmltable();
    Select_with_parensContext *select_with_parens();
    antlr4::tree::TerminalNode *LATERAL_P();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    std::vector<Table_refContext *> table_ref();
    Table_refContext* table_ref(size_t i);
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    std::vector<antlr4::tree::TerminalNode *> CROSS();
    antlr4::tree::TerminalNode* CROSS(size_t i);
    std::vector<antlr4::tree::TerminalNode *> JOIN();
    antlr4::tree::TerminalNode* JOIN(size_t i);
    std::vector<antlr4::tree::TerminalNode *> NATURAL();
    antlr4::tree::TerminalNode* NATURAL(size_t i);
    std::vector<Join_qualContext *> join_qual();
    Join_qualContext* join_qual(size_t i);
    Alias_clauseContext *alias_clause();
    Tablesample_clauseContext *tablesample_clause();
    Func_alias_clauseContext *func_alias_clause();
    std::vector<Join_typeContext *> join_type();
    Join_typeContext* join_type(size_t i);

   
  };

  Table_refContext* table_ref();

  class  Alias_clauseContext : public antlr4::ParserRuleContext {
  public:
    Alias_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ColidContext *colid();
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Name_listContext *name_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Alias_clauseContext* alias_clause();

  class  Func_alias_clauseContext : public antlr4::ParserRuleContext {
  public:
    Func_alias_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Alias_clauseContext *alias_clause();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    TablefuncelementlistContext *tablefuncelementlist();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    antlr4::tree::TerminalNode *AS();
    ColidContext *colid();

   
  };

  Func_alias_clauseContext* func_alias_clause();

  class  Join_typeContext : public antlr4::ParserRuleContext {
  public:
    Join_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FULL();
    antlr4::tree::TerminalNode *LEFT();
    antlr4::tree::TerminalNode *RIGHT();
    antlr4::tree::TerminalNode *INNER_P();
    antlr4::tree::TerminalNode *OUTER_P();

   
  };

  Join_typeContext* join_type();

  class  Join_qualContext : public antlr4::ParserRuleContext {
  public:
    Join_qualContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *USING();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Name_listContext *name_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    antlr4::tree::TerminalNode *ON();
    A_exprContext *a_expr();

   
  };

  Join_qualContext* join_qual();

  class  Relation_exprContext : public antlr4::ParserRuleContext {
  public:
    Relation_exprContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Qualified_nameContext *qualified_name();
    antlr4::tree::TerminalNode *STAR();
    antlr4::tree::TerminalNode *ONLY();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Relation_exprContext* relation_expr();

  class  Relation_expr_listContext : public antlr4::ParserRuleContext {
  public:
    Relation_expr_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Relation_exprContext *> relation_expr();
    Relation_exprContext* relation_expr(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Relation_expr_listContext* relation_expr_list();

  class  Relation_expr_opt_aliasContext : public antlr4::ParserRuleContext {
  public:
    Relation_expr_opt_aliasContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Relation_exprContext *relation_expr();
    ColidContext *colid();
    antlr4::tree::TerminalNode *AS();

   
  };

  Relation_expr_opt_aliasContext* relation_expr_opt_alias();

  class  Tablesample_clauseContext : public antlr4::ParserRuleContext {
  public:
    Tablesample_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TABLESAMPLE();
    Func_nameContext *func_name();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Expr_listContext *expr_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    Repeatable_clause_Context *repeatable_clause_();

   
  };

  Tablesample_clauseContext* tablesample_clause();

  class  Repeatable_clause_Context : public antlr4::ParserRuleContext {
  public:
    Repeatable_clause_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REPEATABLE();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    A_exprContext *a_expr();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Repeatable_clause_Context* repeatable_clause_();

  class  Func_tableContext : public antlr4::ParserRuleContext {
  public:
    Func_tableContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Func_expr_windowlessContext *func_expr_windowless();
    Ordinality_Context *ordinality_();
    antlr4::tree::TerminalNode *ROWS();
    antlr4::tree::TerminalNode *FROM();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Rowsfrom_listContext *rowsfrom_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Func_tableContext* func_table();

  class  Rowsfrom_itemContext : public antlr4::ParserRuleContext {
  public:
    Rowsfrom_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Func_expr_windowlessContext *func_expr_windowless();
    Col_def_list_Context *col_def_list_();

   
  };

  Rowsfrom_itemContext* rowsfrom_item();

  class  Rowsfrom_listContext : public antlr4::ParserRuleContext {
  public:
    Rowsfrom_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Rowsfrom_itemContext *> rowsfrom_item();
    Rowsfrom_itemContext* rowsfrom_item(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Rowsfrom_listContext* rowsfrom_list();

  class  Col_def_list_Context : public antlr4::ParserRuleContext {
  public:
    Col_def_list_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    TablefuncelementlistContext *tablefuncelementlist();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Col_def_list_Context* col_def_list_();

  class  Ordinality_Context : public antlr4::ParserRuleContext {
  public:
    Ordinality_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *ORDINALITY();

   
  };

  Ordinality_Context* ordinality_();

  class  Where_clauseContext : public antlr4::ParserRuleContext {
  public:
    Where_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WHERE();
    A_exprContext *a_expr();

   
  };

  Where_clauseContext* where_clause();

  class  Where_or_current_clauseContext : public antlr4::ParserRuleContext {
  public:
    Where_or_current_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WHERE();
    antlr4::tree::TerminalNode *CURRENT_P();
    antlr4::tree::TerminalNode *OF();
    Cursor_nameContext *cursor_name();
    A_exprContext *a_expr();

   
  };

  Where_or_current_clauseContext* where_or_current_clause();

  class  OpttablefuncelementlistContext : public antlr4::ParserRuleContext {
  public:
    OpttablefuncelementlistContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TablefuncelementlistContext *tablefuncelementlist();

   
  };

  OpttablefuncelementlistContext* opttablefuncelementlist();

  class  TablefuncelementlistContext : public antlr4::ParserRuleContext {
  public:
    TablefuncelementlistContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<TablefuncelementContext *> tablefuncelement();
    TablefuncelementContext* tablefuncelement(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  TablefuncelementlistContext* tablefuncelementlist();

  class  TablefuncelementContext : public antlr4::ParserRuleContext {
  public:
    TablefuncelementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ColidContext *colid();
    TypenameContext *typename_();
    Collate_clause_Context *collate_clause_();

   
  };

  TablefuncelementContext* tablefuncelement();

  class  XmltableContext : public antlr4::ParserRuleContext {
  public:
    XmltableContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *XMLTABLE();
    std::vector<antlr4::tree::TerminalNode *> OPEN_PAREN();
    antlr4::tree::TerminalNode* OPEN_PAREN(size_t i);
    std::vector<antlr4::tree::TerminalNode *> CLOSE_PAREN();
    antlr4::tree::TerminalNode* CLOSE_PAREN(size_t i);
    C_exprContext *c_expr();
    Xmlexists_argumentContext *xmlexists_argument();
    antlr4::tree::TerminalNode *COLUMNS();
    Xmltable_column_listContext *xmltable_column_list();
    antlr4::tree::TerminalNode *XMLNAMESPACES();
    Xml_namespace_listContext *xml_namespace_list();
    antlr4::tree::TerminalNode *COMMA();

   
  };

  XmltableContext* xmltable();

  class  Xmltable_column_listContext : public antlr4::ParserRuleContext {
  public:
    Xmltable_column_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Xmltable_column_elContext *> xmltable_column_el();
    Xmltable_column_elContext* xmltable_column_el(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Xmltable_column_listContext* xmltable_column_list();

  class  Xmltable_column_elContext : public antlr4::ParserRuleContext {
  public:
    Xmltable_column_elContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ColidContext *colid();
    TypenameContext *typename_();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *ORDINALITY();
    Xmltable_column_option_listContext *xmltable_column_option_list();

   
  };

  Xmltable_column_elContext* xmltable_column_el();

  class  Xmltable_column_option_listContext : public antlr4::ParserRuleContext {
  public:
    Xmltable_column_option_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Xmltable_column_option_elContext *> xmltable_column_option_el();
    Xmltable_column_option_elContext* xmltable_column_option_el(size_t i);

   
  };

  Xmltable_column_option_listContext* xmltable_column_option_list();

  class  Xmltable_column_option_elContext : public antlr4::ParserRuleContext {
  public:
    Xmltable_column_option_elContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DEFAULT();
    A_exprContext *a_expr();
    IdentifierContext *identifier();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *NULL_P();

   
  };

  Xmltable_column_option_elContext* xmltable_column_option_el();

  class  Xml_namespace_listContext : public antlr4::ParserRuleContext {
  public:
    Xml_namespace_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Xml_namespace_elContext *> xml_namespace_el();
    Xml_namespace_elContext* xml_namespace_el(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Xml_namespace_listContext* xml_namespace_list();

  class  Xml_namespace_elContext : public antlr4::ParserRuleContext {
  public:
    Xml_namespace_elContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    B_exprContext *b_expr();
    antlr4::tree::TerminalNode *AS();
    ColLabelContext *colLabel();
    antlr4::tree::TerminalNode *DEFAULT();

   
  };

  Xml_namespace_elContext* xml_namespace_el();

  class  TypenameContext : public antlr4::ParserRuleContext {
  public:
    TypenameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    SimpletypenameContext *simpletypename();
    Opt_array_boundsContext *opt_array_bounds();
    antlr4::tree::TerminalNode *ARRAY();
    antlr4::tree::TerminalNode *SETOF();
    antlr4::tree::TerminalNode *OPEN_BRACKET();
    IconstContext *iconst();
    antlr4::tree::TerminalNode *CLOSE_BRACKET();

   
  };

  TypenameContext* typename_();

  class  Opt_array_boundsContext : public antlr4::ParserRuleContext {
  public:
    Opt_array_boundsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<antlr4::tree::TerminalNode *> OPEN_BRACKET();
    antlr4::tree::TerminalNode* OPEN_BRACKET(size_t i);
    std::vector<antlr4::tree::TerminalNode *> CLOSE_BRACKET();
    antlr4::tree::TerminalNode* CLOSE_BRACKET(size_t i);
    std::vector<IconstContext *> iconst();
    IconstContext* iconst(size_t i);

   
  };

  Opt_array_boundsContext* opt_array_bounds();

  class  SimpletypenameContext : public antlr4::ParserRuleContext {
  public:
    SimpletypenameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    GenerictypeContext *generictype();
    NumericContext *numeric();
    BitContext *bit();
    CharacterContext *character();
    ConstdatetimeContext *constdatetime();
    ConstintervalContext *constinterval();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    IconstContext *iconst();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    Interval_Context *interval_();
    JsonTypeContext *jsonType();

   
  };

  SimpletypenameContext* simpletypename();

  class  ConsttypenameContext : public antlr4::ParserRuleContext {
  public:
    ConsttypenameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    NumericContext *numeric();
    ConstbitContext *constbit();
    ConstcharacterContext *constcharacter();
    ConstdatetimeContext *constdatetime();
    JsonTypeContext *jsonType();

   
  };

  ConsttypenameContext* consttypename();

  class  GenerictypeContext : public antlr4::ParserRuleContext {
  public:
    GenerictypeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Type_function_nameContext *type_function_name();
    AttrsContext *attrs();
    Type_modifiers_Context *type_modifiers_();

   
  };

  GenerictypeContext* generictype();

  class  Type_modifiers_Context : public antlr4::ParserRuleContext {
  public:
    Type_modifiers_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Expr_listContext *expr_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Type_modifiers_Context* type_modifiers_();

  class  NumericContext : public antlr4::ParserRuleContext {
  public:
    NumericContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INT_P();
    antlr4::tree::TerminalNode *INTEGER();
    antlr4::tree::TerminalNode *SMALLINT();
    antlr4::tree::TerminalNode *BIGINT();
    antlr4::tree::TerminalNode *REAL();
    antlr4::tree::TerminalNode *FLOAT_P();
    Float_Context *float_();
    antlr4::tree::TerminalNode *DOUBLE_P();
    antlr4::tree::TerminalNode *PRECISION();
    antlr4::tree::TerminalNode *DECIMAL_P();
    Type_modifiers_Context *type_modifiers_();
    antlr4::tree::TerminalNode *DEC();
    antlr4::tree::TerminalNode *NUMERIC();
    antlr4::tree::TerminalNode *BOOLEAN_P();

   
  };

  NumericContext* numeric();

  class  Float_Context : public antlr4::ParserRuleContext {
  public:
    Float_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OPEN_PAREN();
    IconstContext *iconst();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Float_Context* float_();

  class  BitContext : public antlr4::ParserRuleContext {
  public:
    BitContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    BitwithlengthContext *bitwithlength();
    BitwithoutlengthContext *bitwithoutlength();

   
  };

  BitContext* bit();

  class  ConstbitContext : public antlr4::ParserRuleContext {
  public:
    ConstbitContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    BitwithlengthContext *bitwithlength();
    BitwithoutlengthContext *bitwithoutlength();

   
  };

  ConstbitContext* constbit();

  class  BitwithlengthContext : public antlr4::ParserRuleContext {
  public:
    BitwithlengthContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BIT();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Expr_listContext *expr_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    Varying_Context *varying_();

   
  };

  BitwithlengthContext* bitwithlength();

  class  BitwithoutlengthContext : public antlr4::ParserRuleContext {
  public:
    BitwithoutlengthContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BIT();
    Varying_Context *varying_();

   
  };

  BitwithoutlengthContext* bitwithoutlength();

  class  CharacterContext : public antlr4::ParserRuleContext {
  public:
    CharacterContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Character_cContext *character_c();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    IconstContext *iconst();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  CharacterContext* character();

  class  ConstcharacterContext : public antlr4::ParserRuleContext {
  public:
    ConstcharacterContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Character_cContext *character_c();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    IconstContext *iconst();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  ConstcharacterContext* constcharacter();

  class  Character_cContext : public antlr4::ParserRuleContext {
  public:
    Character_cContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CHARACTER();
    antlr4::tree::TerminalNode *CHAR_P();
    antlr4::tree::TerminalNode *NCHAR();
    Varying_Context *varying_();
    antlr4::tree::TerminalNode *VARCHAR();
    antlr4::tree::TerminalNode *NATIONAL();

   
  };

  Character_cContext* character_c();

  class  Varying_Context : public antlr4::ParserRuleContext {
  public:
    Varying_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *VARYING();

   
  };

  Varying_Context* varying_();

  class  ConstdatetimeContext : public antlr4::ParserRuleContext {
  public:
    ConstdatetimeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TIMESTAMP();
    antlr4::tree::TerminalNode *TIME();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    IconstContext *iconst();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    Timezone_Context *timezone_();

   
  };

  ConstdatetimeContext* constdatetime();

  class  ConstintervalContext : public antlr4::ParserRuleContext {
  public:
    ConstintervalContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INTERVAL();

   
  };

  ConstintervalContext* constinterval();

  class  Timezone_Context : public antlr4::ParserRuleContext {
  public:
    Timezone_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *TIME();
    antlr4::tree::TerminalNode *ZONE();
    antlr4::tree::TerminalNode *WITHOUT();

   
  };

  Timezone_Context* timezone_();

  class  Interval_Context : public antlr4::ParserRuleContext {
  public:
    Interval_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *YEAR_P();
    antlr4::tree::TerminalNode *MONTH_P();
    antlr4::tree::TerminalNode *DAY_P();
    antlr4::tree::TerminalNode *HOUR_P();
    antlr4::tree::TerminalNode *MINUTE_P();
    Interval_secondContext *interval_second();
    antlr4::tree::TerminalNode *TO();

   
  };

  Interval_Context* interval_();

  class  Interval_secondContext : public antlr4::ParserRuleContext {
  public:
    Interval_secondContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SECOND_P();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    IconstContext *iconst();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Interval_secondContext* interval_second();

  class  JsonTypeContext : public antlr4::ParserRuleContext {
  public:
    JsonTypeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *JSON();

   
  };

  JsonTypeContext* jsonType();

  class  Escape_Context : public antlr4::ParserRuleContext {
  public:
    Escape_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ESCAPE();
    A_exprContext *a_expr();

   
  };

  Escape_Context* escape_();

  class  A_exprContext : public antlr4::ParserRuleContext {
  public:
    A_exprContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    A_expr_qualContext *a_expr_qual();

   
  };

  A_exprContext* a_expr();

  class  A_expr_qualContext : public antlr4::ParserRuleContext {
  public:
    A_expr_qualContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    A_expr_lesslessContext *a_expr_lessless();
    Qual_opContext *qual_op();

   
  };

  A_expr_qualContext* a_expr_qual();

  class  A_expr_lesslessContext : public antlr4::ParserRuleContext {
  public:
    A_expr_lesslessContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<A_expr_orContext *> a_expr_or();
    A_expr_orContext* a_expr_or(size_t i);
    std::vector<antlr4::tree::TerminalNode *> LESS_LESS();
    antlr4::tree::TerminalNode* LESS_LESS(size_t i);
    std::vector<antlr4::tree::TerminalNode *> GREATER_GREATER();
    antlr4::tree::TerminalNode* GREATER_GREATER(size_t i);

   
  };

  A_expr_lesslessContext* a_expr_lessless();

  class  A_expr_orContext : public antlr4::ParserRuleContext {
  public:
    A_expr_orContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<A_expr_andContext *> a_expr_and();
    A_expr_andContext* a_expr_and(size_t i);
    std::vector<antlr4::tree::TerminalNode *> OR();
    antlr4::tree::TerminalNode* OR(size_t i);

   
  };

  A_expr_orContext* a_expr_or();

  class  A_expr_andContext : public antlr4::ParserRuleContext {
  public:
    A_expr_andContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<A_expr_betweenContext *> a_expr_between();
    A_expr_betweenContext* a_expr_between(size_t i);
    std::vector<antlr4::tree::TerminalNode *> AND();
    antlr4::tree::TerminalNode* AND(size_t i);

   
  };

  A_expr_andContext* a_expr_and();

  class  A_expr_betweenContext : public antlr4::ParserRuleContext {
  public:
    A_expr_betweenContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<A_expr_inContext *> a_expr_in();
    A_expr_inContext* a_expr_in(size_t i);
    antlr4::tree::TerminalNode *BETWEEN();
    antlr4::tree::TerminalNode *AND();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *SYMMETRIC();

   
  };

  A_expr_betweenContext* a_expr_between();

  class  A_expr_inContext : public antlr4::ParserRuleContext {
  public:
    A_expr_inContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    A_expr_unary_notContext *a_expr_unary_not();
    antlr4::tree::TerminalNode *IN_P();
    In_exprContext *in_expr();
    antlr4::tree::TerminalNode *NOT();

   
  };

  A_expr_inContext* a_expr_in();

  class  A_expr_unary_notContext : public antlr4::ParserRuleContext {
  public:
    A_expr_unary_notContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    A_expr_isnullContext *a_expr_isnull();
    antlr4::tree::TerminalNode *NOT();

   
  };

  A_expr_unary_notContext* a_expr_unary_not();

  class  A_expr_isnullContext : public antlr4::ParserRuleContext {
  public:
    A_expr_isnullContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    A_expr_is_notContext *a_expr_is_not();
    antlr4::tree::TerminalNode *ISNULL();
    antlr4::tree::TerminalNode *NOTNULL();

   
  };

  A_expr_isnullContext* a_expr_isnull();

  class  A_expr_is_notContext : public antlr4::ParserRuleContext {
  public:
    A_expr_is_notContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    A_expr_compareContext *a_expr_compare();
    antlr4::tree::TerminalNode *IS();
    antlr4::tree::TerminalNode *NULL_P();
    antlr4::tree::TerminalNode *TRUE_P();
    antlr4::tree::TerminalNode *FALSE_P();
    antlr4::tree::TerminalNode *UNKNOWN();
    antlr4::tree::TerminalNode *DISTINCT();
    antlr4::tree::TerminalNode *FROM();
    A_exprContext *a_expr();
    antlr4::tree::TerminalNode *OF();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Type_listContext *type_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    antlr4::tree::TerminalNode *DOCUMENT_P();
    antlr4::tree::TerminalNode *NORMALIZED();
    antlr4::tree::TerminalNode *NOT();
    Unicode_normal_formContext *unicode_normal_form();

   
  };

  A_expr_is_notContext* a_expr_is_not();

  class  A_expr_compareContext : public antlr4::ParserRuleContext {
  public:
    A_expr_compareContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<A_expr_likeContext *> a_expr_like();
    A_expr_likeContext* a_expr_like(size_t i);
    Subquery_OpContext *subquery_Op();
    Sub_typeContext *sub_type();
    antlr4::tree::TerminalNode *LT();
    antlr4::tree::TerminalNode *GT();
    antlr4::tree::TerminalNode *EQUAL();
    antlr4::tree::TerminalNode *LESS_EQUALS();
    antlr4::tree::TerminalNode *GREATER_EQUALS();
    antlr4::tree::TerminalNode *NOT_EQUALS();
    Select_with_parensContext *select_with_parens();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    A_exprContext *a_expr();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  A_expr_compareContext* a_expr_compare();

  class  A_expr_likeContext : public antlr4::ParserRuleContext {
  public:
    A_expr_likeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<A_expr_qual_opContext *> a_expr_qual_op();
    A_expr_qual_opContext* a_expr_qual_op(size_t i);
    antlr4::tree::TerminalNode *LIKE();
    antlr4::tree::TerminalNode *ILIKE();
    antlr4::tree::TerminalNode *SIMILAR();
    antlr4::tree::TerminalNode *TO();
    antlr4::tree::TerminalNode *NOT();
    Escape_Context *escape_();

   
  };

  A_expr_likeContext* a_expr_like();

  class  A_expr_qual_opContext : public antlr4::ParserRuleContext {
  public:
    A_expr_qual_opContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<A_expr_unary_qualopContext *> a_expr_unary_qualop();
    A_expr_unary_qualopContext* a_expr_unary_qualop(size_t i);
    std::vector<Qual_opContext *> qual_op();
    Qual_opContext* qual_op(size_t i);

   
  };

  A_expr_qual_opContext* a_expr_qual_op();

  class  A_expr_unary_qualopContext : public antlr4::ParserRuleContext {
  public:
    A_expr_unary_qualopContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    A_expr_addContext *a_expr_add();
    Qual_opContext *qual_op();

   
  };

  A_expr_unary_qualopContext* a_expr_unary_qualop();

  class  A_expr_addContext : public antlr4::ParserRuleContext {
  public:
    A_expr_addContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<A_expr_mulContext *> a_expr_mul();
    A_expr_mulContext* a_expr_mul(size_t i);
    std::vector<antlr4::tree::TerminalNode *> MINUS();
    antlr4::tree::TerminalNode* MINUS(size_t i);
    std::vector<antlr4::tree::TerminalNode *> PLUS();
    antlr4::tree::TerminalNode* PLUS(size_t i);

   
  };

  A_expr_addContext* a_expr_add();

  class  A_expr_mulContext : public antlr4::ParserRuleContext {
  public:
    A_expr_mulContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<A_expr_caretContext *> a_expr_caret();
    A_expr_caretContext* a_expr_caret(size_t i);
    std::vector<antlr4::tree::TerminalNode *> STAR();
    antlr4::tree::TerminalNode* STAR(size_t i);
    std::vector<antlr4::tree::TerminalNode *> SLASH();
    antlr4::tree::TerminalNode* SLASH(size_t i);
    std::vector<antlr4::tree::TerminalNode *> PERCENT();
    antlr4::tree::TerminalNode* PERCENT(size_t i);

   
  };

  A_expr_mulContext* a_expr_mul();

  class  A_expr_caretContext : public antlr4::ParserRuleContext {
  public:
    A_expr_caretContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<A_expr_unary_signContext *> a_expr_unary_sign();
    A_expr_unary_signContext* a_expr_unary_sign(size_t i);
    antlr4::tree::TerminalNode *CARET();

   
  };

  A_expr_caretContext* a_expr_caret();

  class  A_expr_unary_signContext : public antlr4::ParserRuleContext {
  public:
    A_expr_unary_signContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    A_expr_at_time_zoneContext *a_expr_at_time_zone();
    antlr4::tree::TerminalNode *MINUS();
    antlr4::tree::TerminalNode *PLUS();

   
  };

  A_expr_unary_signContext* a_expr_unary_sign();

  class  A_expr_at_time_zoneContext : public antlr4::ParserRuleContext {
  public:
    A_expr_at_time_zoneContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    A_expr_collateContext *a_expr_collate();
    antlr4::tree::TerminalNode *AT();
    antlr4::tree::TerminalNode *TIME();
    antlr4::tree::TerminalNode *ZONE();
    A_exprContext *a_expr();

   
  };

  A_expr_at_time_zoneContext* a_expr_at_time_zone();

  class  A_expr_collateContext : public antlr4::ParserRuleContext {
  public:
    A_expr_collateContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    A_expr_typecastContext *a_expr_typecast();
    antlr4::tree::TerminalNode *COLLATE();
    Any_nameContext *any_name();

   
  };

  A_expr_collateContext* a_expr_collate();

  class  A_expr_typecastContext : public antlr4::ParserRuleContext {
  public:
    A_expr_typecastContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    C_exprContext *c_expr();
    std::vector<antlr4::tree::TerminalNode *> TYPECAST();
    antlr4::tree::TerminalNode* TYPECAST(size_t i);
    std::vector<TypenameContext *> typename_();
    TypenameContext* typename_(size_t i);

   
  };

  A_expr_typecastContext* a_expr_typecast();

  class  B_exprContext : public antlr4::ParserRuleContext {
  public:
    B_exprContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    C_exprContext *c_expr();
    std::vector<B_exprContext *> b_expr();
    B_exprContext* b_expr(size_t i);
    antlr4::tree::TerminalNode *PLUS();
    antlr4::tree::TerminalNode *MINUS();
    Qual_opContext *qual_op();
    antlr4::tree::TerminalNode *CARET();
    antlr4::tree::TerminalNode *STAR();
    antlr4::tree::TerminalNode *SLASH();
    antlr4::tree::TerminalNode *PERCENT();
    antlr4::tree::TerminalNode *LT();
    antlr4::tree::TerminalNode *GT();
    antlr4::tree::TerminalNode *EQUAL();
    antlr4::tree::TerminalNode *LESS_EQUALS();
    antlr4::tree::TerminalNode *GREATER_EQUALS();
    antlr4::tree::TerminalNode *NOT_EQUALS();
    antlr4::tree::TerminalNode *TYPECAST();
    TypenameContext *typename_();
    antlr4::tree::TerminalNode *IS();
    antlr4::tree::TerminalNode *DISTINCT();
    antlr4::tree::TerminalNode *FROM();
    antlr4::tree::TerminalNode *OF();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Type_listContext *type_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    antlr4::tree::TerminalNode *DOCUMENT_P();
    antlr4::tree::TerminalNode *NOT();

   
  };

  B_exprContext* b_expr();
  B_exprContext* b_expr(int precedence);
  class  C_exprContext : public antlr4::ParserRuleContext {
  public:
    C_exprContext(antlr4::ParserRuleContext *parent, size_t invokingState);
   
    C_exprContext() = default;
    void copyFrom(C_exprContext *context);
    using antlr4::ParserRuleContext::copyFrom;

    virtual size_t getRuleIndex() const override;

   
  };

  class  C_expr_existsContext : public C_exprContext {
  public:
    C_expr_existsContext(C_exprContext *ctx);

    antlr4::tree::TerminalNode *EXISTS();
    Select_with_parensContext *select_with_parens();
  };

  class  C_expr_caseContext : public C_exprContext {
  public:
    C_expr_caseContext(C_exprContext *ctx);

    Case_exprContext *case_expr();
  };

  class  C_expr_exprContext : public C_exprContext {
  public:
    C_expr_exprContext(C_exprContext *ctx);

    PostgreSQLParser::A_exprContext *a_expr_in_parens = nullptr;
    antlr4::tree::TerminalNode *ARRAY();
    Select_with_parensContext *select_with_parens();
    Array_exprContext *array_expr();
    antlr4::tree::TerminalNode *PARAM();
    Opt_indirectionContext *opt_indirection();
    antlr4::tree::TerminalNode *GROUPING();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Expr_listContext *expr_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    antlr4::tree::TerminalNode *UNIQUE();
    ColumnrefContext *columnref();
    AexprconstContext *aexprconst();
    A_exprContext *a_expr();
    Func_exprContext *func_expr();
    IndirectionContext *indirection();
    Explicit_rowContext *explicit_row();
    Implicit_rowContext *implicit_row();
    std::vector<RowContext *> row();
    RowContext* row(size_t i);
    antlr4::tree::TerminalNode *OVERLAPS();
    antlr4::tree::TerminalNode *DEFAULT();
  };

  C_exprContext* c_expr();

  class  PlsqlvariablenameContext : public antlr4::ParserRuleContext {
  public:
    PlsqlvariablenameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PLSQLVARIABLENAME();

   
  };

  PlsqlvariablenameContext* plsqlvariablename();

  class  Func_applicationContext : public antlr4::ParserRuleContext {
  public:
    Func_applicationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Func_nameContext *func_name();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    Func_arg_listContext *func_arg_list();
    antlr4::tree::TerminalNode *VARIADIC();
    Func_arg_exprContext *func_arg_expr();
    antlr4::tree::TerminalNode *STAR();
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *DISTINCT();
    antlr4::tree::TerminalNode *COMMA();
    Sort_clause_Context *sort_clause_();

   
  };

  Func_applicationContext* func_application();

  class  Func_exprContext : public antlr4::ParserRuleContext {
  public:
    Func_exprContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Func_applicationContext *func_application();
    Within_group_clauseContext *within_group_clause();
    Filter_clauseContext *filter_clause();
    Over_clauseContext *over_clause();
    Func_expr_common_subexprContext *func_expr_common_subexpr();

   
  };

  Func_exprContext* func_expr();

  class  Func_expr_windowlessContext : public antlr4::ParserRuleContext {
  public:
    Func_expr_windowlessContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Func_applicationContext *func_application();
    Func_expr_common_subexprContext *func_expr_common_subexpr();

   
  };

  Func_expr_windowlessContext* func_expr_windowless();

  class  Func_expr_common_subexprContext : public antlr4::ParserRuleContext {
  public:
    Func_expr_common_subexprContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *COLLATION();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    std::vector<A_exprContext *> a_expr();
    A_exprContext* a_expr(size_t i);
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    antlr4::tree::TerminalNode *CURRENT_DATE();
    antlr4::tree::TerminalNode *CURRENT_TIME();
    IconstContext *iconst();
    antlr4::tree::TerminalNode *CURRENT_TIMESTAMP();
    antlr4::tree::TerminalNode *LOCALTIME();
    antlr4::tree::TerminalNode *LOCALTIMESTAMP();
    antlr4::tree::TerminalNode *CURRENT_ROLE();
    antlr4::tree::TerminalNode *CURRENT_USER();
    antlr4::tree::TerminalNode *SESSION_USER();
    antlr4::tree::TerminalNode *SYSTEM_USER();
    antlr4::tree::TerminalNode *USER();
    antlr4::tree::TerminalNode *CURRENT_CATALOG();
    antlr4::tree::TerminalNode *CURRENT_SCHEMA();
    antlr4::tree::TerminalNode *CAST();
    antlr4::tree::TerminalNode *AS();
    TypenameContext *typename_();
    antlr4::tree::TerminalNode *EXTRACT();
    Extract_listContext *extract_list();
    antlr4::tree::TerminalNode *NORMALIZE();
    antlr4::tree::TerminalNode *COMMA();
    Unicode_normal_formContext *unicode_normal_form();
    antlr4::tree::TerminalNode *OVERLAY();
    Overlay_listContext *overlay_list();
    Func_arg_listContext *func_arg_list();
    antlr4::tree::TerminalNode *POSITION();
    Position_listContext *position_list();
    antlr4::tree::TerminalNode *SUBSTRING();
    Substr_listContext *substr_list();
    antlr4::tree::TerminalNode *TREAT();
    antlr4::tree::TerminalNode *TRIM();
    Trim_listContext *trim_list();
    antlr4::tree::TerminalNode *BOTH();
    antlr4::tree::TerminalNode *LEADING();
    antlr4::tree::TerminalNode *TRAILING();
    antlr4::tree::TerminalNode *NULLIF();
    antlr4::tree::TerminalNode *COALESCE();
    Expr_listContext *expr_list();
    antlr4::tree::TerminalNode *GREATEST();
    antlr4::tree::TerminalNode *LEAST();
    antlr4::tree::TerminalNode *XMLCONCAT();
    antlr4::tree::TerminalNode *XMLELEMENT();
    antlr4::tree::TerminalNode *NAME_P();
    ColLabelContext *colLabel();
    Xml_attributesContext *xml_attributes();
    antlr4::tree::TerminalNode *XMLEXISTS();
    C_exprContext *c_expr();
    Xmlexists_argumentContext *xmlexists_argument();
    antlr4::tree::TerminalNode *XMLFOREST();
    Xml_attribute_listContext *xml_attribute_list();
    antlr4::tree::TerminalNode *XMLPARSE();
    Document_or_contentContext *document_or_content();
    Xml_whitespace_optionContext *xml_whitespace_option();
    antlr4::tree::TerminalNode *XMLPI();
    antlr4::tree::TerminalNode *XMLROOT();
    antlr4::tree::TerminalNode *XML_P();
    Xml_root_versionContext *xml_root_version();
    Xml_root_standalone_Context *xml_root_standalone_();
    antlr4::tree::TerminalNode *XMLSERIALIZE();
    SimpletypenameContext *simpletypename();
    antlr4::tree::TerminalNode *JSON_OBJECT();
    Json_name_and_value_listContext *json_name_and_value_list();
    Json_object_constructor_null_clauseContext *json_object_constructor_null_clause();
    Json_key_uniqueness_constraintContext *json_key_uniqueness_constraint();
    Json_returning_clauseContext *json_returning_clause();
    antlr4::tree::TerminalNode *JSON_ARRAY();
    Json_value_expr_listContext *json_value_expr_list();
    Select_no_parensContext *select_no_parens();
    Json_array_constructor_null_clauseContext *json_array_constructor_null_clause();
    Json_format_clauseContext *json_format_clause();
    antlr4::tree::TerminalNode *JSON();
    Json_value_exprContext *json_value_expr();
    antlr4::tree::TerminalNode *JSON_SCALAR();
    antlr4::tree::TerminalNode *JSON_SERIALIZE();
    antlr4::tree::TerminalNode *MERGE_ACTION();
    antlr4::tree::TerminalNode *JSON_QUERY();
    Json_wrapper_behaviorContext *json_wrapper_behavior();
    Json_passing_clauseContext *json_passing_clause();
    Json_quotes_clauseContext *json_quotes_clause();
    Json_behavior_clauseContext *json_behavior_clause();
    antlr4::tree::TerminalNode *JSON_EXISTS();
    Json_on_error_clauseContext *json_on_error_clause();
    antlr4::tree::TerminalNode *JSON_VALUE();

   
  };

  Func_expr_common_subexprContext* func_expr_common_subexpr();

  class  Xml_root_versionContext : public antlr4::ParserRuleContext {
  public:
    Xml_root_versionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *VERSION_P();
    A_exprContext *a_expr();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *VALUE_P();

   
  };

  Xml_root_versionContext* xml_root_version();

  class  Xml_root_standalone_Context : public antlr4::ParserRuleContext {
  public:
    Xml_root_standalone_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *COMMA();
    antlr4::tree::TerminalNode *STANDALONE_P();
    antlr4::tree::TerminalNode *YES_P();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *VALUE_P();

   
  };

  Xml_root_standalone_Context* xml_root_standalone_();

  class  Xml_attributesContext : public antlr4::ParserRuleContext {
  public:
    Xml_attributesContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *XMLATTRIBUTES();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Xml_attribute_listContext *xml_attribute_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Xml_attributesContext* xml_attributes();

  class  Xml_attribute_listContext : public antlr4::ParserRuleContext {
  public:
    Xml_attribute_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Xml_attribute_elContext *> xml_attribute_el();
    Xml_attribute_elContext* xml_attribute_el(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Xml_attribute_listContext* xml_attribute_list();

  class  Xml_attribute_elContext : public antlr4::ParserRuleContext {
  public:
    Xml_attribute_elContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    A_exprContext *a_expr();
    antlr4::tree::TerminalNode *AS();
    ColLabelContext *colLabel();

   
  };

  Xml_attribute_elContext* xml_attribute_el();

  class  Document_or_contentContext : public antlr4::ParserRuleContext {
  public:
    Document_or_contentContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DOCUMENT_P();
    antlr4::tree::TerminalNode *CONTENT_P();

   
  };

  Document_or_contentContext* document_or_content();

  class  Xml_whitespace_optionContext : public antlr4::ParserRuleContext {
  public:
    Xml_whitespace_optionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PRESERVE();
    antlr4::tree::TerminalNode *WHITESPACE_P();
    antlr4::tree::TerminalNode *STRIP_P();

   
  };

  Xml_whitespace_optionContext* xml_whitespace_option();

  class  Xmlexists_argumentContext : public antlr4::ParserRuleContext {
  public:
    Xmlexists_argumentContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PASSING();
    C_exprContext *c_expr();
    std::vector<Xml_passing_mechContext *> xml_passing_mech();
    Xml_passing_mechContext* xml_passing_mech(size_t i);

   
  };

  Xmlexists_argumentContext* xmlexists_argument();

  class  Xml_passing_mechContext : public antlr4::ParserRuleContext {
  public:
    Xml_passing_mechContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BY();
    antlr4::tree::TerminalNode *REF();
    antlr4::tree::TerminalNode *VALUE_P();

   
  };

  Xml_passing_mechContext* xml_passing_mech();

  class  Within_group_clauseContext : public antlr4::ParserRuleContext {
  public:
    Within_group_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WITHIN();
    antlr4::tree::TerminalNode *GROUP_P();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Sort_clauseContext *sort_clause();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Within_group_clauseContext* within_group_clause();

  class  Filter_clauseContext : public antlr4::ParserRuleContext {
  public:
    Filter_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FILTER();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    antlr4::tree::TerminalNode *WHERE();
    A_exprContext *a_expr();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Filter_clauseContext* filter_clause();

  class  Window_clauseContext : public antlr4::ParserRuleContext {
  public:
    Window_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WINDOW();
    Window_definition_listContext *window_definition_list();

   
  };

  Window_clauseContext* window_clause();

  class  Window_definition_listContext : public antlr4::ParserRuleContext {
  public:
    Window_definition_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Window_definitionContext *> window_definition();
    Window_definitionContext* window_definition(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Window_definition_listContext* window_definition_list();

  class  Window_definitionContext : public antlr4::ParserRuleContext {
  public:
    Window_definitionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ColidContext *colid();
    antlr4::tree::TerminalNode *AS();
    Window_specificationContext *window_specification();

   
  };

  Window_definitionContext* window_definition();

  class  Over_clauseContext : public antlr4::ParserRuleContext {
  public:
    Over_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OVER();
    Window_specificationContext *window_specification();
    ColidContext *colid();

   
  };

  Over_clauseContext* over_clause();

  class  Window_specificationContext : public antlr4::ParserRuleContext {
  public:
    Window_specificationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OPEN_PAREN();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    Existing_window_name_Context *existing_window_name_();
    Partition_clause_Context *partition_clause_();
    Sort_clause_Context *sort_clause_();
    Frame_clause_Context *frame_clause_();

   
  };

  Window_specificationContext* window_specification();

  class  Existing_window_name_Context : public antlr4::ParserRuleContext {
  public:
    Existing_window_name_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ColidContext *colid();

   
  };

  Existing_window_name_Context* existing_window_name_();

  class  Partition_clause_Context : public antlr4::ParserRuleContext {
  public:
    Partition_clause_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PARTITION();
    antlr4::tree::TerminalNode *BY();
    Expr_listContext *expr_list();

   
  };

  Partition_clause_Context* partition_clause_();

  class  Frame_clause_Context : public antlr4::ParserRuleContext {
  public:
    Frame_clause_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RANGE();
    Frame_extentContext *frame_extent();
    Window_exclusion_clause_Context *window_exclusion_clause_();
    antlr4::tree::TerminalNode *ROWS();
    antlr4::tree::TerminalNode *GROUPS();

   
  };

  Frame_clause_Context* frame_clause_();

  class  Frame_extentContext : public antlr4::ParserRuleContext {
  public:
    Frame_extentContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Frame_boundContext *> frame_bound();
    Frame_boundContext* frame_bound(size_t i);
    antlr4::tree::TerminalNode *BETWEEN();
    antlr4::tree::TerminalNode *AND();

   
  };

  Frame_extentContext* frame_extent();

  class  Frame_boundContext : public antlr4::ParserRuleContext {
  public:
    Frame_boundContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *UNBOUNDED();
    antlr4::tree::TerminalNode *PRECEDING();
    antlr4::tree::TerminalNode *FOLLOWING();
    antlr4::tree::TerminalNode *CURRENT_P();
    antlr4::tree::TerminalNode *ROW();
    A_exprContext *a_expr();

   
  };

  Frame_boundContext* frame_bound();

  class  Window_exclusion_clause_Context : public antlr4::ParserRuleContext {
  public:
    Window_exclusion_clause_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *EXCLUDE();
    antlr4::tree::TerminalNode *CURRENT_P();
    antlr4::tree::TerminalNode *ROW();
    antlr4::tree::TerminalNode *GROUP_P();
    antlr4::tree::TerminalNode *TIES();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *OTHERS();

   
  };

  Window_exclusion_clause_Context* window_exclusion_clause_();

  class  RowContext : public antlr4::ParserRuleContext {
  public:
    RowContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ROW();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    Expr_listContext *expr_list();
    antlr4::tree::TerminalNode *COMMA();
    A_exprContext *a_expr();

   
  };

  RowContext* row();

  class  Explicit_rowContext : public antlr4::ParserRuleContext {
  public:
    Explicit_rowContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ROW();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    Expr_listContext *expr_list();

   
  };

  Explicit_rowContext* explicit_row();

  class  Implicit_rowContext : public antlr4::ParserRuleContext {
  public:
    Implicit_rowContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Expr_listContext *expr_list();
    antlr4::tree::TerminalNode *COMMA();
    A_exprContext *a_expr();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Implicit_rowContext* implicit_row();

  class  Sub_typeContext : public antlr4::ParserRuleContext {
  public:
    Sub_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ANY();
    antlr4::tree::TerminalNode *SOME();
    antlr4::tree::TerminalNode *ALL();

   
  };

  Sub_typeContext* sub_type();

  class  All_opContext : public antlr4::ParserRuleContext {
  public:
    All_opContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *Operator();
    MathopContext *mathop();

   
  };

  All_opContext* all_op();

  class  MathopContext : public antlr4::ParserRuleContext {
  public:
    MathopContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PLUS();
    antlr4::tree::TerminalNode *MINUS();
    antlr4::tree::TerminalNode *STAR();
    antlr4::tree::TerminalNode *SLASH();
    antlr4::tree::TerminalNode *PERCENT();
    antlr4::tree::TerminalNode *CARET();
    antlr4::tree::TerminalNode *LT();
    antlr4::tree::TerminalNode *GT();
    antlr4::tree::TerminalNode *EQUAL();
    antlr4::tree::TerminalNode *LESS_EQUALS();
    antlr4::tree::TerminalNode *GREATER_EQUALS();
    antlr4::tree::TerminalNode *NOT_EQUALS();

   
  };

  MathopContext* mathop();

  class  Qual_opContext : public antlr4::ParserRuleContext {
  public:
    Qual_opContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *Operator();
    antlr4::tree::TerminalNode *OPERATOR();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Any_operatorContext *any_operator();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Qual_opContext* qual_op();

  class  Qual_all_opContext : public antlr4::ParserRuleContext {
  public:
    Qual_all_opContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    All_opContext *all_op();
    antlr4::tree::TerminalNode *OPERATOR();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Any_operatorContext *any_operator();
    antlr4::tree::TerminalNode *CLOSE_PAREN();

   
  };

  Qual_all_opContext* qual_all_op();

  class  Subquery_OpContext : public antlr4::ParserRuleContext {
  public:
    Subquery_OpContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    All_opContext *all_op();
    antlr4::tree::TerminalNode *OPERATOR();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Any_operatorContext *any_operator();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    antlr4::tree::TerminalNode *LIKE();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *ILIKE();

   
  };

  Subquery_OpContext* subquery_Op();

  class  Expr_listContext : public antlr4::ParserRuleContext {
  public:
    Expr_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<A_exprContext *> a_expr();
    A_exprContext* a_expr(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Expr_listContext* expr_list();

  class  Func_arg_listContext : public antlr4::ParserRuleContext {
  public:
    Func_arg_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Func_arg_exprContext *> func_arg_expr();
    Func_arg_exprContext* func_arg_expr(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Func_arg_listContext* func_arg_list();

  class  Func_arg_exprContext : public antlr4::ParserRuleContext {
  public:
    Func_arg_exprContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    A_exprContext *a_expr();
    Param_nameContext *param_name();
    antlr4::tree::TerminalNode *COLON_EQUALS();
    antlr4::tree::TerminalNode *EQUALS_GREATER();

   
  };

  Func_arg_exprContext* func_arg_expr();

  class  Type_listContext : public antlr4::ParserRuleContext {
  public:
    Type_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<TypenameContext *> typename_();
    TypenameContext* typename_(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Type_listContext* type_list();

  class  Array_exprContext : public antlr4::ParserRuleContext {
  public:
    Array_exprContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OPEN_BRACKET();
    antlr4::tree::TerminalNode *CLOSE_BRACKET();
    Expr_listContext *expr_list();
    Array_expr_listContext *array_expr_list();

   
  };

  Array_exprContext* array_expr();

  class  Array_expr_listContext : public antlr4::ParserRuleContext {
  public:
    Array_expr_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Array_exprContext *> array_expr();
    Array_exprContext* array_expr(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Array_expr_listContext* array_expr_list();

  class  Extract_listContext : public antlr4::ParserRuleContext {
  public:
    Extract_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Extract_argContext *extract_arg();
    antlr4::tree::TerminalNode *FROM();
    A_exprContext *a_expr();

   
  };

  Extract_listContext* extract_list();

  class  Extract_argContext : public antlr4::ParserRuleContext {
  public:
    Extract_argContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    antlr4::tree::TerminalNode *YEAR_P();
    antlr4::tree::TerminalNode *MONTH_P();
    antlr4::tree::TerminalNode *DAY_P();
    antlr4::tree::TerminalNode *HOUR_P();
    antlr4::tree::TerminalNode *MINUTE_P();
    antlr4::tree::TerminalNode *SECOND_P();
    SconstContext *sconst();

   
  };

  Extract_argContext* extract_arg();

  class  Unicode_normal_formContext : public antlr4::ParserRuleContext {
  public:
    Unicode_normal_formContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NFC();
    antlr4::tree::TerminalNode *NFD();
    antlr4::tree::TerminalNode *NFKC();
    antlr4::tree::TerminalNode *NFKD();

   
  };

  Unicode_normal_formContext* unicode_normal_form();

  class  Overlay_listContext : public antlr4::ParserRuleContext {
  public:
    Overlay_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<A_exprContext *> a_expr();
    A_exprContext* a_expr(size_t i);
    antlr4::tree::TerminalNode *PLACING();
    antlr4::tree::TerminalNode *FROM();
    antlr4::tree::TerminalNode *FOR();

   
  };

  Overlay_listContext* overlay_list();

  class  Position_listContext : public antlr4::ParserRuleContext {
  public:
    Position_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<B_exprContext *> b_expr();
    B_exprContext* b_expr(size_t i);
    antlr4::tree::TerminalNode *IN_P();

   
  };

  Position_listContext* position_list();

  class  Substr_listContext : public antlr4::ParserRuleContext {
  public:
    Substr_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<A_exprContext *> a_expr();
    A_exprContext* a_expr(size_t i);
    antlr4::tree::TerminalNode *FROM();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *SIMILAR();
    antlr4::tree::TerminalNode *ESCAPE();

   
  };

  Substr_listContext* substr_list();

  class  Trim_listContext : public antlr4::ParserRuleContext {
  public:
    Trim_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    A_exprContext *a_expr();
    antlr4::tree::TerminalNode *FROM();
    Expr_listContext *expr_list();

   
  };

  Trim_listContext* trim_list();

  class  In_exprContext : public antlr4::ParserRuleContext {
  public:
    In_exprContext(antlr4::ParserRuleContext *parent, size_t invokingState);
   
    In_exprContext() = default;
    void copyFrom(In_exprContext *context);
    using antlr4::ParserRuleContext::copyFrom;

    virtual size_t getRuleIndex() const override;

   
  };

  class  In_expr_listContext : public In_exprContext {
  public:
    In_expr_listContext(In_exprContext *ctx);

    antlr4::tree::TerminalNode *OPEN_PAREN();
    Expr_listContext *expr_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
  };

  class  In_expr_selectContext : public In_exprContext {
  public:
    In_expr_selectContext(In_exprContext *ctx);

    Select_with_parensContext *select_with_parens();
  };

  In_exprContext* in_expr();

  class  Case_exprContext : public antlr4::ParserRuleContext {
  public:
    Case_exprContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CASE();
    When_clause_listContext *when_clause_list();
    antlr4::tree::TerminalNode *END_P();
    Case_argContext *case_arg();
    Case_defaultContext *case_default();

   
  };

  Case_exprContext* case_expr();

  class  When_clause_listContext : public antlr4::ParserRuleContext {
  public:
    When_clause_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<When_clauseContext *> when_clause();
    When_clauseContext* when_clause(size_t i);

   
  };

  When_clause_listContext* when_clause_list();

  class  When_clauseContext : public antlr4::ParserRuleContext {
  public:
    When_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WHEN();
    std::vector<A_exprContext *> a_expr();
    A_exprContext* a_expr(size_t i);
    antlr4::tree::TerminalNode *THEN();

   
  };

  When_clauseContext* when_clause();

  class  Case_defaultContext : public antlr4::ParserRuleContext {
  public:
    Case_defaultContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ELSE();
    A_exprContext *a_expr();

   
  };

  Case_defaultContext* case_default();

  class  Case_argContext : public antlr4::ParserRuleContext {
  public:
    Case_argContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    A_exprContext *a_expr();

   
  };

  Case_argContext* case_arg();

  class  ColumnrefContext : public antlr4::ParserRuleContext {
  public:
    ColumnrefContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ColidContext *colid();
    IndirectionContext *indirection();

   
  };

  ColumnrefContext* columnref();

  class  Indirection_elContext : public antlr4::ParserRuleContext {
  public:
    Indirection_elContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DOT();
    Attr_nameContext *attr_name();
    antlr4::tree::TerminalNode *STAR();
    antlr4::tree::TerminalNode *OPEN_BRACKET();
    antlr4::tree::TerminalNode *CLOSE_BRACKET();
    A_exprContext *a_expr();
    antlr4::tree::TerminalNode *COLON();
    std::vector<Slice_bound_Context *> slice_bound_();
    Slice_bound_Context* slice_bound_(size_t i);

   
  };

  Indirection_elContext* indirection_el();

  class  Slice_bound_Context : public antlr4::ParserRuleContext {
  public:
    Slice_bound_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    A_exprContext *a_expr();

   
  };

  Slice_bound_Context* slice_bound_();

  class  IndirectionContext : public antlr4::ParserRuleContext {
  public:
    IndirectionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Indirection_elContext *> indirection_el();
    Indirection_elContext* indirection_el(size_t i);

   
  };

  IndirectionContext* indirection();

  class  Opt_indirectionContext : public antlr4::ParserRuleContext {
  public:
    Opt_indirectionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Indirection_elContext *> indirection_el();
    Indirection_elContext* indirection_el(size_t i);

   
  };

  Opt_indirectionContext* opt_indirection();

  class  Json_passing_clauseContext : public antlr4::ParserRuleContext {
  public:
    Json_passing_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PASSING();
    Json_argumentsContext *json_arguments();

   
  };

  Json_passing_clauseContext* json_passing_clause();

  class  Json_argumentsContext : public antlr4::ParserRuleContext {
  public:
    Json_argumentsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Json_argumentContext *json_argument();
    Json_argumentsContext *json_arguments();
    antlr4::tree::TerminalNode *COMMA();

   
  };

  Json_argumentsContext* json_arguments();
  Json_argumentsContext* json_arguments(int precedence);
  class  Json_argumentContext : public antlr4::ParserRuleContext {
  public:
    Json_argumentContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Json_value_exprContext *json_value_expr();
    antlr4::tree::TerminalNode *AS();
    ColLabelContext *colLabel();

   
  };

  Json_argumentContext* json_argument();

  class  Json_wrapper_behaviorContext : public antlr4::ParserRuleContext {
  public:
    Json_wrapper_behaviorContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WITHOUT();
    antlr4::tree::TerminalNode *WRAPPER();
    antlr4::tree::TerminalNode *ARRAY();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *CONDITIONAL();
    antlr4::tree::TerminalNode *UNCONDITIONAL();

   
  };

  Json_wrapper_behaviorContext* json_wrapper_behavior();

  class  Json_behaviorContext : public antlr4::ParserRuleContext {
  public:
    Json_behaviorContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DEFAULT();
    A_exprContext *a_expr();
    Json_behavior_typeContext *json_behavior_type();

   
  };

  Json_behaviorContext* json_behavior();

  class  Json_behavior_typeContext : public antlr4::ParserRuleContext {
  public:
    Json_behavior_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ERROR();
    antlr4::tree::TerminalNode *NULL_P();
    antlr4::tree::TerminalNode *TRUE_P();
    antlr4::tree::TerminalNode *FALSE_P();
    antlr4::tree::TerminalNode *UNKNOWN();
    antlr4::tree::TerminalNode *EMPTY_P();
    antlr4::tree::TerminalNode *ARRAY();
    antlr4::tree::TerminalNode *OBJECT_P();

   
  };

  Json_behavior_typeContext* json_behavior_type();

  class  Json_behavior_clauseContext : public antlr4::ParserRuleContext {
  public:
    Json_behavior_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Json_behaviorContext *> json_behavior();
    Json_behaviorContext* json_behavior(size_t i);
    std::vector<antlr4::tree::TerminalNode *> ON();
    antlr4::tree::TerminalNode* ON(size_t i);
    antlr4::tree::TerminalNode *EMPTY_P();
    antlr4::tree::TerminalNode *ERROR();

   
  };

  Json_behavior_clauseContext* json_behavior_clause();

  class  Json_on_error_clauseContext : public antlr4::ParserRuleContext {
  public:
    Json_on_error_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Json_behaviorContext *json_behavior();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *ERROR();

   
  };

  Json_on_error_clauseContext* json_on_error_clause();

  class  Json_value_exprContext : public antlr4::ParserRuleContext {
  public:
    Json_value_exprContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    A_exprContext *a_expr();
    Json_format_clauseContext *json_format_clause();

   
  };

  Json_value_exprContext* json_value_expr();

  class  Json_format_clauseContext : public antlr4::ParserRuleContext {
  public:
    Json_format_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FORMAT_LA();
    antlr4::tree::TerminalNode *JSON();
    antlr4::tree::TerminalNode *ENCODING();
    NameContext *name();

   
  };

  Json_format_clauseContext* json_format_clause();

  class  Json_quotes_clauseContext : public antlr4::ParserRuleContext {
  public:
    Json_quotes_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *KEEP();
    antlr4::tree::TerminalNode *QUOTES();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *SCALAR();
    antlr4::tree::TerminalNode *STRING_P();
    antlr4::tree::TerminalNode *OMIT();

   
  };

  Json_quotes_clauseContext* json_quotes_clause();

  class  Json_returning_clauseContext : public antlr4::ParserRuleContext {
  public:
    Json_returning_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RETURNING();
    TypenameContext *typename_();
    Json_format_clauseContext *json_format_clause();

   
  };

  Json_returning_clauseContext* json_returning_clause();

  class  Json_predicate_type_constraintContext : public antlr4::ParserRuleContext {
  public:
    Json_predicate_type_constraintContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *JSON();
    antlr4::tree::TerminalNode *VALUE_P();
    antlr4::tree::TerminalNode *ARRAY();
    antlr4::tree::TerminalNode *OBJECT_P();
    antlr4::tree::TerminalNode *SCALAR();

   
  };

  Json_predicate_type_constraintContext* json_predicate_type_constraint();

  class  Json_key_uniqueness_constraintContext : public antlr4::ParserRuleContext {
  public:
    Json_key_uniqueness_constraintContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *UNIQUE();
    antlr4::tree::TerminalNode *KEYS();
    antlr4::tree::TerminalNode *WITHOUT();

   
  };

  Json_key_uniqueness_constraintContext* json_key_uniqueness_constraint();

  class  Json_name_and_value_listContext : public antlr4::ParserRuleContext {
  public:
    Json_name_and_value_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Json_name_and_valueContext *json_name_and_value();
    Json_name_and_value_listContext *json_name_and_value_list();
    antlr4::tree::TerminalNode *COMMA();

   
  };

  Json_name_and_value_listContext* json_name_and_value_list();
  Json_name_and_value_listContext* json_name_and_value_list(int precedence);
  class  Json_name_and_valueContext : public antlr4::ParserRuleContext {
  public:
    Json_name_and_valueContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    C_exprContext *c_expr();
    antlr4::tree::TerminalNode *VALUE_P();
    Json_value_exprContext *json_value_expr();
    A_exprContext *a_expr();
    antlr4::tree::TerminalNode *COLON();

   
  };

  Json_name_and_valueContext* json_name_and_value();

  class  Json_object_constructor_null_clauseContext : public antlr4::ParserRuleContext {
  public:
    Json_object_constructor_null_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<antlr4::tree::TerminalNode *> NULL_P();
    antlr4::tree::TerminalNode* NULL_P(size_t i);
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *ABSENT();

   
  };

  Json_object_constructor_null_clauseContext* json_object_constructor_null_clause();

  class  Json_array_constructor_null_clauseContext : public antlr4::ParserRuleContext {
  public:
    Json_array_constructor_null_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<antlr4::tree::TerminalNode *> NULL_P();
    antlr4::tree::TerminalNode* NULL_P(size_t i);
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *ABSENT();

   
  };

  Json_array_constructor_null_clauseContext* json_array_constructor_null_clause();

  class  Json_value_expr_listContext : public antlr4::ParserRuleContext {
  public:
    Json_value_expr_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Json_value_exprContext *json_value_expr();
    Json_value_expr_listContext *json_value_expr_list();
    antlr4::tree::TerminalNode *COMMA();

   
  };

  Json_value_expr_listContext* json_value_expr_list();
  Json_value_expr_listContext* json_value_expr_list(int precedence);
  class  Json_aggregate_funcContext : public antlr4::ParserRuleContext {
  public:
    Json_aggregate_funcContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *JSON_OBJECTAGG();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Json_name_and_valueContext *json_name_and_value();
    Json_returning_clauseContext *json_returning_clause();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    Json_object_constructor_null_clauseContext *json_object_constructor_null_clause();
    Json_key_uniqueness_constraintContext *json_key_uniqueness_constraint();
    antlr4::tree::TerminalNode *JSON_ARRAYAGG();
    Json_value_exprContext *json_value_expr();
    Json_array_aggregate_order_by_clauseContext *json_array_aggregate_order_by_clause();
    Json_array_constructor_null_clauseContext *json_array_constructor_null_clause();

   
  };

  Json_aggregate_funcContext* json_aggregate_func();

  class  Json_array_aggregate_order_by_clauseContext : public antlr4::ParserRuleContext {
  public:
    Json_array_aggregate_order_by_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ORDER();
    antlr4::tree::TerminalNode *BY();
    Sortby_listContext *sortby_list();

   
  };

  Json_array_aggregate_order_by_clauseContext* json_array_aggregate_order_by_clause();

  class  Target_list_Context : public antlr4::ParserRuleContext {
  public:
    Target_list_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Target_listContext *target_list();

   
  };

  Target_list_Context* target_list_();

  class  Target_listContext : public antlr4::ParserRuleContext {
  public:
    Target_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Target_elContext *> target_el();
    Target_elContext* target_el(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Target_listContext* target_list();

  class  Target_elContext : public antlr4::ParserRuleContext {
  public:
    Target_elContext(antlr4::ParserRuleContext *parent, size_t invokingState);
   
    Target_elContext() = default;
    void copyFrom(Target_elContext *context);
    using antlr4::ParserRuleContext::copyFrom;

    virtual size_t getRuleIndex() const override;

   
  };

  class  Target_labelContext : public Target_elContext {
  public:
    Target_labelContext(Target_elContext *ctx);

    A_exprContext *a_expr();
    antlr4::tree::TerminalNode *AS();
    ColLabelContext *colLabel();
    BareColLabelContext *bareColLabel();
  };

  class  Target_starContext : public Target_elContext {
  public:
    Target_starContext(Target_elContext *ctx);

    antlr4::tree::TerminalNode *STAR();
  };

  Target_elContext* target_el();

  class  Qualified_name_listContext : public antlr4::ParserRuleContext {
  public:
    Qualified_name_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Qualified_nameContext *> qualified_name();
    Qualified_nameContext* qualified_name(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Qualified_name_listContext* qualified_name_list();

  class  Qualified_nameContext : public antlr4::ParserRuleContext {
  public:
    Qualified_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ColidContext *colid();
    IndirectionContext *indirection();

   
  };

  Qualified_nameContext* qualified_name();

  class  Name_listContext : public antlr4::ParserRuleContext {
  public:
    Name_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<NameContext *> name();
    NameContext* name(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Name_listContext* name_list();

  class  NameContext : public antlr4::ParserRuleContext {
  public:
    NameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ColidContext *colid();

   
  };

  NameContext* name();

  class  Attr_nameContext : public antlr4::ParserRuleContext {
  public:
    Attr_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ColLabelContext *colLabel();

   
  };

  Attr_nameContext* attr_name();

  class  File_nameContext : public antlr4::ParserRuleContext {
  public:
    File_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    SconstContext *sconst();

   
  };

  File_nameContext* file_name();

  class  Func_nameContext : public antlr4::ParserRuleContext {
  public:
    Func_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Type_function_nameContext *type_function_name();
    ColidContext *colid();
    IndirectionContext *indirection();

   
  };

  Func_nameContext* func_name();

  class  AexprconstContext : public antlr4::ParserRuleContext {
  public:
    AexprconstContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IconstContext *iconst();
    FconstContext *fconst();
    SconstContext *sconst();
    BconstContext *bconst();
    XconstContext *xconst();
    Func_nameContext *func_name();
    antlr4::tree::TerminalNode *OPEN_PAREN();
    Func_arg_listContext *func_arg_list();
    antlr4::tree::TerminalNode *CLOSE_PAREN();
    Sort_clause_Context *sort_clause_();
    ConsttypenameContext *consttypename();
    ConstintervalContext *constinterval();
    Interval_Context *interval_();
    antlr4::tree::TerminalNode *TRUE_P();
    antlr4::tree::TerminalNode *FALSE_P();
    antlr4::tree::TerminalNode *NULL_P();

   
  };

  AexprconstContext* aexprconst();

  class  XconstContext : public antlr4::ParserRuleContext {
  public:
    XconstContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *HexadecimalStringConstant();

   
  };

  XconstContext* xconst();

  class  BconstContext : public antlr4::ParserRuleContext {
  public:
    BconstContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BinaryStringConstant();

   
  };

  BconstContext* bconst();

  class  FconstContext : public antlr4::ParserRuleContext {
  public:
    FconstContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *Numeric();

   
  };

  FconstContext* fconst();

  class  IconstContext : public antlr4::ParserRuleContext {
  public:
    IconstContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *Integral();
    antlr4::tree::TerminalNode *BinaryIntegral();
    antlr4::tree::TerminalNode *OctalIntegral();
    antlr4::tree::TerminalNode *HexadecimalIntegral();

   
  };

  IconstContext* iconst();

  class  SconstContext : public antlr4::ParserRuleContext {
  public:
    SconstContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    AnysconstContext *anysconst();
    Uescape_Context *uescape_();

   
  };

  SconstContext* sconst();

  class  AnysconstContext : public antlr4::ParserRuleContext {
  public:
    AnysconstContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *StringConstant();
    antlr4::tree::TerminalNode *UnicodeEscapeStringConstant();
    antlr4::tree::TerminalNode *BeginDollarStringConstant();
    antlr4::tree::TerminalNode *EndDollarStringConstant();
    std::vector<antlr4::tree::TerminalNode *> DollarText();
    antlr4::tree::TerminalNode* DollarText(size_t i);
    antlr4::tree::TerminalNode *EscapeStringConstant();

   
  };

  AnysconstContext* anysconst();

  class  Uescape_Context : public antlr4::ParserRuleContext {
  public:
    Uescape_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *UESCAPE();
    AnysconstContext *anysconst();

   
  };

  Uescape_Context* uescape_();

  class  SignediconstContext : public antlr4::ParserRuleContext {
  public:
    SignediconstContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IconstContext *iconst();
    antlr4::tree::TerminalNode *PLUS();
    antlr4::tree::TerminalNode *MINUS();

   
  };

  SignediconstContext* signediconst();

  class  RoleidContext : public antlr4::ParserRuleContext {
  public:
    RoleidContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    RolespecContext *rolespec();

   
  };

  RoleidContext* roleid();

  class  RolespecContext : public antlr4::ParserRuleContext {
  public:
    RolespecContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    NonreservedwordContext *nonreservedword();
    antlr4::tree::TerminalNode *CURRENT_USER();
    antlr4::tree::TerminalNode *SESSION_USER();

   
  };

  RolespecContext* rolespec();

  class  Role_listContext : public antlr4::ParserRuleContext {
  public:
    Role_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<RolespecContext *> rolespec();
    RolespecContext* rolespec(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Role_listContext* role_list();

  class  ColidContext : public antlr4::ParserRuleContext {
  public:
    ColidContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    Unreserved_keywordContext *unreserved_keyword();
    Col_name_keywordContext *col_name_keyword();

   
  };

  ColidContext* colid();

  class  Type_function_nameContext : public antlr4::ParserRuleContext {
  public:
    Type_function_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    Unreserved_keywordContext *unreserved_keyword();
    Type_func_name_keywordContext *type_func_name_keyword();

   
  };

  Type_function_nameContext* type_function_name();

  class  NonreservedwordContext : public antlr4::ParserRuleContext {
  public:
    NonreservedwordContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    Unreserved_keywordContext *unreserved_keyword();
    Col_name_keywordContext *col_name_keyword();
    Type_func_name_keywordContext *type_func_name_keyword();

   
  };

  NonreservedwordContext* nonreservedword();

  class  ColLabelContext : public antlr4::ParserRuleContext {
  public:
    ColLabelContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    Unreserved_keywordContext *unreserved_keyword();
    Col_name_keywordContext *col_name_keyword();
    Type_func_name_keywordContext *type_func_name_keyword();
    Reserved_keywordContext *reserved_keyword();
    antlr4::tree::TerminalNode *EXIT();

   
  };

  ColLabelContext* colLabel();

  class  BareColLabelContext : public antlr4::ParserRuleContext {
  public:
    BareColLabelContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    IdentifierContext *identifier();
    Bare_label_keywordContext *bare_label_keyword();

   
  };

  BareColLabelContext* bareColLabel();

  class  Unreserved_keywordContext : public antlr4::ParserRuleContext {
  public:
    Unreserved_keywordContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ABORT_P();
    antlr4::tree::TerminalNode *ABSENT();
    antlr4::tree::TerminalNode *ABSOLUTE_P();
    antlr4::tree::TerminalNode *ACCESS();
    antlr4::tree::TerminalNode *ACTION();
    antlr4::tree::TerminalNode *ADD_P();
    antlr4::tree::TerminalNode *ADMIN();
    antlr4::tree::TerminalNode *AFTER();
    antlr4::tree::TerminalNode *AGGREGATE();
    antlr4::tree::TerminalNode *ALSO();
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *ALWAYS();
    antlr4::tree::TerminalNode *ASENSITIVE();
    antlr4::tree::TerminalNode *ASSERTION();
    antlr4::tree::TerminalNode *ASSIGNMENT();
    antlr4::tree::TerminalNode *AT();
    antlr4::tree::TerminalNode *ATOMIC();
    antlr4::tree::TerminalNode *ATTACH();
    antlr4::tree::TerminalNode *ATTRIBUTE();
    antlr4::tree::TerminalNode *BACKWARD();
    antlr4::tree::TerminalNode *BEFORE();
    antlr4::tree::TerminalNode *BEGIN_P();
    antlr4::tree::TerminalNode *BREADTH();
    antlr4::tree::TerminalNode *BY();
    antlr4::tree::TerminalNode *CACHE();
    antlr4::tree::TerminalNode *CALL();
    antlr4::tree::TerminalNode *CALLED();
    antlr4::tree::TerminalNode *CASCADE();
    antlr4::tree::TerminalNode *CASCADED();
    antlr4::tree::TerminalNode *CATALOG();
    antlr4::tree::TerminalNode *CHAIN();
    antlr4::tree::TerminalNode *CHARACTERISTICS();
    antlr4::tree::TerminalNode *CHECKPOINT();
    antlr4::tree::TerminalNode *CLASS();
    antlr4::tree::TerminalNode *CLOSE();
    antlr4::tree::TerminalNode *CLUSTER();
    antlr4::tree::TerminalNode *COLUMNS();
    antlr4::tree::TerminalNode *COMMENT();
    antlr4::tree::TerminalNode *COMMENTS();
    antlr4::tree::TerminalNode *COMMIT();
    antlr4::tree::TerminalNode *COMMITTED();
    antlr4::tree::TerminalNode *COMPRESSION();
    antlr4::tree::TerminalNode *CONDITIONAL();
    antlr4::tree::TerminalNode *CONFIGURATION();
    antlr4::tree::TerminalNode *CONFLICT();
    antlr4::tree::TerminalNode *CONNECTION();
    antlr4::tree::TerminalNode *CONSTRAINTS();
    antlr4::tree::TerminalNode *CONTENT_P();
    antlr4::tree::TerminalNode *CONTINUE_P();
    antlr4::tree::TerminalNode *CONVERSION_P();
    antlr4::tree::TerminalNode *COPY();
    antlr4::tree::TerminalNode *COST();
    antlr4::tree::TerminalNode *CSV();
    antlr4::tree::TerminalNode *CUBE();
    antlr4::tree::TerminalNode *CURRENT_P();
    antlr4::tree::TerminalNode *CURSOR();
    antlr4::tree::TerminalNode *CYCLE();
    antlr4::tree::TerminalNode *DATA_P();
    antlr4::tree::TerminalNode *DATABASE();
    antlr4::tree::TerminalNode *DAY_P();
    antlr4::tree::TerminalNode *DEALLOCATE();
    antlr4::tree::TerminalNode *DECLARE();
    antlr4::tree::TerminalNode *DEFAULTS();
    antlr4::tree::TerminalNode *DEFERRED();
    antlr4::tree::TerminalNode *DEFINER();
    antlr4::tree::TerminalNode *DELETE_P();
    antlr4::tree::TerminalNode *DELIMITER();
    antlr4::tree::TerminalNode *DELIMITERS();
    antlr4::tree::TerminalNode *DEPENDS();
    antlr4::tree::TerminalNode *DEPTH();
    antlr4::tree::TerminalNode *DETACH();
    antlr4::tree::TerminalNode *DICTIONARY();
    antlr4::tree::TerminalNode *DISABLE_P();
    antlr4::tree::TerminalNode *DISCARD();
    antlr4::tree::TerminalNode *DOCUMENT_P();
    antlr4::tree::TerminalNode *DOMAIN_P();
    antlr4::tree::TerminalNode *DOUBLE_P();
    antlr4::tree::TerminalNode *DROP();
    antlr4::tree::TerminalNode *EACH();
    antlr4::tree::TerminalNode *EMPTY_P();
    antlr4::tree::TerminalNode *ENABLE_P();
    antlr4::tree::TerminalNode *ENCODING();
    antlr4::tree::TerminalNode *ENCRYPTED();
    antlr4::tree::TerminalNode *ENUM_P();
    antlr4::tree::TerminalNode *ERROR();
    antlr4::tree::TerminalNode *ESCAPE();
    antlr4::tree::TerminalNode *EVENT();
    antlr4::tree::TerminalNode *EXCLUDE();
    antlr4::tree::TerminalNode *EXCLUDING();
    antlr4::tree::TerminalNode *EXCLUSIVE();
    antlr4::tree::TerminalNode *EXECUTE();
    antlr4::tree::TerminalNode *EXPLAIN();
    antlr4::tree::TerminalNode *EXPRESSION();
    antlr4::tree::TerminalNode *EXTENSION();
    antlr4::tree::TerminalNode *EXTERNAL();
    antlr4::tree::TerminalNode *FAMILY();
    antlr4::tree::TerminalNode *FILTER();
    antlr4::tree::TerminalNode *FINALIZE();
    antlr4::tree::TerminalNode *FIRST_P();
    antlr4::tree::TerminalNode *FOLLOWING();
    antlr4::tree::TerminalNode *FORCE();
    antlr4::tree::TerminalNode *FORMAT();
    antlr4::tree::TerminalNode *FORWARD();
    antlr4::tree::TerminalNode *FUNCTION();
    antlr4::tree::TerminalNode *FUNCTIONS();
    antlr4::tree::TerminalNode *GENERATED();
    antlr4::tree::TerminalNode *GLOBAL();
    antlr4::tree::TerminalNode *GRANTED();
    antlr4::tree::TerminalNode *GROUPS();
    antlr4::tree::TerminalNode *HANDLER();
    antlr4::tree::TerminalNode *HEADER_P();
    antlr4::tree::TerminalNode *HOLD();
    antlr4::tree::TerminalNode *HOUR_P();
    antlr4::tree::TerminalNode *IDENTITY_P();
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *IMMEDIATE();
    antlr4::tree::TerminalNode *IMMUTABLE();
    antlr4::tree::TerminalNode *IMPLICIT_P();
    antlr4::tree::TerminalNode *IMPORT_P();
    antlr4::tree::TerminalNode *INCLUDE();
    antlr4::tree::TerminalNode *INCLUDING();
    antlr4::tree::TerminalNode *INCREMENT();
    antlr4::tree::TerminalNode *INDENT();
    antlr4::tree::TerminalNode *INDEX();
    antlr4::tree::TerminalNode *INDEXES();
    antlr4::tree::TerminalNode *INHERIT();
    antlr4::tree::TerminalNode *INHERITS();
    antlr4::tree::TerminalNode *INLINE_P();
    antlr4::tree::TerminalNode *INPUT_P();
    antlr4::tree::TerminalNode *INSENSITIVE();
    antlr4::tree::TerminalNode *INSERT();
    antlr4::tree::TerminalNode *INSTEAD();
    antlr4::tree::TerminalNode *INVOKER();
    antlr4::tree::TerminalNode *ISOLATION();
    antlr4::tree::TerminalNode *KEEP();
    antlr4::tree::TerminalNode *KEY();
    antlr4::tree::TerminalNode *KEYS();
    antlr4::tree::TerminalNode *LABEL();
    antlr4::tree::TerminalNode *LANGUAGE();
    antlr4::tree::TerminalNode *LARGE_P();
    antlr4::tree::TerminalNode *LAST_P();
    antlr4::tree::TerminalNode *LEAKPROOF();
    antlr4::tree::TerminalNode *LEVEL();
    antlr4::tree::TerminalNode *LISTEN();
    antlr4::tree::TerminalNode *LOAD();
    antlr4::tree::TerminalNode *LOCAL();
    antlr4::tree::TerminalNode *LOCATION();
    antlr4::tree::TerminalNode *LOCK_P();
    antlr4::tree::TerminalNode *LOCKED();
    antlr4::tree::TerminalNode *LOGGED();
    antlr4::tree::TerminalNode *MAPPING();
    antlr4::tree::TerminalNode *MATCH();
    antlr4::tree::TerminalNode *MATCHED();
    antlr4::tree::TerminalNode *MATERIALIZED();
    antlr4::tree::TerminalNode *MAXVALUE();
    antlr4::tree::TerminalNode *MERGE();
    antlr4::tree::TerminalNode *METHOD();
    antlr4::tree::TerminalNode *MINUTE_P();
    antlr4::tree::TerminalNode *MINVALUE();
    antlr4::tree::TerminalNode *MODE();
    antlr4::tree::TerminalNode *MONTH_P();
    antlr4::tree::TerminalNode *MOVE();
    antlr4::tree::TerminalNode *NAME_P();
    antlr4::tree::TerminalNode *NAMES();
    antlr4::tree::TerminalNode *NESTED();
    antlr4::tree::TerminalNode *NEW();
    antlr4::tree::TerminalNode *NEXT();
    antlr4::tree::TerminalNode *NFC();
    antlr4::tree::TerminalNode *NFD();
    antlr4::tree::TerminalNode *NFKC();
    antlr4::tree::TerminalNode *NFKD();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *NORMALIZED();
    antlr4::tree::TerminalNode *NOTHING();
    antlr4::tree::TerminalNode *NOTIFY();
    antlr4::tree::TerminalNode *NOWAIT();
    antlr4::tree::TerminalNode *NULLS_P();
    antlr4::tree::TerminalNode *OBJECT_P();
    antlr4::tree::TerminalNode *OF();
    antlr4::tree::TerminalNode *OFF();
    antlr4::tree::TerminalNode *OIDS();
    antlr4::tree::TerminalNode *OLD();
    antlr4::tree::TerminalNode *OMIT();
    antlr4::tree::TerminalNode *OPERATOR();
    antlr4::tree::TerminalNode *OPTION();
    antlr4::tree::TerminalNode *OPTIONS();
    antlr4::tree::TerminalNode *ORDINALITY();
    antlr4::tree::TerminalNode *OTHERS();
    antlr4::tree::TerminalNode *OVER();
    antlr4::tree::TerminalNode *OVERRIDING();
    antlr4::tree::TerminalNode *OWNED();
    antlr4::tree::TerminalNode *OWNER();
    antlr4::tree::TerminalNode *PARALLEL();
    antlr4::tree::TerminalNode *PARAMETER();
    antlr4::tree::TerminalNode *PARSER();
    antlr4::tree::TerminalNode *PARTIAL();
    antlr4::tree::TerminalNode *PARTITION();
    antlr4::tree::TerminalNode *PASSING();
    antlr4::tree::TerminalNode *PASSWORD();
    antlr4::tree::TerminalNode *PATH();
    antlr4::tree::TerminalNode *PERIOD();
    antlr4::tree::TerminalNode *PLAN();
    antlr4::tree::TerminalNode *PLANS();
    antlr4::tree::TerminalNode *POLICY();
    antlr4::tree::TerminalNode *PRECEDING();
    antlr4::tree::TerminalNode *PREPARE();
    antlr4::tree::TerminalNode *PREPARED();
    antlr4::tree::TerminalNode *PRESERVE();
    antlr4::tree::TerminalNode *PRIOR();
    antlr4::tree::TerminalNode *PRIVILEGES();
    antlr4::tree::TerminalNode *PROCEDURAL();
    antlr4::tree::TerminalNode *PROCEDURE();
    antlr4::tree::TerminalNode *PROCEDURES();
    antlr4::tree::TerminalNode *PROGRAM();
    antlr4::tree::TerminalNode *PUBLICATION();
    antlr4::tree::TerminalNode *QUOTE();
    antlr4::tree::TerminalNode *QUOTES();
    antlr4::tree::TerminalNode *RANGE();
    antlr4::tree::TerminalNode *READ();
    antlr4::tree::TerminalNode *REASSIGN();
    antlr4::tree::TerminalNode *RECURSIVE();
    antlr4::tree::TerminalNode *REF();
    antlr4::tree::TerminalNode *REFERENCING();
    antlr4::tree::TerminalNode *REFRESH();
    antlr4::tree::TerminalNode *REINDEX();
    antlr4::tree::TerminalNode *RELATIVE_P();
    antlr4::tree::TerminalNode *RELEASE();
    antlr4::tree::TerminalNode *RENAME();
    antlr4::tree::TerminalNode *REPEATABLE();
    antlr4::tree::TerminalNode *REPLACE();
    antlr4::tree::TerminalNode *REPLICA();
    antlr4::tree::TerminalNode *RESET();
    antlr4::tree::TerminalNode *RESTART();
    antlr4::tree::TerminalNode *RESTRICT();
    antlr4::tree::TerminalNode *RETURN();
    antlr4::tree::TerminalNode *RETURNS();
    antlr4::tree::TerminalNode *REVOKE();
    antlr4::tree::TerminalNode *ROLE();
    antlr4::tree::TerminalNode *ROLLBACK();
    antlr4::tree::TerminalNode *ROLLUP();
    antlr4::tree::TerminalNode *ROUTINE();
    antlr4::tree::TerminalNode *ROUTINES();
    antlr4::tree::TerminalNode *ROWS();
    antlr4::tree::TerminalNode *RULE();
    antlr4::tree::TerminalNode *SAVEPOINT();
    antlr4::tree::TerminalNode *SCALAR();
    antlr4::tree::TerminalNode *SCHEMA();
    antlr4::tree::TerminalNode *SCHEMAS();
    antlr4::tree::TerminalNode *SCROLL();
    antlr4::tree::TerminalNode *SEARCH();
    antlr4::tree::TerminalNode *SECOND_P();
    antlr4::tree::TerminalNode *SECURITY();
    antlr4::tree::TerminalNode *SEQUENCE();
    antlr4::tree::TerminalNode *SEQUENCES();
    antlr4::tree::TerminalNode *SERIALIZABLE();
    antlr4::tree::TerminalNode *SERVER();
    antlr4::tree::TerminalNode *SESSION();
    antlr4::tree::TerminalNode *SET();
    antlr4::tree::TerminalNode *SETS();
    antlr4::tree::TerminalNode *SHARE();
    antlr4::tree::TerminalNode *SHOW();
    antlr4::tree::TerminalNode *SIMPLE();
    antlr4::tree::TerminalNode *SKIP_P();
    antlr4::tree::TerminalNode *SNAPSHOT();
    antlr4::tree::TerminalNode *SOURCE();
    antlr4::tree::TerminalNode *SQL_P();
    antlr4::tree::TerminalNode *STABLE();
    antlr4::tree::TerminalNode *STANDALONE_P();
    antlr4::tree::TerminalNode *START();
    antlr4::tree::TerminalNode *STATEMENT();
    antlr4::tree::TerminalNode *STATISTICS();
    antlr4::tree::TerminalNode *STDIN();
    antlr4::tree::TerminalNode *STDOUT();
    antlr4::tree::TerminalNode *STORAGE();
    antlr4::tree::TerminalNode *STORED();
    antlr4::tree::TerminalNode *STRICT_P();
    antlr4::tree::TerminalNode *STRING_P();
    antlr4::tree::TerminalNode *STRIP_P();
    antlr4::tree::TerminalNode *SUBSCRIPTION();
    antlr4::tree::TerminalNode *SUPPORT();
    antlr4::tree::TerminalNode *SYSID();
    antlr4::tree::TerminalNode *SYSTEM_P();
    antlr4::tree::TerminalNode *TABLES();
    antlr4::tree::TerminalNode *TABLESPACE();
    antlr4::tree::TerminalNode *TARGET();
    antlr4::tree::TerminalNode *TEMP();
    antlr4::tree::TerminalNode *TEMPLATE();
    antlr4::tree::TerminalNode *TEMPORARY();
    antlr4::tree::TerminalNode *TEXT_P();
    antlr4::tree::TerminalNode *TIES();
    antlr4::tree::TerminalNode *TRANSACTION();
    antlr4::tree::TerminalNode *TRANSFORM();
    antlr4::tree::TerminalNode *TRIGGER();
    antlr4::tree::TerminalNode *TRUNCATE();
    antlr4::tree::TerminalNode *TRUSTED();
    antlr4::tree::TerminalNode *TYPE_P();
    antlr4::tree::TerminalNode *TYPES_P();
    antlr4::tree::TerminalNode *UESCAPE();
    antlr4::tree::TerminalNode *UNBOUNDED();
    antlr4::tree::TerminalNode *UNCOMMITTED();
    antlr4::tree::TerminalNode *UNCONDITIONAL();
    antlr4::tree::TerminalNode *UNENCRYPTED();
    antlr4::tree::TerminalNode *UNKNOWN();
    antlr4::tree::TerminalNode *UNLISTEN();
    antlr4::tree::TerminalNode *UNLOGGED();
    antlr4::tree::TerminalNode *UNTIL();
    antlr4::tree::TerminalNode *UPDATE();
    antlr4::tree::TerminalNode *VACUUM();
    antlr4::tree::TerminalNode *VALID();
    antlr4::tree::TerminalNode *VALIDATE();
    antlr4::tree::TerminalNode *VALIDATOR();
    antlr4::tree::TerminalNode *VALUE_P();
    antlr4::tree::TerminalNode *VARYING();
    antlr4::tree::TerminalNode *VERSION_P();
    antlr4::tree::TerminalNode *VIEW();
    antlr4::tree::TerminalNode *VIEWS();
    antlr4::tree::TerminalNode *VOLATILE();
    antlr4::tree::TerminalNode *WHITESPACE_P();
    antlr4::tree::TerminalNode *WITHIN();
    antlr4::tree::TerminalNode *WITHOUT();
    antlr4::tree::TerminalNode *WORK();
    antlr4::tree::TerminalNode *WRAPPER();
    antlr4::tree::TerminalNode *WRITE();
    antlr4::tree::TerminalNode *XML_P();
    antlr4::tree::TerminalNode *YEAR_P();
    antlr4::tree::TerminalNode *YES_P();
    antlr4::tree::TerminalNode *ZONE();

   
  };

  Unreserved_keywordContext* unreserved_keyword();

  class  Col_name_keywordContext : public antlr4::ParserRuleContext {
  public:
    Col_name_keywordContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BETWEEN();
    antlr4::tree::TerminalNode *BIGINT();
    antlr4::tree::TerminalNode *BIT();
    antlr4::tree::TerminalNode *BOOLEAN_P();
    antlr4::tree::TerminalNode *CHAR_P();
    CharacterContext *character();
    antlr4::tree::TerminalNode *COALESCE();
    antlr4::tree::TerminalNode *DEC();
    antlr4::tree::TerminalNode *DECIMAL_P();
    antlr4::tree::TerminalNode *EXISTS();
    antlr4::tree::TerminalNode *EXTRACT();
    antlr4::tree::TerminalNode *FLOAT_P();
    antlr4::tree::TerminalNode *GREATEST();
    antlr4::tree::TerminalNode *GROUPING();
    antlr4::tree::TerminalNode *INOUT();
    antlr4::tree::TerminalNode *INT_P();
    antlr4::tree::TerminalNode *INTEGER();
    antlr4::tree::TerminalNode *INTERVAL();
    antlr4::tree::TerminalNode *JSON();
    antlr4::tree::TerminalNode *JSON_ARRAY();
    antlr4::tree::TerminalNode *JSON_ARRAYAGG();
    antlr4::tree::TerminalNode *JSON_EXISTS();
    antlr4::tree::TerminalNode *JSON_OBJECT();
    antlr4::tree::TerminalNode *JSON_OBJECTAGG();
    antlr4::tree::TerminalNode *JSON_QUERY();
    antlr4::tree::TerminalNode *JSON_SCALAR();
    antlr4::tree::TerminalNode *JSON_SERIALIZE();
    antlr4::tree::TerminalNode *JSON_TABLE();
    antlr4::tree::TerminalNode *JSON_VALUE();
    antlr4::tree::TerminalNode *LEAST();
    antlr4::tree::TerminalNode *MERGE_ACTION();
    antlr4::tree::TerminalNode *NATIONAL();
    antlr4::tree::TerminalNode *NCHAR();
    antlr4::tree::TerminalNode *NONE();
    antlr4::tree::TerminalNode *NORMALIZE();
    antlr4::tree::TerminalNode *NULLIF();
    antlr4::tree::TerminalNode *NUMERIC();
    antlr4::tree::TerminalNode *OUT_P();
    antlr4::tree::TerminalNode *OVERLAY();
    antlr4::tree::TerminalNode *POSITION();
    antlr4::tree::TerminalNode *PRECISION();
    antlr4::tree::TerminalNode *REAL();
    antlr4::tree::TerminalNode *ROW();
    antlr4::tree::TerminalNode *SETOF();
    antlr4::tree::TerminalNode *SMALLINT();
    antlr4::tree::TerminalNode *SUBSTRING();
    antlr4::tree::TerminalNode *TIME();
    antlr4::tree::TerminalNode *TIMESTAMP();
    antlr4::tree::TerminalNode *TREAT();
    antlr4::tree::TerminalNode *TRIM();
    antlr4::tree::TerminalNode *VALUES();
    antlr4::tree::TerminalNode *VARCHAR();
    antlr4::tree::TerminalNode *XMLATTRIBUTES();
    antlr4::tree::TerminalNode *XMLCONCAT();
    antlr4::tree::TerminalNode *XMLELEMENT();
    antlr4::tree::TerminalNode *XMLEXISTS();
    antlr4::tree::TerminalNode *XMLFOREST();
    antlr4::tree::TerminalNode *XMLNAMESPACES();
    antlr4::tree::TerminalNode *XMLPARSE();
    antlr4::tree::TerminalNode *XMLPI();
    antlr4::tree::TerminalNode *XMLROOT();
    antlr4::tree::TerminalNode *XMLSERIALIZE();
    antlr4::tree::TerminalNode *XMLTABLE();

   
  };

  Col_name_keywordContext* col_name_keyword();

  class  Type_func_name_keywordContext : public antlr4::ParserRuleContext {
  public:
    Type_func_name_keywordContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *AUTHORIZATION();
    antlr4::tree::TerminalNode *BINARY();
    antlr4::tree::TerminalNode *COLLATION();
    antlr4::tree::TerminalNode *CONCURRENTLY();
    antlr4::tree::TerminalNode *CROSS();
    antlr4::tree::TerminalNode *CURRENT_SCHEMA();
    antlr4::tree::TerminalNode *FREEZE();
    antlr4::tree::TerminalNode *FULL();
    antlr4::tree::TerminalNode *ILIKE();
    antlr4::tree::TerminalNode *INNER_P();
    antlr4::tree::TerminalNode *IS();
    antlr4::tree::TerminalNode *ISNULL();
    antlr4::tree::TerminalNode *JOIN();
    antlr4::tree::TerminalNode *LEFT();
    antlr4::tree::TerminalNode *LIKE();
    antlr4::tree::TerminalNode *NATURAL();
    antlr4::tree::TerminalNode *NOTNULL();
    antlr4::tree::TerminalNode *OUTER_P();
    antlr4::tree::TerminalNode *OVERLAPS();
    antlr4::tree::TerminalNode *RIGHT();
    antlr4::tree::TerminalNode *SIMILAR();
    antlr4::tree::TerminalNode *TABLESAMPLE();
    antlr4::tree::TerminalNode *VERBOSE();

   
  };

  Type_func_name_keywordContext* type_func_name_keyword();

  class  Reserved_keywordContext : public antlr4::ParserRuleContext {
  public:
    Reserved_keywordContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *ANALYSE();
    antlr4::tree::TerminalNode *ANALYZE();
    antlr4::tree::TerminalNode *AND();
    antlr4::tree::TerminalNode *ANY();
    antlr4::tree::TerminalNode *ARRAY();
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *ASC();
    antlr4::tree::TerminalNode *ASYMMETRIC();
    antlr4::tree::TerminalNode *BOTH();
    antlr4::tree::TerminalNode *CASE();
    antlr4::tree::TerminalNode *CAST();
    antlr4::tree::TerminalNode *CHECK();
    antlr4::tree::TerminalNode *COLLATE();
    antlr4::tree::TerminalNode *COLUMN();
    antlr4::tree::TerminalNode *CONSTRAINT();
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *CURRENT_CATALOG();
    antlr4::tree::TerminalNode *CURRENT_DATE();
    antlr4::tree::TerminalNode *CURRENT_ROLE();
    antlr4::tree::TerminalNode *CURRENT_TIME();
    antlr4::tree::TerminalNode *CURRENT_TIMESTAMP();
    antlr4::tree::TerminalNode *CURRENT_USER();
    antlr4::tree::TerminalNode *DEFAULT();
    antlr4::tree::TerminalNode *DEFERRABLE();
    antlr4::tree::TerminalNode *DESC();
    antlr4::tree::TerminalNode *DISTINCT();
    antlr4::tree::TerminalNode *DO();
    antlr4::tree::TerminalNode *ELSE();
    antlr4::tree::TerminalNode *END_P();
    antlr4::tree::TerminalNode *EXCEPT();
    antlr4::tree::TerminalNode *FALSE_P();
    antlr4::tree::TerminalNode *FETCH();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *FOREIGN();
    antlr4::tree::TerminalNode *FROM();
    antlr4::tree::TerminalNode *GRANT();
    antlr4::tree::TerminalNode *GROUP_P();
    antlr4::tree::TerminalNode *HAVING();
    antlr4::tree::TerminalNode *IN_P();
    antlr4::tree::TerminalNode *INITIALLY();
    antlr4::tree::TerminalNode *INTERSECT();
    antlr4::tree::TerminalNode *INTO();
    antlr4::tree::TerminalNode *LATERAL_P();
    antlr4::tree::TerminalNode *LEADING();
    antlr4::tree::TerminalNode *LIMIT();
    antlr4::tree::TerminalNode *LOCALTIME();
    antlr4::tree::TerminalNode *LOCALTIMESTAMP();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *NULL_P();
    antlr4::tree::TerminalNode *OFFSET();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *ONLY();
    antlr4::tree::TerminalNode *OR();
    antlr4::tree::TerminalNode *ORDER();
    antlr4::tree::TerminalNode *PLACING();
    antlr4::tree::TerminalNode *PRIMARY();
    antlr4::tree::TerminalNode *REFERENCES();
    antlr4::tree::TerminalNode *RETURNING();
    antlr4::tree::TerminalNode *SELECT();
    antlr4::tree::TerminalNode *SESSION_USER();
    antlr4::tree::TerminalNode *SOME();
    antlr4::tree::TerminalNode *SYMMETRIC();
    antlr4::tree::TerminalNode *SYSTEM_USER();
    antlr4::tree::TerminalNode *TABLE();
    antlr4::tree::TerminalNode *THEN();
    antlr4::tree::TerminalNode *TO();
    antlr4::tree::TerminalNode *TRAILING();
    antlr4::tree::TerminalNode *TRUE_P();
    antlr4::tree::TerminalNode *UNION();
    antlr4::tree::TerminalNode *UNIQUE();
    antlr4::tree::TerminalNode *USER();
    antlr4::tree::TerminalNode *USING();
    antlr4::tree::TerminalNode *VARIADIC();
    antlr4::tree::TerminalNode *WHEN();
    antlr4::tree::TerminalNode *WHERE();
    antlr4::tree::TerminalNode *WINDOW();
    antlr4::tree::TerminalNode *WITH();

   
  };

  Reserved_keywordContext* reserved_keyword();

  class  Bare_label_keywordContext : public antlr4::ParserRuleContext {
  public:
    Bare_label_keywordContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ABORT_P();
    antlr4::tree::TerminalNode *ABSENT();
    antlr4::tree::TerminalNode *ABSOLUTE_P();
    antlr4::tree::TerminalNode *ACCESS();
    antlr4::tree::TerminalNode *ACTION();
    antlr4::tree::TerminalNode *ADD_P();
    antlr4::tree::TerminalNode *ADMIN();
    antlr4::tree::TerminalNode *AFTER();
    antlr4::tree::TerminalNode *AGGREGATE();
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *ALSO();
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *ALWAYS();
    antlr4::tree::TerminalNode *ANALYSE();
    antlr4::tree::TerminalNode *ANALYZE();
    antlr4::tree::TerminalNode *AND();
    antlr4::tree::TerminalNode *ANY();
    antlr4::tree::TerminalNode *ASC();
    antlr4::tree::TerminalNode *ASENSITIVE();
    antlr4::tree::TerminalNode *ASSERTION();
    antlr4::tree::TerminalNode *ASSIGNMENT();
    antlr4::tree::TerminalNode *ASYMMETRIC();
    antlr4::tree::TerminalNode *AT();
    antlr4::tree::TerminalNode *ATOMIC();
    antlr4::tree::TerminalNode *ATTACH();
    antlr4::tree::TerminalNode *ATTRIBUTE();
    antlr4::tree::TerminalNode *AUTHORIZATION();
    antlr4::tree::TerminalNode *BACKWARD();
    antlr4::tree::TerminalNode *BEFORE();
    antlr4::tree::TerminalNode *BEGIN_P();
    antlr4::tree::TerminalNode *BETWEEN();
    antlr4::tree::TerminalNode *BIGINT();
    antlr4::tree::TerminalNode *BINARY();
    antlr4::tree::TerminalNode *BIT();
    antlr4::tree::TerminalNode *BOOLEAN_P();
    antlr4::tree::TerminalNode *BOTH();
    antlr4::tree::TerminalNode *BREADTH();
    antlr4::tree::TerminalNode *BY();
    antlr4::tree::TerminalNode *CACHE();
    antlr4::tree::TerminalNode *CALL();
    antlr4::tree::TerminalNode *CALLED();
    antlr4::tree::TerminalNode *CASCADE();
    antlr4::tree::TerminalNode *CASCADED();
    antlr4::tree::TerminalNode *CASE();
    antlr4::tree::TerminalNode *CAST();
    antlr4::tree::TerminalNode *CATALOG();
    antlr4::tree::TerminalNode *CHAIN();
    antlr4::tree::TerminalNode *CHARACTERISTICS();
    antlr4::tree::TerminalNode *CHECK();
    antlr4::tree::TerminalNode *CHECKPOINT();
    antlr4::tree::TerminalNode *CLASS();
    antlr4::tree::TerminalNode *CLOSE();
    antlr4::tree::TerminalNode *CLUSTER();
    antlr4::tree::TerminalNode *COALESCE();
    antlr4::tree::TerminalNode *COLLATE();
    antlr4::tree::TerminalNode *COLLATION();
    antlr4::tree::TerminalNode *COLUMN();
    antlr4::tree::TerminalNode *COLUMNS();
    antlr4::tree::TerminalNode *COMMENT();
    antlr4::tree::TerminalNode *COMMENTS();
    antlr4::tree::TerminalNode *COMMIT();
    antlr4::tree::TerminalNode *COMMITTED();
    antlr4::tree::TerminalNode *COMPRESSION();
    antlr4::tree::TerminalNode *CONCURRENTLY();
    antlr4::tree::TerminalNode *CONDITIONAL();
    antlr4::tree::TerminalNode *CONFIGURATION();
    antlr4::tree::TerminalNode *CONFLICT();
    antlr4::tree::TerminalNode *CONNECTION();
    antlr4::tree::TerminalNode *CONSTRAINT();
    antlr4::tree::TerminalNode *CONSTRAINTS();
    antlr4::tree::TerminalNode *CONTENT_P();
    antlr4::tree::TerminalNode *CONTINUE_P();
    antlr4::tree::TerminalNode *CONVERSION_P();
    antlr4::tree::TerminalNode *COPY();
    antlr4::tree::TerminalNode *COST();
    antlr4::tree::TerminalNode *CROSS();
    antlr4::tree::TerminalNode *CSV();
    antlr4::tree::TerminalNode *CUBE();
    antlr4::tree::TerminalNode *CURRENT_CATALOG();
    antlr4::tree::TerminalNode *CURRENT_DATE();
    antlr4::tree::TerminalNode *CURRENT_P();
    antlr4::tree::TerminalNode *CURRENT_ROLE();
    antlr4::tree::TerminalNode *CURRENT_SCHEMA();
    antlr4::tree::TerminalNode *CURRENT_TIME();
    antlr4::tree::TerminalNode *CURRENT_TIMESTAMP();
    antlr4::tree::TerminalNode *CURRENT_USER();
    antlr4::tree::TerminalNode *CURSOR();
    antlr4::tree::TerminalNode *CYCLE();
    antlr4::tree::TerminalNode *DATA_P();
    antlr4::tree::TerminalNode *DATABASE();
    antlr4::tree::TerminalNode *DEALLOCATE();
    antlr4::tree::TerminalNode *DEC();
    antlr4::tree::TerminalNode *DECIMAL_P();
    antlr4::tree::TerminalNode *DECLARE();
    antlr4::tree::TerminalNode *DEFAULT();
    antlr4::tree::TerminalNode *DEFAULTS();
    antlr4::tree::TerminalNode *DEFERRABLE();
    antlr4::tree::TerminalNode *DEFERRED();
    antlr4::tree::TerminalNode *DEFINER();
    antlr4::tree::TerminalNode *DELETE_P();
    antlr4::tree::TerminalNode *DELIMITER();
    antlr4::tree::TerminalNode *DELIMITERS();
    antlr4::tree::TerminalNode *DEPENDS();
    antlr4::tree::TerminalNode *DEPTH();
    antlr4::tree::TerminalNode *DESC();
    antlr4::tree::TerminalNode *DETACH();
    antlr4::tree::TerminalNode *DICTIONARY();
    antlr4::tree::TerminalNode *DISABLE_P();
    antlr4::tree::TerminalNode *DISCARD();
    antlr4::tree::TerminalNode *DISTINCT();
    antlr4::tree::TerminalNode *DO();
    antlr4::tree::TerminalNode *DOCUMENT_P();
    antlr4::tree::TerminalNode *DOMAIN_P();
    antlr4::tree::TerminalNode *DOUBLE_P();
    antlr4::tree::TerminalNode *DROP();
    antlr4::tree::TerminalNode *EACH();
    antlr4::tree::TerminalNode *ELSE();
    antlr4::tree::TerminalNode *EMPTY_P();
    antlr4::tree::TerminalNode *ENABLE_P();
    antlr4::tree::TerminalNode *ENCODING();
    antlr4::tree::TerminalNode *ENCRYPTED();
    antlr4::tree::TerminalNode *END_P();
    antlr4::tree::TerminalNode *ENUM_P();
    antlr4::tree::TerminalNode *ERROR();
    antlr4::tree::TerminalNode *ESCAPE();
    antlr4::tree::TerminalNode *EVENT();
    antlr4::tree::TerminalNode *EXCLUDE();
    antlr4::tree::TerminalNode *EXCLUDING();
    antlr4::tree::TerminalNode *EXCLUSIVE();
    antlr4::tree::TerminalNode *EXECUTE();
    antlr4::tree::TerminalNode *EXISTS();
    antlr4::tree::TerminalNode *EXPLAIN();
    antlr4::tree::TerminalNode *EXPRESSION();
    antlr4::tree::TerminalNode *EXTENSION();
    antlr4::tree::TerminalNode *EXTERNAL();
    antlr4::tree::TerminalNode *EXTRACT();
    antlr4::tree::TerminalNode *FALSE_P();
    antlr4::tree::TerminalNode *FAMILY();
    antlr4::tree::TerminalNode *FINALIZE();
    antlr4::tree::TerminalNode *FIRST_P();
    antlr4::tree::TerminalNode *FLOAT_P();
    antlr4::tree::TerminalNode *FOLLOWING();
    antlr4::tree::TerminalNode *FORCE();
    antlr4::tree::TerminalNode *FOREIGN();
    antlr4::tree::TerminalNode *FORMAT();
    antlr4::tree::TerminalNode *FORWARD();
    antlr4::tree::TerminalNode *FREEZE();
    antlr4::tree::TerminalNode *FULL();
    antlr4::tree::TerminalNode *FUNCTION();
    antlr4::tree::TerminalNode *FUNCTIONS();
    antlr4::tree::TerminalNode *GENERATED();
    antlr4::tree::TerminalNode *GLOBAL();
    antlr4::tree::TerminalNode *GRANTED();
    antlr4::tree::TerminalNode *GREATEST();
    antlr4::tree::TerminalNode *GROUPING();
    antlr4::tree::TerminalNode *GROUPS();
    antlr4::tree::TerminalNode *HANDLER();
    antlr4::tree::TerminalNode *HEADER_P();
    antlr4::tree::TerminalNode *HOLD();
    antlr4::tree::TerminalNode *IDENTITY_P();
    antlr4::tree::TerminalNode *IF_P();
    antlr4::tree::TerminalNode *ILIKE();
    antlr4::tree::TerminalNode *IMMEDIATE();
    antlr4::tree::TerminalNode *IMMUTABLE();
    antlr4::tree::TerminalNode *IMPLICIT_P();
    antlr4::tree::TerminalNode *IMPORT_P();
    antlr4::tree::TerminalNode *IN_P();
    antlr4::tree::TerminalNode *INCLUDE();
    antlr4::tree::TerminalNode *INCLUDING();
    antlr4::tree::TerminalNode *INCREMENT();
    antlr4::tree::TerminalNode *INDENT();
    antlr4::tree::TerminalNode *INDEX();
    antlr4::tree::TerminalNode *INDEXES();
    antlr4::tree::TerminalNode *INHERIT();
    antlr4::tree::TerminalNode *INHERITS();
    antlr4::tree::TerminalNode *INITIALLY();
    antlr4::tree::TerminalNode *INLINE_P();
    antlr4::tree::TerminalNode *INNER_P();
    antlr4::tree::TerminalNode *INOUT();
    antlr4::tree::TerminalNode *INPUT_P();
    antlr4::tree::TerminalNode *INSENSITIVE();
    antlr4::tree::TerminalNode *INSERT();
    antlr4::tree::TerminalNode *INSTEAD();
    antlr4::tree::TerminalNode *INT_P();
    antlr4::tree::TerminalNode *INTEGER();
    antlr4::tree::TerminalNode *INTERVAL();
    antlr4::tree::TerminalNode *INVOKER();
    antlr4::tree::TerminalNode *IS();
    antlr4::tree::TerminalNode *ISOLATION();
    antlr4::tree::TerminalNode *JOIN();
    antlr4::tree::TerminalNode *JSON();
    antlr4::tree::TerminalNode *JSON_ARRAY();
    antlr4::tree::TerminalNode *JSON_ARRAYAGG();
    antlr4::tree::TerminalNode *JSON_EXISTS();
    antlr4::tree::TerminalNode *JSON_OBJECT();
    antlr4::tree::TerminalNode *JSON_OBJECTAGG();
    antlr4::tree::TerminalNode *JSON_QUERY();
    antlr4::tree::TerminalNode *JSON_SCALAR();
    antlr4::tree::TerminalNode *JSON_SERIALIZE();
    antlr4::tree::TerminalNode *JSON_TABLE();
    antlr4::tree::TerminalNode *JSON_VALUE();
    antlr4::tree::TerminalNode *KEEP();
    antlr4::tree::TerminalNode *KEY();
    antlr4::tree::TerminalNode *KEYS();
    antlr4::tree::TerminalNode *LABEL();
    antlr4::tree::TerminalNode *LANGUAGE();
    antlr4::tree::TerminalNode *LARGE_P();
    antlr4::tree::TerminalNode *LAST_P();
    antlr4::tree::TerminalNode *LATERAL_P();
    antlr4::tree::TerminalNode *LEADING();
    antlr4::tree::TerminalNode *LEAKPROOF();
    antlr4::tree::TerminalNode *LEAST();
    antlr4::tree::TerminalNode *LEFT();
    antlr4::tree::TerminalNode *LEVEL();
    antlr4::tree::TerminalNode *LIKE();
    antlr4::tree::TerminalNode *LISTEN();
    antlr4::tree::TerminalNode *LOAD();
    antlr4::tree::TerminalNode *LOCAL();
    antlr4::tree::TerminalNode *LOCALTIME();
    antlr4::tree::TerminalNode *LOCALTIMESTAMP();
    antlr4::tree::TerminalNode *LOCATION();
    antlr4::tree::TerminalNode *LOCK_P();
    antlr4::tree::TerminalNode *LOCKED();
    antlr4::tree::TerminalNode *LOGGED();
    antlr4::tree::TerminalNode *MAPPING();
    antlr4::tree::TerminalNode *MATCH();
    antlr4::tree::TerminalNode *MATCHED();
    antlr4::tree::TerminalNode *MATERIALIZED();
    antlr4::tree::TerminalNode *MAXVALUE();
    antlr4::tree::TerminalNode *MERGE();
    antlr4::tree::TerminalNode *MERGE_ACTION();
    antlr4::tree::TerminalNode *METHOD();
    antlr4::tree::TerminalNode *MINVALUE();
    antlr4::tree::TerminalNode *MODE();
    antlr4::tree::TerminalNode *MOVE();
    antlr4::tree::TerminalNode *NAME_P();
    antlr4::tree::TerminalNode *NAMES();
    antlr4::tree::TerminalNode *NATIONAL();
    antlr4::tree::TerminalNode *NATURAL();
    antlr4::tree::TerminalNode *NCHAR();
    antlr4::tree::TerminalNode *NESTED();
    antlr4::tree::TerminalNode *NEW();
    antlr4::tree::TerminalNode *NEXT();
    antlr4::tree::TerminalNode *NFC();
    antlr4::tree::TerminalNode *NFD();
    antlr4::tree::TerminalNode *NFKC();
    antlr4::tree::TerminalNode *NFKD();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *NONE();
    antlr4::tree::TerminalNode *NORMALIZE();
    antlr4::tree::TerminalNode *NORMALIZED();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *NOTHING();
    antlr4::tree::TerminalNode *NOTIFY();
    antlr4::tree::TerminalNode *NOWAIT();
    antlr4::tree::TerminalNode *NULL_P();
    antlr4::tree::TerminalNode *NULLIF();
    antlr4::tree::TerminalNode *NULLS_P();
    antlr4::tree::TerminalNode *NUMERIC();
    antlr4::tree::TerminalNode *OBJECT_P();
    antlr4::tree::TerminalNode *OF();
    antlr4::tree::TerminalNode *OFF();
    antlr4::tree::TerminalNode *OIDS();
    antlr4::tree::TerminalNode *OLD();
    antlr4::tree::TerminalNode *OMIT();
    antlr4::tree::TerminalNode *ONLY();
    antlr4::tree::TerminalNode *OPERATOR();
    antlr4::tree::TerminalNode *OPTION();
    antlr4::tree::TerminalNode *OPTIONS();
    antlr4::tree::TerminalNode *OR();
    antlr4::tree::TerminalNode *ORDINALITY();
    antlr4::tree::TerminalNode *OTHERS();
    antlr4::tree::TerminalNode *OUT_P();
    antlr4::tree::TerminalNode *OUTER_P();
    antlr4::tree::TerminalNode *OVERLAY();
    antlr4::tree::TerminalNode *OVERRIDING();
    antlr4::tree::TerminalNode *OWNED();
    antlr4::tree::TerminalNode *OWNER();
    antlr4::tree::TerminalNode *PARALLEL();
    antlr4::tree::TerminalNode *PARAMETER();
    antlr4::tree::TerminalNode *PARSER();
    antlr4::tree::TerminalNode *PARTIAL();
    antlr4::tree::TerminalNode *PARTITION();
    antlr4::tree::TerminalNode *PASSING();
    antlr4::tree::TerminalNode *PASSWORD();
    antlr4::tree::TerminalNode *PATH();
    antlr4::tree::TerminalNode *PERIOD();
    antlr4::tree::TerminalNode *PLACING();
    antlr4::tree::TerminalNode *PLAN();
    antlr4::tree::TerminalNode *PLANS();
    antlr4::tree::TerminalNode *POLICY();
    antlr4::tree::TerminalNode *POSITION();
    antlr4::tree::TerminalNode *PRECEDING();
    antlr4::tree::TerminalNode *PREPARE();
    antlr4::tree::TerminalNode *PREPARED();
    antlr4::tree::TerminalNode *PRESERVE();
    antlr4::tree::TerminalNode *PRIMARY();
    antlr4::tree::TerminalNode *PRIOR();
    antlr4::tree::TerminalNode *PRIVILEGES();
    antlr4::tree::TerminalNode *PROCEDURAL();
    antlr4::tree::TerminalNode *PROCEDURE();
    antlr4::tree::TerminalNode *PROCEDURES();
    antlr4::tree::TerminalNode *PROGRAM();
    antlr4::tree::TerminalNode *PUBLICATION();
    antlr4::tree::TerminalNode *QUOTE();
    antlr4::tree::TerminalNode *QUOTES();
    antlr4::tree::TerminalNode *RANGE();
    antlr4::tree::TerminalNode *READ();
    antlr4::tree::TerminalNode *REAL();
    antlr4::tree::TerminalNode *REASSIGN();
    antlr4::tree::TerminalNode *RECURSIVE();
    antlr4::tree::TerminalNode *REF();
    antlr4::tree::TerminalNode *REFERENCES();
    antlr4::tree::TerminalNode *REFERENCING();
    antlr4::tree::TerminalNode *REFRESH();
    antlr4::tree::TerminalNode *REINDEX();
    antlr4::tree::TerminalNode *RELATIVE_P();
    antlr4::tree::TerminalNode *RELEASE();
    antlr4::tree::TerminalNode *RENAME();
    antlr4::tree::TerminalNode *REPEATABLE();
    antlr4::tree::TerminalNode *REPLACE();
    antlr4::tree::TerminalNode *REPLICA();
    antlr4::tree::TerminalNode *RESET();
    antlr4::tree::TerminalNode *RESTART();
    antlr4::tree::TerminalNode *RESTRICT();
    antlr4::tree::TerminalNode *RETURN();
    antlr4::tree::TerminalNode *RETURNS();
    antlr4::tree::TerminalNode *REVOKE();
    antlr4::tree::TerminalNode *RIGHT();
    antlr4::tree::TerminalNode *ROLE();
    antlr4::tree::TerminalNode *ROLLBACK();
    antlr4::tree::TerminalNode *ROLLUP();
    antlr4::tree::TerminalNode *ROUTINE();
    antlr4::tree::TerminalNode *ROUTINES();
    antlr4::tree::TerminalNode *ROW();
    antlr4::tree::TerminalNode *ROWS();
    antlr4::tree::TerminalNode *RULE();
    antlr4::tree::TerminalNode *SAVEPOINT();
    antlr4::tree::TerminalNode *SCALAR();
    antlr4::tree::TerminalNode *SCHEMA();
    antlr4::tree::TerminalNode *SCHEMAS();
    antlr4::tree::TerminalNode *SCROLL();
    antlr4::tree::TerminalNode *SEARCH();
    antlr4::tree::TerminalNode *SECURITY();
    antlr4::tree::TerminalNode *SELECT();
    antlr4::tree::TerminalNode *SEQUENCE();
    antlr4::tree::TerminalNode *SEQUENCES();
    antlr4::tree::TerminalNode *SERIALIZABLE();
    antlr4::tree::TerminalNode *SERVER();
    antlr4::tree::TerminalNode *SESSION();
    antlr4::tree::TerminalNode *SESSION_USER();
    antlr4::tree::TerminalNode *SET();
    antlr4::tree::TerminalNode *SETOF();
    antlr4::tree::TerminalNode *SETS();
    antlr4::tree::TerminalNode *SHARE();
    antlr4::tree::TerminalNode *SHOW();
    antlr4::tree::TerminalNode *SIMILAR();
    antlr4::tree::TerminalNode *SIMPLE();
    antlr4::tree::TerminalNode *SKIP_P();
    antlr4::tree::TerminalNode *SMALLINT();
    antlr4::tree::TerminalNode *SNAPSHOT();
    antlr4::tree::TerminalNode *SOME();
    antlr4::tree::TerminalNode *SOURCE();
    antlr4::tree::TerminalNode *SQL_P();
    antlr4::tree::TerminalNode *STABLE();
    antlr4::tree::TerminalNode *STANDALONE_P();
    antlr4::tree::TerminalNode *START();
    antlr4::tree::TerminalNode *STATEMENT();
    antlr4::tree::TerminalNode *STATISTICS();
    antlr4::tree::TerminalNode *STDIN();
    antlr4::tree::TerminalNode *STDOUT();
    antlr4::tree::TerminalNode *STORAGE();
    antlr4::tree::TerminalNode *STORED();
    antlr4::tree::TerminalNode *STRICT_P();
    antlr4::tree::TerminalNode *STRING_P();
    antlr4::tree::TerminalNode *STRIP_P();
    antlr4::tree::TerminalNode *SUBSCRIPTION();
    antlr4::tree::TerminalNode *SUBSTRING();
    antlr4::tree::TerminalNode *SUPPORT();
    antlr4::tree::TerminalNode *SYMMETRIC();
    antlr4::tree::TerminalNode *SYSID();
    antlr4::tree::TerminalNode *SYSTEM_P();
    antlr4::tree::TerminalNode *SYSTEM_USER();
    antlr4::tree::TerminalNode *TABLE();
    antlr4::tree::TerminalNode *TABLES();
    antlr4::tree::TerminalNode *TABLESAMPLE();
    antlr4::tree::TerminalNode *TABLESPACE();
    antlr4::tree::TerminalNode *TARGET();
    antlr4::tree::TerminalNode *TEMP();
    antlr4::tree::TerminalNode *TEMPLATE();
    antlr4::tree::TerminalNode *TEMPORARY();
    antlr4::tree::TerminalNode *TEXT_P();
    antlr4::tree::TerminalNode *THEN();
    antlr4::tree::TerminalNode *TIES();
    antlr4::tree::TerminalNode *TIME();
    antlr4::tree::TerminalNode *TIMESTAMP();
    antlr4::tree::TerminalNode *TRAILING();
    antlr4::tree::TerminalNode *TRANSACTION();
    antlr4::tree::TerminalNode *TRANSFORM();
    antlr4::tree::TerminalNode *TREAT();
    antlr4::tree::TerminalNode *TRIGGER();
    antlr4::tree::TerminalNode *TRIM();
    antlr4::tree::TerminalNode *TRUE_P();
    antlr4::tree::TerminalNode *TRUNCATE();
    antlr4::tree::TerminalNode *TRUSTED();
    antlr4::tree::TerminalNode *TYPE_P();
    antlr4::tree::TerminalNode *TYPES_P();
    antlr4::tree::TerminalNode *UESCAPE();
    antlr4::tree::TerminalNode *UNBOUNDED();
    antlr4::tree::TerminalNode *UNCOMMITTED();
    antlr4::tree::TerminalNode *UNCONDITIONAL();
    antlr4::tree::TerminalNode *UNENCRYPTED();
    antlr4::tree::TerminalNode *UNIQUE();
    antlr4::tree::TerminalNode *UNKNOWN();
    antlr4::tree::TerminalNode *UNLISTEN();
    antlr4::tree::TerminalNode *UNLOGGED();
    antlr4::tree::TerminalNode *UNTIL();
    antlr4::tree::TerminalNode *UPDATE();
    antlr4::tree::TerminalNode *USER();
    antlr4::tree::TerminalNode *USING();
    antlr4::tree::TerminalNode *VACUUM();
    antlr4::tree::TerminalNode *VALID();
    antlr4::tree::TerminalNode *VALIDATE();
    antlr4::tree::TerminalNode *VALIDATOR();
    antlr4::tree::TerminalNode *VALUE_P();
    antlr4::tree::TerminalNode *VALUES();
    antlr4::tree::TerminalNode *VARCHAR();
    antlr4::tree::TerminalNode *VARIADIC();
    antlr4::tree::TerminalNode *VERBOSE();
    antlr4::tree::TerminalNode *VERSION_P();
    antlr4::tree::TerminalNode *VIEW();
    antlr4::tree::TerminalNode *VIEWS();
    antlr4::tree::TerminalNode *VOLATILE();
    antlr4::tree::TerminalNode *WHEN();
    antlr4::tree::TerminalNode *WHITESPACE_P();
    antlr4::tree::TerminalNode *WORK();
    antlr4::tree::TerminalNode *WRAPPER();
    antlr4::tree::TerminalNode *WRITE();
    antlr4::tree::TerminalNode *XML_P();
    antlr4::tree::TerminalNode *XMLATTRIBUTES();
    antlr4::tree::TerminalNode *XMLCONCAT();
    antlr4::tree::TerminalNode *XMLELEMENT();
    antlr4::tree::TerminalNode *XMLEXISTS();
    antlr4::tree::TerminalNode *XMLFOREST();
    antlr4::tree::TerminalNode *XMLNAMESPACES();
    antlr4::tree::TerminalNode *XMLPARSE();
    antlr4::tree::TerminalNode *XMLPI();
    antlr4::tree::TerminalNode *XMLROOT();
    antlr4::tree::TerminalNode *XMLSERIALIZE();
    antlr4::tree::TerminalNode *XMLTABLE();
    antlr4::tree::TerminalNode *YES_P();
    antlr4::tree::TerminalNode *ZONE();

   
  };

  Bare_label_keywordContext* bare_label_keyword();

  class  Any_identifierContext : public antlr4::ParserRuleContext {
  public:
    Any_identifierContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ColidContext *colid();

   
  };

  Any_identifierContext* any_identifier();

  class  IdentifierContext : public antlr4::ParserRuleContext {
  public:
    IdentifierContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *Identifier();
    Uescape_Context *uescape_();
    antlr4::tree::TerminalNode *QuotedIdentifier();
    antlr4::tree::TerminalNode *UnicodeQuotedIdentifier();
    antlr4::tree::TerminalNode *PLSQLVARIABLENAME();

   
  };

  IdentifierContext* identifier();


  bool sempred(antlr4::RuleContext *_localctx, size_t ruleIndex, size_t predicateIndex) override;

  bool a_expr_qualSempred(A_expr_qualContext *_localctx, size_t predicateIndex);
  bool b_exprSempred(B_exprContext *_localctx, size_t predicateIndex);
  bool json_argumentsSempred(Json_argumentsContext *_localctx, size_t predicateIndex);
  bool json_name_and_value_listSempred(Json_name_and_value_listContext *_localctx, size_t predicateIndex);
  bool json_value_expr_listSempred(Json_value_expr_listContext *_localctx, size_t predicateIndex);

  // By default the static state used to implement the parser is lazily initialized during the first
  // call to the constructor. You can call this function if you wish to initialize the static state
  // ahead of time.
  static void initialize();

private:
};

}  // namespace graphit::antlr::postgresql
