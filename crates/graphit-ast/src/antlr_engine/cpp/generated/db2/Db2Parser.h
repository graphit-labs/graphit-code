
// Generated from crates/graphit-ast/src/antlr_engine/grammars/db2/Db2Parser.g4 by ANTLR 4.13.2

#pragma once


#include "antlr4-runtime.h"


namespace graphit::antlr::db2 {


class  Db2Parser : public antlr4::Parser {
public:
  enum {
    ACCESS = 1, ACCESSCTRL = 2, ACCESSTOKEN = 3, ACCESSTOKENTYPE = 4, ACCORDING = 5, 
    ACCUMULATE = 6, ACTION = 7, ACTIVATE = 8, ACTIVE = 9, ACTIVITIES = 10, 
    ACTIVITY = 11, ACTIVITYTOTALRUNTIME = 12, ACTIVITYTOTALRUNTIMEINALLSC = 13, 
    ACTIVITYTOTALTIME = 14, ACTUALS = 15, ADAPTIVE = 16, ADC = 17, ADD = 18, 
    ADDRESS = 19, ADMIN = 20, ADMISSION = 21, AFTER = 22, AGE = 23, AGGREGATE = 24, 
    AGGSQLTEMPSPACE = 25, ALIAS = 26, ALL = 27, ALLOCATE = 28, ALLOW = 29, 
    ALTER = 30, ALTERIN = 31, ALWAYS = 32, ANALYZE = 33, ANCHOR = 34, AND = 35, 
    ANY = 36, APIKEY = 37, APPEND = 38, APPLICATION = 39, APPLNAME = 40, 
    ARRAY = 41, ARRAY_EXISTS = 42, AS = 43, ASC = 44, ASCII = 45, ASENSITIVE = 46, 
    ASSOCIATE = 47, ASYNCHRONY = 48, AT = 49, ATOMIC = 50, ATTACH = 51, 
    ATTRIBUTE = 52, ATTRIBUTES = 53, AUDIT = 54, AUTHENTICATION = 55, AUTHID = 56, 
    AUTHORIZATION = 57, AUTHORIZATIONS = 58, AUTHORIZED = 59, AUTOMATIC = 60, 
    AUTONOMOUS = 61, AUTORESIZE = 62, AUTOSTART = 63, BACKUP = 64, BASE = 65, 
    BATCH = 66, BEFORE = 67, BEGIN = 68, BERNOULLI = 69, BETWEEN = 70, BIGINT = 71, 
    BIN = 72, BINARY = 73, BIND = 74, BINDADD = 75, BIT = 76, BLOB = 77, 
    BLOCKED = 78, BLOCKING = 79, BLOCKINSERT = 80, BLOCKSIZE = 81, BODY = 82, 
    BOOLEAN = 83, BOTH = 84, BREAK = 85, BSON = 86, BUFFER = 87, BUFFERPOOL = 88, 
    BUFFERPOOLS = 89, BUFFERSIZE = 90, BUILD = 91, BUSINESS_TIME = 92, BUSINESS_TYPE = 93, 
    BY = 94, CACHE = 95, CACHING = 96, CALL = 97, CALLED = 98, CALLER = 99, 
    CAPTURE = 100, CARDINALITY = 101, CASCADE = 102, CASCADED = 103, CASE = 104, 
    CAST = 105, CATEGORIES = 106, CBMCFGVALUES = 107, CCSID = 108, CFGALL = 109, 
    CHANGE = 110, CHANGES = 111, CHAR = 112, CHARACTER = 113, CHECK = 114, 
    CHECKED = 115, CHECKING = 116, CLASS_ = 117, CLIENT = 118, CLIENT_ACCTNG = 119, 
    CLIENT_APPLNAME = 120, CLIENT_USERID = 121, CLIENT_WRKSTNNAME = 122, 
    CLOB = 123, CLOSE = 124, CLUSTER = 125, CLR = 126, COBOL = 127, CODEUNITS16 = 128, 
    CODEUNITS32 = 129, COLLECT = 130, COLUMN = 131, COLUMNS = 132, COMMENT = 133, 
    COMMIT = 134, COMPACT = 135, COMPARISONS = 136, COMPILATION = 137, COMPONENT = 138, 
    COMPONENTS = 139, COMPOUND = 140, COMPRESS = 141, COMPRESSION = 142, 
    CONCAT = 143, CONCURRENTDBCOORDACTIVITIES = 144, CONCURRENTWORKLOADACTIVITIES = 145, 
    CONCURRENTWORKLOADOCCURRENCES = 146, CONDITION = 147, CONFIRM = 148, 
    CONNECT = 149, CONNECTION = 150, CONNECTIONIDLETIME = 151, CONSTANT = 152, 
    CONSTRAINT = 153, CONTAINERS = 154, CONTAINS = 155, CONTEXT = 156, CONTINUE = 157, 
    CONTROL = 158, CONVERT = 159, COORDINATOR = 160, CORRELATOR = 161, COUNT = 162, 
    CPP = 163, CPU = 164, CPUTIME = 165, CPUTIMEINSC = 166, CREATE = 167, 
    CREATEIN = 168, CREATETAB = 169, CREATE_EXTERNAL_ROUTINE = 170, CREATE_NOT_FENCED_ROUTINE = 171, 
    CREATE_SECURE_OBJECT = 172, CREATE_SOURCE_OBJECT = 173, CRITICAL = 174, 
    CROSS = 175, CS = 176, CUBE = 177, CURRENT = 178, CURRENT_PATH = 179, 
    CURRENT_USER = 180, ACCTNG = 181, USERID = 182, WRKSTNNAME = 183, TIMEZONE = 184, 
    SQLID = 185, CURRENT_DATE = 186, CURRENT_TIME = 187, CURRENT_TIMESTAMP = 188, 
    CURRENT_SCHEMA = 189, CURRENT_SQLID = 190, CURRENT_SERVER = 191, CURRENT_TIMEZONE = 192, 
    SYSDATE = 193, LOCALTIMESTAMP = 194, CURSOR = 195, CURSORS = 196, CUSTOM = 197, 
    CYCLE = 198, C_ = 199, DATA = 200, DATAACCESS = 201, DATABASE = 202, 
    DATALAKE = 203, DATATAGINSC = 204, DATE = 205, DAY = 206, DAYS = 207, 
    DB = 208, DB2GENERAL = 209, DB2LBACREADARRAY = 210, DB2LBACREADSET = 211, 
    DB2LBACREADTREE = 212, DB2LBACRULES = 213, DB2LBACWRITEARRAY = 214, 
    DB2LBACWRITESET = 215, DB2LBACWRITETREE = 216, DB2SECURITYLABEL = 217, 
    DB2SQL = 218, DB2_RETURN_STATUS = 219, DB2_SQL_NESTING_LEVEL = 220, 
    DB2_TOKEN_STRING = 221, DBADM = 222, DBCFG = 223, DBCFGVALUES = 224, 
    DBCLOB = 225, DBINFO = 226, DBMCFG = 227, DBPARTITIONNUM = 228, DBPARTITIONNUMS = 229, 
    DDL = 230, DDLALL = 231, DDLDATA = 232, DDLFEDERATED = 233, DDLMONITOR = 234, 
    DDLSECURITY = 235, DDLSQL = 236, DDLSTORAGE = 237, DDLWLM = 238, DDLXML = 239, 
    DEACTIVATE = 240, DEADLOCK = 241, DEC = 242, DECFLOAT = 243, DECIMAL = 244, 
    DECLARE = 245, DECOMPOSITION = 246, DEFAULT = 247, DEFAULTS = 248, DEFERRED = 249, 
    DEGREE_ = 250, DELETE = 251, DELETEIN = 252, DESC = 253, DESCRIBE = 254, 
    DESCRIPTOR = 255, DETACH = 256, DETAIL = 257, DETAILED = 258, DETAILS = 259, 
    DETERMINISTIC = 260, DEVICE = 261, DIAGNOSTICS = 262, DIMENSIONS = 263, 
    DISABLE = 264, DISALLOW = 265, DISCONNECT = 266, DISPATCH = 267, DISTINCT = 268, 
    DISTRIBUTE = 269, DISTRIBUTED = 270, DISTRIBUTION = 271, DML = 272, 
    DO = 273, DOUBLE = 274, DROP = 275, DROPIN = 276, DROPPED = 277, DYNAMIC = 278, 
    EACH = 279, ELEMENT = 280, ELSE = 281, ELSEIF = 282, EMPTY = 283, EMPTY_BLOB = 284, 
    EMPTY_CLOB = 285, EMPTY_DBCLOB = 286, EMPTY_NCLOB = 287, ENABLE = 288, 
    ENCRYPTION = 289, END = 290, ENDING = 291, ENFORCED = 292, ENFORCEMENT = 293, 
    ENVIRONMENT = 294, ERROR = 295, ESCAPE = 296, ESTIMATEDCOST = 297, ESTIMATEDSQLCOST = 298, 
    EVALUATE = 299, EVENT = 300, EVERY = 301, EXACT = 302, EXCEEDED = 303, 
    EXCEPT = 304, EXCEPTION = 305, EXCLUDING = 306, EXCLUSIVE = 307, EXECUTABLE = 308, 
    EXECUTE = 309, EXECUTEIN = 310, EXECUTETIME = 311, EXECUTION = 312, 
    EXEMPTION = 313, EXISTS = 314, EXPLAIN = 315, EXPRESSION = 316, EXTEND = 317, 
    EXTENDED = 318, EXTENSION = 319, EXTENTSIZE = 320, EXTERNAL = 321, FAILURE = 322, 
    FALSE = 323, FEDERATED = 324, FEDERATED_TOOL = 325, FENCED = 326, FETCH = 327, 
    FIFO = 328, FILE = 329, FILTER = 330, FINAL = 331, FINALIZE = 332, FIRST = 333, 
    FIRST_VALUE = 334, FLOAT = 335, FLUSH = 336, FOLLOWING = 337, FOR = 338, 
    FORCE = 339, FOREIGN = 340, FORMAT = 341, FOUND = 342, FREE = 343, FROM = 344, 
    FULL = 345, FUNCTION = 346, G = 347, GENERAL = 348, GENERATE = 349, 
    GENERATED = 350, GENERIC = 351, GET = 352, GLOBAL = 353, GO = 354, GOTO = 355, 
    GRANT = 356, GRAPHIC = 357, GROUP = 358, GROUPING = 359, HARD = 360, 
    HASH = 361, HIDDEN_ = 362, HIERARCHY = 363, HIGH = 364, HISTOGRAM = 365, 
    HISTORY = 366, HOLD = 367, HOUR = 368, HOURS = 369, IDENTITY = 370, 
    IF = 371, IGNORE = 372, IMMEDIATE = 373, IMPLEMENTATION = 374, IMPLICIT = 375, 
    IMPLICITLY = 376, IMPLICIT_SCHEMA = 377, IN = 378, INACTIVE = 379, INCLUDE = 380, 
    INCLUDING = 381, INCLUSIVE = 382, INCREASESIZE = 383, INCREMENT = 384, 
    INCREMENTAL = 385, INDEX = 386, INDEXES = 387, INFIX = 388, INHERIT = 389, 
    INITIALIZE = 390, INITIALLY = 391, INITIALSIZE = 392, INLINE = 393, 
    INNER = 394, INOUT = 395, INPUT = 396, INSENSITIVE = 397, INSERT = 398, 
    INSERTIN = 399, INSTANTIABLE = 400, INSTEAD = 401, INT = 402, INTEGER = 403, 
    INTEGRITY = 404, INTERACTIVE = 405, INTERARRIVALTIME = 406, INTERSECT = 407, 
    INTERVAL = 408, INTO = 409, INVALIDATION = 410, IS = 411, ISOLATION = 412, 
    ITERATE = 413, JAVA = 414, JOIN = 415, JSON = 416, JSON_EXISTS = 417, 
    K = 418, KEEP = 419, KEY = 420, LABEL = 421, LANGUAGE = 422, LARGE = 423, 
    LAST = 424, LAST_VALUE = 425, LATENCY = 426, LATERAL = 427, LC_MESSAGES_ = 428, 
    LC_TIME_ = 429, LEAVE = 430, LEFT = 431, LENGTH = 432, LEVEL = 433, 
    LIBRARY = 434, LIFETIME = 435, LIKE = 436, LIMIT = 437, LIST = 438, 
    LOAD = 439, LOCAL = 440, LOCALE = 441, LOCATION = 442, LOCATOR = 443, 
    LOCATORS = 444, LOCK = 445, LOCKED = 446, LOCKING = 447, LOCKS = 448, 
    LOCKSIZE = 449, LOG = 450, LOGGED = 451, LOGICAL = 452, LONG = 453, 
    LONGVAR = 454, LOOP = 455, LOW = 456, LOWER = 457, L_ONE = 458, L_ZERO = 459, 
    M = 460, MAIN = 461, MAINTAINED = 462, MANAGED = 463, MANUALSTART = 464, 
    MAP = 465, MAPPING = 466, MARK = 467, MASK = 468, MATCHED = 469, MATCHING = 470, 
    MATERIALIZED = 471, MAX = 472, MAXFILES = 473, MAXFILESIZE = 474, MAXIMUM = 475, 
    MAXSIZE = 476, MAXVALUE = 477, MDC = 478, MEDIUM = 479, MEMBER = 480, 
    MEMBERS = 481, MERGE = 482, MESSAGE_TEXT = 483, METHOD = 484, METRICS = 485, 
    MICROSECOND = 486, MICROSECONDS = 487, MINIMUM = 488, MINUTE = 489, 
    MINUTES = 490, MINVALUE = 491, MIXED = 492, MODE = 493, MODEL = 494, 
    MODULE = 495, MODIFICATION = 496, MODIFIES = 497, MONITOR = 498, MONTH = 499, 
    MONTHS = 500, MORE_ = 501, MOVEMENT = 502, MOVETABLE = 503, MULTIPLIER = 504, 
    NAME = 505, NAMESPACE = 506, NATIONAL = 507, NCHAR = 508, NCLOB = 509, 
    NESTED = 510, NEW = 511, NEXT = 512, NICKNAME = 513, NO = 514, NONBLOCKED = 515, 
    NONE = 516, NORCAC = 517, NORMAL = 518, NOT = 519, NOWAIT = 520, NPSGENERIC = 521, 
    NTH_VALUE = 522, NULLS = 523, NULL_ = 524, NUM = 525, NUMBER = 526, 
    NUMBLOCKPAGES = 527, NUMERIC = 528, NUM_EXECUTIONS = 529, NVARCHAR = 530, 
    OBJECT = 531, OBJMAINT = 532, OCCURENCE = 533, OCTETS = 534, OF = 535, 
    OFF = 536, OFFSET = 537, OLD = 538, OLE = 539, OLEDB = 540, ON = 541, 
    ONCE = 542, ONLINE = 543, ONLINERECOVERY = 544, ONLY = 545, OPEN = 546, 
    OPTIMIZATION = 547, OPTIMIZE = 548, OPTION = 549, OPTIONS = 550, OR = 551, 
    ORDER = 552, ORDINALITY = 553, ORGANIZED = 554, OUT = 555, OUTBOUND = 556, 
    OUTCOME = 557, OUTER = 558, OUTPUT = 559, OVER = 560, OVERFLOW = 561, 
    OVERHEAD = 562, OVERLAPS = 563, OVERRIDE_ = 564, OVERRIDING = 565, OWNERSHIP = 566, 
    PACKAGE = 567, PACKAGESET = 568, PAGESIZE = 569, PARALLEL = 570, PARAMETER = 571, 
    PARAMETERS = 572, PARTITION = 573, PARTITIONINGS = 574, PARTITIONS = 575, 
    PASSTHRU = 576, PASSWORD = 577, PATH = 578, PCTDEACTIVATE = 579, PCTFREE = 580, 
    PERCENT = 581, PERFORM = 582, PERIOD = 583, PERMISSION = 584, PIPE = 585, 
    PLAN = 586, POLICY = 587, PORTION = 588, POSITION = 589, PRECEDING = 590, 
    PRECISION = 591, PREDICATES = 592, PREFETCH = 593, PREFETCHSIZE = 594, 
    PREPARE = 595, PRESERVE = 596, PREVENT = 597, PREVIOUS = 598, PRIMARY = 599, 
    PRIORITY = 600, PRIVILEGES = 601, PROCEDURE = 602, PROFILE = 603, PROGRAM = 604, 
    PROPAGATE = 605, PRUNE = 606, PUBLIC = 607, PUBLISH = 608, PYTHON = 609, 
    P_ = 610, QUERY = 611, QUERYNO = 612, QUERYTAG = 613, QUEUE = 614, QUEUEDACTIVITIES = 615, 
    QUEUEDCONNECTIONS = 616, QUEUETIME = 617, QUIESCE_CONNECT = 618, RANDOM = 619, 
    RANGE = 620, RATE = 621, RATION_TO_REPORT = 622, READ = 623, READS = 624, 
    REAL = 625, REBALANCE = 626, RECOMMEND = 627, RECORD = 628, RECOVERY = 629, 
    REDISTRIBUTE = 630, REDUCE = 631, REF = 632, REFERENCE = 633, REFERENCES = 634, 
    REFERENCING = 635, REFRESH = 636, REGEXP_LIKE = 637, REGISTERS = 638, 
    REGULAR = 639, REGVAR = 640, REGVARVALUES = 641, RELATIONAL = 642, RELEASE = 643, 
    RELEASED = 644, REMAP = 645, REMOTE = 646, REMOVE = 647, RENAME = 648, 
    REOPT = 649, REORG = 650, REPEAT = 651, REPEATABLE = 652, REPLACE = 653, 
    REPLICATION = 654, REQUEST = 655, REQUIRE = 656, RESET = 657, RESIDENT = 658, 
    RESIGNAL = 659, RESIZE = 660, RESOURCE = 661, RESPECT = 662, RESTART = 663, 
    RESTORE = 664, RESTRICT = 665, RESULT = 666, RESUME = 667, RETAIN = 668, 
    RETURN = 669, RETURNS = 670, REUSE = 671, REVERT = 672, REVOKE = 673, 
    RIGHT = 674, ROLE = 675, ROLLBACK = 676, ROLLFORWARD = 677, ROLLOUT = 678, 
    ROLLUP = 679, ROOT = 680, ROUNDING = 681, ROUND_CEILING = 682, ROUND_DOWN = 683, 
    ROUND_FLOOR = 684, ROUND_HALF_EVEN = 685, ROUND_HALF_UP = 686, ROUTINES = 687, 
    ROW = 688, ROWS = 689, ROW_COUNT = 690, ROW_NUMBER = 691, RR = 692, 
    RS = 693, RULE = 694, RULES = 695, RUN = 696, RUNSTATS = 697, SAVEPOINT = 698, 
    SCALEBACK = 699, SCHEMA = 700, SCHEMAADM = 701, SCOPE = 702, SCRATCHPAD = 703, 
    SEARCH = 704, SECADM = 705, SECMAINT = 706, SECOND = 707, SECONDS = 708, 
    SECTION = 709, SECURED = 710, SECURITY = 711, SELECT = 712, SELECTIN = 713, 
    SELECTION = 714, SELECTIVITY = 715, SELF = 716, SEQUENCE = 717, SERVER = 718, 
    SERVICE = 719, SESSION = 720, SESSION_USER = 721, SET = 722, SETS = 723, 
    SETSESSIONUSER = 724, SHARE = 725, SHARES = 726, SHOW = 727, SIGNAL = 728, 
    SINGLE = 729, SIZE = 730, SKIP_ = 731, SMALLINT = 732, SNAPSHOT = 733, 
    SOFT = 734, SOME = 735, SORTMEM = 736, SORTSHRHEAPUTIL = 737, SOURCE = 738, 
    SPECIAL = 739, SPECIFIC = 740, SQL = 741, SQLADM = 742, SQLCA = 743, 
    SQLCODE = 744, SQLDA = 745, SQLERROR = 746, SQLROWSREAD = 747, SQLROWSREADINSC = 748, 
    SQLROWSRETURNED = 749, SQLSTATE = 750, SQLTEMPSPACE = 751, SQLWARNING = 752, 
    SQL_CCFLAGS = 753, STAGING = 754, START = 755, STARTING = 756, STATE = 757, 
    STATEMENT = 758, STATEMENTS = 759, STATIC = 760, STATISTICS = 761, STATUS = 762, 
    STAY = 763, STMT_EXEC_TIME = 764, STOGROUP = 765, STOP = 766, STORAGE = 767, 
    STRIPE = 768, STRONG = 769, STYLE = 770, SUB = 771, SUCCESS = 772, SUSPEND = 773, 
    SWITCH = 774, SYSADMIN = 775, SYSDEFAULTHISTOGRAM = 776, SYSDEFAULTUSERCLASS = 777, 
    SYSTEM = 778, SYSTEM_TIME = 779, SYSTEM_USER = 780, S_ = 781, TABLE = 782, 
    TABLES = 783, TABLESAMPLE = 784, TABLESPACE = 785, TABLESPACES = 786, 
    TAG = 787, TARGET = 788, TEMPLATE = 789, TEMPORAL = 790, TEMPORARY = 791, 
    TEXT = 792, THAN = 793, THEN = 794, THREADSAFE = 795, THRESHOLD = 796, 
    THROUGH = 797, TIME = 798, TIMEOUT = 799, TIMERONCOST = 800, TIMESTAMP = 801, 
    TO = 802, TOKEN = 803, TOTALMEMBERCONNECTIONS = 804, TOTALSCMEMBERCONNECTIONS = 805, 
    TRACKING = 806, TRANSACTION = 807, TRANSFER = 808, TRANSFERRATE = 809, 
    TRANSFORM = 810, TRANSFORMS = 811, TREAT = 812, TREE = 813, TRIGGER = 814, 
    TRIGGERS = 815, TRUE = 816, TRUNCATE = 817, TRUSTED = 818, TYPE = 819, 
    TYPES = 820, UNBOUNDED = 821, UNCHECKED = 822, UNDER = 823, UNFORMATTED = 824, 
    UNICODE = 825, UNION = 826, UNIQUE = 827, UNIT = 828, UNITS = 829, UNKNOWN = 830, 
    UNNEST = 831, UNTIL = 832, UOW = 833, UOWTOTALTIME = 834, UPDATE = 835, 
    UPDATED_SINCE_BOUNDARY_TIME = 836, UPDATEIN = 837, UPON = 838, UR = 839, 
    URI = 840, USAGE = 841, USE = 842, USER = 843, USING = 844, UTILALL = 845, 
    VALIDATE = 846, VALIDATED = 847, VALUE = 848, VALUES = 849, VARBINARY = 850, 
    VARCHAR = 851, VARGRAPHIC = 852, VARIABLE = 853, VARYING = 854, VERSION = 855, 
    VERSIONING = 856, VIEW = 857, VIOLATIONS = 858, VOLATILE = 859, WAIT = 860, 
    WAITING = 861, WATER = 862, WEAK = 863, WHEN = 864, WHENEVER = 865, 
    WHERE = 866, WHILE = 867, WITH = 868, WITHIN = 869, WITHOUT = 870, WLMADM = 871, 
    WORK = 872, WORKLOAD = 873, WRAP = 874, WRAPPER = 875, WRITE = 876, 
    WRITEDOWN = 877, WRITEUP = 878, XML = 879, XMLCAST = 880, XMLEXISTS = 881, 
    XMLPARSE = 882, XMLSCHEMA = 883, XQUERY = 884, XSROBJECT = 885, YEAR = 886, 
    YEARS = 887, YES = 888, INF = 889, INFINITY = 890, NAN = 891, SNAN = 892, 
    WHITE_SPACE = 893, SQL_COMMENT = 894, LINE_COMMENT = 895, HEX_CHARACTER_LITERAL = 896, 
    GRAPHIC_G_LITERAL = 897, GRAPHIC_N_LITERAL = 898, HEX_GRAPHIC_LITERAL = 899, 
    BINARY_STRING_LITERAL = 900, UTF16BE_GRAPHIC_LITERAL = 901, UNICODE_STRING_LITERAL = 902, 
    UESCAPE = 903, DOUBLE_QUOTE_ID = 904, SINGLE_QUOTE = 905, SYSPROC = 906, 
    ID = 907, STRING_LITERAL = 908, DECIMAL_LITERAL = 909, SINGLE_DIGIT = 910, 
    FLOAT_LITERAL = 911, REAL_LITERAL = 912, CHAR_LITERAL = 913, NE = 914, 
    LTGT = 915, EQ = 916, GT = 917, GE = 918, LT = 919, LE = 920, EXCLAMATION = 921, 
    PIPE_CHAR = 922, PIPE_PIPE = 923, DOT = 924, LEFT_RND_BKT = 925, RIGHT_RND_BKT = 926, 
    LEFT_SQR_BKT = 927, RIGHT_SQR_BKT = 928, LEFT_CURLY_BKT = 929, RIGHT_CURLY_BKT = 930, 
    COMMA = 931, SEMI = 932, TILDE = 933, STAR = 934, STAR_STAR = 935, DIVIDE = 936, 
    MODULO = 937, PLUS = 938, MINUS = 939, CARET = 940, AMPERSAND = 941, 
    COLON = 942, DOT_DOT = 943, ARROW = 944, ASSOC = 945
  };

  enum {
    RuleDb2_file = 0, RuleBatch = 1, RuleSql_statement = 2, RuleSql_schema_statement = 3, 
    RuleSql_data_change_statement = 4, RuleSql_data_statement = 5, RuleSql_transaction_statement = 6, 
    RuleSql_connection_statement = 7, RuleSql_dynamic_statement = 8, RuleSql_session_statement = 9, 
    RuleSql_embedded_host_language_statement = 10, RuleSql_constrol_statement = 11, 
    RuleSelect_statement = 12, RuleRead_only_clause = 13, RuleUpdate_clause = 14, 
    RuleOptimize_for_clause = 15, RuleConcurrent_access_resolution_clause = 16, 
    RuleDelete_statement = 17, RuleDelete_statement_searched_delete = 18, 
    RuleTable_or_view_name = 19, RuleDelete_statement_positioned_delete = 20, 
    RuleDelete_deltalake_statement = 21, RuleInsert_statement = 22, RuleInsert_datalake_statement = 23, 
    RuleValues_item = 24, RuleMerge_statement = 25, RuleTable_view_fullselect = 26, 
    RuleCommon_table_expression_list = 27, RuleMatching_condition = 28, 
    RuleModification_operation = 29, RuleUpdate_operation = 30, RuleDelete_operation = 31, 
    RuleInsert_operation = 32, RuleExpr_null_default_list = 33, RuleIsolation_level = 34, 
    RuleTruncate_statement = 35, RuleUpdate_statement = 36, RuleUpdate_statement_searched_update = 37, 
    RuleSkip_wait = 38, RuleUpdate_statement_positioned_update = 39, RuleInclude_columns = 40, 
    RuleAssignment_clause = 41, RuleAssignment_item = 42, RulePeriod_clause = 43, 
    RuleTime_sec = 44, RuleUpdate_datalake_statement = 45, RuleGrant_database_authorities_statement = 46, 
    RuleDb_privilege_list = 47, RuleDb_privilege = 48, RuleGrantee = 49, 
    RuleGrantee_user_group = 50, RuleUser_group = 51, RuleGrantee_list = 52, 
    RuleGrantee_list_public = 53, RuleGrantee_list_user_group = 54, RuleGrant_exemption_statement = 55, 
    RuleExemption_privilege = 56, RuleGrant_global_variable_privileges_statement = 57, 
    RuleVariable_privilege = 58, RuleRead_write = 59, RuleWith_grant_option = 60, 
    RuleGrant_index_privileges_statement = 61, RuleGrant_module_privileges_statement = 62, 
    RuleGrant_package_privileges_statement = 63, RulePackage_privilege_list = 64, 
    RulePackage_privilege = 65, RuleGrant_role_statement = 66, RuleRole_list = 67, 
    RuleGrant_routine_privileges_statement = 68, RuleGrant_schema_privileges_statement = 69, 
    RuleSchema_privilege_list = 70, RuleSchema_privilege = 71, RuleGrant_security_label_statement = 72, 
    RuleGrant_sequence_privileges_statement = 73, RuleSequence_privilege_list = 74, 
    RuleSequence_privilege = 75, RuleGrant_server_privileges_statement = 76, 
    RuleGrant_setsessionuser_privilege_statement = 77, RuleUser_list = 78, 
    RuleUser_auth = 79, RuleGrant_table_space_privileges_statement = 80, 
    RuleGrant_table_view_or_nickname_privileges_statement = 81, RuleTvn_privilege_list = 82, 
    RuleTvn_privilege = 83, RuleColumn_name_list_paren = 84, RuleColumn_name_list = 85, 
    RuleGrant_workload_privileges_statement = 86, RuleGrant_xsr_object_privileges_statement = 87, 
    RuleRevoke_database_authorities_statement = 88, RuleBy_all = 89, RuleRevoke_exemption_statement = 90, 
    RuleRevoke_global_variable_privileges_statement = 91, RuleRevoke_index_privileges_statement = 92, 
    RuleRevoke_module_privileges_statement = 93, RuleRevoke_package_privileges_statement = 94, 
    RuleRevoke_role_statement = 95, RuleRevoke_routine_privileges_statement = 96, 
    RuleRevoke_schema_privileges_statement = 97, RuleRevoke_security_label_statement = 98, 
    RuleRevoke_sequence_privileges_statement = 99, RuleRevoke_server_privileges_statement = 100, 
    RuleRevoke_setsessionuser_privilege_statement = 101, RuleRevoke_table_space_privileges_statement = 102, 
    RuleRevoke_table_view_or_nickname_privileges_statement = 103, RuleRevoke_workload_privileges_statement = 104, 
    RuleRevoke_xsr_object_privileges_statement = 105, RuleUser_group_role = 106, 
    RuleRollback_statement = 107, RuleSavepoint_statement = 108, RuleRelease_savepoint_statement = 109, 
    RuleAllocate_cursor_statement = 110, RuleAlter_audit_policy_statement = 111, 
    RuleStatus_spec = 112, RuleNormal_audit = 113, RuleAlter_bufferpool_statement = 114, 
    RuleImmediate_deferred = 115, RuleAlter_database_partition_group_statement = 116, 
    RuleDb_partition_group_list_item = 117, RuleDb_partition_num_nums = 118, 
    RuleDb_partitions_clause = 119, RuleDb_partition_options = 120, RuleAlter_database_statement = 121, 
    RuleAlter_database_opts = 122, RuleAlter_event_monitor_statement = 123, 
    RuleAlter_event_monitor_opts = 124, RuleAlter_function_statement = 125, 
    RuleAlter_function_opts = 126, RuleFunction_designator = 127, RuleData_type_list = 128, 
    RuleData_type_list_paren = 129, RuleAlter_histogram_template_statement = 130, 
    RuleAlter_index_statement = 131, RuleYes_no = 132, RuleAlter_mask_statement = 133, 
    RuleEnable_disable = 134, RuleAlter_method_statement = 135, RuleMethod_designator = 136, 
    RuleAlter_model_statement = 137, RuleAlter_module_statement = 138, RuleAlter_module_opts = 139, 
    RuleModule_function_definition = 140, RuleModule_procedure_definition = 141, 
    RuleModule_type_definition = 142, RuleModule_variable_definition = 143, 
    RuleModule_condition_definition = 144, RuleModule_object_identification = 145, 
    RuleModule_function_designator = 146, RuleModule_procedure_designator = 147, 
    RuleAlter_nickname_statement = 148, RuleAlter_nickname_opts_1 = 149, 
    RuleAlter_nickname_opts_1_item = 150, RuleAlter_nickname_opts_2 = 151, 
    RuleAlter_nickname_opts_2_item = 152, RuleConstraint_alteration = 153, 
    RuleAlter_package_statement = 154, RuleAlter_package_opts = 155, RuleAlter_permission_statement = 156, 
    RuleAlter_procedure_external_statement = 157, RuleAlter_procedure_external_opts = 158, 
    RuleProcedure_designator = 159, RuleAlter_procedure_sourced_statement = 160, 
    RuleParameter_alteration = 161, RuleAlter_procedure_sql_statement = 162, 
    RuleAlter_schema_statement = 163, RuleNone_changes = 164, RuleAlter_security_label_component_statement = 165, 
    RuleAdd_element_clause = 166, RuleArray_element_clause = 167, RuleTree_element_clause = 168, 
    RuleAlter_security_policy_statement = 169, RuleAlter_security_policy_opts = 170, 
    RuleAlter_sequence_statement = 171, RuleAlter_sequence_opts = 172, RuleAlter_server_statement = 173, 
    RuleAlter_server_opts = 174, RuleAlter_service_class_statement = 175, 
    RuleAlter_service_class_opts = 176, RuleDefault_on_off = 177, RuleDefault_high_medium_low = 178, 
    RuleAlter_stogroup_statement = 179, RuleAlter_stogroup_opts = 180, RuleAlter_table_statement = 181, 
    RuleAlter_table_opts = 182, RuleNull_on_off = 183, RuleCascade_restrict = 184, 
    RuleMaterialized_query_definition = 185, RuleRefreshable_table_options = 186, 
    RuleColumn_alteration = 187, RuleGeneration_alteration = 188, RuleIdentity_alteration = 189, 
    RuleGeneration_attribute = 190, RuleAs_identity_clause = 191, RuleAs_identity_clause_opts = 192, 
    RulePeriod_definition_alter = 193, RuleAdd_partition = 194, RuleBoundary_spec_alter = 195, 
    RuleAttach_partition = 196, RuleActivate_deactivate = 197, RuleAlter_tablespace_statement = 198, 
    RuleAlter_tablespace_opts = 199, RuleAdd_clause = 200, RuleDb_container_clause = 201, 
    RuleDb_container_clause_opts = 202, RuleDrop_container_clause = 203, 
    RuleFile_device = 204, RuleAll_containers_clause = 205, RuleSystem_container_clause = 206, 
    RuleStripeset = 207, RuleKm = 208, RuleKmg_percent = 209, RuleAlter_threshold_statement = 210, 
    RuleAlter_threshold_opts = 211, RuleAlter_threshold_predicate = 212, 
    RuleAlter_threshold_exceeded_actions = 213, RuleDt_units = 214, RuleDt_units_with_seconds = 215, 
    RuleAlter_trigger_statement = 216, RuleAlter_trusted_context_statement = 217, 
    RuleAlter_trusted_context_opts = 218, RuleAlter_trusted_context_opts_alter_opts = 219, 
    RuleAddr_clause_encryption_val = 220, RuleAddress_clause = 221, RuleUser_clause = 222, 
    RuleUse_for_opts = 223, RuleUse_for_opts_2 = 224, RuleAlter_type_statement = 225, 
    RuleAlter_type_opts = 226, RuleMethod_identifier = 227, RuleMethod_options = 228, 
    RuleAlter_usage_list_statement = 229, RuleAlter_usage_list_opts_item = 230, 
    RuleAlter_user_mapping_statement = 231, RuleAlter_user_mapping_opts_item = 232, 
    RuleAdd_set = 233, RuleAlter_view_statement = 234, RuleAlter_view_opts = 235, 
    RuleAlter_work_action_set_statement = 236, RuleAlter_work_action_set_opts = 237, 
    RuleWork_action_alteration = 238, RuleWork_action_alteration_opts = 239, 
    RuleAlter_action_types_clause = 240, RuleThreshold_predicate_clause = 241, 
    RuleAlter_work_class_set_statement = 242, RuleAlter_work_class_set_opts = 243, 
    RuleWork_class_alteration = 244, RuleWork_class_alteration_opts = 245, 
    RuleFor_from_to_alter_clause = 246, RuleSchema_alter_clause = 247, RuleData_tag_alter_clause = 248, 
    RuleAlter_workload_statement = 249, RuleAlter_workload_opts_item = 250, 
    RulePackage_executable = 251, RuleBase_none = 252, RuleExtended_base_none = 253, 
    RuleAlter_collect_activity_data_clause = 254, RuleWith_opts = 255, RuleAlter_collect_history_clause = 256, 
    RuleAlter_collect_lock_wait_data_clause = 257, RuleAlter_wrapper_statement = 258, 
    RuleAlter_wrapper_opts_item = 259, RuleAlter_xsrobject_statement = 260, 
    RuleString = 261, RuleString_constant = 262, RuleNumeric_constant = 263, 
    RuleData_type = 264, RuleAnchored_data_type = 265, RuleAnchored_non_row_data_type = 266, 
    RuleAnchored_row_data_type = 267, RuleSource_data_type = 268, RuleData_type_constrainst = 269, 
    RuleCheck_condition = 270, RuleData_type_2 = 271, RuleBuilt_in_type = 272, 
    RuleInferred_builtin_type = 273, RuleDesignator_data_type = 274, RuleDesignator_data_type_list = 275, 
    RuleDesignator_data_type_list_paren = 276, RuleInteger_paren = 277, 
    RuleInteger_kmg_paren = 278, RuleChar_character = 279, RuleOctets_codeunits = 280, 
    RuleCodeunits = 281, RuleKmg = 282, RuleRs_locator_variable = 283, RuleInteger_constant_list = 284, 
    RuleInteger_constant = 285, RuleInteger_value = 286, RulePositive_integer = 287, 
    RuleBigint_value = 288, RuleBigint_constant = 289, RuleMember_number = 290, 
    RuleVersion_id = 291, RuleDrop_statement = 292, RuleAlias_designator = 293, 
    RuleService_class_designator = 294, RuleTablespace_name_list = 295, 
    RuleAssociate_locators_statement = 296, RuleAudit_statement = 297, RuleBegin_declare_section_statement = 298, 
    RuleCall_statement = 299, RuleArg_list_paren = 300, RuleArg_list = 301, 
    RuleArgument = 302, RuleCase_statement = 303, RuleSearched_case_statement_when_clause = 304, 
    RuleSimple_case_statement_when_clause = 305, RuleClose_statement = 306, 
    RuleComment_statement = 307, RuleColumn_comment = 308, RuleComment_objects = 309, 
    RuleCommit_statement = 310, RuleConnect_type_1_statement = 311, RuleAuthorization = 312, 
    RulePasswords = 313, RuleLock_block = 314, RuleAccesstoken = 315, RuleToken = 316, 
    RuleApi_key = 317, RuleToken_type = 318, RuleDeclare_cursor_statement = 319, 
    RuleDeclare_global_temporary_table_statement = 320, RuleDescribe_statement = 321, 
    RuleXquery_statement = 322, RuleDescribe_input_statement = 323, RuleDescribe_output_statement = 324, 
    RuleDisconnect_statement = 325, RuleEnd_declare_section_statement = 326, 
    RuleExecute_statement = 327, RuleHost_variable_expression = 328, RuleAssignment_target = 329, 
    RuleExecute_immediate_statement = 330, RuleExplain_statement = 331, 
    RuleExplainable_sql_statement = 332, RuleFetch_statement = 333, RuleFlush_bufferpools_statement = 334, 
    RuleFlush_event_monitor_statement = 335, RuleFlush_federated_cache_statement = 336, 
    RuleFlush_optimization_profile_cache_statement = 337, RuleFlush_package_cache_statement = 338, 
    RuleFlush_authentication_cache_statement = 339, RuleFree_locator_statement = 340, 
    RuleGet_diagnostics_statement = 341, RuleStatement_information = 342, 
    RuleCondition_information = 343, RuleCondition_var_assignment = 344, 
    RuleLock_table_statement = 345, RulePipe_statement = 346, RuleRefresh_table_statement = 347, 
    RuleRelease_connection_statement = 348, RuleRename_statement = 349, 
    RuleRename_stogroup_statement = 350, RuleRename_tablespace_statement = 351, 
    RuleSet_statement = 352, RuleAccess_mode_clause = 353, RuleCascade_clause = 354, 
    RuleTo_descendent_types = 355, RuleTable_type_list = 356, RuleTable_type = 357, 
    RuleTable_checked_options_list = 358, RuleTable_checked_options = 359, 
    RuleOnline_options = 360, RuleQuery_optimization_options = 361, RuleCheck_options = 362, 
    RuleIncremental_options = 363, RuleException_clause = 364, RuleIn_table_use_clause = 365, 
    RuleTable_unchecked_options = 366, RuleFull_access = 367, RuleIntegrity_options = 368, 
    RuleIntegrity_options_item = 369, RuleVar_def_list = 370, RuleVar_def = 371, 
    RuleExpr_null = 372, RuleExpr_null_default = 373, RuleArray_index = 374, 
    RuleRow_fullselect = 375, RuleTarget_variable = 376, RuleTarget_cursor_variable = 377, 
    RuleTarget_row_variable = 378, RuleRow_array_element_specification = 379, 
    RuleRow_field_reference = 380, RuleField_reference = 381, RuleSearch_condition = 382, 
    RulePredicate = 383, RuleAccording_to_clause = 384, RuleXml_schema_identification_list = 385, 
    RuleXml_schema_identification = 386, RuleFullselect_in_parentheses = 387, 
    RuleSome_any_all = 388, RuleRow_value_expression = 389, RuleComparison_operator = 390, 
    RuleRow_expression = 391, RulePath_opt_list = 392, RulePath_opt = 393, 
    RulePkg_opt_list = 394, RulePkg_opt = 395, RuleMaintain_opt_list = 396, 
    RuleMaintain_opt = 397, RuleVariable = 398, RuleHost_variable = 399, 
    RuleSet_integrity_statement = 400, RuleTransfer_ownership_statement = 401, 
    RuleObjects = 402, RuleWhenever_statement = 403, RuleFor_statement = 404, 
    RuleGoto_statement = 405, RuleIf_statement = 406, RuleInclude_statement = 407, 
    RuleResignal_statement = 408, RuleSignal_information = 409, RuleDiagnostic_string_constant = 410, 
    RuleSignal_statement = 411, RuleSqlstate_string_constant = 412, RuleSqlstate_string_variable = 413, 
    RuleSignal_information_2 = 414, RuleDiagnostic_string_expression = 415, 
    RuleIterate_statement = 416, RuleLeave_statement = 417, RuleLoop_statement = 418, 
    RuleOpen_statement = 419, RuleVariable_or_expression = 420, RuleSelect_into_statement = 421, 
    RuleValues_into_statement = 422, RulePrepare_statement = 423, RuleRepeat_statement = 424, 
    RuleReturn_statement = 425, RuleWhile_statement = 426, RuleSql_routine_statement = 427, 
    RuleCommon_table_expression = 428, RuleCreate_alias_statement = 429, 
    RuleDeclared_alias_name = 430, RuleTable_alias = 431, RuleModule_alias = 432, 
    RuleSequence_alias = 433, RuleOr_replace = 434, RuleCreate_audit_policy_statement = 435, 
    RuleAudit_policy_opts = 436, RuleAudit_policy_categories_opts = 437, 
    RuleCreate_bufferpool_statement = 438, RuleBufferpool_opts = 439, RuleExcept_clause = 440, 
    RuleMember_list = 441, RuleMember_list_item = 442, RuleCreate_database_partition_group_statement = 443, 
    RuleCreate_event_monitor_statement = 444, RuleCreate_event_monitor_activities_statement = 445, 
    RuleFormatted_event_table_info_3 = 446, RuleCreate_event_monitor_change_history_statement = 447, 
    RuleEvent_control_list = 448, RuleEvent_control = 449, RuleCreate_event_monitor_locking_statement = 450, 
    RuleCreate_event_monitor_package_cache_statement = 451, RuleFilter_and_collection_options = 452, 
    RuleEvent_condition = 453, RuleEvent_condition_item = 454, RuleCreate_event_monitor_statistics_statement = 455, 
    RuleEvent_monitor_statistics_opts = 456, RuleCreate_event_monitor_threshold_violations_statement = 457, 
    RuleFormatted_event_table_info_2 = 458, RuleFile_options = 459, RuleEvent_monitor_threshold_opts = 460, 
    RulePages = 461, RuleCreate_event_monitor_unit_of_work = 462, RuleFormatted_event_table_info = 463, 
    RuleAutostart_manualstart = 464, RuleEvm_group = 465, RuleTarget_table_options = 466, 
    RuleCreate_external_table_statement = 467, RuleExt_table_option = 468, 
    RuleExt_table_option_value = 469, RuleCreate_function_statement = 470, 
    RuleCreate_function_aggregate_interface_statement = 471, RuleAgg_fn_param_decl = 472, 
    RuleAggregate_return_clause = 473, RuleAggregate_implementation_clause = 474, 
    RuleAgg_fn_option_list = 475, RuleState_variable_declaration = 476, 
    RuleAggregate_cast_target_type = 477, RuleAggregate_builtin_type = 478, 
    RuleCreate_function_external_scalar_statement = 479, RuleExternal_scalar_ordinary_return_type = 480, 
    RuleExternal_scalar_cast_source_type = 481, RuleExt_scalar_param_decl = 482, 
    RuleExt_scalar_option_list = 483, RuleExt_scalar_option_list_item = 484, 
    RulePredicate_specification = 485, RuleData_filter = 486, RuleIndex_exploitation = 487, 
    RuleExploitation_rule = 488, RuleCreate_function_external_table_statement = 489, 
    RuleExt_table_param_decl_list = 490, RuleExt_table_param_decl = 491, 
    RuleExternal_table_return_type = 492, RuleExt_table_option_list = 493, 
    RuleExt_table_option_list_item = 494, RuleCreate_function_old_db_external_function_statement = 495, 
    RuleOledb_return_type = 496, RuleOledb_option_list = 497, RuleOledb_option_list_item = 498, 
    RuleCreate_function_sourced_or_template_statement = 499, RuleSourced_function_param_decl_list = 500, 
    RuleSourced_function_param_decl = 501, RuleSourced_function_parameter_type = 502, 
    RuleSourced_function_return_type = 503, RuleTemplate_function_return_type = 504, 
    RuleFn_return_opts = 505, RuleFn_return_opts_item = 506, RuleTemplate_opts = 507, 
    RuleTemplate_opts_item = 508, RuleAscii_unicode = 509, RuleParam_decl_list_3 = 510, 
    RuleParam_decl_3 = 511, RuleCreate_function_sql_scalar_table_or_row_statement = 512, 
    RuleSql_function_return_builtin_type = 513, RuleSql_function_return_named_type = 514, 
    RuleSql_function_return_data_type = 515, RuleSql_function_return_column = 516, 
    RuleSql_function_return_table_columns = 517, RuleSql_function_return_row_columns = 518, 
    RuleParam_decl_list_2 = 519, RuleParam_decl_2 = 520, RuleSql_function_body = 521, 
    RuleCreate_function_mapping_statement = 522, RuleFunction_options = 523, 
    RuleFunction_option_name = 524, RuleCreate_global_temporary_table_statement = 525, 
    RuleCreate_global_temporary_table_opts = 526, RuleCreate_global_temporary_table_item = 527, 
    RuleDelete_preserve = 528, RuleCreate_histogram_template_statement = 529, 
    RuleCreate_index_statement = 530, RuleIndex_cluster_clause = 531, RuleIndex_col_opts = 532, 
    RuleIndex_col_opts_item = 533, RuleKey_expression = 534, RuleCreate_index_extension_statement = 535, 
    RuleParam_list = 536, RuleIndex_maintenance = 537, RuleTable_function_invocation = 538, 
    RuleIndex_search = 539, RuleSearch_method_definition = 540, RuleCreate_mask_statement = 541, 
    RuleCase_expression = 542, RuleRange_producing_funciton_invocation = 543, 
    RuleIndex_filtering_function_invocation = 544, RuleCreate_method_statement = 545, 
    RuleMethod_opts = 546, RuleMethod_opts_item = 547, RuleMethod_signature = 548, 
    RuleMethod_ordinary_return_type = 549, RuleMethod_reference_type = 550, 
    RuleMethod_cast_non_xml_type = 551, RuleMethod_cast_non_xml_builtin_type = 552, 
    RuleMethod_cast_source_type = 553, RuleMethod_cast_target_type = 554, 
    RuleMethod_param_list = 555, RuleMethod_signature_parameter_decl = 556, 
    RuleMethod_parameter_type = 557, RuleMethod_builtin_type = 558, RuleMethod_non_xml_builtin_type = 559, 
    RuleMethod_named_type = 560, RuleMethod_integer_attribute = 561, RuleData_type_3 = 562, 
    RuleSql_method_body = 563, RuleCompound_sql_inlined = 564, RuleDeclare_variable_statement = 565, 
    RuleAssignment_statement = 566, RuleDeclare_variable_default_clause = 567, 
    RuleCompound_body_statement = 568, RuleSql_statement_inlined = 569, 
    RuleCompound_sql_compiled = 570, RuleSql_statement_compiled = 571, RuleCreate_module_statement = 572, 
    RuleCreate_nickname_statement = 573, RuleNick_name_option_name = 574, 
    RuleRemote_object_name = 575, RuleNon_relational_data_definition = 576, 
    RuleNick_name_column_list = 577, RuleNick_name_column_list_item = 578, 
    RuleNick_name_column_definition = 579, RuleNick_name_column_options = 580, 
    RuleFederated_column_options = 581, RuleColumn_option_name = 582, RuleCreate_permission_statement = 583, 
    RuleCreate_procedure_statement = 584, RuleCreate_procedure_external_statement = 585, 
    RuleProc_ext_param_list = 586, RuleProc_ext_param = 587, RuleProcedure_external_xml_type = 588, 
    RuleOption_list_2 = 589, RuleOption_list_2_item = 590, RuleCreate_procedure_sourced_statement = 591, 
    RuleSource_procedure_clause = 592, RuleSource_object_name = 593, RuleOption_list_1 = 594, 
    RuleOption_list_1_item = 595, RuleResult_set_element_number = 596, RuleUnique_id = 597, 
    RuleCreate_procedure_sql_statement = 598, RuleProc_parameter_list = 599, 
    RuleProc_parameter_list_item = 600, RuleIn_out_inout = 601, RuleOption_list = 602, 
    RuleOption_list_item = 603, RuleSql_procedure_body = 604, RuleCreate_role_statement = 605, 
    RuleCreate_schema_statement = 606, RuleSchema_sql_statement = 607, RuleCreate_security_label_component_statement = 608, 
    RuleArray_clause = 609, RuleSet_clause = 610, RuleTree_clause = 611, 
    RuleTree_clause_item = 612, RuleCreate_security_label_statement = 613, 
    RuleCreate_security_label_item = 614, RuleCreate_security_policy_statement = 615, 
    RuleCreate_sequence_statement = 616, RuleCreate_sequence_opts = 617, 
    RuleCreate_sequence_opts_item = 618, RuleCreate_service_class_statement = 619, 
    RuleHigh_medium_low = 620, RuleOn_off = 621, RuleSoft_hard = 622, RuleCreate_server_statement = 623, 
    RulePassword_ = 624, RuleCreate_stogroup_statement = 625, RuleCreate_stogroup_opts = 626, 
    RuleCreate_synonym_statement = 627, RuleCreate_table_statement = 628, 
    RuleCreate_table_opts = 629, RuleTable_option_list = 630, RuleTable_option_list_item = 631, 
    RuleTable_option_name = 632, RuleElement_list = 633, RuleElement_list_item = 634, 
    RuleColumn_definition = 635, RulePeriod_definition = 636, RuleUnique_constraint = 637, 
    RuleReferential_constraint = 638, RuleCheck_constraint = 639, RuleColumn_options = 640, 
    RuleColumn_options_item = 641, RuleReferences_clause = 642, RuleRule_clause = 643, 
    RuleConstraint_attributes = 644, RuleRoutine_default_clause = 645, RuleRoutine_default_value = 646, 
    RuleRoutine_default_special_register = 647, RuleRoutine_default_timestamp_register = 648, 
    RuleRoutine_default_integer_attribute = 649, RuleDefault_clause = 650, 
    RuleDefault_values = 651, RuleGenerated_clause = 652, RuleDatetime_special_register = 653, 
    RuleUser_special_register = 654, RuleCast_function = 655, RuleIdentity_options = 656, 
    RuleIdentity_options_item = 657, RuleAs_row_change_timestamp_clause = 658, 
    RuleAs_generated_expression_clause = 659, RuleGeneration_expression = 660, 
    RuleAs_row_transaction_timestamp_clause = 661, RuleAs_row_transaction_start_id_clause = 662, 
    RuleOid_column_definition = 663, RuleRange_partition_spec = 664, RulePartition_expression_list = 665, 
    RulePartition_expression = 666, RulePartition_element_list = 667, RulePartition_element = 668, 
    RuleBoundary_spec = 669, RulePartition_tablespace_options = 670, RuleDuration_label = 671, 
    RuleStarting_clause = 672, RuleConst_min_max_list = 673, RuleConst_min_max = 674, 
    RuleEnding_clause = 675, RuleTyped_table_options = 676, RuleTyped_element_list = 677, 
    RuleTyped_element_list_item = 678, RuleAs_result_table = 679, RuleCopy_options = 680, 
    RuleMaterialized_query_options = 681, RuleStaging_table_definition = 682, 
    RuleDimensions_clause = 683, RuleCol_names = 684, RuleSequence_key_spec = 685, 
    RuleSequence_key_spec_list = 686, RuleSequence_key_spec_list_item = 687, 
    RuleTablespace_clauses = 688, RuleDistribution_clause = 689, RulePartitioning_clause = 690, 
    RuleIf_not_exists = 691, RuleCreate_tablespace_statement = 692, RuleStorage_group = 693, 
    RuleSize_attributes = 694, RuleSystem_containers = 695, RuleContainer_string_list = 696, 
    RuleDatabase_containers = 697, RuleContainer_clause = 698, RuleContainer_clause_list = 699, 
    RuleContainer_clause_list_item = 700, RuleOn_db_partitions_clause = 701, 
    RuleDb_partition_number_list = 702, RuleDb_partition_number_list_item = 703, 
    RuleDb_partition_number = 704, RuleNumber_of_pages = 705, RuleNumber_of_files = 706, 
    RuleNumber_of_milliseconds = 707, RuleNumber_megabytes_per_second = 708, 
    RuleCreate_threshold_statement = 709, RuleThreshold_domain = 710, RuleStatement_text = 711, 
    RuleExecutable_id = 712, RuleEnforcement_scope = 713, RuleThreshold_predicate = 714, 
    RuleChecking_every = 715, RuleHour_to_seconds = 716, RuleDay_to_minutes = 717, 
    RuleDay_to_seconds = 718, RuleThreshold_exceeded_actions_2 = 719, RuleDetails_section = 720, 
    RuleRemap_activity_action = 721, RuleCreate_transform_statement = 722, 
    RuleTranform_list = 723, RuleTranform_list_item = 724, RuleTransform_group_list = 725, 
    RuleTransform_group_list_item = 726, RuleCreate_trigger_statement = 727, 
    RuleRef_list = 728, RuleRef_list_item = 729, RuleOld_new = 730, RuleCorrelation_name = 731, 
    RuleIdentifier = 732, RuleTrigger_event = 733, RuleTriggered_action = 734, 
    RuleSql_procedure_statement = 735, RuleSql_function_statement = 736, 
    RuleCreate_trusted_context_statement = 737, RuleAttr_list = 738, RuleAttr_list_item = 739, 
    RuleAuth_list = 740, RuleAuth_list_item = 741, RuleAddress_value = 742, 
    RuleEncryption_value = 743, RuleCreate_type_statement = 744, RuleCreate_type_array_statement = 745, 
    RuleCreate_type_cursor_statement = 746, RuleCreate_type_distinct_statement = 747, 
    RuleCreate_type_row_statement = 748, RuleField_definition_list_paren = 749, 
    RuleField_definition_list = 750, RuleField_definition = 751, RuleCreate_type_structured_statement = 752, 
    RuleStructured_type_seq = 753, RuleAttribute_definition_list_paren = 754, 
    RuleAttribute_definition_list = 755, RuleAttribute_definition = 756, 
    RuleMethod_specification_list = 757, RuleMethod_specification = 758, 
    RuleMethod_specification_return_clause = 759, RuleMethod_specification_seq = 760, 
    RuleAs_locator = 761, RuleMethod_spec_parameter_list_paren = 762, RuleMethod_spec_parameter_list = 763, 
    RuleMethod_spec_parameter_decl = 764, RuleMethod_spec_parameter_type = 765, 
    RuleParam_decl_list_paren = 766, RuleParam_decl_list = 767, RuleParam_decl = 768, 
    RuleSql_routine_characteristics = 769, RuleExternal_routine_characteristics = 770, 
    RuleLength = 771, RuleRep_type = 772, RuleVarchars = 773, RuleVarbinaries = 774, 
    RuleFor_bit_data = 775, RuleLob_options = 776, RuleCreate_type_mapping_statement = 777, 
    RuleFor_bit_data_precision = 778, RulePrecision = 779, RuleScale = 780, 
    RulePrecision_scale_comp = 781, RuleFrom_to = 782, RuleData_source_data_type = 783, 
    RuleLocal_data_type = 784, RuleRemote_server = 785, RuleServer_version = 786, 
    RuleServer_type = 787, RuleVersion = 788, RuleRelease = 789, RuleMod = 790, 
    RuleCreate_usage_list_statement = 791, RuleCreate_user_mapping_statement = 792, 
    RuleUser_mapping_options_paren = 793, RuleUser_mapping_options = 794, 
    RuleCreate_variable_statement = 795, RuleConstant_ = 796, RuleSigned_numeric_constant = 797, 
    RuleDecfloat_special_constant = 798, RuleSigned_decfloat_special_constant = 799, 
    RuleBoolean_constant = 800, RuleTyped_datetime_constant = 801, RuleUnicode_string_constant = 802, 
    RuleUnicode_escape_clause = 803, RulePrefixed_string_constant = 804, 
    RuleHex_character_string_constant = 805, RuleGraphic_string_constant = 806, 
    RuleHex_graphic_string_constant = 807, RuleBinary_string_constant = 808, 
    RuleUtf16be_graphic_string_constant = 809, RuleSpecial_register = 810, 
    RuleNative_special_register = 811, RuleTimestamp_register_invocation = 812, 
    RuleSchema_qualified_variable_declaration_name = 813, RuleGlobal_variable = 814, 
    RuleQualified_global_variable_reference = 815, RuleData_type_1 = 816, 
    RuleCursor_value_constructor = 817, RuleAnchored_variable_data_type = 818, 
    RuleHoldability = 819, RuleReturnability = 820, RuleCreate_view_statement = 821, 
    RuleCreate_view_seq = 822, RuleFullselect = 823, RuleSubselect = 824, 
    RuleSelect_clause = 825, RuleSelect_clause_item = 826, RuleFrom_clause = 827, 
    RuleTable_reference = 828, RuleTable_reference_list = 829, RuleSingles_table_reference = 830, 
    RuleTable_reference_name = 831, RuleTable_reference_schema = 832, RulePeriod_specification = 833, 
    RuleValue = 834, RuleCorrelation_clause = 835, RuleTablesample_clause = 836, 
    RuleNumeric_expression = 837, RuleSingle_view_reference = 838, RuleSingle_nickname_reference = 839, 
    RuleOnly_table_reference = 840, RuleOuter_table_reference = 841, RuleAnalyze_table_reference = 842, 
    RuleImplementation_clause = 843, RuleNested_table_reference = 844, RuleContinue_handler = 845, 
    RuleSpecific_condition_value = 846, RuleData_change_table_reference = 847, 
    RuleSearched_update_statement = 848, RuleSearched_delete_statement = 849, 
    RuleFinal_new = 850, RuleFinal_new_old = 851, RuleTable_function_reference = 852, 
    RuleTable_udf_cardinality_clause = 853, RuleTyped_correlation_clause = 854, 
    RuleColumn_name_data_type = 855, RuleCollection_derived_table = 856, 
    RuleTable_function = 857, RuleXmltable_expression = 858, RuleXmltable_function = 859, 
    RuleJoined_table = 860, RuleJoin_condition = 861, RuleOuter = 862, RuleExternal_table_reference = 863, 
    RuleColumn_definition_2 = 864, RuleFile_name = 865, RuleWhere_clause = 866, 
    RuleGroup_by_clause = 867, RuleGroup_by_clause_opts = 868, RuleGrouping_expression = 869, 
    RuleGrouping_sets = 870, RuleSuper_groups = 871, RuleGrant_total = 872, 
    RuleHaving_clause = 873, RuleOrder_by_clause = 874, RuleOrder_by_clause_opts = 875, 
    RuleTable_designator = 876, RuleAsc_desc = 877, RuleFirst_last = 878, 
    RuleSort_key = 879, RuleSimple_column_name = 880, RuleSimple_integer = 881, 
    RuleSork_key_expression = 882, RuleOffset_clause = 883, RuleOffset_row_count = 884, 
    RuleFetch_clause = 885, RuleFetch_row_count = 886, RuleRow_rows = 887, 
    RuleIsolation_clause = 888, RuleLock_request_clause = 889, RuleValues_clause = 890, 
    RuleValues_row = 891, RuleRoot_view_definition = 892, RuleSubview_definition = 893, 
    RuleOid_column = 894, RuleWith_options = 895, RuleWith_option_def = 896, 
    RuleWith_option_scope_def = 897, RuleUnder_clause = 898, RuleCreate_work_action_set_statement = 899, 
    RuleWork_action_definition_list_paren = 900, RuleWork_action_definition_list = 901, 
    RuleWork_action_definition = 902, RuleAction_types_clause = 903, RuleThreshold_types_clause = 904, 
    RuleSecond_seconds = 905, RuleHours_minutes = 906, RuleThreshold_exceeded_actions = 907, 
    RuleCollect_activity_data_clause = 908, RuleWith_without = 909, RuleHistogram_templace_clause = 910, 
    RuleCreate_work_class_set_statement = 911, RuleWork_class_definition_list_paren = 912, 
    RuleWork_class_definition_list = 913, RuleWork_class_definition = 914, 
    RuleWork_attributes = 915, RulePosition_clause = 916, RulePosition_ = 917, 
    RuleFor_from_to_clause = 918, RuleFrom_value = 919, RuleTo_value = 920, 
    RuleData_tag_clause = 921, RuleSchema_clause = 922, RuleCreate_workload_statement = 923, 
    RulePkg_exec_seq = 924, RulePosition_clause_2 = 925, RuleConnection_attributes = 926, 
    RuleString_list = 927, RuleString_list_paren = 928, RuleWorkload_attributes = 929, 
    RuleDegree = 930, RuleAllow_disallow = 931, RuleCollect_on_clause = 932, 
    RuleCollect_details_clause = 933, RuleCollect_lock_wait_options = 934, 
    RuleWait_time = 935, RuleCreate_wrapper_statement = 936, RuleWrapper_option_list = 937, 
    RuleWrapper_option = 938, RuleExpression = 939, RuleFunction_invocation = 940, 
    RuleAll_distinct = 941, RuleScalar_fullselect = 942, RuleCast_specification = 943, 
    RuleCursor_cast_specification = 944, RuleRow_cast_specification = 945, 
    RuleInterval_cast_specification = 946, RuleXmlcast_specification = 947, 
    RuleArray_element_specification = 948, RuleArray_constructor = 949, 
    RuleMethod_invocation = 950, RuleOlap_specification = 951, RuleOrdered_olap_specification = 952, 
    RuleWindow_partition_clause = 953, RuleWindow_order_clause = 954, RuleNumbering_specification = 955, 
    RuleAggregation_specification = 956, RuleOlap_aggregate_function = 957, 
    RuleFirst_value_function = 958, RuleLast_value_function = 959, RuleNth_value_function = 960, 
    RuleRatio_to_report_function = 961, RuleIgnore_respect_nulls = 962, 
    RuleFrom_first_last = 963, RuleWindow_aggregation_group_clause = 964, 
    RuleGroup_start = 965, RuleGroup_between = 966, RuleGroup_bound1 = 967, 
    RuleGroup_bound2 = 968, RuleGroup_end = 969, RuleRow_change_expression = 970, 
    RuleSequence_reference = 971, RuleSubtype_treatment = 972, RuleExpression_list = 973, 
    RuleExpression_list_in_parentheses = 974, RuleId_ = 975, RuleExposed_name = 976, 
    RuleName = 977, RuleLabel = 978, RuleHost_label = 979, RuleLibrary_name = 980, 
    RuleArray_type_name = 981, RuleAttribute_name = 982, RuleRow_type_name = 983, 
    RuleAuthorization_name = 984, RuleBoolean_variable_name = 985, RuleArray_variable_name = 986, 
    RuleColumn_name = 987, RuleConstraint_name = 988, RuleDescriptor_name = 989, 
    RuleDistinct_type_name = 990, RuleCursor_name = 991, RuleCursor_type_name = 992, 
    RuleCondition_name = 993, RuleData_source_name = 994, RuleExpression_name = 995, 
    RuleGroup_name = 996, RulePolicy_name = 997, RuleBufferpool_name = 998, 
    RuleDb_partition_name = 999, RuleDatabase_name = 1000, RuleEvent_monitor_name = 1001, 
    RuleField_name = 1002, RuleFor_loop_name = 1003, RuleFunction_name = 1004, 
    RuleFunction_mapping_name = 1005, RuleGlobal_variable_name = 1006, RuleHierarchy_name = 1007, 
    RuleHost_variable_name = 1008, RuleParameter_marker = 1009, RuleTemplate_name = 1010, 
    RuleIndex_name = 1011, RuleIndex_extension_name = 1012, RuleInput_descriptor_name = 1013, 
    RuleMask_name = 1014, RuleMethod_name = 1015, RuleModel_name = 1016, 
    RuleModule_name = 1017, RuleNew_owner = 1018, RuleNick_name = 1019, 
    RuleObject_name = 1020, RuleOid_column_name = 1021, RuleOptimization_profile_name = 1022, 
    RulePackage_name = 1023, RulePartition_name = 1024, RulePath_name = 1025, 
    RulePermission_name = 1026, RulePipe_name = 1027, RuleProcedure_name = 1028, 
    RuleResult_descriptor_name = 1029, RuleRole_name = 1030, RuleRoot_table_name = 1031, 
    RuleRoot_view_name = 1032, RuleRow_variable_name = 1033, RuleSource_schema_name = 1034, 
    RuleSource_package_name = 1035, RuleSource_procedure_name = 1036, RuleSql_parameter_name = 1037, 
    RuleSql_variable_name = 1038, RuleTransition_variable_name = 1039, RuleSavepoint_name = 1040, 
    RuleSpecific_name = 1041, RuleSchema = 1042, RuleSchema_name = 1043, 
    RuleSearch_method_name = 1044, RuleServer_name = 1045, RuleServer_option_name = 1046, 
    RuleSession_authorization_name = 1047, RuleComponent_name = 1048, RuleSec_label_comp_name = 1049, 
    RuleSecurity_policy_name = 1050, RuleSecurity_label_name = 1051, RuleSequence_name = 1052, 
    RuleService_class_name = 1053, RuleService_superclass_name = 1054, RuleStoragegroup_name = 1055, 
    RuleSupertype_name = 1056, RuleSuperview_name = 1057, RuleService_subclass_name = 1058, 
    RuleStatement_name = 1059, RuleTable_name = 1060, RuleTablespace_name = 1061, 
    RuleTarget_identifier = 1062, RuleThreshold_name = 1063, RuleTrigger_name = 1064, 
    RuleContext_name = 1065, RuleUsage_list_name = 1066, RuleUnqualified_type_name = 1067, 
    RuleType_name = 1068, RuleType_mapping_name = 1069, RuleTyped_table_name = 1070, 
    RuleTyped_view_name = 1071, RuleUser_mapping_option_name = 1072, RuleView_name = 1073, 
    RuleVariable_name = 1074, RuleWork_action_set_name = 1075, RuleWork_class_set_name = 1076, 
    RuleWorkload_name = 1077, RuleWork_action_name = 1078, RuleWork_class_name = 1079, 
    RuleWrapper_name = 1080, RuleWrapper_option_name = 1081, RuleXsrobject_name = 1082, 
    RuleParameter_name = 1083, RuleCursor_variable_name = 1084, RuleAlias_name = 1085, 
    RuleDb_partition_group_name = 1086, RuleSource_index_name = 1087, RuleSource_table_name = 1088, 
    RuleSource_storagegroup_name = 1089, RuleTarget_storagegroup_name = 1090, 
    RuleSource_tablespace_name = 1091, RuleTarget_tablespace_name = 1092, 
    RuleUnqualified_function_name = 1093, RuleUnqualified_procedure_name = 1094, 
    RuleUnqualified_specific_name = 1095, RulePeriod_name = 1096, RuleHistory_table_name = 1097, 
    RuleXml_schema_name = 1098, RuleTodo = 1099
  };

  explicit Db2Parser(antlr4::TokenStream *input);

  Db2Parser(antlr4::TokenStream *input, const antlr4::atn::ParserATNSimulatorOptions &options);

  ~Db2Parser() override;

  std::string getGrammarFileName() const override;

  const antlr4::atn::ATN& getATN() const override;

  const std::vector<std::string>& getRuleNames() const override;

  const antlr4::dfa::Vocabulary& getVocabulary() const override;

  antlr4::atn::SerializedATNView getSerializedATN() const override;


  class Db2_fileContext;
  class BatchContext;
  class Sql_statementContext;
  class Sql_schema_statementContext;
  class Sql_data_change_statementContext;
  class Sql_data_statementContext;
  class Sql_transaction_statementContext;
  class Sql_connection_statementContext;
  class Sql_dynamic_statementContext;
  class Sql_session_statementContext;
  class Sql_embedded_host_language_statementContext;
  class Sql_constrol_statementContext;
  class Select_statementContext;
  class Read_only_clauseContext;
  class Update_clauseContext;
  class Optimize_for_clauseContext;
  class Concurrent_access_resolution_clauseContext;
  class Delete_statementContext;
  class Delete_statement_searched_deleteContext;
  class Table_or_view_nameContext;
  class Delete_statement_positioned_deleteContext;
  class Delete_deltalake_statementContext;
  class Insert_statementContext;
  class Insert_datalake_statementContext;
  class Values_itemContext;
  class Merge_statementContext;
  class Table_view_fullselectContext;
  class Common_table_expression_listContext;
  class Matching_conditionContext;
  class Modification_operationContext;
  class Update_operationContext;
  class Delete_operationContext;
  class Insert_operationContext;
  class Expr_null_default_listContext;
  class Isolation_levelContext;
  class Truncate_statementContext;
  class Update_statementContext;
  class Update_statement_searched_updateContext;
  class Skip_waitContext;
  class Update_statement_positioned_updateContext;
  class Include_columnsContext;
  class Assignment_clauseContext;
  class Assignment_itemContext;
  class Period_clauseContext;
  class Time_secContext;
  class Update_datalake_statementContext;
  class Grant_database_authorities_statementContext;
  class Db_privilege_listContext;
  class Db_privilegeContext;
  class GranteeContext;
  class Grantee_user_groupContext;
  class User_groupContext;
  class Grantee_listContext;
  class Grantee_list_publicContext;
  class Grantee_list_user_groupContext;
  class Grant_exemption_statementContext;
  class Exemption_privilegeContext;
  class Grant_global_variable_privileges_statementContext;
  class Variable_privilegeContext;
  class Read_writeContext;
  class With_grant_optionContext;
  class Grant_index_privileges_statementContext;
  class Grant_module_privileges_statementContext;
  class Grant_package_privileges_statementContext;
  class Package_privilege_listContext;
  class Package_privilegeContext;
  class Grant_role_statementContext;
  class Role_listContext;
  class Grant_routine_privileges_statementContext;
  class Grant_schema_privileges_statementContext;
  class Schema_privilege_listContext;
  class Schema_privilegeContext;
  class Grant_security_label_statementContext;
  class Grant_sequence_privileges_statementContext;
  class Sequence_privilege_listContext;
  class Sequence_privilegeContext;
  class Grant_server_privileges_statementContext;
  class Grant_setsessionuser_privilege_statementContext;
  class User_listContext;
  class User_authContext;
  class Grant_table_space_privileges_statementContext;
  class Grant_table_view_or_nickname_privileges_statementContext;
  class Tvn_privilege_listContext;
  class Tvn_privilegeContext;
  class Column_name_list_parenContext;
  class Column_name_listContext;
  class Grant_workload_privileges_statementContext;
  class Grant_xsr_object_privileges_statementContext;
  class Revoke_database_authorities_statementContext;
  class By_allContext;
  class Revoke_exemption_statementContext;
  class Revoke_global_variable_privileges_statementContext;
  class Revoke_index_privileges_statementContext;
  class Revoke_module_privileges_statementContext;
  class Revoke_package_privileges_statementContext;
  class Revoke_role_statementContext;
  class Revoke_routine_privileges_statementContext;
  class Revoke_schema_privileges_statementContext;
  class Revoke_security_label_statementContext;
  class Revoke_sequence_privileges_statementContext;
  class Revoke_server_privileges_statementContext;
  class Revoke_setsessionuser_privilege_statementContext;
  class Revoke_table_space_privileges_statementContext;
  class Revoke_table_view_or_nickname_privileges_statementContext;
  class Revoke_workload_privileges_statementContext;
  class Revoke_xsr_object_privileges_statementContext;
  class User_group_roleContext;
  class Rollback_statementContext;
  class Savepoint_statementContext;
  class Release_savepoint_statementContext;
  class Allocate_cursor_statementContext;
  class Alter_audit_policy_statementContext;
  class Status_specContext;
  class Normal_auditContext;
  class Alter_bufferpool_statementContext;
  class Immediate_deferredContext;
  class Alter_database_partition_group_statementContext;
  class Db_partition_group_list_itemContext;
  class Db_partition_num_numsContext;
  class Db_partitions_clauseContext;
  class Db_partition_optionsContext;
  class Alter_database_statementContext;
  class Alter_database_optsContext;
  class Alter_event_monitor_statementContext;
  class Alter_event_monitor_optsContext;
  class Alter_function_statementContext;
  class Alter_function_optsContext;
  class Function_designatorContext;
  class Data_type_listContext;
  class Data_type_list_parenContext;
  class Alter_histogram_template_statementContext;
  class Alter_index_statementContext;
  class Yes_noContext;
  class Alter_mask_statementContext;
  class Enable_disableContext;
  class Alter_method_statementContext;
  class Method_designatorContext;
  class Alter_model_statementContext;
  class Alter_module_statementContext;
  class Alter_module_optsContext;
  class Module_function_definitionContext;
  class Module_procedure_definitionContext;
  class Module_type_definitionContext;
  class Module_variable_definitionContext;
  class Module_condition_definitionContext;
  class Module_object_identificationContext;
  class Module_function_designatorContext;
  class Module_procedure_designatorContext;
  class Alter_nickname_statementContext;
  class Alter_nickname_opts_1Context;
  class Alter_nickname_opts_1_itemContext;
  class Alter_nickname_opts_2Context;
  class Alter_nickname_opts_2_itemContext;
  class Constraint_alterationContext;
  class Alter_package_statementContext;
  class Alter_package_optsContext;
  class Alter_permission_statementContext;
  class Alter_procedure_external_statementContext;
  class Alter_procedure_external_optsContext;
  class Procedure_designatorContext;
  class Alter_procedure_sourced_statementContext;
  class Parameter_alterationContext;
  class Alter_procedure_sql_statementContext;
  class Alter_schema_statementContext;
  class None_changesContext;
  class Alter_security_label_component_statementContext;
  class Add_element_clauseContext;
  class Array_element_clauseContext;
  class Tree_element_clauseContext;
  class Alter_security_policy_statementContext;
  class Alter_security_policy_optsContext;
  class Alter_sequence_statementContext;
  class Alter_sequence_optsContext;
  class Alter_server_statementContext;
  class Alter_server_optsContext;
  class Alter_service_class_statementContext;
  class Alter_service_class_optsContext;
  class Default_on_offContext;
  class Default_high_medium_lowContext;
  class Alter_stogroup_statementContext;
  class Alter_stogroup_optsContext;
  class Alter_table_statementContext;
  class Alter_table_optsContext;
  class Null_on_offContext;
  class Cascade_restrictContext;
  class Materialized_query_definitionContext;
  class Refreshable_table_optionsContext;
  class Column_alterationContext;
  class Generation_alterationContext;
  class Identity_alterationContext;
  class Generation_attributeContext;
  class As_identity_clauseContext;
  class As_identity_clause_optsContext;
  class Period_definition_alterContext;
  class Add_partitionContext;
  class Boundary_spec_alterContext;
  class Attach_partitionContext;
  class Activate_deactivateContext;
  class Alter_tablespace_statementContext;
  class Alter_tablespace_optsContext;
  class Add_clauseContext;
  class Db_container_clauseContext;
  class Db_container_clause_optsContext;
  class Drop_container_clauseContext;
  class File_deviceContext;
  class All_containers_clauseContext;
  class System_container_clauseContext;
  class StripesetContext;
  class KmContext;
  class Kmg_percentContext;
  class Alter_threshold_statementContext;
  class Alter_threshold_optsContext;
  class Alter_threshold_predicateContext;
  class Alter_threshold_exceeded_actionsContext;
  class Dt_unitsContext;
  class Dt_units_with_secondsContext;
  class Alter_trigger_statementContext;
  class Alter_trusted_context_statementContext;
  class Alter_trusted_context_optsContext;
  class Alter_trusted_context_opts_alter_optsContext;
  class Addr_clause_encryption_valContext;
  class Address_clauseContext;
  class User_clauseContext;
  class Use_for_optsContext;
  class Use_for_opts_2Context;
  class Alter_type_statementContext;
  class Alter_type_optsContext;
  class Method_identifierContext;
  class Method_optionsContext;
  class Alter_usage_list_statementContext;
  class Alter_usage_list_opts_itemContext;
  class Alter_user_mapping_statementContext;
  class Alter_user_mapping_opts_itemContext;
  class Add_setContext;
  class Alter_view_statementContext;
  class Alter_view_optsContext;
  class Alter_work_action_set_statementContext;
  class Alter_work_action_set_optsContext;
  class Work_action_alterationContext;
  class Work_action_alteration_optsContext;
  class Alter_action_types_clauseContext;
  class Threshold_predicate_clauseContext;
  class Alter_work_class_set_statementContext;
  class Alter_work_class_set_optsContext;
  class Work_class_alterationContext;
  class Work_class_alteration_optsContext;
  class For_from_to_alter_clauseContext;
  class Schema_alter_clauseContext;
  class Data_tag_alter_clauseContext;
  class Alter_workload_statementContext;
  class Alter_workload_opts_itemContext;
  class Package_executableContext;
  class Base_noneContext;
  class Extended_base_noneContext;
  class Alter_collect_activity_data_clauseContext;
  class With_optsContext;
  class Alter_collect_history_clauseContext;
  class Alter_collect_lock_wait_data_clauseContext;
  class Alter_wrapper_statementContext;
  class Alter_wrapper_opts_itemContext;
  class Alter_xsrobject_statementContext;
  class StringContext;
  class String_constantContext;
  class Numeric_constantContext;
  class Data_typeContext;
  class Anchored_data_typeContext;
  class Anchored_non_row_data_typeContext;
  class Anchored_row_data_typeContext;
  class Source_data_typeContext;
  class Data_type_constrainstContext;
  class Check_conditionContext;
  class Data_type_2Context;
  class Built_in_typeContext;
  class Inferred_builtin_typeContext;
  class Designator_data_typeContext;
  class Designator_data_type_listContext;
  class Designator_data_type_list_parenContext;
  class Integer_parenContext;
  class Integer_kmg_parenContext;
  class Char_characterContext;
  class Octets_codeunitsContext;
  class CodeunitsContext;
  class KmgContext;
  class Rs_locator_variableContext;
  class Integer_constant_listContext;
  class Integer_constantContext;
  class Integer_valueContext;
  class Positive_integerContext;
  class Bigint_valueContext;
  class Bigint_constantContext;
  class Member_numberContext;
  class Version_idContext;
  class Drop_statementContext;
  class Alias_designatorContext;
  class Service_class_designatorContext;
  class Tablespace_name_listContext;
  class Associate_locators_statementContext;
  class Audit_statementContext;
  class Begin_declare_section_statementContext;
  class Call_statementContext;
  class Arg_list_parenContext;
  class Arg_listContext;
  class ArgumentContext;
  class Case_statementContext;
  class Searched_case_statement_when_clauseContext;
  class Simple_case_statement_when_clauseContext;
  class Close_statementContext;
  class Comment_statementContext;
  class Column_commentContext;
  class Comment_objectsContext;
  class Commit_statementContext;
  class Connect_type_1_statementContext;
  class AuthorizationContext;
  class PasswordsContext;
  class Lock_blockContext;
  class AccesstokenContext;
  class TokenContext;
  class Api_keyContext;
  class Token_typeContext;
  class Declare_cursor_statementContext;
  class Declare_global_temporary_table_statementContext;
  class Describe_statementContext;
  class Xquery_statementContext;
  class Describe_input_statementContext;
  class Describe_output_statementContext;
  class Disconnect_statementContext;
  class End_declare_section_statementContext;
  class Execute_statementContext;
  class Host_variable_expressionContext;
  class Assignment_targetContext;
  class Execute_immediate_statementContext;
  class Explain_statementContext;
  class Explainable_sql_statementContext;
  class Fetch_statementContext;
  class Flush_bufferpools_statementContext;
  class Flush_event_monitor_statementContext;
  class Flush_federated_cache_statementContext;
  class Flush_optimization_profile_cache_statementContext;
  class Flush_package_cache_statementContext;
  class Flush_authentication_cache_statementContext;
  class Free_locator_statementContext;
  class Get_diagnostics_statementContext;
  class Statement_informationContext;
  class Condition_informationContext;
  class Condition_var_assignmentContext;
  class Lock_table_statementContext;
  class Pipe_statementContext;
  class Refresh_table_statementContext;
  class Release_connection_statementContext;
  class Rename_statementContext;
  class Rename_stogroup_statementContext;
  class Rename_tablespace_statementContext;
  class Set_statementContext;
  class Access_mode_clauseContext;
  class Cascade_clauseContext;
  class To_descendent_typesContext;
  class Table_type_listContext;
  class Table_typeContext;
  class Table_checked_options_listContext;
  class Table_checked_optionsContext;
  class Online_optionsContext;
  class Query_optimization_optionsContext;
  class Check_optionsContext;
  class Incremental_optionsContext;
  class Exception_clauseContext;
  class In_table_use_clauseContext;
  class Table_unchecked_optionsContext;
  class Full_accessContext;
  class Integrity_optionsContext;
  class Integrity_options_itemContext;
  class Var_def_listContext;
  class Var_defContext;
  class Expr_nullContext;
  class Expr_null_defaultContext;
  class Array_indexContext;
  class Row_fullselectContext;
  class Target_variableContext;
  class Target_cursor_variableContext;
  class Target_row_variableContext;
  class Row_array_element_specificationContext;
  class Row_field_referenceContext;
  class Field_referenceContext;
  class Search_conditionContext;
  class PredicateContext;
  class According_to_clauseContext;
  class Xml_schema_identification_listContext;
  class Xml_schema_identificationContext;
  class Fullselect_in_parenthesesContext;
  class Some_any_allContext;
  class Row_value_expressionContext;
  class Comparison_operatorContext;
  class Row_expressionContext;
  class Path_opt_listContext;
  class Path_optContext;
  class Pkg_opt_listContext;
  class Pkg_optContext;
  class Maintain_opt_listContext;
  class Maintain_optContext;
  class VariableContext;
  class Host_variableContext;
  class Set_integrity_statementContext;
  class Transfer_ownership_statementContext;
  class ObjectsContext;
  class Whenever_statementContext;
  class For_statementContext;
  class Goto_statementContext;
  class If_statementContext;
  class Include_statementContext;
  class Resignal_statementContext;
  class Signal_informationContext;
  class Diagnostic_string_constantContext;
  class Signal_statementContext;
  class Sqlstate_string_constantContext;
  class Sqlstate_string_variableContext;
  class Signal_information_2Context;
  class Diagnostic_string_expressionContext;
  class Iterate_statementContext;
  class Leave_statementContext;
  class Loop_statementContext;
  class Open_statementContext;
  class Variable_or_expressionContext;
  class Select_into_statementContext;
  class Values_into_statementContext;
  class Prepare_statementContext;
  class Repeat_statementContext;
  class Return_statementContext;
  class While_statementContext;
  class Sql_routine_statementContext;
  class Common_table_expressionContext;
  class Create_alias_statementContext;
  class Declared_alias_nameContext;
  class Table_aliasContext;
  class Module_aliasContext;
  class Sequence_aliasContext;
  class Or_replaceContext;
  class Create_audit_policy_statementContext;
  class Audit_policy_optsContext;
  class Audit_policy_categories_optsContext;
  class Create_bufferpool_statementContext;
  class Bufferpool_optsContext;
  class Except_clauseContext;
  class Member_listContext;
  class Member_list_itemContext;
  class Create_database_partition_group_statementContext;
  class Create_event_monitor_statementContext;
  class Create_event_monitor_activities_statementContext;
  class Formatted_event_table_info_3Context;
  class Create_event_monitor_change_history_statementContext;
  class Event_control_listContext;
  class Event_controlContext;
  class Create_event_monitor_locking_statementContext;
  class Create_event_monitor_package_cache_statementContext;
  class Filter_and_collection_optionsContext;
  class Event_conditionContext;
  class Event_condition_itemContext;
  class Create_event_monitor_statistics_statementContext;
  class Event_monitor_statistics_optsContext;
  class Create_event_monitor_threshold_violations_statementContext;
  class Formatted_event_table_info_2Context;
  class File_optionsContext;
  class Event_monitor_threshold_optsContext;
  class PagesContext;
  class Create_event_monitor_unit_of_workContext;
  class Formatted_event_table_infoContext;
  class Autostart_manualstartContext;
  class Evm_groupContext;
  class Target_table_optionsContext;
  class Create_external_table_statementContext;
  class Ext_table_optionContext;
  class Ext_table_option_valueContext;
  class Create_function_statementContext;
  class Create_function_aggregate_interface_statementContext;
  class Agg_fn_param_declContext;
  class Aggregate_return_clauseContext;
  class Aggregate_implementation_clauseContext;
  class Agg_fn_option_listContext;
  class State_variable_declarationContext;
  class Aggregate_cast_target_typeContext;
  class Aggregate_builtin_typeContext;
  class Create_function_external_scalar_statementContext;
  class External_scalar_ordinary_return_typeContext;
  class External_scalar_cast_source_typeContext;
  class Ext_scalar_param_declContext;
  class Ext_scalar_option_listContext;
  class Ext_scalar_option_list_itemContext;
  class Predicate_specificationContext;
  class Data_filterContext;
  class Index_exploitationContext;
  class Exploitation_ruleContext;
  class Create_function_external_table_statementContext;
  class Ext_table_param_decl_listContext;
  class Ext_table_param_declContext;
  class External_table_return_typeContext;
  class Ext_table_option_listContext;
  class Ext_table_option_list_itemContext;
  class Create_function_old_db_external_function_statementContext;
  class Oledb_return_typeContext;
  class Oledb_option_listContext;
  class Oledb_option_list_itemContext;
  class Create_function_sourced_or_template_statementContext;
  class Sourced_function_param_decl_listContext;
  class Sourced_function_param_declContext;
  class Sourced_function_parameter_typeContext;
  class Sourced_function_return_typeContext;
  class Template_function_return_typeContext;
  class Fn_return_optsContext;
  class Fn_return_opts_itemContext;
  class Template_optsContext;
  class Template_opts_itemContext;
  class Ascii_unicodeContext;
  class Param_decl_list_3Context;
  class Param_decl_3Context;
  class Create_function_sql_scalar_table_or_row_statementContext;
  class Sql_function_return_builtin_typeContext;
  class Sql_function_return_named_typeContext;
  class Sql_function_return_data_typeContext;
  class Sql_function_return_columnContext;
  class Sql_function_return_table_columnsContext;
  class Sql_function_return_row_columnsContext;
  class Param_decl_list_2Context;
  class Param_decl_2Context;
  class Sql_function_bodyContext;
  class Create_function_mapping_statementContext;
  class Function_optionsContext;
  class Function_option_nameContext;
  class Create_global_temporary_table_statementContext;
  class Create_global_temporary_table_optsContext;
  class Create_global_temporary_table_itemContext;
  class Delete_preserveContext;
  class Create_histogram_template_statementContext;
  class Create_index_statementContext;
  class Index_cluster_clauseContext;
  class Index_col_optsContext;
  class Index_col_opts_itemContext;
  class Key_expressionContext;
  class Create_index_extension_statementContext;
  class Param_listContext;
  class Index_maintenanceContext;
  class Table_function_invocationContext;
  class Index_searchContext;
  class Search_method_definitionContext;
  class Create_mask_statementContext;
  class Case_expressionContext;
  class Range_producing_funciton_invocationContext;
  class Index_filtering_function_invocationContext;
  class Create_method_statementContext;
  class Method_optsContext;
  class Method_opts_itemContext;
  class Method_signatureContext;
  class Method_ordinary_return_typeContext;
  class Method_reference_typeContext;
  class Method_cast_non_xml_typeContext;
  class Method_cast_non_xml_builtin_typeContext;
  class Method_cast_source_typeContext;
  class Method_cast_target_typeContext;
  class Method_param_listContext;
  class Method_signature_parameter_declContext;
  class Method_parameter_typeContext;
  class Method_builtin_typeContext;
  class Method_non_xml_builtin_typeContext;
  class Method_named_typeContext;
  class Method_integer_attributeContext;
  class Data_type_3Context;
  class Sql_method_bodyContext;
  class Compound_sql_inlinedContext;
  class Declare_variable_statementContext;
  class Assignment_statementContext;
  class Declare_variable_default_clauseContext;
  class Compound_body_statementContext;
  class Sql_statement_inlinedContext;
  class Compound_sql_compiledContext;
  class Sql_statement_compiledContext;
  class Create_module_statementContext;
  class Create_nickname_statementContext;
  class Nick_name_option_nameContext;
  class Remote_object_nameContext;
  class Non_relational_data_definitionContext;
  class Nick_name_column_listContext;
  class Nick_name_column_list_itemContext;
  class Nick_name_column_definitionContext;
  class Nick_name_column_optionsContext;
  class Federated_column_optionsContext;
  class Column_option_nameContext;
  class Create_permission_statementContext;
  class Create_procedure_statementContext;
  class Create_procedure_external_statementContext;
  class Proc_ext_param_listContext;
  class Proc_ext_paramContext;
  class Procedure_external_xml_typeContext;
  class Option_list_2Context;
  class Option_list_2_itemContext;
  class Create_procedure_sourced_statementContext;
  class Source_procedure_clauseContext;
  class Source_object_nameContext;
  class Option_list_1Context;
  class Option_list_1_itemContext;
  class Result_set_element_numberContext;
  class Unique_idContext;
  class Create_procedure_sql_statementContext;
  class Proc_parameter_listContext;
  class Proc_parameter_list_itemContext;
  class In_out_inoutContext;
  class Option_listContext;
  class Option_list_itemContext;
  class Sql_procedure_bodyContext;
  class Create_role_statementContext;
  class Create_schema_statementContext;
  class Schema_sql_statementContext;
  class Create_security_label_component_statementContext;
  class Array_clauseContext;
  class Set_clauseContext;
  class Tree_clauseContext;
  class Tree_clause_itemContext;
  class Create_security_label_statementContext;
  class Create_security_label_itemContext;
  class Create_security_policy_statementContext;
  class Create_sequence_statementContext;
  class Create_sequence_optsContext;
  class Create_sequence_opts_itemContext;
  class Create_service_class_statementContext;
  class High_medium_lowContext;
  class On_offContext;
  class Soft_hardContext;
  class Create_server_statementContext;
  class Password_Context;
  class Create_stogroup_statementContext;
  class Create_stogroup_optsContext;
  class Create_synonym_statementContext;
  class Create_table_statementContext;
  class Create_table_optsContext;
  class Table_option_listContext;
  class Table_option_list_itemContext;
  class Table_option_nameContext;
  class Element_listContext;
  class Element_list_itemContext;
  class Column_definitionContext;
  class Period_definitionContext;
  class Unique_constraintContext;
  class Referential_constraintContext;
  class Check_constraintContext;
  class Column_optionsContext;
  class Column_options_itemContext;
  class References_clauseContext;
  class Rule_clauseContext;
  class Constraint_attributesContext;
  class Routine_default_clauseContext;
  class Routine_default_valueContext;
  class Routine_default_special_registerContext;
  class Routine_default_timestamp_registerContext;
  class Routine_default_integer_attributeContext;
  class Default_clauseContext;
  class Default_valuesContext;
  class Generated_clauseContext;
  class Datetime_special_registerContext;
  class User_special_registerContext;
  class Cast_functionContext;
  class Identity_optionsContext;
  class Identity_options_itemContext;
  class As_row_change_timestamp_clauseContext;
  class As_generated_expression_clauseContext;
  class Generation_expressionContext;
  class As_row_transaction_timestamp_clauseContext;
  class As_row_transaction_start_id_clauseContext;
  class Oid_column_definitionContext;
  class Range_partition_specContext;
  class Partition_expression_listContext;
  class Partition_expressionContext;
  class Partition_element_listContext;
  class Partition_elementContext;
  class Boundary_specContext;
  class Partition_tablespace_optionsContext;
  class Duration_labelContext;
  class Starting_clauseContext;
  class Const_min_max_listContext;
  class Const_min_maxContext;
  class Ending_clauseContext;
  class Typed_table_optionsContext;
  class Typed_element_listContext;
  class Typed_element_list_itemContext;
  class As_result_tableContext;
  class Copy_optionsContext;
  class Materialized_query_optionsContext;
  class Staging_table_definitionContext;
  class Dimensions_clauseContext;
  class Col_namesContext;
  class Sequence_key_specContext;
  class Sequence_key_spec_listContext;
  class Sequence_key_spec_list_itemContext;
  class Tablespace_clausesContext;
  class Distribution_clauseContext;
  class Partitioning_clauseContext;
  class If_not_existsContext;
  class Create_tablespace_statementContext;
  class Storage_groupContext;
  class Size_attributesContext;
  class System_containersContext;
  class Container_string_listContext;
  class Database_containersContext;
  class Container_clauseContext;
  class Container_clause_listContext;
  class Container_clause_list_itemContext;
  class On_db_partitions_clauseContext;
  class Db_partition_number_listContext;
  class Db_partition_number_list_itemContext;
  class Db_partition_numberContext;
  class Number_of_pagesContext;
  class Number_of_filesContext;
  class Number_of_millisecondsContext;
  class Number_megabytes_per_secondContext;
  class Create_threshold_statementContext;
  class Threshold_domainContext;
  class Statement_textContext;
  class Executable_idContext;
  class Enforcement_scopeContext;
  class Threshold_predicateContext;
  class Checking_everyContext;
  class Hour_to_secondsContext;
  class Day_to_minutesContext;
  class Day_to_secondsContext;
  class Threshold_exceeded_actions_2Context;
  class Details_sectionContext;
  class Remap_activity_actionContext;
  class Create_transform_statementContext;
  class Tranform_listContext;
  class Tranform_list_itemContext;
  class Transform_group_listContext;
  class Transform_group_list_itemContext;
  class Create_trigger_statementContext;
  class Ref_listContext;
  class Ref_list_itemContext;
  class Old_newContext;
  class Correlation_nameContext;
  class IdentifierContext;
  class Trigger_eventContext;
  class Triggered_actionContext;
  class Sql_procedure_statementContext;
  class Sql_function_statementContext;
  class Create_trusted_context_statementContext;
  class Attr_listContext;
  class Attr_list_itemContext;
  class Auth_listContext;
  class Auth_list_itemContext;
  class Address_valueContext;
  class Encryption_valueContext;
  class Create_type_statementContext;
  class Create_type_array_statementContext;
  class Create_type_cursor_statementContext;
  class Create_type_distinct_statementContext;
  class Create_type_row_statementContext;
  class Field_definition_list_parenContext;
  class Field_definition_listContext;
  class Field_definitionContext;
  class Create_type_structured_statementContext;
  class Structured_type_seqContext;
  class Attribute_definition_list_parenContext;
  class Attribute_definition_listContext;
  class Attribute_definitionContext;
  class Method_specification_listContext;
  class Method_specificationContext;
  class Method_specification_return_clauseContext;
  class Method_specification_seqContext;
  class As_locatorContext;
  class Method_spec_parameter_list_parenContext;
  class Method_spec_parameter_listContext;
  class Method_spec_parameter_declContext;
  class Method_spec_parameter_typeContext;
  class Param_decl_list_parenContext;
  class Param_decl_listContext;
  class Param_declContext;
  class Sql_routine_characteristicsContext;
  class External_routine_characteristicsContext;
  class LengthContext;
  class Rep_typeContext;
  class VarcharsContext;
  class VarbinariesContext;
  class For_bit_dataContext;
  class Lob_optionsContext;
  class Create_type_mapping_statementContext;
  class For_bit_data_precisionContext;
  class PrecisionContext;
  class ScaleContext;
  class Precision_scale_compContext;
  class From_toContext;
  class Data_source_data_typeContext;
  class Local_data_typeContext;
  class Remote_serverContext;
  class Server_versionContext;
  class Server_typeContext;
  class VersionContext;
  class ReleaseContext;
  class ModContext;
  class Create_usage_list_statementContext;
  class Create_user_mapping_statementContext;
  class User_mapping_options_parenContext;
  class User_mapping_optionsContext;
  class Create_variable_statementContext;
  class Constant_Context;
  class Signed_numeric_constantContext;
  class Decfloat_special_constantContext;
  class Signed_decfloat_special_constantContext;
  class Boolean_constantContext;
  class Typed_datetime_constantContext;
  class Unicode_string_constantContext;
  class Unicode_escape_clauseContext;
  class Prefixed_string_constantContext;
  class Hex_character_string_constantContext;
  class Graphic_string_constantContext;
  class Hex_graphic_string_constantContext;
  class Binary_string_constantContext;
  class Utf16be_graphic_string_constantContext;
  class Special_registerContext;
  class Native_special_registerContext;
  class Timestamp_register_invocationContext;
  class Schema_qualified_variable_declaration_nameContext;
  class Global_variableContext;
  class Qualified_global_variable_referenceContext;
  class Data_type_1Context;
  class Cursor_value_constructorContext;
  class Anchored_variable_data_typeContext;
  class HoldabilityContext;
  class ReturnabilityContext;
  class Create_view_statementContext;
  class Create_view_seqContext;
  class FullselectContext;
  class SubselectContext;
  class Select_clauseContext;
  class Select_clause_itemContext;
  class From_clauseContext;
  class Table_referenceContext;
  class Table_reference_listContext;
  class Singles_table_referenceContext;
  class Table_reference_nameContext;
  class Table_reference_schemaContext;
  class Period_specificationContext;
  class ValueContext;
  class Correlation_clauseContext;
  class Tablesample_clauseContext;
  class Numeric_expressionContext;
  class Single_view_referenceContext;
  class Single_nickname_referenceContext;
  class Only_table_referenceContext;
  class Outer_table_referenceContext;
  class Analyze_table_referenceContext;
  class Implementation_clauseContext;
  class Nested_table_referenceContext;
  class Continue_handlerContext;
  class Specific_condition_valueContext;
  class Data_change_table_referenceContext;
  class Searched_update_statementContext;
  class Searched_delete_statementContext;
  class Final_newContext;
  class Final_new_oldContext;
  class Table_function_referenceContext;
  class Table_udf_cardinality_clauseContext;
  class Typed_correlation_clauseContext;
  class Column_name_data_typeContext;
  class Collection_derived_tableContext;
  class Table_functionContext;
  class Xmltable_expressionContext;
  class Xmltable_functionContext;
  class Joined_tableContext;
  class Join_conditionContext;
  class OuterContext;
  class External_table_referenceContext;
  class Column_definition_2Context;
  class File_nameContext;
  class Where_clauseContext;
  class Group_by_clauseContext;
  class Group_by_clause_optsContext;
  class Grouping_expressionContext;
  class Grouping_setsContext;
  class Super_groupsContext;
  class Grant_totalContext;
  class Having_clauseContext;
  class Order_by_clauseContext;
  class Order_by_clause_optsContext;
  class Table_designatorContext;
  class Asc_descContext;
  class First_lastContext;
  class Sort_keyContext;
  class Simple_column_nameContext;
  class Simple_integerContext;
  class Sork_key_expressionContext;
  class Offset_clauseContext;
  class Offset_row_countContext;
  class Fetch_clauseContext;
  class Fetch_row_countContext;
  class Row_rowsContext;
  class Isolation_clauseContext;
  class Lock_request_clauseContext;
  class Values_clauseContext;
  class Values_rowContext;
  class Root_view_definitionContext;
  class Subview_definitionContext;
  class Oid_columnContext;
  class With_optionsContext;
  class With_option_defContext;
  class With_option_scope_defContext;
  class Under_clauseContext;
  class Create_work_action_set_statementContext;
  class Work_action_definition_list_parenContext;
  class Work_action_definition_listContext;
  class Work_action_definitionContext;
  class Action_types_clauseContext;
  class Threshold_types_clauseContext;
  class Second_secondsContext;
  class Hours_minutesContext;
  class Threshold_exceeded_actionsContext;
  class Collect_activity_data_clauseContext;
  class With_withoutContext;
  class Histogram_templace_clauseContext;
  class Create_work_class_set_statementContext;
  class Work_class_definition_list_parenContext;
  class Work_class_definition_listContext;
  class Work_class_definitionContext;
  class Work_attributesContext;
  class Position_clauseContext;
  class Position_Context;
  class For_from_to_clauseContext;
  class From_valueContext;
  class To_valueContext;
  class Data_tag_clauseContext;
  class Schema_clauseContext;
  class Create_workload_statementContext;
  class Pkg_exec_seqContext;
  class Position_clause_2Context;
  class Connection_attributesContext;
  class String_listContext;
  class String_list_parenContext;
  class Workload_attributesContext;
  class DegreeContext;
  class Allow_disallowContext;
  class Collect_on_clauseContext;
  class Collect_details_clauseContext;
  class Collect_lock_wait_optionsContext;
  class Wait_timeContext;
  class Create_wrapper_statementContext;
  class Wrapper_option_listContext;
  class Wrapper_optionContext;
  class ExpressionContext;
  class Function_invocationContext;
  class All_distinctContext;
  class Scalar_fullselectContext;
  class Cast_specificationContext;
  class Cursor_cast_specificationContext;
  class Row_cast_specificationContext;
  class Interval_cast_specificationContext;
  class Xmlcast_specificationContext;
  class Array_element_specificationContext;
  class Array_constructorContext;
  class Method_invocationContext;
  class Olap_specificationContext;
  class Ordered_olap_specificationContext;
  class Window_partition_clauseContext;
  class Window_order_clauseContext;
  class Numbering_specificationContext;
  class Aggregation_specificationContext;
  class Olap_aggregate_functionContext;
  class First_value_functionContext;
  class Last_value_functionContext;
  class Nth_value_functionContext;
  class Ratio_to_report_functionContext;
  class Ignore_respect_nullsContext;
  class From_first_lastContext;
  class Window_aggregation_group_clauseContext;
  class Group_startContext;
  class Group_betweenContext;
  class Group_bound1Context;
  class Group_bound2Context;
  class Group_endContext;
  class Row_change_expressionContext;
  class Sequence_referenceContext;
  class Subtype_treatmentContext;
  class Expression_listContext;
  class Expression_list_in_parenthesesContext;
  class Id_Context;
  class Exposed_nameContext;
  class NameContext;
  class LabelContext;
  class Host_labelContext;
  class Library_nameContext;
  class Array_type_nameContext;
  class Attribute_nameContext;
  class Row_type_nameContext;
  class Authorization_nameContext;
  class Boolean_variable_nameContext;
  class Array_variable_nameContext;
  class Column_nameContext;
  class Constraint_nameContext;
  class Descriptor_nameContext;
  class Distinct_type_nameContext;
  class Cursor_nameContext;
  class Cursor_type_nameContext;
  class Condition_nameContext;
  class Data_source_nameContext;
  class Expression_nameContext;
  class Group_nameContext;
  class Policy_nameContext;
  class Bufferpool_nameContext;
  class Db_partition_nameContext;
  class Database_nameContext;
  class Event_monitor_nameContext;
  class Field_nameContext;
  class For_loop_nameContext;
  class Function_nameContext;
  class Function_mapping_nameContext;
  class Global_variable_nameContext;
  class Hierarchy_nameContext;
  class Host_variable_nameContext;
  class Parameter_markerContext;
  class Template_nameContext;
  class Index_nameContext;
  class Index_extension_nameContext;
  class Input_descriptor_nameContext;
  class Mask_nameContext;
  class Method_nameContext;
  class Model_nameContext;
  class Module_nameContext;
  class New_ownerContext;
  class Nick_nameContext;
  class Object_nameContext;
  class Oid_column_nameContext;
  class Optimization_profile_nameContext;
  class Package_nameContext;
  class Partition_nameContext;
  class Path_nameContext;
  class Permission_nameContext;
  class Pipe_nameContext;
  class Procedure_nameContext;
  class Result_descriptor_nameContext;
  class Role_nameContext;
  class Root_table_nameContext;
  class Root_view_nameContext;
  class Row_variable_nameContext;
  class Source_schema_nameContext;
  class Source_package_nameContext;
  class Source_procedure_nameContext;
  class Sql_parameter_nameContext;
  class Sql_variable_nameContext;
  class Transition_variable_nameContext;
  class Savepoint_nameContext;
  class Specific_nameContext;
  class SchemaContext;
  class Schema_nameContext;
  class Search_method_nameContext;
  class Server_nameContext;
  class Server_option_nameContext;
  class Session_authorization_nameContext;
  class Component_nameContext;
  class Sec_label_comp_nameContext;
  class Security_policy_nameContext;
  class Security_label_nameContext;
  class Sequence_nameContext;
  class Service_class_nameContext;
  class Service_superclass_nameContext;
  class Storagegroup_nameContext;
  class Supertype_nameContext;
  class Superview_nameContext;
  class Service_subclass_nameContext;
  class Statement_nameContext;
  class Table_nameContext;
  class Tablespace_nameContext;
  class Target_identifierContext;
  class Threshold_nameContext;
  class Trigger_nameContext;
  class Context_nameContext;
  class Usage_list_nameContext;
  class Unqualified_type_nameContext;
  class Type_nameContext;
  class Type_mapping_nameContext;
  class Typed_table_nameContext;
  class Typed_view_nameContext;
  class User_mapping_option_nameContext;
  class View_nameContext;
  class Variable_nameContext;
  class Work_action_set_nameContext;
  class Work_class_set_nameContext;
  class Workload_nameContext;
  class Work_action_nameContext;
  class Work_class_nameContext;
  class Wrapper_nameContext;
  class Wrapper_option_nameContext;
  class Xsrobject_nameContext;
  class Parameter_nameContext;
  class Cursor_variable_nameContext;
  class Alias_nameContext;
  class Db_partition_group_nameContext;
  class Source_index_nameContext;
  class Source_table_nameContext;
  class Source_storagegroup_nameContext;
  class Target_storagegroup_nameContext;
  class Source_tablespace_nameContext;
  class Target_tablespace_nameContext;
  class Unqualified_function_nameContext;
  class Unqualified_procedure_nameContext;
  class Unqualified_specific_nameContext;
  class Period_nameContext;
  class History_table_nameContext;
  class Xml_schema_nameContext;
  class TodoContext; 

  class  Db2_fileContext : public antlr4::ParserRuleContext {
  public:
    Db2_fileContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *EOF();
    BatchContext *batch();

   
  };

  Db2_fileContext* db2_file();

  class  BatchContext : public antlr4::ParserRuleContext {
  public:
    BatchContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Sql_statementContext *> sql_statement();
    Sql_statementContext* sql_statement(size_t i);
    std::vector<antlr4::tree::TerminalNode *> SEMI();
    antlr4::tree::TerminalNode* SEMI(size_t i);

   
  };

  BatchContext* batch();

  class  Sql_statementContext : public antlr4::ParserRuleContext {
  public:
    Sql_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Sql_schema_statementContext *sql_schema_statement();
    Sql_data_change_statementContext *sql_data_change_statement();
    Sql_data_statementContext *sql_data_statement();
    Sql_transaction_statementContext *sql_transaction_statement();
    Sql_connection_statementContext *sql_connection_statement();
    Sql_dynamic_statementContext *sql_dynamic_statement();
    Sql_session_statementContext *sql_session_statement();
    Sql_embedded_host_language_statementContext *sql_embedded_host_language_statement();
    Sql_constrol_statementContext *sql_constrol_statement();
    Select_statementContext *select_statement();
    Compound_sql_inlinedContext *compound_sql_inlined();

   
  };

  Sql_statementContext* sql_statement();

  class  Sql_schema_statementContext : public antlr4::ParserRuleContext {
  public:
    Sql_schema_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Alter_audit_policy_statementContext *alter_audit_policy_statement();
    Alter_bufferpool_statementContext *alter_bufferpool_statement();
    Alter_database_statementContext *alter_database_statement();
    Alter_event_monitor_statementContext *alter_event_monitor_statement();
    Alter_database_partition_group_statementContext *alter_database_partition_group_statement();
    Alter_function_statementContext *alter_function_statement();
    Alter_histogram_template_statementContext *alter_histogram_template_statement();
    Alter_index_statementContext *alter_index_statement();
    Alter_mask_statementContext *alter_mask_statement();
    Alter_method_statementContext *alter_method_statement();
    Alter_model_statementContext *alter_model_statement();
    Alter_module_statementContext *alter_module_statement();
    Alter_nickname_statementContext *alter_nickname_statement();
    Alter_package_statementContext *alter_package_statement();
    Alter_permission_statementContext *alter_permission_statement();
    Alter_procedure_external_statementContext *alter_procedure_external_statement();
    Alter_procedure_sourced_statementContext *alter_procedure_sourced_statement();
    Alter_procedure_sql_statementContext *alter_procedure_sql_statement();
    Alter_schema_statementContext *alter_schema_statement();
    Alter_security_label_component_statementContext *alter_security_label_component_statement();
    Alter_security_policy_statementContext *alter_security_policy_statement();
    Alter_sequence_statementContext *alter_sequence_statement();
    Alter_server_statementContext *alter_server_statement();
    Alter_service_class_statementContext *alter_service_class_statement();
    Alter_stogroup_statementContext *alter_stogroup_statement();
    Alter_table_statementContext *alter_table_statement();
    Alter_tablespace_statementContext *alter_tablespace_statement();
    Alter_threshold_statementContext *alter_threshold_statement();
    Alter_trigger_statementContext *alter_trigger_statement();
    Alter_trusted_context_statementContext *alter_trusted_context_statement();
    Alter_type_statementContext *alter_type_statement();
    Alter_usage_list_statementContext *alter_usage_list_statement();
    Alter_user_mapping_statementContext *alter_user_mapping_statement();
    Alter_view_statementContext *alter_view_statement();
    Alter_work_action_set_statementContext *alter_work_action_set_statement();
    Alter_work_class_set_statementContext *alter_work_class_set_statement();
    Alter_workload_statementContext *alter_workload_statement();
    Alter_wrapper_statementContext *alter_wrapper_statement();
    Alter_xsrobject_statementContext *alter_xsrobject_statement();
    Audit_statementContext *audit_statement();
    Comment_statementContext *comment_statement();
    Create_alias_statementContext *create_alias_statement();
    Create_audit_policy_statementContext *create_audit_policy_statement();
    Create_bufferpool_statementContext *create_bufferpool_statement();
    Create_database_partition_group_statementContext *create_database_partition_group_statement();
    Create_event_monitor_statementContext *create_event_monitor_statement();
    Create_event_monitor_activities_statementContext *create_event_monitor_activities_statement();
    Create_event_monitor_change_history_statementContext *create_event_monitor_change_history_statement();
    Create_event_monitor_locking_statementContext *create_event_monitor_locking_statement();
    Create_event_monitor_package_cache_statementContext *create_event_monitor_package_cache_statement();
    Create_event_monitor_statistics_statementContext *create_event_monitor_statistics_statement();
    Create_event_monitor_threshold_violations_statementContext *create_event_monitor_threshold_violations_statement();
    Create_external_table_statementContext *create_external_table_statement();
    Create_function_statementContext *create_function_statement();
    Create_function_aggregate_interface_statementContext *create_function_aggregate_interface_statement();
    Create_function_external_scalar_statementContext *create_function_external_scalar_statement();
    Create_function_external_table_statementContext *create_function_external_table_statement();
    Create_function_old_db_external_function_statementContext *create_function_old_db_external_function_statement();
    Create_function_sourced_or_template_statementContext *create_function_sourced_or_template_statement();
    Create_function_sql_scalar_table_or_row_statementContext *create_function_sql_scalar_table_or_row_statement();
    Create_function_mapping_statementContext *create_function_mapping_statement();
    Create_global_temporary_table_statementContext *create_global_temporary_table_statement();
    Create_histogram_template_statementContext *create_histogram_template_statement();
    Create_index_statementContext *create_index_statement();
    Create_index_extension_statementContext *create_index_extension_statement();
    Create_mask_statementContext *create_mask_statement();
    Create_method_statementContext *create_method_statement();
    Create_module_statementContext *create_module_statement();
    Create_nickname_statementContext *create_nickname_statement();
    Create_permission_statementContext *create_permission_statement();
    Create_procedure_statementContext *create_procedure_statement();
    Create_role_statementContext *create_role_statement();
    Create_schema_statementContext *create_schema_statement();
    Create_security_label_component_statementContext *create_security_label_component_statement();
    Create_security_label_statementContext *create_security_label_statement();
    Create_security_policy_statementContext *create_security_policy_statement();
    Create_sequence_statementContext *create_sequence_statement();
    Create_server_statementContext *create_server_statement();
    Create_service_class_statementContext *create_service_class_statement();
    Create_stogroup_statementContext *create_stogroup_statement();
    Create_synonym_statementContext *create_synonym_statement();
    Create_table_statementContext *create_table_statement();
    Create_tablespace_statementContext *create_tablespace_statement();
    Create_threshold_statementContext *create_threshold_statement();
    Create_transform_statementContext *create_transform_statement();
    Create_trigger_statementContext *create_trigger_statement();
    Create_trusted_context_statementContext *create_trusted_context_statement();
    Create_type_statementContext *create_type_statement();
    Create_type_array_statementContext *create_type_array_statement();
    Create_type_cursor_statementContext *create_type_cursor_statement();
    Create_type_distinct_statementContext *create_type_distinct_statement();
    Create_type_row_statementContext *create_type_row_statement();
    Create_type_structured_statementContext *create_type_structured_statement();
    Create_type_mapping_statementContext *create_type_mapping_statement();
    Create_usage_list_statementContext *create_usage_list_statement();
    Create_user_mapping_statementContext *create_user_mapping_statement();
    Create_variable_statementContext *create_variable_statement();
    Create_view_statementContext *create_view_statement();
    Create_work_action_set_statementContext *create_work_action_set_statement();
    Create_work_class_set_statementContext *create_work_class_set_statement();
    Create_workload_statementContext *create_workload_statement();
    Create_wrapper_statementContext *create_wrapper_statement();
    Drop_statementContext *drop_statement();
    Grant_database_authorities_statementContext *grant_database_authorities_statement();
    Grant_exemption_statementContext *grant_exemption_statement();
    Grant_global_variable_privileges_statementContext *grant_global_variable_privileges_statement();
    Grant_index_privileges_statementContext *grant_index_privileges_statement();
    Grant_module_privileges_statementContext *grant_module_privileges_statement();
    Grant_package_privileges_statementContext *grant_package_privileges_statement();
    Grant_role_statementContext *grant_role_statement();
    Grant_routine_privileges_statementContext *grant_routine_privileges_statement();
    Grant_schema_privileges_statementContext *grant_schema_privileges_statement();
    Grant_security_label_statementContext *grant_security_label_statement();
    Grant_sequence_privileges_statementContext *grant_sequence_privileges_statement();
    Grant_server_privileges_statementContext *grant_server_privileges_statement();
    Grant_setsessionuser_privilege_statementContext *grant_setsessionuser_privilege_statement();
    Grant_table_space_privileges_statementContext *grant_table_space_privileges_statement();
    Grant_table_view_or_nickname_privileges_statementContext *grant_table_view_or_nickname_privileges_statement();
    Grant_workload_privileges_statementContext *grant_workload_privileges_statement();
    Grant_xsr_object_privileges_statementContext *grant_xsr_object_privileges_statement();
    Refresh_table_statementContext *refresh_table_statement();
    Rename_statementContext *rename_statement();
    Rename_stogroup_statementContext *rename_stogroup_statement();
    Rename_tablespace_statementContext *rename_tablespace_statement();
    Revoke_database_authorities_statementContext *revoke_database_authorities_statement();
    Revoke_exemption_statementContext *revoke_exemption_statement();
    Revoke_global_variable_privileges_statementContext *revoke_global_variable_privileges_statement();
    Revoke_index_privileges_statementContext *revoke_index_privileges_statement();
    Revoke_module_privileges_statementContext *revoke_module_privileges_statement();
    Revoke_package_privileges_statementContext *revoke_package_privileges_statement();
    Revoke_role_statementContext *revoke_role_statement();
    Revoke_routine_privileges_statementContext *revoke_routine_privileges_statement();
    Revoke_schema_privileges_statementContext *revoke_schema_privileges_statement();
    Revoke_security_label_statementContext *revoke_security_label_statement();
    Revoke_sequence_privileges_statementContext *revoke_sequence_privileges_statement();
    Revoke_server_privileges_statementContext *revoke_server_privileges_statement();
    Revoke_setsessionuser_privilege_statementContext *revoke_setsessionuser_privilege_statement();
    Revoke_table_space_privileges_statementContext *revoke_table_space_privileges_statement();
    Revoke_table_view_or_nickname_privileges_statementContext *revoke_table_view_or_nickname_privileges_statement();
    Revoke_workload_privileges_statementContext *revoke_workload_privileges_statement();
    Revoke_xsr_object_privileges_statementContext *revoke_xsr_object_privileges_statement();
    Set_integrity_statementContext *set_integrity_statement();
    Transfer_ownership_statementContext *transfer_ownership_statement();

   
  };

  Sql_schema_statementContext* sql_schema_statement();

  class  Sql_data_change_statementContext : public antlr4::ParserRuleContext {
  public:
    Sql_data_change_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Delete_statementContext *delete_statement();
    Delete_deltalake_statementContext *delete_deltalake_statement();
    Insert_statementContext *insert_statement();
    Insert_datalake_statementContext *insert_datalake_statement();
    Merge_statementContext *merge_statement();
    Truncate_statementContext *truncate_statement();
    Update_statementContext *update_statement();
    Update_datalake_statementContext *update_datalake_statement();

   
  };

  Sql_data_change_statementContext* sql_data_change_statement();

  class  Sql_data_statementContext : public antlr4::ParserRuleContext {
  public:
    Sql_data_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Allocate_cursor_statementContext *allocate_cursor_statement();
    Associate_locators_statementContext *associate_locators_statement();
    Close_statementContext *close_statement();
    Declare_cursor_statementContext *declare_cursor_statement();
    Fetch_statementContext *fetch_statement();
    Flush_authentication_cache_statementContext *flush_authentication_cache_statement();
    Flush_bufferpools_statementContext *flush_bufferpools_statement();
    Flush_event_monitor_statementContext *flush_event_monitor_statement();
    Flush_federated_cache_statementContext *flush_federated_cache_statement();
    Flush_optimization_profile_cache_statementContext *flush_optimization_profile_cache_statement();
    Flush_package_cache_statementContext *flush_package_cache_statement();
    Free_locator_statementContext *free_locator_statement();
    Lock_table_statementContext *lock_table_statement();
    Open_statementContext *open_statement();
    Select_into_statementContext *select_into_statement();
    Values_into_statementContext *values_into_statement();

   
  };

  Sql_data_statementContext* sql_data_statement();

  class  Sql_transaction_statementContext : public antlr4::ParserRuleContext {
  public:
    Sql_transaction_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Commit_statementContext *commit_statement();
    Release_savepoint_statementContext *release_savepoint_statement();
    Rollback_statementContext *rollback_statement();
    Savepoint_statementContext *savepoint_statement();

   
  };

  Sql_transaction_statementContext* sql_transaction_statement();

  class  Sql_connection_statementContext : public antlr4::ParserRuleContext {
  public:
    Sql_connection_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Connect_type_1_statementContext *connect_type_1_statement();
    Disconnect_statementContext *disconnect_statement();
    Release_connection_statementContext *release_connection_statement();

   
  };

  Sql_connection_statementContext* sql_connection_statement();

  class  Sql_dynamic_statementContext : public antlr4::ParserRuleContext {
  public:
    Sql_dynamic_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Describe_statementContext *describe_statement();
    Describe_input_statementContext *describe_input_statement();
    Describe_output_statementContext *describe_output_statement();
    Execute_statementContext *execute_statement();
    Execute_immediate_statementContext *execute_immediate_statement();
    Prepare_statementContext *prepare_statement();

   
  };

  Sql_dynamic_statementContext* sql_dynamic_statement();

  class  Sql_session_statementContext : public antlr4::ParserRuleContext {
  public:
    Sql_session_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Declare_global_temporary_table_statementContext *declare_global_temporary_table_statement();
    Explain_statementContext *explain_statement();
    Set_statementContext *set_statement();

   
  };

  Sql_session_statementContext* sql_session_statement();

  class  Sql_embedded_host_language_statementContext : public antlr4::ParserRuleContext {
  public:
    Sql_embedded_host_language_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Begin_declare_section_statementContext *begin_declare_section_statement();
    End_declare_section_statementContext *end_declare_section_statement();
    Get_diagnostics_statementContext *get_diagnostics_statement();
    Include_statementContext *include_statement();
    Whenever_statementContext *whenever_statement();

   
  };

  Sql_embedded_host_language_statementContext* sql_embedded_host_language_statement();

  class  Sql_constrol_statementContext : public antlr4::ParserRuleContext {
  public:
    Sql_constrol_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Call_statementContext *call_statement();
    Case_statementContext *case_statement();
    For_statementContext *for_statement();
    Goto_statementContext *goto_statement();
    If_statementContext *if_statement();
    Iterate_statementContext *iterate_statement();
    Leave_statementContext *leave_statement();
    Loop_statementContext *loop_statement();
    Pipe_statementContext *pipe_statement();
    Repeat_statementContext *repeat_statement();
    Resignal_statementContext *resignal_statement();
    Return_statementContext *return_statement();
    Signal_statementContext *signal_statement();
    While_statementContext *while_statement();

   
  };

  Sql_constrol_statementContext* sql_constrol_statement();

  class  Select_statementContext : public antlr4::ParserRuleContext {
  public:
    Select_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    FullselectContext *fullselect();
    antlr4::tree::TerminalNode *WITH();
    Common_table_expression_listContext *common_table_expression_list();
    Read_only_clauseContext *read_only_clause();
    Update_clauseContext *update_clause();
    Optimize_for_clauseContext *optimize_for_clause();
    Isolation_clauseContext *isolation_clause();
    Concurrent_access_resolution_clauseContext *concurrent_access_resolution_clause();
    Offset_clauseContext *offset_clause();
    Fetch_clauseContext *fetch_clause();

   
  };

  Select_statementContext* select_statement();

  class  Read_only_clauseContext : public antlr4::ParserRuleContext {
  public:
    Read_only_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *ONLY();
    antlr4::tree::TerminalNode *READ();
    antlr4::tree::TerminalNode *FETCH();

   
  };

  Read_only_clauseContext* read_only_clause();

  class  Update_clauseContext : public antlr4::ParserRuleContext {
  public:
    Update_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *UPDATE();
    antlr4::tree::TerminalNode *OF();
    Column_name_listContext *column_name_list();

   
  };

  Update_clauseContext* update_clause();

  class  Optimize_for_clauseContext : public antlr4::ParserRuleContext {
  public:
    Optimize_for_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OPTIMIZE();
    antlr4::tree::TerminalNode *FOR();
    Integer_constantContext *integer_constant();
    Row_rowsContext *row_rows();

   
  };

  Optimize_for_clauseContext* optimize_for_clause();

  class  Concurrent_access_resolution_clauseContext : public antlr4::ParserRuleContext {
  public:
    Concurrent_access_resolution_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Skip_waitContext *skip_wait();

   
  };

  Concurrent_access_resolution_clauseContext* concurrent_access_resolution_clause();

  class  Delete_statementContext : public antlr4::ParserRuleContext {
  public:
    Delete_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Delete_statement_searched_deleteContext *delete_statement_searched_delete();
    Delete_statement_positioned_deleteContext *delete_statement_positioned_delete();

   
  };

  Delete_statementContext* delete_statement();

  class  Delete_statement_searched_deleteContext : public antlr4::ParserRuleContext {
  public:
    Delete_statement_searched_deleteContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DELETE();
    antlr4::tree::TerminalNode *FROM();
    Table_or_view_nameContext *table_or_view_name();
    Nick_nameContext *nick_name();
    antlr4::tree::TerminalNode *ONLY();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    FullselectContext *fullselect();
    Correlation_clauseContext *correlation_clause();
    Include_columnsContext *include_columns();
    Assignment_clauseContext *assignment_clause();
    Where_clauseContext *where_clause();
    Order_by_clauseContext *order_by_clause();
    Offset_clauseContext *offset_clause();
    Fetch_clauseContext *fetch_clause();
    std::vector<antlr4::tree::TerminalNode *> WITH();
    antlr4::tree::TerminalNode* WITH(size_t i);
    Isolation_levelContext *isolation_level();
    Skip_waitContext *skip_wait();
    Period_clauseContext *period_clause();
    Common_table_expression_listContext *common_table_expression_list();

   
  };

  Delete_statement_searched_deleteContext* delete_statement_searched_delete();

  class  Table_or_view_nameContext : public antlr4::ParserRuleContext {
  public:
    Table_or_view_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Table_nameContext *table_name();
    View_nameContext *view_name();

   
  };

  Table_or_view_nameContext* table_or_view_name();

  class  Delete_statement_positioned_deleteContext : public antlr4::ParserRuleContext {
  public:
    Delete_statement_positioned_deleteContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DELETE();
    antlr4::tree::TerminalNode *FROM();
    antlr4::tree::TerminalNode *WHERE();
    antlr4::tree::TerminalNode *CURRENT();
    antlr4::tree::TerminalNode *OF();
    Cursor_nameContext *cursor_name();
    Table_or_view_nameContext *table_or_view_name();
    Nick_nameContext *nick_name();
    antlr4::tree::TerminalNode *ONLY();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    FullselectContext *fullselect();
    Correlation_clauseContext *correlation_clause();
    antlr4::tree::TerminalNode *WITH();
    Common_table_expression_listContext *common_table_expression_list();

   
  };

  Delete_statement_positioned_deleteContext* delete_statement_positioned_delete();

  class  Delete_deltalake_statementContext : public antlr4::ParserRuleContext {
  public:
    Delete_deltalake_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DELETE();
    antlr4::tree::TerminalNode *FROM();
    Table_nameContext *table_name();
    Where_clauseContext *where_clause();
    Order_by_clauseContext *order_by_clause();
    Offset_clauseContext *offset_clause();
    Fetch_clauseContext *fetch_clause();

   
  };

  Delete_deltalake_statementContext* delete_deltalake_statement();

  class  Insert_statementContext : public antlr4::ParserRuleContext {
  public:
    Insert_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INSERT();
    antlr4::tree::TerminalNode *INTO();
    Table_or_view_nameContext *table_or_view_name();
    Nick_nameContext *nick_name();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<FullselectContext *> fullselect();
    FullselectContext* fullselect(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *VALUES();
    std::vector<Values_itemContext *> values_item();
    Values_itemContext* values_item(size_t i);
    Column_name_listContext *column_name_list();
    Include_columnsContext *include_columns();
    std::vector<antlr4::tree::TerminalNode *> WITH();
    antlr4::tree::TerminalNode* WITH(size_t i);
    Isolation_levelContext *isolation_level();
    std::vector<Common_table_expression_listContext *> common_table_expression_list();
    Common_table_expression_listContext* common_table_expression_list(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Insert_statementContext* insert_statement();

  class  Insert_datalake_statementContext : public antlr4::ParserRuleContext {
  public:
    Insert_datalake_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INSERT();
    antlr4::tree::TerminalNode *INTO();
    Table_nameContext *table_name();
    antlr4::tree::TerminalNode *VALUES();
    std::vector<Values_itemContext *> values_item();
    Values_itemContext* values_item(size_t i);
    FullselectContext *fullselect();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);
    antlr4::tree::TerminalNode *WITH();
    Common_table_expression_listContext *common_table_expression_list();

   
  };

  Insert_datalake_statementContext* insert_datalake_statement();

  class  Values_itemContext : public antlr4::ParserRuleContext {
  public:
    Values_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Expr_null_defaultContext *expr_null_default();
    Expr_null_default_listContext *expr_null_default_list();
    Row_expressionContext *row_expression();

   
  };

  Values_itemContext* values_item();

  class  Merge_statementContext : public antlr4::ParserRuleContext {
  public:
    Merge_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *MERGE();
    antlr4::tree::TerminalNode *INTO();
    Table_view_fullselectContext *table_view_fullselect();
    antlr4::tree::TerminalNode *USING();
    Table_referenceContext *table_reference();
    antlr4::tree::TerminalNode *ON();
    Search_conditionContext *search_condition();
    Correlation_clauseContext *correlation_clause();
    std::vector<antlr4::tree::TerminalNode *> WHEN();
    antlr4::tree::TerminalNode* WHEN(size_t i);
    std::vector<Matching_conditionContext *> matching_condition();
    Matching_conditionContext* matching_condition(size_t i);
    std::vector<antlr4::tree::TerminalNode *> THEN();
    antlr4::tree::TerminalNode* THEN(size_t i);
    antlr4::tree::TerminalNode *ELSE();
    antlr4::tree::TerminalNode *IGNORE();
    antlr4::tree::TerminalNode *WITH();
    Isolation_levelContext *isolation_level();
    std::vector<Modification_operationContext *> modification_operation();
    Modification_operationContext* modification_operation(size_t i);
    std::vector<Signal_statementContext *> signal_statement();
    Signal_statementContext* signal_statement(size_t i);

   
  };

  Merge_statementContext* merge_statement();

  class  Table_view_fullselectContext : public antlr4::ParserRuleContext {
  public:
    Table_view_fullselectContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Table_or_view_nameContext *table_or_view_name();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    FullselectContext *fullselect();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *WITH();
    Common_table_expression_listContext *common_table_expression_list();

   
  };

  Table_view_fullselectContext* table_view_fullselect();

  class  Common_table_expression_listContext : public antlr4::ParserRuleContext {
  public:
    Common_table_expression_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Common_table_expressionContext *> common_table_expression();
    Common_table_expressionContext* common_table_expression(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Common_table_expression_listContext* common_table_expression_list();

  class  Matching_conditionContext : public antlr4::ParserRuleContext {
  public:
    Matching_conditionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *MATCHED();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *AND();
    Search_conditionContext *search_condition();

   
  };

  Matching_conditionContext* matching_condition();

  class  Modification_operationContext : public antlr4::ParserRuleContext {
  public:
    Modification_operationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Update_operationContext *update_operation();
    Delete_operationContext *delete_operation();
    Insert_operationContext *insert_operation();

   
  };

  Modification_operationContext* modification_operation();

  class  Update_operationContext : public antlr4::ParserRuleContext {
  public:
    Update_operationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *UPDATE();
    antlr4::tree::TerminalNode *SET();
    Assignment_clauseContext *assignment_clause();
    Period_clauseContext *period_clause();

   
  };

  Update_operationContext* update_operation();

  class  Delete_operationContext : public antlr4::ParserRuleContext {
  public:
    Delete_operationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DELETE();
    Period_clauseContext *period_clause();

   
  };

  Delete_operationContext* delete_operation();

  class  Insert_operationContext : public antlr4::ParserRuleContext {
  public:
    Insert_operationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INSERT();
    antlr4::tree::TerminalNode *VALUES();
    Expr_null_defaultContext *expr_null_default();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Expr_null_default_listContext *expr_null_default_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    Column_name_list_parenContext *column_name_list_paren();

   
  };

  Insert_operationContext* insert_operation();

  class  Expr_null_default_listContext : public antlr4::ParserRuleContext {
  public:
    Expr_null_default_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Expr_null_defaultContext *> expr_null_default();
    Expr_null_defaultContext* expr_null_default(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Expr_null_default_listContext* expr_null_default_list();

  class  Isolation_levelContext : public antlr4::ParserRuleContext {
  public:
    Isolation_levelContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RR();
    antlr4::tree::TerminalNode *RS();
    antlr4::tree::TerminalNode *CS();
    antlr4::tree::TerminalNode *UR();

   
  };

  Isolation_levelContext* isolation_level();

  class  Truncate_statementContext : public antlr4::ParserRuleContext {
  public:
    Truncate_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TRUNCATE();
    Table_nameContext *table_name();
    antlr4::tree::TerminalNode *TABLE();
    antlr4::tree::TerminalNode *STORAGE();
    antlr4::tree::TerminalNode *DELETE();
    antlr4::tree::TerminalNode *TRIGGERS();
    antlr4::tree::TerminalNode *CONTINUE();
    antlr4::tree::TerminalNode *IDENTITY();
    antlr4::tree::TerminalNode *IMMEDIATE();
    antlr4::tree::TerminalNode *DROP();
    antlr4::tree::TerminalNode *REUSE();
    antlr4::tree::TerminalNode *IGNORE();
    antlr4::tree::TerminalNode *RESTRICT();
    antlr4::tree::TerminalNode *WHEN();

   
  };

  Truncate_statementContext* truncate_statement();

  class  Update_statementContext : public antlr4::ParserRuleContext {
  public:
    Update_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Update_statement_searched_updateContext *update_statement_searched_update();
    Update_statement_positioned_updateContext *update_statement_positioned_update();

   
  };

  Update_statementContext* update_statement();

  class  Update_statement_searched_updateContext : public antlr4::ParserRuleContext {
  public:
    Update_statement_searched_updateContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *UPDATE();
    antlr4::tree::TerminalNode *SET();
    Assignment_clauseContext *assignment_clause();
    Table_or_view_nameContext *table_or_view_name();
    Nick_nameContext *nick_name();
    antlr4::tree::TerminalNode *ONLY();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    FullselectContext *fullselect();
    Correlation_clauseContext *correlation_clause();
    Include_columnsContext *include_columns();
    antlr4::tree::TerminalNode *FROM();
    Table_reference_listContext *table_reference_list();
    Where_clauseContext *where_clause();
    Order_by_clauseContext *order_by_clause();
    Offset_clauseContext *offset_clause();
    Fetch_clauseContext *fetch_clause();
    std::vector<antlr4::tree::TerminalNode *> WITH();
    antlr4::tree::TerminalNode* WITH(size_t i);
    Isolation_levelContext *isolation_level();
    Skip_waitContext *skip_wait();
    Period_clauseContext *period_clause();
    Common_table_expression_listContext *common_table_expression_list();

   
  };

  Update_statement_searched_updateContext* update_statement_searched_update();

  class  Skip_waitContext : public antlr4::ParserRuleContext {
  public:
    Skip_waitContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SKIP_();
    antlr4::tree::TerminalNode *LOCKED();
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *WAIT();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *OUTCOME();
    antlr4::tree::TerminalNode *NOWAIT();
    Time_secContext *time_sec();

   
  };

  Skip_waitContext* skip_wait();

  class  Update_statement_positioned_updateContext : public antlr4::ParserRuleContext {
  public:
    Update_statement_positioned_updateContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *UPDATE();
    antlr4::tree::TerminalNode *SET();
    Assignment_clauseContext *assignment_clause();
    antlr4::tree::TerminalNode *WHERE();
    antlr4::tree::TerminalNode *CURRENT();
    antlr4::tree::TerminalNode *OF();
    Cursor_nameContext *cursor_name();
    Table_or_view_nameContext *table_or_view_name();
    Nick_nameContext *nick_name();
    antlr4::tree::TerminalNode *ONLY();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    FullselectContext *fullselect();
    antlr4::tree::TerminalNode *WITH();
    Common_table_expression_listContext *common_table_expression_list();

   
  };

  Update_statement_positioned_updateContext* update_statement_positioned_update();

  class  Include_columnsContext : public antlr4::ParserRuleContext {
  public:
    Include_columnsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INCLUDE();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<Column_nameContext *> column_name();
    Column_nameContext* column_name(size_t i);
    std::vector<Data_typeContext *> data_type();
    Data_typeContext* data_type(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Include_columnsContext* include_columns();

  class  Assignment_clauseContext : public antlr4::ParserRuleContext {
  public:
    Assignment_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Assignment_itemContext *> assignment_item();
    Assignment_itemContext* assignment_item(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Assignment_clauseContext* assignment_clause();

  class  Assignment_itemContext : public antlr4::ParserRuleContext {
  public:
    Assignment_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Column_nameContext *column_name();
    antlr4::tree::TerminalNode *EQ();
    Expr_null_defaultContext *expr_null_default();
    Column_name_list_parenContext *column_name_list_paren();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    Expr_null_default_listContext *expr_null_default_list();
    Row_fullselectContext *row_fullselect();

   
  };

  Assignment_itemContext* assignment_item();

  class  Period_clauseContext : public antlr4::ParserRuleContext {
  public:
    Period_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *PORTION();
    antlr4::tree::TerminalNode *OF();
    antlr4::tree::TerminalNode *BUSINESS_TYPE();
    antlr4::tree::TerminalNode *FROM();
    std::vector<ValueContext *> value();
    ValueContext* value(size_t i);
    antlr4::tree::TerminalNode *TO();

   
  };

  Period_clauseContext* period_clause();

  class  Time_secContext : public antlr4::ParserRuleContext {
  public:
    Time_secContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DECIMAL_LITERAL();

   
  };

  Time_secContext* time_sec();

  class  Update_datalake_statementContext : public antlr4::ParserRuleContext {
  public:
    Update_datalake_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *UPDATE();
    Table_nameContext *table_name();
    antlr4::tree::TerminalNode *SET();
    Assignment_clauseContext *assignment_clause();
    Correlation_clauseContext *correlation_clause();
    antlr4::tree::TerminalNode *FROM();
    Table_reference_listContext *table_reference_list();
    Where_clauseContext *where_clause();
    Order_by_clauseContext *order_by_clause();
    Offset_clauseContext *offset_clause();
    Fetch_clauseContext *fetch_clause();

   
  };

  Update_datalake_statementContext* update_datalake_statement();

  class  Grant_database_authorities_statementContext : public antlr4::ParserRuleContext {
  public:
    Grant_database_authorities_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GRANT();
    Db_privilege_listContext *db_privilege_list();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *DATABASE();
    antlr4::tree::TerminalNode *TO();
    Grantee_listContext *grantee_list();
    antlr4::tree::TerminalNode *PUBLIC();

   
  };

  Grant_database_authorities_statementContext* grant_database_authorities_statement();

  class  Db_privilege_listContext : public antlr4::ParserRuleContext {
  public:
    Db_privilege_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Db_privilegeContext *> db_privilege();
    Db_privilegeContext* db_privilege(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Db_privilege_listContext* db_privilege_list();

  class  Db_privilegeContext : public antlr4::ParserRuleContext {
  public:
    Db_privilegeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ACCESSCTRL();
    antlr4::tree::TerminalNode *BINDADD();
    antlr4::tree::TerminalNode *CONNECT();
    antlr4::tree::TerminalNode *CREATETAB();
    antlr4::tree::TerminalNode *CREATE_EXTERNAL_ROUTINE();
    antlr4::tree::TerminalNode *CREATE_NOT_FENCED_ROUTINE();
    antlr4::tree::TerminalNode *CREATE_SOURCE_OBJECT();
    antlr4::tree::TerminalNode *DATAACCESS();
    antlr4::tree::TerminalNode *DBADM();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *WITHOUT();
    antlr4::tree::TerminalNode *EXPLAIN();
    antlr4::tree::TerminalNode *IMPLICIT_SCHEMA();
    antlr4::tree::TerminalNode *LOAD();
    antlr4::tree::TerminalNode *QUIESCE_CONNECT();
    antlr4::tree::TerminalNode *SECADM();
    antlr4::tree::TerminalNode *SQLADM();
    antlr4::tree::TerminalNode *WLMADM();

   
  };

  Db_privilegeContext* db_privilege();

  class  GranteeContext : public antlr4::ParserRuleContext {
  public:
    GranteeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Authorization_nameContext *authorization_name();
    User_group_roleContext *user_group_role();

   
  };

  GranteeContext* grantee();

  class  Grantee_user_groupContext : public antlr4::ParserRuleContext {
  public:
    Grantee_user_groupContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    User_groupContext *user_group();
    Authorization_nameContext *authorization_name();

   
  };

  Grantee_user_groupContext* grantee_user_group();

  class  User_groupContext : public antlr4::ParserRuleContext {
  public:
    User_groupContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *USER();
    antlr4::tree::TerminalNode *GROUP();

   
  };

  User_groupContext* user_group();

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

  class  Grantee_list_publicContext : public antlr4::ParserRuleContext {
  public:
    Grantee_list_publicContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Grantee_listContext *grantee_list();
    antlr4::tree::TerminalNode *PUBLIC();

   
  };

  Grantee_list_publicContext* grantee_list_public();

  class  Grantee_list_user_groupContext : public antlr4::ParserRuleContext {
  public:
    Grantee_list_user_groupContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Grantee_user_groupContext *> grantee_user_group();
    Grantee_user_groupContext* grantee_user_group(size_t i);
    antlr4::tree::TerminalNode *COMMA();

   
  };

  Grantee_list_user_groupContext* grantee_list_user_group();

  class  Grant_exemption_statementContext : public antlr4::ParserRuleContext {
  public:
    Grant_exemption_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GRANT();
    antlr4::tree::TerminalNode *EXEMPTION();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *RULE();
    Exemption_privilegeContext *exemption_privilege();
    antlr4::tree::TerminalNode *FOR();
    Policy_nameContext *policy_name();
    antlr4::tree::TerminalNode *TO();
    Grantee_listContext *grantee_list();

   
  };

  Grant_exemption_statementContext* grant_exemption_statement();

  class  Exemption_privilegeContext : public antlr4::ParserRuleContext {
  public:
    Exemption_privilegeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DB2LBACREADARRAY();
    antlr4::tree::TerminalNode *DB2LBACREADSET();
    antlr4::tree::TerminalNode *DB2LBACREADTREE();
    antlr4::tree::TerminalNode *DB2LBACWRITEARRAY();
    antlr4::tree::TerminalNode *WRITEDOWN();
    antlr4::tree::TerminalNode *WRITEUP();
    antlr4::tree::TerminalNode *DB2LBACWRITESET();
    antlr4::tree::TerminalNode *DB2LBACWRITETREE();
    antlr4::tree::TerminalNode *ALL();

   
  };

  Exemption_privilegeContext* exemption_privilege();

  class  Grant_global_variable_privileges_statementContext : public antlr4::ParserRuleContext {
  public:
    Grant_global_variable_privileges_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GRANT();
    Variable_privilegeContext *variable_privilege();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *VARIABLE();
    Variable_nameContext *variable_name();
    antlr4::tree::TerminalNode *TO();
    Grantee_listContext *grantee_list();
    antlr4::tree::TerminalNode *PUBLIC();
    With_grant_optionContext *with_grant_option();

   
  };

  Grant_global_variable_privileges_statementContext* grant_global_variable_privileges_statement();

  class  Variable_privilegeContext : public antlr4::ParserRuleContext {
  public:
    Variable_privilegeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *PRIVILEGES();
    std::vector<Read_writeContext *> read_write();
    Read_writeContext* read_write(size_t i);
    antlr4::tree::TerminalNode *COMMA();

   
  };

  Variable_privilegeContext* variable_privilege();

  class  Read_writeContext : public antlr4::ParserRuleContext {
  public:
    Read_writeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *READ();
    antlr4::tree::TerminalNode *WRITE();

   
  };

  Read_writeContext* read_write();

  class  With_grant_optionContext : public antlr4::ParserRuleContext {
  public:
    With_grant_optionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *GRANT();
    antlr4::tree::TerminalNode *OPTION();

   
  };

  With_grant_optionContext* with_grant_option();

  class  Grant_index_privileges_statementContext : public antlr4::ParserRuleContext {
  public:
    Grant_index_privileges_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GRANT();
    antlr4::tree::TerminalNode *CONTROL();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *INDEX();
    Index_nameContext *index_name();
    antlr4::tree::TerminalNode *TO();
    Grantee_listContext *grantee_list();
    antlr4::tree::TerminalNode *PUBLIC();

   
  };

  Grant_index_privileges_statementContext* grant_index_privileges_statement();

  class  Grant_module_privileges_statementContext : public antlr4::ParserRuleContext {
  public:
    Grant_module_privileges_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GRANT();
    antlr4::tree::TerminalNode *EXECUTE();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *MODULE();
    Module_nameContext *module_name();
    antlr4::tree::TerminalNode *TO();
    Grantee_listContext *grantee_list();
    antlr4::tree::TerminalNode *PUBLIC();
    With_grant_optionContext *with_grant_option();

   
  };

  Grant_module_privileges_statementContext* grant_module_privileges_statement();

  class  Grant_package_privileges_statementContext : public antlr4::ParserRuleContext {
  public:
    Grant_package_privileges_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GRANT();
    Package_privilege_listContext *package_privilege_list();
    antlr4::tree::TerminalNode *ON();
    Package_nameContext *package_name();
    antlr4::tree::TerminalNode *TO();
    antlr4::tree::TerminalNode *PACKAGE();
    antlr4::tree::TerminalNode *PROGRAM();
    Grantee_listContext *grantee_list();
    antlr4::tree::TerminalNode *PUBLIC();
    With_grant_optionContext *with_grant_option();

   
  };

  Grant_package_privileges_statementContext* grant_package_privileges_statement();

  class  Package_privilege_listContext : public antlr4::ParserRuleContext {
  public:
    Package_privilege_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Package_privilegeContext *> package_privilege();
    Package_privilegeContext* package_privilege(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Package_privilege_listContext* package_privilege_list();

  class  Package_privilegeContext : public antlr4::ParserRuleContext {
  public:
    Package_privilegeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BIND();
    antlr4::tree::TerminalNode *CONTROL();
    antlr4::tree::TerminalNode *EXECUTE();
    antlr4::tree::TerminalNode *RUN();

   
  };

  Package_privilegeContext* package_privilege();

  class  Grant_role_statementContext : public antlr4::ParserRuleContext {
  public:
    Grant_role_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GRANT();
    Role_listContext *role_list();
    antlr4::tree::TerminalNode *TO();
    Grantee_listContext *grantee_list();
    antlr4::tree::TerminalNode *PUBLIC();
    antlr4::tree::TerminalNode *ROLE();
    With_grant_optionContext *with_grant_option();

   
  };

  Grant_role_statementContext* grant_role_statement();

  class  Role_listContext : public antlr4::ParserRuleContext {
  public:
    Role_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Role_nameContext *> role_name();
    Role_nameContext* role_name(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Role_listContext* role_list();

  class  Grant_routine_privileges_statementContext : public antlr4::ParserRuleContext {
  public:
    Grant_routine_privileges_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GRANT();
    antlr4::tree::TerminalNode *EXECUTE();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *TO();
    Function_designatorContext *function_designator();
    antlr4::tree::TerminalNode *FUNCTION();
    std::vector<antlr4::tree::TerminalNode *> STAR();
    antlr4::tree::TerminalNode* STAR(size_t i);
    Method_designatorContext *method_designator();
    antlr4::tree::TerminalNode *METHOD();
    antlr4::tree::TerminalNode *FOR();
    Procedure_designatorContext *procedure_designator();
    antlr4::tree::TerminalNode *PROCEDURE();
    Grantee_listContext *grantee_list();
    antlr4::tree::TerminalNode *PUBLIC();
    With_grant_optionContext *with_grant_option();
    Type_nameContext *type_name();
    SchemaContext *schema();
    antlr4::tree::TerminalNode *DOT();

   
  };

  Grant_routine_privileges_statementContext* grant_routine_privileges_statement();

  class  Grant_schema_privileges_statementContext : public antlr4::ParserRuleContext {
  public:
    Grant_schema_privileges_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GRANT();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *TO();
    antlr4::tree::TerminalNode *ALL();
    Schema_privilege_listContext *schema_privilege_list();
    antlr4::tree::TerminalNode *SCHEMA();
    Schema_nameContext *schema_name();
    antlr4::tree::TerminalNode *CURRENT();
    Grantee_listContext *grantee_list();
    antlr4::tree::TerminalNode *PUBLIC();
    With_grant_optionContext *with_grant_option();
    antlr4::tree::TerminalNode *PRIVILEGES();

   
  };

  Grant_schema_privileges_statementContext* grant_schema_privileges_statement();

  class  Schema_privilege_listContext : public antlr4::ParserRuleContext {
  public:
    Schema_privilege_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Schema_privilegeContext *> schema_privilege();
    Schema_privilegeContext* schema_privilege(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Schema_privilege_listContext* schema_privilege_list();

  class  Schema_privilegeContext : public antlr4::ParserRuleContext {
  public:
    Schema_privilegeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ACCESSCTRL();
    antlr4::tree::TerminalNode *ALTERIN();
    antlr4::tree::TerminalNode *CREATEIN();
    antlr4::tree::TerminalNode *DATAACCESS();
    antlr4::tree::TerminalNode *DELETEIN();
    antlr4::tree::TerminalNode *DROPIN();
    antlr4::tree::TerminalNode *EXECUTEIN();
    antlr4::tree::TerminalNode *INSERTIN();
    antlr4::tree::TerminalNode *LOAD();
    antlr4::tree::TerminalNode *SCHEMAADM();
    antlr4::tree::TerminalNode *SELECTIN();
    antlr4::tree::TerminalNode *UPDATEIN();

   
  };

  Schema_privilegeContext* schema_privilege();

  class  Grant_security_label_statementContext : public antlr4::ParserRuleContext {
  public:
    Grant_security_label_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GRANT();
    antlr4::tree::TerminalNode *SECURITY();
    antlr4::tree::TerminalNode *LABEL();
    Security_label_nameContext *security_label_name();
    antlr4::tree::TerminalNode *TO();
    Grantee_listContext *grantee_list();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *ACCESS();
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *READ();
    antlr4::tree::TerminalNode *WRITE();

   
  };

  Grant_security_label_statementContext* grant_security_label_statement();

  class  Grant_sequence_privileges_statementContext : public antlr4::ParserRuleContext {
  public:
    Grant_sequence_privileges_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GRANT();
    Sequence_privilege_listContext *sequence_privilege_list();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *SEQUENCE();
    Sequence_nameContext *sequence_name();
    antlr4::tree::TerminalNode *TO();
    Grantee_listContext *grantee_list();
    antlr4::tree::TerminalNode *PUBLIC();
    With_grant_optionContext *with_grant_option();

   
  };

  Grant_sequence_privileges_statementContext* grant_sequence_privileges_statement();

  class  Sequence_privilege_listContext : public antlr4::ParserRuleContext {
  public:
    Sequence_privilege_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Sequence_privilegeContext *> sequence_privilege();
    Sequence_privilegeContext* sequence_privilege(size_t i);
    antlr4::tree::TerminalNode *COMMA();

   
  };

  Sequence_privilege_listContext* sequence_privilege_list();

  class  Sequence_privilegeContext : public antlr4::ParserRuleContext {
  public:
    Sequence_privilegeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *USAGE();
    antlr4::tree::TerminalNode *ALTER();

   
  };

  Sequence_privilegeContext* sequence_privilege();

  class  Grant_server_privileges_statementContext : public antlr4::ParserRuleContext {
  public:
    Grant_server_privileges_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GRANT();
    antlr4::tree::TerminalNode *PASSTHRU();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *SERVER();
    Server_nameContext *server_name();
    antlr4::tree::TerminalNode *TO();
    Grantee_listContext *grantee_list();
    antlr4::tree::TerminalNode *PUBLIC();

   
  };

  Grant_server_privileges_statementContext* grant_server_privileges_statement();

  class  Grant_setsessionuser_privilege_statementContext : public antlr4::ParserRuleContext {
  public:
    Grant_setsessionuser_privilege_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GRANT();
    antlr4::tree::TerminalNode *SETSESSIONUSER();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *TO();
    Grantee_list_user_groupContext *grantee_list_user_group();
    User_listContext *user_list();
    antlr4::tree::TerminalNode *PUBLIC();

   
  };

  Grant_setsessionuser_privilege_statementContext* grant_setsessionuser_privilege_statement();

  class  User_listContext : public antlr4::ParserRuleContext {
  public:
    User_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<User_authContext *> user_auth();
    User_authContext* user_auth(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  User_listContext* user_list();

  class  User_authContext : public antlr4::ParserRuleContext {
  public:
    User_authContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *USER();
    Session_authorization_nameContext *session_authorization_name();

   
  };

  User_authContext* user_auth();

  class  Grant_table_space_privileges_statementContext : public antlr4::ParserRuleContext {
  public:
    Grant_table_space_privileges_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GRANT();
    antlr4::tree::TerminalNode *USE();
    antlr4::tree::TerminalNode *OF();
    antlr4::tree::TerminalNode *TABLESPACE();
    Tablespace_nameContext *tablespace_name();
    antlr4::tree::TerminalNode *TO();
    Grantee_listContext *grantee_list();
    antlr4::tree::TerminalNode *PUBLIC();
    With_grant_optionContext *with_grant_option();

   
  };

  Grant_table_space_privileges_statementContext* grant_table_space_privileges_statement();

  class  Grant_table_view_or_nickname_privileges_statementContext : public antlr4::ParserRuleContext {
  public:
    Grant_table_view_or_nickname_privileges_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GRANT();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *TO();
    antlr4::tree::TerminalNode *ALL();
    Tvn_privilege_listContext *tvn_privilege_list();
    Table_or_view_nameContext *table_or_view_name();
    Nick_nameContext *nick_name();
    Grantee_listContext *grantee_list();
    antlr4::tree::TerminalNode *PUBLIC();
    antlr4::tree::TerminalNode *TABLE();
    With_grant_optionContext *with_grant_option();
    antlr4::tree::TerminalNode *PRIVILEGES();

   
  };

  Grant_table_view_or_nickname_privileges_statementContext* grant_table_view_or_nickname_privileges_statement();

  class  Tvn_privilege_listContext : public antlr4::ParserRuleContext {
  public:
    Tvn_privilege_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Tvn_privilegeContext *> tvn_privilege();
    Tvn_privilegeContext* tvn_privilege(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Tvn_privilege_listContext* tvn_privilege_list();

  class  Tvn_privilegeContext : public antlr4::ParserRuleContext {
  public:
    Tvn_privilegeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *CONTROL();
    antlr4::tree::TerminalNode *DELETE();
    antlr4::tree::TerminalNode *INDEX();
    antlr4::tree::TerminalNode *INSERT();
    antlr4::tree::TerminalNode *REFERENCES();
    Column_name_list_parenContext *column_name_list_paren();
    antlr4::tree::TerminalNode *SELECT();
    antlr4::tree::TerminalNode *UPDATE();

   
  };

  Tvn_privilegeContext* tvn_privilege();

  class  Column_name_list_parenContext : public antlr4::ParserRuleContext {
  public:
    Column_name_list_parenContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Column_name_listContext *column_name_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();

   
  };

  Column_name_list_parenContext* column_name_list_paren();

  class  Column_name_listContext : public antlr4::ParserRuleContext {
  public:
    Column_name_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Column_nameContext *> column_name();
    Column_nameContext* column_name(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Column_name_listContext* column_name_list();

  class  Grant_workload_privileges_statementContext : public antlr4::ParserRuleContext {
  public:
    Grant_workload_privileges_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GRANT();
    antlr4::tree::TerminalNode *USAGE();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *WORKLOAD();
    Workload_nameContext *workload_name();
    antlr4::tree::TerminalNode *TO();
    Grantee_listContext *grantee_list();
    antlr4::tree::TerminalNode *PUBLIC();

   
  };

  Grant_workload_privileges_statementContext* grant_workload_privileges_statement();

  class  Grant_xsr_object_privileges_statementContext : public antlr4::ParserRuleContext {
  public:
    Grant_xsr_object_privileges_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GRANT();
    antlr4::tree::TerminalNode *USAGE();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *XSROBJECT();
    Xsrobject_nameContext *xsrobject_name();
    antlr4::tree::TerminalNode *TO();
    antlr4::tree::TerminalNode *PUBLIC();

   
  };

  Grant_xsr_object_privileges_statementContext* grant_xsr_object_privileges_statement();

  class  Revoke_database_authorities_statementContext : public antlr4::ParserRuleContext {
  public:
    Revoke_database_authorities_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REVOKE();
    Db_privilege_listContext *db_privilege_list();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *DATABASE();
    antlr4::tree::TerminalNode *FROM();
    Grantee_list_publicContext *grantee_list_public();
    By_allContext *by_all();

   
  };

  Revoke_database_authorities_statementContext* revoke_database_authorities_statement();

  class  By_allContext : public antlr4::ParserRuleContext {
  public:
    By_allContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BY();
    antlr4::tree::TerminalNode *ALL();

   
  };

  By_allContext* by_all();

  class  Revoke_exemption_statementContext : public antlr4::ParserRuleContext {
  public:
    Revoke_exemption_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REVOKE();
    antlr4::tree::TerminalNode *EXEMPTION();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *RULE();
    Exemption_privilegeContext *exemption_privilege();
    antlr4::tree::TerminalNode *FOR();
    Policy_nameContext *policy_name();
    antlr4::tree::TerminalNode *FROM();
    Grantee_list_publicContext *grantee_list_public();

   
  };

  Revoke_exemption_statementContext* revoke_exemption_statement();

  class  Revoke_global_variable_privileges_statementContext : public antlr4::ParserRuleContext {
  public:
    Revoke_global_variable_privileges_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REVOKE();
    Variable_privilegeContext *variable_privilege();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *VARIABLE();
    Variable_nameContext *variable_name();
    antlr4::tree::TerminalNode *FROM();
    Grantee_list_publicContext *grantee_list_public();
    By_allContext *by_all();
    antlr4::tree::TerminalNode *RESTRICT();

   
  };

  Revoke_global_variable_privileges_statementContext* revoke_global_variable_privileges_statement();

  class  Revoke_index_privileges_statementContext : public antlr4::ParserRuleContext {
  public:
    Revoke_index_privileges_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REVOKE();
    antlr4::tree::TerminalNode *CONTROL();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *INDEX();
    Index_nameContext *index_name();
    antlr4::tree::TerminalNode *FROM();
    Grantee_list_publicContext *grantee_list_public();
    By_allContext *by_all();

   
  };

  Revoke_index_privileges_statementContext* revoke_index_privileges_statement();

  class  Revoke_module_privileges_statementContext : public antlr4::ParserRuleContext {
  public:
    Revoke_module_privileges_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REVOKE();
    antlr4::tree::TerminalNode *EXECUTE();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *MODULE();
    Module_nameContext *module_name();
    antlr4::tree::TerminalNode *FROM();
    Grantee_list_publicContext *grantee_list_public();

   
  };

  Revoke_module_privileges_statementContext* revoke_module_privileges_statement();

  class  Revoke_package_privileges_statementContext : public antlr4::ParserRuleContext {
  public:
    Revoke_package_privileges_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REVOKE();
    Package_privilegeContext *package_privilege();
    antlr4::tree::TerminalNode *ON();
    Package_nameContext *package_name();
    antlr4::tree::TerminalNode *FROM();
    Grantee_list_publicContext *grantee_list_public();
    antlr4::tree::TerminalNode *PACKAGE();
    antlr4::tree::TerminalNode *PROGRAM();
    By_allContext *by_all();

   
  };

  Revoke_package_privileges_statementContext* revoke_package_privileges_statement();

  class  Revoke_role_statementContext : public antlr4::ParserRuleContext {
  public:
    Revoke_role_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REVOKE();
    Role_listContext *role_list();
    antlr4::tree::TerminalNode *FROM();
    Grantee_list_publicContext *grantee_list_public();
    antlr4::tree::TerminalNode *ADMIN();
    antlr4::tree::TerminalNode *OPTION();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *ROLE();
    By_allContext *by_all();

   
  };

  Revoke_role_statementContext* revoke_role_statement();

  class  Revoke_routine_privileges_statementContext : public antlr4::ParserRuleContext {
  public:
    Revoke_routine_privileges_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REVOKE();
    antlr4::tree::TerminalNode *EXECUTE();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *FROM();
    Grantee_list_publicContext *grantee_list_public();
    antlr4::tree::TerminalNode *RESTRICT();
    Function_designatorContext *function_designator();
    antlr4::tree::TerminalNode *FUNCTION();
    std::vector<antlr4::tree::TerminalNode *> STAR();
    antlr4::tree::TerminalNode* STAR(size_t i);
    Method_designatorContext *method_designator();
    antlr4::tree::TerminalNode *METHOD();
    antlr4::tree::TerminalNode *FOR();
    Procedure_designatorContext *procedure_designator();
    antlr4::tree::TerminalNode *PROCEDURE();
    By_allContext *by_all();
    Type_nameContext *type_name();
    SchemaContext *schema();
    antlr4::tree::TerminalNode *DOT();

   
  };

  Revoke_routine_privileges_statementContext* revoke_routine_privileges_statement();

  class  Revoke_schema_privileges_statementContext : public antlr4::ParserRuleContext {
  public:
    Revoke_schema_privileges_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REVOKE();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *FROM();
    Grantee_list_publicContext *grantee_list_public();
    antlr4::tree::TerminalNode *ALL();
    Schema_privilege_listContext *schema_privilege_list();
    antlr4::tree::TerminalNode *SCHEMA();
    Schema_nameContext *schema_name();
    antlr4::tree::TerminalNode *CURRENT();
    By_allContext *by_all();
    antlr4::tree::TerminalNode *PRIVILEGES();

   
  };

  Revoke_schema_privileges_statementContext* revoke_schema_privileges_statement();

  class  Revoke_security_label_statementContext : public antlr4::ParserRuleContext {
  public:
    Revoke_security_label_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REVOKE();
    antlr4::tree::TerminalNode *SECURITY();
    antlr4::tree::TerminalNode *LABEL();
    Security_label_nameContext *security_label_name();
    antlr4::tree::TerminalNode *FROM();
    Grantee_list_publicContext *grantee_list_public();

   
  };

  Revoke_security_label_statementContext* revoke_security_label_statement();

  class  Revoke_sequence_privileges_statementContext : public antlr4::ParserRuleContext {
  public:
    Revoke_sequence_privileges_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REVOKE();
    Sequence_privilege_listContext *sequence_privilege_list();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *SEQUENCE();
    Sequence_nameContext *sequence_name();
    antlr4::tree::TerminalNode *FROM();
    Grantee_list_publicContext *grantee_list_public();
    By_allContext *by_all();
    antlr4::tree::TerminalNode *RESTRICT();

   
  };

  Revoke_sequence_privileges_statementContext* revoke_sequence_privileges_statement();

  class  Revoke_server_privileges_statementContext : public antlr4::ParserRuleContext {
  public:
    Revoke_server_privileges_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REVOKE();
    antlr4::tree::TerminalNode *PASSTHRU();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *SERVER();
    Server_nameContext *server_name();
    antlr4::tree::TerminalNode *FROM();
    Grantee_list_publicContext *grantee_list_public();
    By_allContext *by_all();

   
  };

  Revoke_server_privileges_statementContext* revoke_server_privileges_statement();

  class  Revoke_setsessionuser_privilege_statementContext : public antlr4::ParserRuleContext {
  public:
    Revoke_setsessionuser_privilege_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REVOKE();
    antlr4::tree::TerminalNode *SETSESSIONUSER();
    User_listContext *user_list();
    antlr4::tree::TerminalNode *FROM();
    Grantee_list_user_groupContext *grantee_list_user_group();

   
  };

  Revoke_setsessionuser_privilege_statementContext* revoke_setsessionuser_privilege_statement();

  class  Revoke_table_space_privileges_statementContext : public antlr4::ParserRuleContext {
  public:
    Revoke_table_space_privileges_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REVOKE();
    antlr4::tree::TerminalNode *USE();
    antlr4::tree::TerminalNode *OF();
    antlr4::tree::TerminalNode *TABLESPACE();
    Tablespace_nameContext *tablespace_name();
    antlr4::tree::TerminalNode *FROM();
    Grantee_list_publicContext *grantee_list_public();
    By_allContext *by_all();

   
  };

  Revoke_table_space_privileges_statementContext* revoke_table_space_privileges_statement();

  class  Revoke_table_view_or_nickname_privileges_statementContext : public antlr4::ParserRuleContext {
  public:
    Revoke_table_view_or_nickname_privileges_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REVOKE();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *FROM();
    Grantee_list_publicContext *grantee_list_public();
    antlr4::tree::TerminalNode *ALL();
    Tvn_privilege_listContext *tvn_privilege_list();
    Table_or_view_nameContext *table_or_view_name();
    Nick_nameContext *nick_name();
    antlr4::tree::TerminalNode *TABLE();
    By_allContext *by_all();
    antlr4::tree::TerminalNode *PRIVILEGES();

   
  };

  Revoke_table_view_or_nickname_privileges_statementContext* revoke_table_view_or_nickname_privileges_statement();

  class  Revoke_workload_privileges_statementContext : public antlr4::ParserRuleContext {
  public:
    Revoke_workload_privileges_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REVOKE();
    antlr4::tree::TerminalNode *USAGE();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *WORKLOAD();
    Workload_nameContext *workload_name();
    antlr4::tree::TerminalNode *FROM();
    Grantee_list_publicContext *grantee_list_public();
    By_allContext *by_all();

   
  };

  Revoke_workload_privileges_statementContext* revoke_workload_privileges_statement();

  class  Revoke_xsr_object_privileges_statementContext : public antlr4::ParserRuleContext {
  public:
    Revoke_xsr_object_privileges_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REVOKE();
    antlr4::tree::TerminalNode *USAGE();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *XSROBJECT();
    Xsrobject_nameContext *xsrobject_name();
    antlr4::tree::TerminalNode *FROM();
    antlr4::tree::TerminalNode *PUBLIC();
    By_allContext *by_all();

   
  };

  Revoke_xsr_object_privileges_statementContext* revoke_xsr_object_privileges_statement();

  class  User_group_roleContext : public antlr4::ParserRuleContext {
  public:
    User_group_roleContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *USER();
    antlr4::tree::TerminalNode *GROUP();
    antlr4::tree::TerminalNode *ROLE();

   
  };

  User_group_roleContext* user_group_role();

  class  Rollback_statementContext : public antlr4::ParserRuleContext {
  public:
    Rollback_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ROLLBACK();
    antlr4::tree::TerminalNode *WORK();
    antlr4::tree::TerminalNode *TO();
    antlr4::tree::TerminalNode *SAVEPOINT();
    Savepoint_nameContext *savepoint_name();

   
  };

  Rollback_statementContext* rollback_statement();

  class  Savepoint_statementContext : public antlr4::ParserRuleContext {
  public:
    Savepoint_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SAVEPOINT();
    Savepoint_nameContext *savepoint_name();
    std::vector<antlr4::tree::TerminalNode *> ON();
    antlr4::tree::TerminalNode* ON(size_t i);
    std::vector<antlr4::tree::TerminalNode *> ROLLBACK();
    antlr4::tree::TerminalNode* ROLLBACK(size_t i);
    std::vector<antlr4::tree::TerminalNode *> RETAIN();
    antlr4::tree::TerminalNode* RETAIN(size_t i);
    antlr4::tree::TerminalNode *CURSORS();
    antlr4::tree::TerminalNode *UNIQUE();
    antlr4::tree::TerminalNode *LOCKS();

   
  };

  Savepoint_statementContext* savepoint_statement();

  class  Release_savepoint_statementContext : public antlr4::ParserRuleContext {
  public:
    Release_savepoint_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RELEASE();
    antlr4::tree::TerminalNode *SAVEPOINT();
    Savepoint_nameContext *savepoint_name();
    antlr4::tree::TerminalNode *TO();

   
  };

  Release_savepoint_statementContext* release_savepoint_statement();

  class  Allocate_cursor_statementContext : public antlr4::ParserRuleContext {
  public:
    Allocate_cursor_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALLOCATE();
    Cursor_nameContext *cursor_name();
    antlr4::tree::TerminalNode *CURSOR();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *RESULT();
    antlr4::tree::TerminalNode *SET();
    Rs_locator_variableContext *rs_locator_variable();

   
  };

  Allocate_cursor_statementContext* allocate_cursor_statement();

  class  Alter_audit_policy_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_audit_policy_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *AUDIT();
    antlr4::tree::TerminalNode *POLICY();
    Policy_nameContext *policy_name();
    antlr4::tree::TerminalNode *CATEGORIES();
    std::vector<Audit_policy_categories_optsContext *> audit_policy_categories_opts();
    Audit_policy_categories_optsContext* audit_policy_categories_opts(size_t i);
    antlr4::tree::TerminalNode *ERROR();
    antlr4::tree::TerminalNode *TYPE();
    Normal_auditContext *normal_audit();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Alter_audit_policy_statementContext* alter_audit_policy_statement();

  class  Status_specContext : public antlr4::ParserRuleContext {
  public:
    Status_specContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *STATUS();
    antlr4::tree::TerminalNode *BOTH();
    antlr4::tree::TerminalNode *FAILURE();
    antlr4::tree::TerminalNode *NONE();
    antlr4::tree::TerminalNode *SUCCESS();

   
  };

  Status_specContext* status_spec();

  class  Normal_auditContext : public antlr4::ParserRuleContext {
  public:
    Normal_auditContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NORMAL();
    antlr4::tree::TerminalNode *AUDIT();

   
  };

  Normal_auditContext* normal_audit();

  class  Alter_bufferpool_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_bufferpool_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *BUFFERPOOL();
    Bufferpool_nameContext *bufferpool_name();
    antlr4::tree::TerminalNode *SIZE();
    antlr4::tree::TerminalNode *ADD();
    antlr4::tree::TerminalNode *DATABASE();
    antlr4::tree::TerminalNode *PARTITION();
    antlr4::tree::TerminalNode *GROUP();
    Db_partition_group_nameContext *db_partition_group_name();
    antlr4::tree::TerminalNode *NUMBLOCKPAGES();
    std::vector<Number_of_pagesContext *> number_of_pages();
    Number_of_pagesContext* number_of_pages(size_t i);
    antlr4::tree::TerminalNode *BLOCKSIZE();
    antlr4::tree::TerminalNode *AUTOMATIC();
    Immediate_deferredContext *immediate_deferred();
    antlr4::tree::TerminalNode *MEMBER();
    Member_numberContext *member_number();

   
  };

  Alter_bufferpool_statementContext* alter_bufferpool_statement();

  class  Immediate_deferredContext : public antlr4::ParserRuleContext {
  public:
    Immediate_deferredContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *IMMEDIATE();
    antlr4::tree::TerminalNode *DEFERRED();

   
  };

  Immediate_deferredContext* immediate_deferred();

  class  Alter_database_partition_group_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_database_partition_group_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *DATABASE();
    antlr4::tree::TerminalNode *PARTITION();
    antlr4::tree::TerminalNode *GROUP();
    Db_partition_nameContext *db_partition_name();
    std::vector<Db_partition_group_list_itemContext *> db_partition_group_list_item();
    Db_partition_group_list_itemContext* db_partition_group_list_item(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Alter_database_partition_group_statementContext* alter_database_partition_group_statement();

  class  Db_partition_group_list_itemContext : public antlr4::ParserRuleContext {
  public:
    Db_partition_group_list_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ADD();
    Db_partition_num_numsContext *db_partition_num_nums();
    Db_partitions_clauseContext *db_partitions_clause();
    Db_partition_optionsContext *db_partition_options();
    antlr4::tree::TerminalNode *DROP();

   
  };

  Db_partition_group_list_itemContext* db_partition_group_list_item();

  class  Db_partition_num_numsContext : public antlr4::ParserRuleContext {
  public:
    Db_partition_num_numsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DBPARTITIONNUM();
    antlr4::tree::TerminalNode *DBPARTITIONNUMS();

   
  };

  Db_partition_num_numsContext* db_partition_num_nums();

  class  Db_partitions_clauseContext : public antlr4::ParserRuleContext {
  public:
    Db_partitions_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Db_partition_number_listContext *db_partition_number_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();

   
  };

  Db_partitions_clauseContext* db_partitions_clause();

  class  Db_partition_optionsContext : public antlr4::ParserRuleContext {
  public:
    Db_partition_optionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LIKE();
    antlr4::tree::TerminalNode *DBPARTITIONNUM();
    Db_partition_numberContext *db_partition_number();
    antlr4::tree::TerminalNode *WITHOUT();
    antlr4::tree::TerminalNode *TABLESPACES();

   
  };

  Db_partition_optionsContext* db_partition_options();

  class  Alter_database_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_database_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *DATABASE();
    Database_nameContext *database_name();
    std::vector<Alter_database_optsContext *> alter_database_opts();
    Alter_database_optsContext* alter_database_opts(size_t i);

   
  };

  Alter_database_statementContext* alter_database_statement();

  class  Alter_database_optsContext : public antlr4::ParserRuleContext {
  public:
    Db2Parser::String_listContext *sp = nullptr;
    Alter_database_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *STORAGE();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *ADD();
    antlr4::tree::TerminalNode *DROP();
    String_listContext *string_list();

   
  };

  Alter_database_optsContext* alter_database_opts();

  class  Alter_event_monitor_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_event_monitor_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *EVENT();
    antlr4::tree::TerminalNode *MONITOR();
    Event_monitor_nameContext *event_monitor_name();
    std::vector<Alter_event_monitor_optsContext *> alter_event_monitor_opts();
    Alter_event_monitor_optsContext* alter_event_monitor_opts(size_t i);

   
  };

  Alter_event_monitor_statementContext* alter_event_monitor_statement();

  class  Alter_event_monitor_optsContext : public antlr4::ParserRuleContext {
  public:
    Alter_event_monitor_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ADD();
    antlr4::tree::TerminalNode *LOGICAL();
    antlr4::tree::TerminalNode *GROUP();
    Evm_groupContext *evm_group();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    std::vector<Target_table_optionsContext *> target_table_options();
    Target_table_optionsContext* target_table_options(size_t i);

   
  };

  Alter_event_monitor_optsContext* alter_event_monitor_opts();

  class  Alter_function_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_function_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    Function_designatorContext *function_designator();
    std::vector<Alter_function_optsContext *> alter_function_opts();
    Alter_function_optsContext* alter_function_opts(size_t i);

   
  };

  Alter_function_statementContext* alter_function_statement();

  class  Alter_function_optsContext : public antlr4::ParserRuleContext {
  public:
    Alter_function_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *EXTERNAL();
    antlr4::tree::TerminalNode *NAME();
    StringContext *string();
    Id_Context *id_();
    antlr4::tree::TerminalNode *FENCED();
    antlr4::tree::TerminalNode *SECURED();
    antlr4::tree::TerminalNode *THREADSAFE();
    antlr4::tree::TerminalNode *NOT();

   
  };

  Alter_function_optsContext* alter_function_opts();

  class  Function_designatorContext : public antlr4::ParserRuleContext {
  public:
    Function_designatorContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FUNCTION();
    Function_nameContext *function_name();
    Designator_data_type_list_parenContext *designator_data_type_list_paren();
    antlr4::tree::TerminalNode *SPECIFIC();
    Specific_nameContext *specific_name();

   
  };

  Function_designatorContext* function_designator();

  class  Data_type_listContext : public antlr4::ParserRuleContext {
  public:
    Data_type_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Data_typeContext *> data_type();
    Data_typeContext* data_type(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Data_type_listContext* data_type_list();

  class  Data_type_list_parenContext : public antlr4::ParserRuleContext {
  public:
    Data_type_list_parenContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    Data_type_listContext *data_type_list();

   
  };

  Data_type_list_parenContext* data_type_list_paren();

  class  Alter_histogram_template_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_histogram_template_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *HISTOGRAM();
    antlr4::tree::TerminalNode *TEMPLATE();
    Template_nameContext *template_name();
    antlr4::tree::TerminalNode *HIGH();
    antlr4::tree::TerminalNode *BIN();
    antlr4::tree::TerminalNode *VALUE();
    Bigint_constantContext *bigint_constant();

   
  };

  Alter_histogram_template_statementContext* alter_histogram_template_statement();

  class  Alter_index_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_index_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *INDEX();
    Index_nameContext *index_name();
    antlr4::tree::TerminalNode *COMPRESS();
    Yes_noContext *yes_no();

   
  };

  Alter_index_statementContext* alter_index_statement();

  class  Yes_noContext : public antlr4::ParserRuleContext {
  public:
    Yes_noContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *YES();
    antlr4::tree::TerminalNode *NO();

   
  };

  Yes_noContext* yes_no();

  class  Alter_mask_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_mask_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *MASK();
    Mask_nameContext *mask_name();
    Enable_disableContext *enable_disable();

   
  };

  Alter_mask_statementContext* alter_mask_statement();

  class  Enable_disableContext : public antlr4::ParserRuleContext {
  public:
    Enable_disableContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ENABLE();
    antlr4::tree::TerminalNode *DISABLE();

   
  };

  Enable_disableContext* enable_disable();

  class  Alter_method_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_method_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    Method_designatorContext *method_designator();
    antlr4::tree::TerminalNode *EXTERNAL();
    antlr4::tree::TerminalNode *NAME();
    StringContext *string();
    Id_Context *id_();

   
  };

  Alter_method_statementContext* alter_method_statement();

  class  Method_designatorContext : public antlr4::ParserRuleContext {
  public:
    Method_designatorContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *METHOD();
    Method_nameContext *method_name();
    antlr4::tree::TerminalNode *FOR();
    Type_nameContext *type_name();
    Designator_data_type_list_parenContext *designator_data_type_list_paren();
    antlr4::tree::TerminalNode *SPECIFIC();
    Specific_nameContext *specific_name();

   
  };

  Method_designatorContext* method_designator();

  class  Alter_model_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_model_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *MODEL();
    Model_nameContext *model_name();
    antlr4::tree::TerminalNode *ON();
    Table_nameContext *table_name();
    Enable_disableContext *enable_disable();
    antlr4::tree::TerminalNode *REVERT();

   
  };

  Alter_model_statementContext* alter_model_statement();

  class  Alter_module_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_module_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *MODULE();
    Module_nameContext *module_name();
    antlr4::tree::TerminalNode *ADD();
    Alter_module_optsContext *alter_module_opts();
    antlr4::tree::TerminalNode *DROP();
    antlr4::tree::TerminalNode *PUBLISH();
    antlr4::tree::TerminalNode *BODY();
    Module_object_identificationContext *module_object_identification();

   
  };

  Alter_module_statementContext* alter_module_statement();

  class  Alter_module_optsContext : public antlr4::ParserRuleContext {
  public:
    Alter_module_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Module_condition_definitionContext *module_condition_definition();
    Module_function_definitionContext *module_function_definition();
    Module_procedure_definitionContext *module_procedure_definition();
    Module_type_definitionContext *module_type_definition();
    Module_variable_definitionContext *module_variable_definition();

   
  };

  Alter_module_optsContext* alter_module_opts();

  class  Module_function_definitionContext : public antlr4::ParserRuleContext {
  public:
    Module_function_definitionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FUNCTION();
    Unqualified_function_nameContext *unqualified_function_name();
    Data_type_list_parenContext *data_type_list_paren();
    antlr4::tree::TerminalNode *SPECIFIC();
    Unqualified_specific_nameContext *unqualified_specific_name();

   
  };

  Module_function_definitionContext* module_function_definition();

  class  Module_procedure_definitionContext : public antlr4::ParserRuleContext {
  public:
    Module_procedure_definitionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PROCEDURE();
    Unqualified_procedure_nameContext *unqualified_procedure_name();
    Data_type_list_parenContext *data_type_list_paren();
    antlr4::tree::TerminalNode *SPECIFIC();
    Unqualified_specific_nameContext *unqualified_specific_name();

   
  };

  Module_procedure_definitionContext* module_procedure_definition();

  class  Module_type_definitionContext : public antlr4::ParserRuleContext {
  public:
    Module_type_definitionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<antlr4::tree::TerminalNode *> TYPE();
    antlr4::tree::TerminalNode* TYPE(size_t i);
    Unqualified_type_nameContext *unqualified_type_name();
    antlr4::tree::TerminalNode *AS();
    Data_typeContext *data_type();
    antlr4::tree::TerminalNode *ARRAY();
    antlr4::tree::TerminalNode *LEFT_SQR_BKT();
    antlr4::tree::TerminalNode *RIGHT_SQR_BKT();
    Or_replaceContext *or_replace();
    Integer_constantContext *integer_constant();
    Data_type_2Context *data_type_2();
    antlr4::tree::TerminalNode *CURSOR();
    Anchored_row_data_typeContext *anchored_row_data_type();
    Row_type_nameContext *row_type_name();
    Source_data_typeContext *source_data_type();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *STRONG();
    antlr4::tree::TerminalNode *RULES();
    antlr4::tree::TerminalNode *WEAK();
    Data_type_constrainstContext *data_type_constrainst();
    antlr4::tree::TerminalNode *ROW();
    Field_definition_list_parenContext *field_definition_list_paren();

   
  };

  Module_type_definitionContext* module_type_definition();

  class  Module_variable_definitionContext : public antlr4::ParserRuleContext {
  public:
    Module_variable_definitionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *VARIABLE();
    Variable_nameContext *variable_name();
    Data_type_1Context *data_type_1();
    Or_replaceContext *or_replace();
    antlr4::tree::TerminalNode *DEFAULT();
    antlr4::tree::TerminalNode *CONSTANT();
    antlr4::tree::TerminalNode *NULL_();
    Constant_Context *constant_();
    Special_registerContext *special_register();
    Global_variableContext *global_variable();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Cursor_value_constructorContext *cursor_value_constructor();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    ExpressionContext *expression();

   
  };

  Module_variable_definitionContext* module_variable_definition();

  class  Module_condition_definitionContext : public antlr4::ParserRuleContext {
  public:
    Module_condition_definitionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CONDITION();
    Condition_nameContext *condition_name();
    antlr4::tree::TerminalNode *FOR();
    String_constantContext *string_constant();
    antlr4::tree::TerminalNode *SQLSTATE();
    antlr4::tree::TerminalNode *VALUE();

   
  };

  Module_condition_definitionContext* module_condition_definition();

  class  Module_object_identificationContext : public antlr4::ParserRuleContext {
  public:
    Module_object_identificationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Module_function_designatorContext *module_function_designator();
    Module_procedure_designatorContext *module_procedure_designator();
    antlr4::tree::TerminalNode *CONDITION();
    Condition_nameContext *condition_name();
    antlr4::tree::TerminalNode *TYPE();
    Unqualified_type_nameContext *unqualified_type_name();
    antlr4::tree::TerminalNode *VARIABLE();
    Variable_nameContext *variable_name();

   
  };

  Module_object_identificationContext* module_object_identification();

  class  Module_function_designatorContext : public antlr4::ParserRuleContext {
  public:
    Module_function_designatorContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FUNCTION();
    Unqualified_function_nameContext *unqualified_function_name();
    Designator_data_type_list_parenContext *designator_data_type_list_paren();
    antlr4::tree::TerminalNode *SPECIFIC();
    Unqualified_specific_nameContext *unqualified_specific_name();

   
  };

  Module_function_designatorContext* module_function_designator();

  class  Module_procedure_designatorContext : public antlr4::ParserRuleContext {
  public:
    Module_procedure_designatorContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PROCEDURE();
    Unqualified_procedure_nameContext *unqualified_procedure_name();
    Designator_data_type_list_parenContext *designator_data_type_list_paren();
    antlr4::tree::TerminalNode *SPECIFIC();
    Unqualified_specific_nameContext *unqualified_specific_name();

   
  };

  Module_procedure_designatorContext* module_procedure_designator();

  class  Alter_nickname_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_nickname_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *NICKNAME();
    Nick_nameContext *nick_name();
    Alter_nickname_opts_1Context *alter_nickname_opts_1();
    std::vector<Alter_nickname_opts_2Context *> alter_nickname_opts_2();
    Alter_nickname_opts_2Context* alter_nickname_opts_2(size_t i);

   
  };

  Alter_nickname_statementContext* alter_nickname_statement();

  class  Alter_nickname_opts_1Context : public antlr4::ParserRuleContext {
  public:
    Alter_nickname_opts_1Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OPTIONS();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<Alter_nickname_opts_1_itemContext *> alter_nickname_opts_1_item();
    Alter_nickname_opts_1_itemContext* alter_nickname_opts_1_item(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Alter_nickname_opts_1Context* alter_nickname_opts_1();

  class  Alter_nickname_opts_1_itemContext : public antlr4::ParserRuleContext {
  public:
    Alter_nickname_opts_1_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Nick_name_option_nameContext *nick_name_option_name();
    String_constantContext *string_constant();
    Add_setContext *add_set();
    antlr4::tree::TerminalNode *DROP();

   
  };

  Alter_nickname_opts_1_itemContext* alter_nickname_opts_1_item();

  class  Alter_nickname_opts_2Context : public antlr4::ParserRuleContext {
  public:
    Alter_nickname_opts_2Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    Column_nameContext *column_name();
    std::vector<Alter_nickname_opts_2_itemContext *> alter_nickname_opts_2_item();
    Alter_nickname_opts_2_itemContext* alter_nickname_opts_2_item(size_t i);
    antlr4::tree::TerminalNode *COLUMN();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);
    antlr4::tree::TerminalNode *ADD();
    Unique_constraintContext *unique_constraint();
    Referential_constraintContext *referential_constraint();
    Check_constraintContext *check_constraint();
    Constraint_nameContext *constraint_name();
    antlr4::tree::TerminalNode *FOREIGN();
    antlr4::tree::TerminalNode *KEY();
    antlr4::tree::TerminalNode *CHECK();
    std::vector<Constraint_alterationContext *> constraint_alteration();
    Constraint_alterationContext* constraint_alteration(size_t i);
    antlr4::tree::TerminalNode *DROP();
    antlr4::tree::TerminalNode *PRIMARY();
    antlr4::tree::TerminalNode *UNIQUE();
    antlr4::tree::TerminalNode *CONSTRAINT();
    Allow_disallowContext *allow_disallow();
    antlr4::tree::TerminalNode *CACHING();

   
  };

  Alter_nickname_opts_2Context* alter_nickname_opts_2();

  class  Alter_nickname_opts_2_itemContext : public antlr4::ParserRuleContext {
  public:
    Alter_nickname_opts_2_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LOCAL();
    antlr4::tree::TerminalNode *NAME();
    Column_nameContext *column_name();
    antlr4::tree::TerminalNode *TYPE();
    Local_data_typeContext *local_data_type();
    Federated_column_optionsContext *federated_column_options();

   
  };

  Alter_nickname_opts_2_itemContext* alter_nickname_opts_2_item();

  class  Constraint_alterationContext : public antlr4::ParserRuleContext {
  public:
    Constraint_alterationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Enable_disableContext *enable_disable();
    antlr4::tree::TerminalNode *QUERY();
    antlr4::tree::TerminalNode *OPTIMIZATION();
    std::vector<antlr4::tree::TerminalNode *> NOT();
    antlr4::tree::TerminalNode* NOT(size_t i);
    antlr4::tree::TerminalNode *ENFORCED();
    antlr4::tree::TerminalNode *TRUSTED();

   
  };

  Constraint_alterationContext* constraint_alteration();

  class  Alter_package_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_package_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *PACKAGE();
    Package_nameContext *package_name();
    Version_idContext *version_id();
    std::vector<Alter_package_optsContext *> alter_package_opts();
    Alter_package_optsContext* alter_package_opts(size_t i);
    antlr4::tree::TerminalNode *VERSION();

   
  };

  Alter_package_statementContext* alter_package_statement();

  class  Alter_package_optsContext : public antlr4::ParserRuleContext {
  public:
    Alter_package_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ACCESS();
    antlr4::tree::TerminalNode *PLAN();
    antlr4::tree::TerminalNode *REUSE();
    Yes_noContext *yes_no();
    antlr4::tree::TerminalNode *OPTIMIZE();
    antlr4::tree::TerminalNode *PROFILE();
    antlr4::tree::TerminalNode *NONE();
    Optimization_profile_nameContext *optimization_profile_name();
    antlr4::tree::TerminalNode *KEEP();
    antlr4::tree::TerminalNode *DYNAMIC();

   
  };

  Alter_package_optsContext* alter_package_opts();

  class  Alter_permission_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_permission_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *PERMISSION();
    Permission_nameContext *permission_name();
    Enable_disableContext *enable_disable();

   
  };

  Alter_permission_statementContext* alter_permission_statement();

  class  Alter_procedure_external_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_procedure_external_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    Procedure_designatorContext *procedure_designator();
    std::vector<Alter_procedure_external_optsContext *> alter_procedure_external_opts();
    Alter_procedure_external_optsContext* alter_procedure_external_opts(size_t i);

   
  };

  Alter_procedure_external_statementContext* alter_procedure_external_statement();

  class  Alter_procedure_external_optsContext : public antlr4::ParserRuleContext {
  public:
    Alter_procedure_external_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *EXTERNAL();
    antlr4::tree::TerminalNode *NAME();
    StringContext *string();
    Id_Context *id_();
    antlr4::tree::TerminalNode *FENCED();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *ACTION();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *THREADSAFE();
    antlr4::tree::TerminalNode *NEW();
    antlr4::tree::TerminalNode *SAVEPOINT();
    antlr4::tree::TerminalNode *LEVEL();

   
  };

  Alter_procedure_external_optsContext* alter_procedure_external_opts();

  class  Procedure_designatorContext : public antlr4::ParserRuleContext {
  public:
    Procedure_designatorContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PROCEDURE();
    Procedure_nameContext *procedure_name();
    Designator_data_type_list_parenContext *designator_data_type_list_paren();
    antlr4::tree::TerminalNode *SPECIFIC();
    Specific_nameContext *specific_name();

   
  };

  Procedure_designatorContext* procedure_designator();

  class  Alter_procedure_sourced_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_procedure_sourced_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<antlr4::tree::TerminalNode *> ALTER();
    antlr4::tree::TerminalNode* ALTER(size_t i);
    Procedure_designatorContext *procedure_designator();
    std::vector<antlr4::tree::TerminalNode *> PARAMETER();
    antlr4::tree::TerminalNode* PARAMETER(size_t i);
    std::vector<Parameter_alterationContext *> parameter_alteration();
    Parameter_alterationContext* parameter_alteration(size_t i);

   
  };

  Alter_procedure_sourced_statementContext* alter_procedure_sourced_statement();

  class  Parameter_alterationContext : public antlr4::ParserRuleContext {
  public:
    Parameter_alterationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Parameter_nameContext *parameter_name();
    antlr4::tree::TerminalNode *SET();
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *TYPE();
    Data_typeContext *data_type();

   
  };

  Parameter_alterationContext* parameter_alteration();

  class  Alter_procedure_sql_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_procedure_sql_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    Procedure_designatorContext *procedure_designator();
    std::vector<antlr4::tree::TerminalNode *> EXTERNAL();
    antlr4::tree::TerminalNode* EXTERNAL(size_t i);
    std::vector<antlr4::tree::TerminalNode *> ACTION();
    antlr4::tree::TerminalNode* ACTION(size_t i);
    std::vector<antlr4::tree::TerminalNode *> NEW();
    antlr4::tree::TerminalNode* NEW(size_t i);
    std::vector<antlr4::tree::TerminalNode *> SAVEPOINT();
    antlr4::tree::TerminalNode* SAVEPOINT(size_t i);
    std::vector<antlr4::tree::TerminalNode *> LEVEL();
    antlr4::tree::TerminalNode* LEVEL(size_t i);
    std::vector<antlr4::tree::TerminalNode *> NO();
    antlr4::tree::TerminalNode* NO(size_t i);

   
  };

  Alter_procedure_sql_statementContext* alter_procedure_sql_statement();

  class  Alter_schema_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_schema_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *SCHEMA();
    Schema_nameContext *schema_name();
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *CAPTURE();
    None_changesContext *none_changes();
    Enable_disableContext *enable_disable();
    antlr4::tree::TerminalNode *ROW();
    antlr4::tree::TerminalNode *MODIFICATION();
    antlr4::tree::TerminalNode *TRACKING();

   
  };

  Alter_schema_statementContext* alter_schema_statement();

  class  None_changesContext : public antlr4::ParserRuleContext {
  public:
    None_changesContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NONE();
    antlr4::tree::TerminalNode *CHANGES();

   
  };

  None_changesContext* none_changes();

  class  Alter_security_label_component_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_security_label_component_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *SECURITY();
    antlr4::tree::TerminalNode *LABEL();
    antlr4::tree::TerminalNode *COMPONENT();
    Component_nameContext *component_name();
    Add_element_clauseContext *add_element_clause();

   
  };

  Alter_security_label_component_statementContext* alter_security_label_component_statement();

  class  Add_element_clauseContext : public antlr4::ParserRuleContext {
  public:
    Add_element_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ADD();
    antlr4::tree::TerminalNode *ELEMENT();
    String_constantContext *string_constant();
    Array_element_clauseContext *array_element_clause();
    Tree_element_clauseContext *tree_element_clause();

   
  };

  Add_element_clauseContext* add_element_clause();

  class  Array_element_clauseContext : public antlr4::ParserRuleContext {
  public:
    Array_element_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    String_constantContext *string_constant();
    antlr4::tree::TerminalNode *BEFORE();
    antlr4::tree::TerminalNode *AFTER();

   
  };

  Array_element_clauseContext* array_element_clause();

  class  Tree_element_clauseContext : public antlr4::ParserRuleContext {
  public:
    Tree_element_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ROOT();
    antlr4::tree::TerminalNode *UNDER();
    std::vector<String_constantContext *> string_constant();
    String_constantContext* string_constant(size_t i);
    std::vector<antlr4::tree::TerminalNode *> OVER();
    antlr4::tree::TerminalNode* OVER(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Tree_element_clauseContext* tree_element_clause();

  class  Alter_security_policy_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_security_policy_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *SECURITY();
    antlr4::tree::TerminalNode *POLICY();
    Security_policy_nameContext *security_policy_name();
    std::vector<Alter_security_policy_optsContext *> alter_security_policy_opts();
    Alter_security_policy_optsContext* alter_security_policy_opts(size_t i);

   
  };

  Alter_security_policy_statementContext* alter_security_policy_statement();

  class  Alter_security_policy_optsContext : public antlr4::ParserRuleContext {
  public:
    Alter_security_policy_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ADD();
    antlr4::tree::TerminalNode *SECURITY();
    antlr4::tree::TerminalNode *LABEL();
    antlr4::tree::TerminalNode *COMPONENT();
    Component_nameContext *component_name();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *AUTHORIZED();
    antlr4::tree::TerminalNode *WRITE();
    antlr4::tree::TerminalNode *OVERRIDE_();
    antlr4::tree::TerminalNode *RESTRICT();
    antlr4::tree::TerminalNode *AUTHORIZATIONS();
    antlr4::tree::TerminalNode *USE();
    antlr4::tree::TerminalNode *IGNORE();
    antlr4::tree::TerminalNode *GROUP();
    antlr4::tree::TerminalNode *ROLE();

   
  };

  Alter_security_policy_optsContext* alter_security_policy_opts();

  class  Alter_sequence_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_sequence_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *SEQUENCE();
    Sequence_nameContext *sequence_name();
    std::vector<Alter_sequence_optsContext *> alter_sequence_opts();
    Alter_sequence_optsContext* alter_sequence_opts(size_t i);

   
  };

  Alter_sequence_statementContext* alter_sequence_statement();

  class  Alter_sequence_optsContext : public antlr4::ParserRuleContext {
  public:
    Alter_sequence_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RESTART();
    antlr4::tree::TerminalNode *WITH();
    Numeric_constantContext *numeric_constant();
    antlr4::tree::TerminalNode *INCREMENT();
    antlr4::tree::TerminalNode *BY();
    antlr4::tree::TerminalNode *MINVALUE();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *MAXVALUE();
    antlr4::tree::TerminalNode *CYCLE();
    antlr4::tree::TerminalNode *CACHE();
    Integer_constantContext *integer_constant();
    antlr4::tree::TerminalNode *ORDER();

   
  };

  Alter_sequence_optsContext* alter_sequence_opts();

  class  Alter_server_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_server_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *SERVER();
    Server_nameContext *server_name();
    antlr4::tree::TerminalNode *TYPE();
    Server_typeContext *server_type();
    antlr4::tree::TerminalNode *OPTIONS();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<Alter_server_optsContext *> alter_server_opts();
    Alter_server_optsContext* alter_server_opts(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *VERSION();
    Server_versionContext *server_version();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);
    antlr4::tree::TerminalNode *WRAPPER();
    Wrapper_nameContext *wrapper_name();

   
  };

  Alter_server_statementContext* alter_server_statement();

  class  Alter_server_optsContext : public antlr4::ParserRuleContext {
  public:
    Alter_server_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Server_option_nameContext *server_option_name();
    String_constantContext *string_constant();
    Add_setContext *add_set();
    antlr4::tree::TerminalNode *DROP();

   
  };

  Alter_server_optsContext* alter_server_opts();

  class  Alter_service_class_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_service_class_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *SERVICE();
    antlr4::tree::TerminalNode *CLASS_();
    Service_class_nameContext *service_class_name();
    antlr4::tree::TerminalNode *UNDER();
    Service_superclass_nameContext *service_superclass_name();
    std::vector<Alter_service_class_optsContext *> alter_service_class_opts();
    Alter_service_class_optsContext* alter_service_class_opts(size_t i);

   
  };

  Alter_service_class_statementContext* alter_service_class_statement();

  class  Alter_service_class_optsContext : public antlr4::ParserRuleContext {
  public:
    Alter_service_class_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RESOURCE();
    antlr4::tree::TerminalNode *SHARES();
    Integer_constantContext *integer_constant();
    Soft_hardContext *soft_hard();
    antlr4::tree::TerminalNode *CPU();
    antlr4::tree::TerminalNode *LIMIT();
    antlr4::tree::TerminalNode *NONE();
    antlr4::tree::TerminalNode *ACTIVITY();
    antlr4::tree::TerminalNode *SORTMEM();
    antlr4::tree::TerminalNode *MINIMUM();
    antlr4::tree::TerminalNode *SHARE();
    antlr4::tree::TerminalNode *PERCENT();
    antlr4::tree::TerminalNode *ADMISSION();
    antlr4::tree::TerminalNode *QUEUE();
    antlr4::tree::TerminalNode *ORDER();
    antlr4::tree::TerminalNode *FIFO();
    antlr4::tree::TerminalNode *LATENCY();
    antlr4::tree::TerminalNode *DEGREE_();
    antlr4::tree::TerminalNode *SCALEBACK();
    Default_on_offContext *default_on_off();
    antlr4::tree::TerminalNode *MAXIMUM();
    antlr4::tree::TerminalNode *PREFETCH();
    antlr4::tree::TerminalNode *PRIORITY();
    Default_high_medium_lowContext *default_high_medium_low();
    antlr4::tree::TerminalNode *OUTBOUND();
    antlr4::tree::TerminalNode *CORRELATOR();
    String_constantContext *string_constant();
    antlr4::tree::TerminalNode *BUFFERPOOL();
    antlr4::tree::TerminalNode *COLLECT();
    antlr4::tree::TerminalNode *AGGREGATE();
    antlr4::tree::TerminalNode *DATA();
    Extended_base_noneContext *extended_base_none();
    antlr4::tree::TerminalNode *REQUEST();
    Base_noneContext *base_none();
    antlr4::tree::TerminalNode *UNIT();
    antlr4::tree::TerminalNode *OF();
    antlr4::tree::TerminalNode *WORK();
    antlr4::tree::TerminalNode *METRICS();
    antlr4::tree::TerminalNode *HISTOGRAM();
    antlr4::tree::TerminalNode *TEMPLATE();
    Template_nameContext *template_name();
    antlr4::tree::TerminalNode *LIFETIME();
    antlr4::tree::TerminalNode *QUEUETIME();
    antlr4::tree::TerminalNode *ESTIMATEDCOST();
    antlr4::tree::TerminalNode *EXECUTETIME();
    antlr4::tree::TerminalNode *INTERARRIVALTIME();
    antlr4::tree::TerminalNode *UOW();
    Enable_disableContext *enable_disable();

   
  };

  Alter_service_class_optsContext* alter_service_class_opts();

  class  Default_on_offContext : public antlr4::ParserRuleContext {
  public:
    Default_on_offContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DEFAULT();
    On_offContext *on_off();

   
  };

  Default_on_offContext* default_on_off();

  class  Default_high_medium_lowContext : public antlr4::ParserRuleContext {
  public:
    Default_high_medium_lowContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DEFAULT();
    High_medium_lowContext *high_medium_low();

   
  };

  Default_high_medium_lowContext* default_high_medium_low();

  class  Alter_stogroup_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_stogroup_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *STOGROUP();
    Storagegroup_nameContext *storagegroup_name();
    std::vector<Alter_stogroup_optsContext *> alter_stogroup_opts();
    Alter_stogroup_optsContext* alter_stogroup_opts(size_t i);

   
  };

  Alter_stogroup_statementContext* alter_stogroup_statement();

  class  Alter_stogroup_optsContext : public antlr4::ParserRuleContext {
  public:
    Alter_stogroup_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    String_listContext *string_list();
    antlr4::tree::TerminalNode *ADD();
    antlr4::tree::TerminalNode *DROP();
    antlr4::tree::TerminalNode *OVERHEAD();
    Number_of_millisecondsContext *number_of_milliseconds();
    antlr4::tree::TerminalNode *DEVICE();
    antlr4::tree::TerminalNode *READ();
    antlr4::tree::TerminalNode *RATE();
    Number_megabytes_per_secondContext *number_megabytes_per_second();
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *TAG();
    Integer_constantContext *integer_constant();
    antlr4::tree::TerminalNode *NONE();
    antlr4::tree::TerminalNode *SET();
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *DEFAULT();

   
  };

  Alter_stogroup_optsContext* alter_stogroup_opts();

  class  Alter_table_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_table_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    std::vector<antlr4::tree::TerminalNode *> TABLE();
    antlr4::tree::TerminalNode* TABLE(size_t i);
    std::vector<Table_nameContext *> table_name();
    Table_nameContext* table_name(size_t i);
    antlr4::tree::TerminalNode *ADD();
    antlr4::tree::TerminalNode *PARTITION();
    Add_partitionContext *add_partition();
    antlr4::tree::TerminalNode *ATTACH();
    Attach_partitionContext *attach_partition();
    antlr4::tree::TerminalNode *DETACH();
    Partition_nameContext *partition_name();
    antlr4::tree::TerminalNode *INTO();
    antlr4::tree::TerminalNode *SECURITY();
    antlr4::tree::TerminalNode *POLICY();
    Policy_nameContext *policy_name();
    antlr4::tree::TerminalNode *DROP();
    antlr4::tree::TerminalNode *VERSIONING();
    antlr4::tree::TerminalNode *USE();
    antlr4::tree::TerminalNode *HISTORY();
    History_table_nameContext *history_table_name();
    std::vector<Alter_table_optsContext *> alter_table_opts();
    Alter_table_optsContext* alter_table_opts(size_t i);

   
  };

  Alter_table_statementContext* alter_table_statement();

  class  Alter_table_optsContext : public antlr4::ParserRuleContext {
  public:
    Db2Parser::Column_nameContext *s = nullptr;
    Db2Parser::Column_nameContext *t = nullptr;
    Alter_table_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ADD();
    Column_definitionContext *column_definition();
    Unique_constraintContext *unique_constraint();
    Referential_constraintContext *referential_constraint();
    Check_constraintContext *check_constraint();
    Distribution_clauseContext *distribution_clause();
    antlr4::tree::TerminalNode *RESTRICT();
    antlr4::tree::TerminalNode *ON();
    std::vector<antlr4::tree::TerminalNode *> DROP();
    antlr4::tree::TerminalNode* DROP(size_t i);
    antlr4::tree::TerminalNode *COLUMN();
    Materialized_query_definitionContext *materialized_query_definition();
    antlr4::tree::TerminalNode *QUERY();
    antlr4::tree::TerminalNode *MATERIALIZED();
    antlr4::tree::TerminalNode *ALTER();
    Constraint_nameContext *constraint_name();
    Constraint_alterationContext *constraint_alteration();
    antlr4::tree::TerminalNode *FOREIGN();
    antlr4::tree::TerminalNode *KEY();
    antlr4::tree::TerminalNode *CHECK();
    Column_alterationContext *column_alteration();
    Activate_deactivateContext *activate_deactivate();
    antlr4::tree::TerminalNode *ACCESS();
    antlr4::tree::TerminalNode *CONTROL();
    antlr4::tree::TerminalNode *ROW();
    antlr4::tree::TerminalNode *RENAME();
    antlr4::tree::TerminalNode *TO();
    std::vector<Column_nameContext *> column_name();
    Column_nameContext* column_name(size_t i);
    antlr4::tree::TerminalNode *PRIMARY();
    antlr4::tree::TerminalNode *UNIQUE();
    antlr4::tree::TerminalNode *CONSTRAINT();
    Cascade_restrictContext *cascade_restrict();
    antlr4::tree::TerminalNode *DISTRIBUTION();
    antlr4::tree::TerminalNode *PERIOD();
    Period_definition_alterContext *period_definition_alter();
    Period_nameContext *period_name();
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *CAPTURE();
    antlr4::tree::TerminalNode *NONE();
    antlr4::tree::TerminalNode *CHANGE();
    antlr4::tree::TerminalNode *INCLUDE();
    antlr4::tree::TerminalNode *LONGVAR();
    antlr4::tree::TerminalNode *COLUMNS();
    antlr4::tree::TerminalNode *ACTIVITY();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *LOGGED();
    antlr4::tree::TerminalNode *INITIALLY();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *EMPTY();
    antlr4::tree::TerminalNode *TABLE();
    antlr4::tree::TerminalNode *PCTFREE();
    Integer_valueContext *integer_value();
    antlr4::tree::TerminalNode *LOCKSIZE();
    antlr4::tree::TerminalNode *BLOCKINSERT();
    antlr4::tree::TerminalNode *APPEND();
    On_offContext *on_off();
    antlr4::tree::TerminalNode *VOLATILE();
    antlr4::tree::TerminalNode *CARDINALITY();
    antlr4::tree::TerminalNode *COMPRESS();
    antlr4::tree::TerminalNode *YES();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *ADAPTIVE();
    antlr4::tree::TerminalNode *STATIC();
    antlr4::tree::TerminalNode *VALUE();
    antlr4::tree::TerminalNode *COMPRESSION();
    antlr4::tree::TerminalNode *LOG();
    antlr4::tree::TerminalNode *INDEX();
    antlr4::tree::TerminalNode *BUILD();
    Null_on_offContext *null_on_off();

   
  };

  Alter_table_optsContext* alter_table_opts();

  class  Null_on_offContext : public antlr4::ParserRuleContext {
  public:
    Null_on_offContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NULL_();
    On_offContext *on_off();

   
  };

  Null_on_offContext* null_on_off();

  class  Cascade_restrictContext : public antlr4::ParserRuleContext {
  public:
    Cascade_restrictContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CASCADE();
    antlr4::tree::TerminalNode *RESTRICT();

   
  };

  Cascade_restrictContext* cascade_restrict();

  class  Materialized_query_definitionContext : public antlr4::ParserRuleContext {
  public:
    Materialized_query_definitionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    FullselectContext *fullselect();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    std::vector<Refreshable_table_optionsContext *> refreshable_table_options();
    Refreshable_table_optionsContext* refreshable_table_options(size_t i);

   
  };

  Materialized_query_definitionContext* materialized_query_definition();

  class  Refreshable_table_optionsContext : public antlr4::ParserRuleContext {
  public:
    Refreshable_table_optionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *INITIALLY();
    antlr4::tree::TerminalNode *DEFERRED();
    antlr4::tree::TerminalNode *REFRESH();
    antlr4::tree::TerminalNode *IMMEDIATE();
    Enable_disableContext *enable_disable();
    antlr4::tree::TerminalNode *QUERY();
    antlr4::tree::TerminalNode *OPTIMIZATION();
    antlr4::tree::TerminalNode *MAINTAINED();
    antlr4::tree::TerminalNode *BY();
    antlr4::tree::TerminalNode *USER();
    antlr4::tree::TerminalNode *REPLICATION();
    antlr4::tree::TerminalNode *FEDERATED_TOOL();
    antlr4::tree::TerminalNode *SYSTEM();

   
  };

  Refreshable_table_optionsContext* refreshable_table_options();

  class  Column_alterationContext : public antlr4::ParserRuleContext {
  public:
    Column_alterationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Column_nameContext *column_name();
    antlr4::tree::TerminalNode *SET();
    Generation_alterationContext *generation_alteration();
    Identity_alterationContext *identity_alteration();
    Generation_attributeContext *generation_attribute();
    As_identity_clauseContext *as_identity_clause();
    antlr4::tree::TerminalNode *GENERATE();
    antlr4::tree::TerminalNode *DROP();
    antlr4::tree::TerminalNode *ADD();
    antlr4::tree::TerminalNode *SCOPE();
    antlr4::tree::TerminalNode *COMPRESS();
    antlr4::tree::TerminalNode *SECURED();
    antlr4::tree::TerminalNode *WITH();
    Security_label_nameContext *security_label_name();
    antlr4::tree::TerminalNode *COLUMN();
    antlr4::tree::TerminalNode *SECURITY();
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *TYPE();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *NULL_();
    antlr4::tree::TerminalNode *INLINE();
    antlr4::tree::TerminalNode *LENGTH();
    Integer_valueContext *integer_value();
    Default_clauseContext *default_clause();
    antlr4::tree::TerminalNode *EXPRESSION();
    As_generated_expression_clauseContext *as_generated_expression_clause();
    antlr4::tree::TerminalNode *HIDDEN_();
    As_row_transaction_start_id_clauseContext *as_row_transaction_start_id_clause();
    As_row_transaction_timestamp_clauseContext *as_row_transaction_timestamp_clause();
    antlr4::tree::TerminalNode *DEFAULT();
    antlr4::tree::TerminalNode *GENERATED();
    Typed_table_nameContext *typed_table_name();
    Typed_view_nameContext *typed_view_name();
    antlr4::tree::TerminalNode *SYSTEM();
    antlr4::tree::TerminalNode *OFF();
    antlr4::tree::TerminalNode *IMPLICITLY();
    antlr4::tree::TerminalNode *ALWAYS();

   
  };

  Column_alterationContext* column_alteration();

  class  Generation_alterationContext : public antlr4::ParserRuleContext {
  public:
    Generation_alterationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SET();
    antlr4::tree::TerminalNode *GENERATED();
    antlr4::tree::TerminalNode *ALWAYS();
    antlr4::tree::TerminalNode *BY();
    antlr4::tree::TerminalNode *DEFAULT();

   
  };

  Generation_alterationContext* generation_alteration();

  class  Identity_alterationContext : public antlr4::ParserRuleContext {
  public:
    Identity_alterationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SET();
    antlr4::tree::TerminalNode *INCREMENT();
    antlr4::tree::TerminalNode *BY();
    Numeric_constantContext *numeric_constant();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *MINVALUE();
    antlr4::tree::TerminalNode *MAXVALUE();
    antlr4::tree::TerminalNode *CYCLE();
    antlr4::tree::TerminalNode *CACHE();
    Integer_constantContext *integer_constant();
    antlr4::tree::TerminalNode *ORDER();
    antlr4::tree::TerminalNode *RESTART();
    antlr4::tree::TerminalNode *WITH();

   
  };

  Identity_alterationContext* identity_alteration();

  class  Generation_attributeContext : public antlr4::ParserRuleContext {
  public:
    Generation_attributeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GENERATED();
    antlr4::tree::TerminalNode *ALWAYS();
    antlr4::tree::TerminalNode *BY();
    antlr4::tree::TerminalNode *DEFAULT();

   
  };

  Generation_attributeContext* generation_attribute();

  class  As_identity_clauseContext : public antlr4::ParserRuleContext {
  public:
    As_identity_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *IDENTITY();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    std::vector<As_identity_clause_optsContext *> as_identity_clause_opts();
    As_identity_clause_optsContext* as_identity_clause_opts(size_t i);

   
  };

  As_identity_clauseContext* as_identity_clause();

  class  As_identity_clause_optsContext : public antlr4::ParserRuleContext {
  public:
    As_identity_clause_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *START();
    antlr4::tree::TerminalNode *WITH();
    Numeric_constantContext *numeric_constant();
    antlr4::tree::TerminalNode *INCREMENT();
    antlr4::tree::TerminalNode *BY();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *MINVALUE();
    antlr4::tree::TerminalNode *MAXVALUE();
    antlr4::tree::TerminalNode *CYCLE();
    antlr4::tree::TerminalNode *CACHE();
    Integer_constantContext *integer_constant();

   
  };

  As_identity_clause_optsContext* as_identity_clause_opts();

  class  Period_definition_alterContext : public antlr4::ParserRuleContext {
  public:
    Db2Parser::Column_nameContext *b = nullptr;
    Db2Parser::Column_nameContext *e = nullptr;
    Period_definition_alterContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *COMMA();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *SYSTEM_TIME();
    antlr4::tree::TerminalNode *BUSINESS_TIME();
    std::vector<Column_nameContext *> column_name();
    Column_nameContext* column_name(size_t i);

   
  };

  Period_definition_alterContext* period_definition_alter();

  class  Add_partitionContext : public antlr4::ParserRuleContext {
  public:
    Add_partitionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Boundary_spec_alterContext *boundary_spec_alter();
    std::vector<antlr4::tree::TerminalNode *> IN();
    antlr4::tree::TerminalNode* IN(size_t i);
    std::vector<Tablespace_nameContext *> tablespace_name();
    Tablespace_nameContext* tablespace_name(size_t i);
    Partition_nameContext *partition_name();
    antlr4::tree::TerminalNode *INDEX();
    antlr4::tree::TerminalNode *LONG();

   
  };

  Add_partitionContext* add_partition();

  class  Boundary_spec_alterContext : public antlr4::ParserRuleContext {
  public:
    Boundary_spec_alterContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Starting_clauseContext *starting_clause();
    Ending_clauseContext *ending_clause();

   
  };

  Boundary_spec_alterContext* boundary_spec_alter();

  class  Attach_partitionContext : public antlr4::ParserRuleContext {
  public:
    Attach_partitionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Boundary_spec_alterContext *boundary_spec_alter();
    antlr4::tree::TerminalNode *FROM();
    Table_nameContext *table_name();
    Partition_nameContext *partition_name();
    antlr4::tree::TerminalNode *REQUIRE();
    antlr4::tree::TerminalNode *MATCHING();
    antlr4::tree::TerminalNode *INDEXES();

   
  };

  Attach_partitionContext* attach_partition();

  class  Activate_deactivateContext : public antlr4::ParserRuleContext {
  public:
    Activate_deactivateContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ACTIVATE();
    antlr4::tree::TerminalNode *DEACTIVATE();

   
  };

  Activate_deactivateContext* activate_deactivate();

  class  Alter_tablespace_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_tablespace_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *TABLESPACE();
    Tablespace_nameContext *tablespace_name();
    std::vector<Alter_tablespace_optsContext *> alter_tablespace_opts();
    Alter_tablespace_optsContext* alter_tablespace_opts(size_t i);

   
  };

  Alter_tablespace_statementContext* alter_tablespace_statement();

  class  Alter_tablespace_optsContext : public antlr4::ParserRuleContext {
  public:
    Alter_tablespace_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ADD();
    Add_clauseContext *add_clause();
    antlr4::tree::TerminalNode *BEGIN();
    antlr4::tree::TerminalNode *NEW();
    antlr4::tree::TerminalNode *STRIPE();
    antlr4::tree::TerminalNode *SET();
    Db_container_clauseContext *db_container_clause();
    On_db_partitions_clauseContext *on_db_partitions_clause();
    antlr4::tree::TerminalNode *DROP();
    Drop_container_clauseContext *drop_container_clause();
    antlr4::tree::TerminalNode *REDUCE();
    All_containers_clauseContext *all_containers_clause();
    antlr4::tree::TerminalNode *MAX();
    antlr4::tree::TerminalNode *STOP();
    Integer_valueContext *integer_value();
    Kmg_percentContext *kmg_percent();
    antlr4::tree::TerminalNode *EXTEND();
    antlr4::tree::TerminalNode *RESIZE();
    antlr4::tree::TerminalNode *REBALANCE();
    antlr4::tree::TerminalNode *SUSPEND();
    antlr4::tree::TerminalNode *RESUME();
    antlr4::tree::TerminalNode *PREFETCHSIZE();
    antlr4::tree::TerminalNode *AUTOMATIC();
    Number_of_pagesContext *number_of_pages();
    KmContext *km();
    antlr4::tree::TerminalNode *BUFFERPOOL();
    Bufferpool_nameContext *bufferpool_name();
    antlr4::tree::TerminalNode *OVERHEAD();
    Number_of_millisecondsContext *number_of_milliseconds();
    antlr4::tree::TerminalNode *INHERIT();
    antlr4::tree::TerminalNode *TRANSFERRATE();
    antlr4::tree::TerminalNode *FILE();
    antlr4::tree::TerminalNode *SYSTEM();
    antlr4::tree::TerminalNode *CACHING();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *DROPPED();
    antlr4::tree::TerminalNode *TABLE();
    antlr4::tree::TerminalNode *RECOVERY();
    Yes_noContext *yes_no();
    antlr4::tree::TerminalNode *SWITCH();
    antlr4::tree::TerminalNode *ONLINE();
    antlr4::tree::TerminalNode *AUTORESIZE();
    antlr4::tree::TerminalNode *INCREASESIZE();
    antlr4::tree::TerminalNode *MAXSIZE();
    KmgContext *kmg();
    antlr4::tree::TerminalNode *NONE();
    antlr4::tree::TerminalNode *CONVERT();
    antlr4::tree::TerminalNode *TO();
    antlr4::tree::TerminalNode *LARGE();
    antlr4::tree::TerminalNode *LOWER();
    antlr4::tree::TerminalNode *HIGH();
    antlr4::tree::TerminalNode *WATER();
    antlr4::tree::TerminalNode *MARK();
    antlr4::tree::TerminalNode *USING();
    antlr4::tree::TerminalNode *STOGROUP();
    Storagegroup_nameContext *storagegroup_name();
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *TAG();
    Integer_constantContext *integer_constant();
    antlr4::tree::TerminalNode *MANAGED();
    antlr4::tree::TerminalNode *BY();
    antlr4::tree::TerminalNode *STORAGE();

   
  };

  Alter_tablespace_optsContext* alter_tablespace_opts();

  class  Add_clauseContext : public antlr4::ParserRuleContext {
  public:
    Add_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Db_container_clauseContext *db_container_clause();
    antlr4::tree::TerminalNode *TO();
    antlr4::tree::TerminalNode *STRIPE();
    antlr4::tree::TerminalNode *SET();
    StripesetContext *stripeset();
    On_db_partitions_clauseContext *on_db_partitions_clause();
    System_container_clauseContext *system_container_clause();

   
  };

  Add_clauseContext* add_clause();

  class  Db_container_clauseContext : public antlr4::ParserRuleContext {
  public:
    Db_container_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<Db_container_clause_optsContext *> db_container_clause_opts();
    Db_container_clause_optsContext* db_container_clause_opts(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Db_container_clauseContext* db_container_clause();

  class  Db_container_clause_optsContext : public antlr4::ParserRuleContext {
  public:
    Db_container_clause_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    File_deviceContext *file_device();
    StringContext *string();
    Number_of_pagesContext *number_of_pages();
    Integer_valueContext *integer_value();
    KmgContext *kmg();

   
  };

  Db_container_clause_optsContext* db_container_clause_opts();

  class  Drop_container_clauseContext : public antlr4::ParserRuleContext {
  public:
    Drop_container_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<File_deviceContext *> file_device();
    File_deviceContext* file_device(size_t i);
    std::vector<StringContext *> string();
    StringContext* string(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Drop_container_clauseContext* drop_container_clause();

  class  File_deviceContext : public antlr4::ParserRuleContext {
  public:
    File_deviceContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FILE();
    antlr4::tree::TerminalNode *DEVICE();

   
  };

  File_deviceContext* file_device();

  class  All_containers_clauseContext : public antlr4::ParserRuleContext {
  public:
    All_containers_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    Number_of_pagesContext *number_of_pages();
    Integer_valueContext *integer_value();
    KmgContext *kmg();
    antlr4::tree::TerminalNode *CONTAINERS();

   
  };

  All_containers_clauseContext* all_containers_clause();

  class  System_container_clauseContext : public antlr4::ParserRuleContext {
  public:
    System_container_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    String_listContext *string_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();

   
  };

  System_container_clauseContext* system_container_clause();

  class  StripesetContext : public antlr4::ParserRuleContext {
  public:
    StripesetContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TodoContext *todo();

   
  };

  StripesetContext* stripeset();

  class  KmContext : public antlr4::ParserRuleContext {
  public:
    KmContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *K();
    antlr4::tree::TerminalNode *M();

   
  };

  KmContext* km();

  class  Kmg_percentContext : public antlr4::ParserRuleContext {
  public:
    Kmg_percentContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    KmgContext *kmg();
    antlr4::tree::TerminalNode *PERCENT();

   
  };

  Kmg_percentContext* kmg_percent();

  class  Alter_threshold_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_threshold_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *THRESHOLD();
    Threshold_nameContext *threshold_name();
    std::vector<Alter_threshold_optsContext *> alter_threshold_opts();
    Alter_threshold_optsContext* alter_threshold_opts(size_t i);

   
  };

  Alter_threshold_statementContext* alter_threshold_statement();

  class  Alter_threshold_optsContext : public antlr4::ParserRuleContext {
  public:
    Alter_threshold_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WHEN();
    Alter_threshold_predicateContext *alter_threshold_predicate();
    antlr4::tree::TerminalNode *PERFORM();
    antlr4::tree::TerminalNode *ACTION();
    antlr4::tree::TerminalNode *EXCEEDED();
    std::vector<Alter_threshold_exceeded_actionsContext *> alter_threshold_exceeded_actions();
    Alter_threshold_exceeded_actionsContext* alter_threshold_exceeded_actions(size_t i);
    Enable_disableContext *enable_disable();

   
  };

  Alter_threshold_optsContext* alter_threshold_opts();

  class  Alter_threshold_predicateContext : public antlr4::ParserRuleContext {
  public:
    Alter_threshold_predicateContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TOTALMEMBERCONNECTIONS();
    std::vector<antlr4::tree::TerminalNode *> GT();
    antlr4::tree::TerminalNode* GT(size_t i);
    std::vector<Integer_valueContext *> integer_value();
    Integer_valueContext* integer_value(size_t i);
    antlr4::tree::TerminalNode *TOTALSCMEMBERCONNECTIONS();
    antlr4::tree::TerminalNode *AND();
    antlr4::tree::TerminalNode *QUEUEDCONNECTIONS();
    antlr4::tree::TerminalNode *UNBOUNDED();
    antlr4::tree::TerminalNode *CONNECTIONIDLETIME();
    Day_to_minutesContext *day_to_minutes();
    antlr4::tree::TerminalNode *CONCURRENTWORKLOADOCCURRENCES();
    antlr4::tree::TerminalNode *CONCURRENTWORKLOADACTIVITIES();
    antlr4::tree::TerminalNode *CONCURRENTDBCOORDACTIVITIES();
    antlr4::tree::TerminalNode *ESTIMATEDSQLCOST();
    Bigint_valueContext *bigint_value();
    antlr4::tree::TerminalNode *SQLROWSRETURNED();
    Day_to_secondsContext *day_to_seconds();
    antlr4::tree::TerminalNode *ACTIVITYTOTALTIME();
    antlr4::tree::TerminalNode *UOWTOTALTIME();
    KmgContext *kmg();
    antlr4::tree::TerminalNode *SQLTEMPSPACE();
    antlr4::tree::TerminalNode *AGGSQLTEMPSPACE();
    antlr4::tree::TerminalNode *SQLROWSREAD();
    antlr4::tree::TerminalNode *SQLROWSREADINSC();
    Checking_everyContext *checking_every();
    Hour_to_secondsContext *hour_to_seconds();
    antlr4::tree::TerminalNode *CPUTIME();
    antlr4::tree::TerminalNode *CPUTIMEINSC();
    antlr4::tree::TerminalNode *ACTIVITYTOTALRUNTIME();
    antlr4::tree::TerminalNode *ACTIVITYTOTALRUNTIMEINALLSC();
    antlr4::tree::TerminalNode *SORTSHRHEAPUTIL();
    antlr4::tree::TerminalNode *PERCENT();
    antlr4::tree::TerminalNode *BLOCKING();
    antlr4::tree::TerminalNode *ADMISSION();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *DATATAGINSC();
    antlr4::tree::TerminalNode *IN();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Integer_constant_listContext *integer_constant_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *NOT();

   
  };

  Alter_threshold_predicateContext* alter_threshold_predicate();

  class  Alter_threshold_exceeded_actionsContext : public antlr4::ParserRuleContext {
  public:
    Alter_threshold_exceeded_actionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *COLLECT();
    antlr4::tree::TerminalNode *ACTIVITY();
    antlr4::tree::TerminalNode *DATA();
    Alter_collect_activity_data_clauseContext *alter_collect_activity_data_clause();
    antlr4::tree::TerminalNode *NONE();
    antlr4::tree::TerminalNode *STOP();
    antlr4::tree::TerminalNode *EXECUTION();
    antlr4::tree::TerminalNode *CONTINUE();
    antlr4::tree::TerminalNode *FORCE();
    antlr4::tree::TerminalNode *APPLICATION();
    Remap_activity_actionContext *remap_activity_action();

   
  };

  Alter_threshold_exceeded_actionsContext* alter_threshold_exceeded_actions();

  class  Dt_unitsContext : public antlr4::ParserRuleContext {
  public:
    Dt_unitsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DAY();
    antlr4::tree::TerminalNode *DAYS();
    antlr4::tree::TerminalNode *HOUR();
    antlr4::tree::TerminalNode *HOURS();
    antlr4::tree::TerminalNode *MINUTE();
    antlr4::tree::TerminalNode *MINUTES();

   
  };

  Dt_unitsContext* dt_units();

  class  Dt_units_with_secondsContext : public antlr4::ParserRuleContext {
  public:
    Dt_units_with_secondsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Dt_unitsContext *dt_units();
    antlr4::tree::TerminalNode *SECONDS();

   
  };

  Dt_units_with_secondsContext* dt_units_with_seconds();

  class  Alter_trigger_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_trigger_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *TRIGGER();
    Trigger_nameContext *trigger_name();
    antlr4::tree::TerminalNode *SECURED();
    antlr4::tree::TerminalNode *NOT();

   
  };

  Alter_trigger_statementContext* alter_trigger_statement();

  class  Alter_trusted_context_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_trusted_context_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *TRUSTED();
    antlr4::tree::TerminalNode *CONTEXT();
    Context_nameContext *context_name();
    std::vector<Alter_trusted_context_optsContext *> alter_trusted_context_opts();
    Alter_trusted_context_optsContext* alter_trusted_context_opts(size_t i);

   
  };

  Alter_trusted_context_statementContext* alter_trusted_context_statement();

  class  Alter_trusted_context_optsContext : public antlr4::ParserRuleContext {
  public:
    Alter_trusted_context_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    std::vector<Alter_trusted_context_opts_alter_optsContext *> alter_trusted_context_opts_alter_opts();
    Alter_trusted_context_opts_alter_optsContext* alter_trusted_context_opts_alter_opts(size_t i);
    antlr4::tree::TerminalNode *ADD();
    antlr4::tree::TerminalNode *ATTRIBUTES();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<Address_clauseContext *> address_clause();
    Address_clauseContext* address_clause(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);
    antlr4::tree::TerminalNode *DROP();
    std::vector<antlr4::tree::TerminalNode *> ADDRESS();
    antlr4::tree::TerminalNode* ADDRESS(size_t i);
    std::vector<Address_valueContext *> address_value();
    Address_valueContext* address_value(size_t i);
    User_clauseContext *user_clause();

   
  };

  Alter_trusted_context_optsContext* alter_trusted_context_opts();

  class  Alter_trusted_context_opts_alter_optsContext : public antlr4::ParserRuleContext {
  public:
    Alter_trusted_context_opts_alter_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SYSTEM();
    antlr4::tree::TerminalNode *AUTHID();
    Authorization_nameContext *authorization_name();
    antlr4::tree::TerminalNode *ATTRIBUTES();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<Addr_clause_encryption_valContext *> addr_clause_encryption_val();
    Addr_clause_encryption_valContext* addr_clause_encryption_val(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *DEFAULT();
    antlr4::tree::TerminalNode *ROLE();
    Role_nameContext *role_name();
    Enable_disableContext *enable_disable();

   
  };

  Alter_trusted_context_opts_alter_optsContext* alter_trusted_context_opts_alter_opts();

  class  Addr_clause_encryption_valContext : public antlr4::ParserRuleContext {
  public:
    Addr_clause_encryption_valContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Address_clauseContext *address_clause();
    antlr4::tree::TerminalNode *ENCRYPTION();
    Encryption_valueContext *encryption_value();

   
  };

  Addr_clause_encryption_valContext* addr_clause_encryption_val();

  class  Address_clauseContext : public antlr4::ParserRuleContext {
  public:
    Address_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ADDRESS();
    Address_valueContext *address_value();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *ENCRYPTION();
    Encryption_valueContext *encryption_value();

   
  };

  Address_clauseContext* address_clause();

  class  User_clauseContext : public antlr4::ParserRuleContext {
  public:
    User_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *USE();
    antlr4::tree::TerminalNode *FOR();
    std::vector<Use_for_optsContext *> use_for_opts();
    Use_for_optsContext* use_for_opts(size_t i);
    antlr4::tree::TerminalNode *ADD();
    antlr4::tree::TerminalNode *REPLACE();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);
    antlr4::tree::TerminalNode *DROP();
    std::vector<Use_for_opts_2Context *> use_for_opts_2();
    Use_for_opts_2Context* use_for_opts_2(size_t i);

   
  };

  User_clauseContext* user_clause();

  class  Use_for_optsContext : public antlr4::ParserRuleContext {
  public:
    Use_for_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Authorization_nameContext *authorization_name();
    antlr4::tree::TerminalNode *PUBLIC();
    With_withoutContext *with_without();
    antlr4::tree::TerminalNode *AUTHENTICATION();
    antlr4::tree::TerminalNode *ROLE();
    Role_nameContext *role_name();

   
  };

  Use_for_optsContext* use_for_opts();

  class  Use_for_opts_2Context : public antlr4::ParserRuleContext {
  public:
    Use_for_opts_2Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Authorization_nameContext *authorization_name();
    antlr4::tree::TerminalNode *PUBLIC();

   
  };

  Use_for_opts_2Context* use_for_opts_2();

  class  Alter_type_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_type_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *TYPE();
    Type_nameContext *type_name();
    std::vector<Alter_type_optsContext *> alter_type_opts();
    Alter_type_optsContext* alter_type_opts(size_t i);

   
  };

  Alter_type_statementContext* alter_type_statement();

  class  Alter_type_optsContext : public antlr4::ParserRuleContext {
  public:
    Alter_type_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ADD();
    antlr4::tree::TerminalNode *ATTRIBUTE();
    Attribute_definitionContext *attribute_definition();
    antlr4::tree::TerminalNode *DROP();
    Attribute_nameContext *attribute_name();
    antlr4::tree::TerminalNode *RESTRICT();
    Method_specificationContext *method_specification();
    antlr4::tree::TerminalNode *ALTER();
    Method_identifierContext *method_identifier();
    Method_optionsContext *method_options();

   
  };

  Alter_type_optsContext* alter_type_opts();

  class  Method_identifierContext : public antlr4::ParserRuleContext {
  public:
    Method_identifierContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *METHOD();
    Method_nameContext *method_name();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    Data_type_list_parenContext *data_type_list_paren();
    antlr4::tree::TerminalNode *SPECIFIC();
    Specific_nameContext *specific_name();

   
  };

  Method_identifierContext* method_identifier();

  class  Method_optionsContext : public antlr4::ParserRuleContext {
  public:
    Method_optionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FENCED();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *THREADSAFE();

   
  };

  Method_optionsContext* method_options();

  class  Alter_usage_list_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_usage_list_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *USAGE();
    antlr4::tree::TerminalNode *LIST();
    Usage_list_nameContext *usage_list_name();
    std::vector<Alter_usage_list_opts_itemContext *> alter_usage_list_opts_item();
    Alter_usage_list_opts_itemContext* alter_usage_list_opts_item(size_t i);

   
  };

  Alter_usage_list_statementContext* alter_usage_list_statement();

  class  Alter_usage_list_opts_itemContext : public antlr4::ParserRuleContext {
  public:
    Alter_usage_list_opts_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LIST();
    antlr4::tree::TerminalNode *SIZE();
    Integer_valueContext *integer_value();
    antlr4::tree::TerminalNode *WHEN();
    antlr4::tree::TerminalNode *FULL();
    antlr4::tree::TerminalNode *WRAP();
    antlr4::tree::TerminalNode *DEACTIVATE();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *START();
    antlr4::tree::TerminalNode *DATABASE();
    antlr4::tree::TerminalNode *INACTIVE();
    antlr4::tree::TerminalNode *ACTIVE();

   
  };

  Alter_usage_list_opts_itemContext* alter_usage_list_opts_item();

  class  Alter_user_mapping_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_user_mapping_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    std::vector<antlr4::tree::TerminalNode *> USER();
    antlr4::tree::TerminalNode* USER(size_t i);
    antlr4::tree::TerminalNode *MAPPING();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *SERVER();
    Server_nameContext *server_name();
    antlr4::tree::TerminalNode *OPTIONS();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<Alter_user_mapping_opts_itemContext *> alter_user_mapping_opts_item();
    Alter_user_mapping_opts_itemContext* alter_user_mapping_opts_item(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    Authorization_nameContext *authorization_name();
    antlr4::tree::TerminalNode *PUBLIC();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Alter_user_mapping_statementContext* alter_user_mapping_statement();

  class  Alter_user_mapping_opts_itemContext : public antlr4::ParserRuleContext {
  public:
    Alter_user_mapping_opts_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    User_mapping_option_nameContext *user_mapping_option_name();
    String_constantContext *string_constant();
    Add_setContext *add_set();
    antlr4::tree::TerminalNode *DROP();

   
  };

  Alter_user_mapping_opts_itemContext* alter_user_mapping_opts_item();

  class  Add_setContext : public antlr4::ParserRuleContext {
  public:
    Add_setContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ADD();
    antlr4::tree::TerminalNode *SET();

   
  };

  Add_setContext* add_set();

  class  Alter_view_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_view_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *VIEW();
    View_nameContext *view_name();
    antlr4::tree::TerminalNode *QUERY();
    antlr4::tree::TerminalNode *OPTIMIZATION();
    std::vector<Alter_view_optsContext *> alter_view_opts();
    Alter_view_optsContext* alter_view_opts(size_t i);
    Enable_disableContext *enable_disable();

   
  };

  Alter_view_statementContext* alter_view_statement();

  class  Alter_view_optsContext : public antlr4::ParserRuleContext {
  public:
    Alter_view_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    Column_nameContext *column_name();
    antlr4::tree::TerminalNode *ADD();
    antlr4::tree::TerminalNode *SCOPE();
    Typed_table_nameContext *typed_table_name();
    Typed_view_nameContext *typed_view_name();
    antlr4::tree::TerminalNode *COLUMN();

   
  };

  Alter_view_optsContext* alter_view_opts();

  class  Alter_work_action_set_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_work_action_set_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *WORK();
    antlr4::tree::TerminalNode *ACTION();
    antlr4::tree::TerminalNode *SET();
    Work_action_set_nameContext *work_action_set_name();
    std::vector<Alter_work_action_set_optsContext *> alter_work_action_set_opts();
    Alter_work_action_set_optsContext* alter_work_action_set_opts(size_t i);

   
  };

  Alter_work_action_set_statementContext* alter_work_action_set_statement();

  class  Alter_work_action_set_optsContext : public antlr4::ParserRuleContext {
  public:
    Alter_work_action_set_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ADD();
    Work_action_definitionContext *work_action_definition();
    antlr4::tree::TerminalNode *ALTER();
    Work_action_alterationContext *work_action_alteration();
    antlr4::tree::TerminalNode *DROP();
    Work_action_nameContext *work_action_name();
    antlr4::tree::TerminalNode *WORK();
    antlr4::tree::TerminalNode *ACTION();
    Enable_disableContext *enable_disable();

   
  };

  Alter_work_action_set_optsContext* alter_work_action_set_opts();

  class  Work_action_alterationContext : public antlr4::ParserRuleContext {
  public:
    Work_action_alterationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Work_action_nameContext *work_action_name();
    antlr4::tree::TerminalNode *WORK();
    antlr4::tree::TerminalNode *ACTION();
    std::vector<Work_action_alteration_optsContext *> work_action_alteration_opts();
    Work_action_alteration_optsContext* work_action_alteration_opts(size_t i);

   
  };

  Work_action_alterationContext* work_action_alteration();

  class  Work_action_alteration_optsContext : public antlr4::ParserRuleContext {
  public:
    Work_action_alteration_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SET();
    antlr4::tree::TerminalNode *WORK();
    antlr4::tree::TerminalNode *CLASS_();
    Work_class_nameContext *work_class_name();
    Alter_action_types_clauseContext *alter_action_types_clause();
    antlr4::tree::TerminalNode *ACTIVITY();
    antlr4::tree::TerminalNode *HISTOGRAM();
    antlr4::tree::TerminalNode *TEMPLATE();
    Template_nameContext *template_name();
    antlr4::tree::TerminalNode *LIFETIME();
    antlr4::tree::TerminalNode *QUEUETIME();
    antlr4::tree::TerminalNode *EXECUTETIME();
    antlr4::tree::TerminalNode *ESTIMATEDCOST();
    antlr4::tree::TerminalNode *INTERARRIVALTIME();
    Enable_disableContext *enable_disable();

   
  };

  Work_action_alteration_optsContext* work_action_alteration_opts();

  class  Alter_action_types_clauseContext : public antlr4::ParserRuleContext {
  public:
    Alter_action_types_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *MAP();
    antlr4::tree::TerminalNode *ACTIVITY();
    antlr4::tree::TerminalNode *TO();
    Service_subclass_nameContext *service_subclass_name();
    With_withoutContext *with_without();
    antlr4::tree::TerminalNode *NESTED();
    antlr4::tree::TerminalNode *WHEN();
    Threshold_predicate_clauseContext *threshold_predicate_clause();
    antlr4::tree::TerminalNode *PERFORM();
    antlr4::tree::TerminalNode *ACTION();
    Alter_threshold_exceeded_actionsContext *alter_threshold_exceeded_actions();
    antlr4::tree::TerminalNode *EXCEEDED();
    antlr4::tree::TerminalNode *PREVENT();
    antlr4::tree::TerminalNode *EXECUTION();
    antlr4::tree::TerminalNode *COUNT();
    antlr4::tree::TerminalNode *COLLECT();
    antlr4::tree::TerminalNode *DATA();
    Alter_collect_activity_data_clauseContext *alter_collect_activity_data_clause();
    antlr4::tree::TerminalNode *AGGREGATE();
    antlr4::tree::TerminalNode *BASE();
    antlr4::tree::TerminalNode *EXTENDED();

   
  };

  Alter_action_types_clauseContext* alter_action_types_clause();

  class  Threshold_predicate_clauseContext : public antlr4::ParserRuleContext {
  public:
    Threshold_predicate_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CONCURRENTDBCOORDACTIVITIES();
    std::vector<antlr4::tree::TerminalNode *> GT();
    antlr4::tree::TerminalNode* GT(size_t i);
    std::vector<Integer_valueContext *> integer_value();
    Integer_valueContext* integer_value(size_t i);
    antlr4::tree::TerminalNode *AND();
    antlr4::tree::TerminalNode *QUEUEDACTIVITIES();
    antlr4::tree::TerminalNode *UNBOUNDED();
    antlr4::tree::TerminalNode *SQLTEMPSPACE();
    KmgContext *kmg();
    antlr4::tree::TerminalNode *SQLROWSRETURNED();
    antlr4::tree::TerminalNode *ESTIMATEDSQLCOST();
    Bigint_valueContext *bigint_value();
    antlr4::tree::TerminalNode *CPUTIME();
    Hours_minutesContext *hours_minutes();
    Checking_everyContext *checking_every();
    antlr4::tree::TerminalNode *SQLROWSREAD();
    antlr4::tree::TerminalNode *SORTSHRHEAPUTIL();
    antlr4::tree::TerminalNode *PERCENT();
    antlr4::tree::TerminalNode *BLOCKING();
    antlr4::tree::TerminalNode *ADMISSION();
    antlr4::tree::TerminalNode *FOR();
    Day_to_secondsContext *day_to_seconds();
    antlr4::tree::TerminalNode *ACTIVITYTOTALTIME();
    antlr4::tree::TerminalNode *ACTIVITYTOTALRUNTIME();

   
  };

  Threshold_predicate_clauseContext* threshold_predicate_clause();

  class  Alter_work_class_set_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_work_class_set_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *WORK();
    antlr4::tree::TerminalNode *CLASS_();
    antlr4::tree::TerminalNode *SET();
    Work_class_set_nameContext *work_class_set_name();
    std::vector<Alter_work_class_set_optsContext *> alter_work_class_set_opts();
    Alter_work_class_set_optsContext* alter_work_class_set_opts(size_t i);

   
  };

  Alter_work_class_set_statementContext* alter_work_class_set_statement();

  class  Alter_work_class_set_optsContext : public antlr4::ParserRuleContext {
  public:
    Alter_work_class_set_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ADD();
    Work_class_definitionContext *work_class_definition();
    antlr4::tree::TerminalNode *ALTER();
    Work_class_alterationContext *work_class_alteration();
    antlr4::tree::TerminalNode *DROP();
    Work_class_nameContext *work_class_name();
    antlr4::tree::TerminalNode *WORK();
    antlr4::tree::TerminalNode *CLASS_();

   
  };

  Alter_work_class_set_optsContext* alter_work_class_set_opts();

  class  Work_class_alterationContext : public antlr4::ParserRuleContext {
  public:
    Work_class_alterationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Work_class_nameContext *work_class_name();
    antlr4::tree::TerminalNode *WORK();
    antlr4::tree::TerminalNode *CLASS_();
    std::vector<Work_class_alteration_optsContext *> work_class_alteration_opts();
    Work_class_alteration_optsContext* work_class_alteration_opts(size_t i);

   
  };

  Work_class_alterationContext* work_class_alteration();

  class  Work_class_alteration_optsContext : public antlr4::ParserRuleContext {
  public:
    Work_class_alteration_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    For_from_to_alter_clauseContext *for_from_to_alter_clause();
    Schema_alter_clauseContext *schema_alter_clause();
    Data_tag_alter_clauseContext *data_tag_alter_clause();
    Position_clauseContext *position_clause();

   
  };

  Work_class_alteration_optsContext* work_class_alteration_opts();

  class  For_from_to_alter_clauseContext : public antlr4::ParserRuleContext {
  public:
    For_from_to_alter_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *FROM();
    From_valueContext *from_value();
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *UNITS();
    antlr4::tree::TerminalNode *UNBOUNDED();
    antlr4::tree::TerminalNode *TIMERONCOST();
    antlr4::tree::TerminalNode *CARDINALITY();
    antlr4::tree::TerminalNode *TO();
    To_valueContext *to_value();

   
  };

  For_from_to_alter_clauseContext* for_from_to_alter_clause();

  class  Schema_alter_clauseContext : public antlr4::ParserRuleContext {
  public:
    Schema_alter_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ROUTINES();
    antlr4::tree::TerminalNode *IN();
    antlr4::tree::TerminalNode *SCHEMA();
    Schema_nameContext *schema_name();
    antlr4::tree::TerminalNode *ALL();

   
  };

  Schema_alter_clauseContext* schema_alter_clause();

  class  Data_tag_alter_clauseContext : public antlr4::ParserRuleContext {
  public:
    Data_tag_alter_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *TAG();
    antlr4::tree::TerminalNode *LIST();
    antlr4::tree::TerminalNode *CONTAINS();
    Integer_constantContext *integer_constant();
    antlr4::tree::TerminalNode *ANY();

   
  };

  Data_tag_alter_clauseContext* data_tag_alter_clause();

  class  Alter_workload_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_workload_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *WORKLOAD();
    Workload_nameContext *workload_name();
    std::vector<Alter_workload_opts_itemContext *> alter_workload_opts_item();
    Alter_workload_opts_itemContext* alter_workload_opts_item(size_t i);

   
  };

  Alter_workload_statementContext* alter_workload_statement();

  class  Alter_workload_opts_itemContext : public antlr4::ParserRuleContext {
  public:
    Alter_workload_opts_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ADD();
    Connection_attributesContext *connection_attributes();
    antlr4::tree::TerminalNode *DROP();
    Allow_disallowContext *allow_disallow();
    antlr4::tree::TerminalNode *DB();
    antlr4::tree::TerminalNode *ACCESS();
    Enable_disableContext *enable_disable();
    antlr4::tree::TerminalNode *MAXIMUM();
    antlr4::tree::TerminalNode *DEGREE_();
    antlr4::tree::TerminalNode *DEFAULT();
    DegreeContext *degree();
    antlr4::tree::TerminalNode *SERVICE();
    antlr4::tree::TerminalNode *CLASS_();
    Service_class_nameContext *service_class_name();
    antlr4::tree::TerminalNode *UNDER();
    Service_superclass_nameContext *service_superclass_name();
    antlr4::tree::TerminalNode *POSITION();
    antlr4::tree::TerminalNode *CRITICAL();
    antlr4::tree::TerminalNode *HIGH();
    antlr4::tree::TerminalNode *MEDIUM();
    antlr4::tree::TerminalNode *LOW();
    antlr4::tree::TerminalNode *COLLECT();
    antlr4::tree::TerminalNode *ACTIVITY();
    antlr4::tree::TerminalNode *DATA();
    Alter_collect_activity_data_clauseContext *alter_collect_activity_data_clause();
    antlr4::tree::TerminalNode *NONE();
    antlr4::tree::TerminalNode *METRICS();
    Extended_base_noneContext *extended_base_none();
    antlr4::tree::TerminalNode *AGGREGATE();
    antlr4::tree::TerminalNode *UNIT();
    antlr4::tree::TerminalNode *OF();
    antlr4::tree::TerminalNode *WORK();
    Base_noneContext *base_none();
    antlr4::tree::TerminalNode *LOCK();
    antlr4::tree::TerminalNode *TIMEOUT();
    Alter_collect_history_clauseContext *alter_collect_history_clause();
    antlr4::tree::TerminalNode *DEADLOCK();
    antlr4::tree::TerminalNode *WAIT();
    Alter_collect_lock_wait_data_clauseContext *alter_collect_lock_wait_data_clause();
    antlr4::tree::TerminalNode *BASE();
    antlr4::tree::TerminalNode *INCLUDE();
    std::vector<Package_executableContext *> package_executable();
    Package_executableContext* package_executable(size_t i);
    std::vector<antlr4::tree::TerminalNode *> LIST();
    antlr4::tree::TerminalNode* LIST(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);
    antlr4::tree::TerminalNode *LIFETIME();
    antlr4::tree::TerminalNode *HISTOGRAM();
    antlr4::tree::TerminalNode *TEMPLATE();
    Template_nameContext *template_name();

   
  };

  Alter_workload_opts_itemContext* alter_workload_opts_item();

  class  Package_executableContext : public antlr4::ParserRuleContext {
  public:
    Package_executableContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PACKAGE();
    antlr4::tree::TerminalNode *EXECUTABLE();

   
  };

  Package_executableContext* package_executable();

  class  Base_noneContext : public antlr4::ParserRuleContext {
  public:
    Base_noneContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BASE();
    antlr4::tree::TerminalNode *NONE();

   
  };

  Base_noneContext* base_none();

  class  Extended_base_noneContext : public antlr4::ParserRuleContext {
  public:
    Extended_base_noneContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *EXTENDED();
    Base_noneContext *base_none();

   
  };

  Extended_base_noneContext* extended_base_none();

  class  Alter_collect_activity_data_clauseContext : public antlr4::ParserRuleContext {
  public:
    Alter_collect_activity_data_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *COORDINATOR();
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *WITHOUT();
    antlr4::tree::TerminalNode *DETAILS();
    antlr4::tree::TerminalNode *WITH();
    std::vector<With_optsContext *> with_opts();
    With_optsContext* with_opts(size_t i);
    antlr4::tree::TerminalNode *MEMBER();
    antlr4::tree::TerminalNode *MEMBERS();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);
    antlr4::tree::TerminalNode *AND();
    antlr4::tree::TerminalNode *VALUES();

   
  };

  Alter_collect_activity_data_clauseContext* alter_collect_activity_data_clause();

  class  With_optsContext : public antlr4::ParserRuleContext {
  public:
    With_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DETAILS();
    antlr4::tree::TerminalNode *SECTION();
    antlr4::tree::TerminalNode *INCLUDE();
    antlr4::tree::TerminalNode *ACTUALS();
    antlr4::tree::TerminalNode *BASE();

   
  };

  With_optsContext* with_opts();

  class  Alter_collect_history_clauseContext : public antlr4::ParserRuleContext {
  public:
    Alter_collect_history_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WITHOUT();
    antlr4::tree::TerminalNode *HISTORY();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *AND();
    antlr4::tree::TerminalNode *VALUES();

   
  };

  Alter_collect_history_clauseContext* alter_collect_history_clause();

  class  Alter_collect_lock_wait_data_clauseContext : public antlr4::ParserRuleContext {
  public:
    Alter_collect_lock_wait_data_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *LOCKS();
    antlr4::tree::TerminalNode *WAITING();
    antlr4::tree::TerminalNode *MORE_();
    antlr4::tree::TerminalNode *THAN();
    Wait_timeContext *wait_time();
    antlr4::tree::TerminalNode *SECONDS();
    antlr4::tree::TerminalNode *MICROSECONDS();
    Alter_collect_history_clauseContext *alter_collect_history_clause();

   
  };

  Alter_collect_lock_wait_data_clauseContext* alter_collect_lock_wait_data_clause();

  class  Alter_wrapper_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_wrapper_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *WRAPPER();
    Wrapper_nameContext *wrapper_name();
    antlr4::tree::TerminalNode *OPTIONS();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<Alter_wrapper_opts_itemContext *> alter_wrapper_opts_item();
    Alter_wrapper_opts_itemContext* alter_wrapper_opts_item(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Alter_wrapper_statementContext* alter_wrapper_statement();

  class  Alter_wrapper_opts_itemContext : public antlr4::ParserRuleContext {
  public:
    Alter_wrapper_opts_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Wrapper_option_nameContext *wrapper_option_name();
    String_constantContext *string_constant();
    Add_setContext *add_set();
    antlr4::tree::TerminalNode *DROP();

   
  };

  Alter_wrapper_opts_itemContext* alter_wrapper_opts_item();

  class  Alter_xsrobject_statementContext : public antlr4::ParserRuleContext {
  public:
    Alter_xsrobject_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALTER();
    antlr4::tree::TerminalNode *XSROBJECT();
    Xsrobject_nameContext *xsrobject_name();
    Enable_disableContext *enable_disable();
    antlr4::tree::TerminalNode *DECOMPOSITION();

   
  };

  Alter_xsrobject_statementContext* alter_xsrobject_statement();

  class  StringContext : public antlr4::ParserRuleContext {
  public:
    StringContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *STRING_LITERAL();

   
  };

  StringContext* string();

  class  String_constantContext : public antlr4::ParserRuleContext {
  public:
    String_constantContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    StringContext *string();

   
  };

  String_constantContext* string_constant();

  class  Numeric_constantContext : public antlr4::ParserRuleContext {
  public:
    Numeric_constantContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DECIMAL_LITERAL();
    antlr4::tree::TerminalNode *FLOAT_LITERAL();
    antlr4::tree::TerminalNode *REAL_LITERAL();

   
  };

  Numeric_constantContext* numeric_constant();

  class  Data_typeContext : public antlr4::ParserRuleContext {
  public:
    Data_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Built_in_typeContext *built_in_type();
    Anchored_data_typeContext *anchored_data_type();
    Row_type_nameContext *row_type_name();
    Array_type_nameContext *array_type_name();

   
  };

  Data_typeContext* data_type();

  class  Anchored_data_typeContext : public antlr4::ParserRuleContext {
  public:
    Anchored_data_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ANCHOR();
    Variable_nameContext *variable_name();
    Table_nameContext *table_name();
    std::vector<antlr4::tree::TerminalNode *> DOT();
    antlr4::tree::TerminalNode* DOT(size_t i);
    Column_nameContext *column_name();
    Qualified_global_variable_referenceContext *qualified_global_variable_reference();
    antlr4::tree::TerminalNode *ROW();
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *TYPE();
    antlr4::tree::TerminalNode *TO();
    Table_or_view_nameContext *table_or_view_name();
    Cursor_variable_nameContext *cursor_variable_name();
    Schema_nameContext *schema_name();
    antlr4::tree::TerminalNode *OF();

   
  };

  Anchored_data_typeContext* anchored_data_type();

  class  Anchored_non_row_data_typeContext : public antlr4::ParserRuleContext {
  public:
    Anchored_non_row_data_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ANCHOR();
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *TYPE();
    antlr4::tree::TerminalNode *TO();
    Variable_nameContext *variable_name();
    Table_nameContext *table_name();
    std::vector<antlr4::tree::TerminalNode *> DOT();
    antlr4::tree::TerminalNode* DOT(size_t i);
    Column_nameContext *column_name();
    Qualified_global_variable_referenceContext *qualified_global_variable_reference();
    Schema_nameContext *schema_name();

   
  };

  Anchored_non_row_data_typeContext* anchored_non_row_data_type();

  class  Anchored_row_data_typeContext : public antlr4::ParserRuleContext {
  public:
    Anchored_row_data_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ANCHOR();
    Variable_nameContext *variable_name();
    Qualified_global_variable_referenceContext *qualified_global_variable_reference();
    antlr4::tree::TerminalNode *ROW();
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *TYPE();
    antlr4::tree::TerminalNode *TO();
    Table_or_view_nameContext *table_or_view_name();
    Cursor_variable_nameContext *cursor_variable_name();
    antlr4::tree::TerminalNode *OF();

   
  };

  Anchored_row_data_typeContext* anchored_row_data_type();

  class  Source_data_typeContext : public antlr4::ParserRuleContext {
  public:
    Source_data_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Built_in_typeContext *built_in_type();
    Anchored_data_typeContext *anchored_data_type();

   
  };

  Source_data_typeContext* source_data_type();

  class  Data_type_constrainstContext : public antlr4::ParserRuleContext {
  public:
    Data_type_constrainstContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *NULL_();
    antlr4::tree::TerminalNode *CHECK();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Check_conditionContext *check_condition();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();

   
  };

  Data_type_constrainstContext* data_type_constrainst();

  class  Check_conditionContext : public antlr4::ParserRuleContext {
  public:
    Check_conditionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Search_conditionContext *search_condition();
    ExpressionContext *expression();

   
  };

  Check_conditionContext* check_condition();

  class  Data_type_2Context : public antlr4::ParserRuleContext {
  public:
    Data_type_2Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INTEGER();
    antlr4::tree::TerminalNode *INT();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Integer_valueContext *integer_value();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *VARCHAR();
    Char_characterContext *char_character();
    antlr4::tree::TerminalNode *VARYING();
    Octets_codeunitsContext *octets_codeunits();
    Anchored_non_row_data_typeContext *anchored_non_row_data_type();

   
  };

  Data_type_2Context* data_type_2();

  class  Built_in_typeContext : public antlr4::ParserRuleContext {
  public:
    Built_in_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SMALLINT();
    antlr4::tree::TerminalNode *INT();
    antlr4::tree::TerminalNode *INTEGER();
    antlr4::tree::TerminalNode *BIGINT();
    antlr4::tree::TerminalNode *DEC();
    antlr4::tree::TerminalNode *DECIMAL();
    antlr4::tree::TerminalNode *NUMERIC();
    antlr4::tree::TerminalNode *NUM();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<Integer_valueContext *> integer_value();
    Integer_valueContext* integer_value(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *COMMA();
    antlr4::tree::TerminalNode *DECFLOAT();
    antlr4::tree::TerminalNode *FLOAT();
    antlr4::tree::TerminalNode *REAL();
    antlr4::tree::TerminalNode *DOUBLE();
    antlr4::tree::TerminalNode *PRECISION();
    Char_characterContext *char_character();
    antlr4::tree::TerminalNode *VARCHAR();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *BIT();
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *VARYING();
    Octets_codeunitsContext *octets_codeunits();
    antlr4::tree::TerminalNode *CLOB();
    antlr4::tree::TerminalNode *LARGE();
    antlr4::tree::TerminalNode *OBJECT();
    KmgContext *kmg();
    antlr4::tree::TerminalNode *GRAPHIC();
    antlr4::tree::TerminalNode *VARGRAPHIC();
    CodeunitsContext *codeunits();
    antlr4::tree::TerminalNode *DBCLOB();
    antlr4::tree::TerminalNode *NCHAR();
    antlr4::tree::TerminalNode *NATIONAL();
    Integer_parenContext *integer_paren();
    antlr4::tree::TerminalNode *NVARCHAR();
    antlr4::tree::TerminalNode *NCLOB();
    antlr4::tree::TerminalNode *CHARACTER();
    Integer_kmg_parenContext *integer_kmg_paren();
    antlr4::tree::TerminalNode *BINARY();
    antlr4::tree::TerminalNode *VARBINARY();
    antlr4::tree::TerminalNode *BLOB();
    antlr4::tree::TerminalNode *DATE();
    antlr4::tree::TerminalNode *TIME();
    antlr4::tree::TerminalNode *TIMESTAMP();
    antlr4::tree::TerminalNode *BOOLEAN();
    antlr4::tree::TerminalNode *XML();
    antlr4::tree::TerminalNode *CURSOR();

   
  };

  Built_in_typeContext* built_in_type();

  class  Inferred_builtin_typeContext : public antlr4::ParserRuleContext {
  public:
    Inferred_builtin_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *DEC();
    antlr4::tree::TerminalNode *DECIMAL();
    antlr4::tree::TerminalNode *NUMERIC();
    antlr4::tree::TerminalNode *NUM();
    antlr4::tree::TerminalNode *DECFLOAT();
    Char_characterContext *char_character();
    antlr4::tree::TerminalNode *VARCHAR();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *BIT();
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *VARYING();
    antlr4::tree::TerminalNode *CLOB();
    antlr4::tree::TerminalNode *LARGE();
    antlr4::tree::TerminalNode *OBJECT();
    antlr4::tree::TerminalNode *GRAPHIC();
    antlr4::tree::TerminalNode *VARGRAPHIC();
    antlr4::tree::TerminalNode *DBCLOB();
    antlr4::tree::TerminalNode *NCHAR();
    antlr4::tree::TerminalNode *NATIONAL();
    antlr4::tree::TerminalNode *NVARCHAR();
    antlr4::tree::TerminalNode *NCLOB();
    antlr4::tree::TerminalNode *CHARACTER();
    antlr4::tree::TerminalNode *BINARY();
    antlr4::tree::TerminalNode *VARBINARY();
    antlr4::tree::TerminalNode *BLOB();
    antlr4::tree::TerminalNode *TIMESTAMP();

   
  };

  Inferred_builtin_typeContext* inferred_builtin_type();

  class  Designator_data_typeContext : public antlr4::ParserRuleContext {
  public:
    Designator_data_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Data_typeContext *data_type();
    Inferred_builtin_typeContext *inferred_builtin_type();

   
  };

  Designator_data_typeContext* designator_data_type();

  class  Designator_data_type_listContext : public antlr4::ParserRuleContext {
  public:
    Designator_data_type_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Designator_data_typeContext *> designator_data_type();
    Designator_data_typeContext* designator_data_type(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Designator_data_type_listContext* designator_data_type_list();

  class  Designator_data_type_list_parenContext : public antlr4::ParserRuleContext {
  public:
    Designator_data_type_list_parenContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    Designator_data_type_listContext *designator_data_type_list();

   
  };

  Designator_data_type_list_parenContext* designator_data_type_list_paren();

  class  Integer_parenContext : public antlr4::ParserRuleContext {
  public:
    Integer_parenContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Integer_valueContext *integer_value();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();

   
  };

  Integer_parenContext* integer_paren();

  class  Integer_kmg_parenContext : public antlr4::ParserRuleContext {
  public:
    Integer_kmg_parenContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Integer_valueContext *integer_value();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    KmgContext *kmg();

   
  };

  Integer_kmg_parenContext* integer_kmg_paren();

  class  Char_characterContext : public antlr4::ParserRuleContext {
  public:
    Char_characterContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CHAR();
    antlr4::tree::TerminalNode *CHARACTER();

   
  };

  Char_characterContext* char_character();

  class  Octets_codeunitsContext : public antlr4::ParserRuleContext {
  public:
    Octets_codeunitsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OCTETS();
    antlr4::tree::TerminalNode *CODEUNITS32();

   
  };

  Octets_codeunitsContext* octets_codeunits();

  class  CodeunitsContext : public antlr4::ParserRuleContext {
  public:
    CodeunitsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CODEUNITS16();
    antlr4::tree::TerminalNode *CODEUNITS32();

   
  };

  CodeunitsContext* codeunits();

  class  KmgContext : public antlr4::ParserRuleContext {
  public:
    KmgContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *K();
    antlr4::tree::TerminalNode *M();
    antlr4::tree::TerminalNode *G();

   
  };

  KmgContext* kmg();

  class  Rs_locator_variableContext : public antlr4::ParserRuleContext {
  public:
    Rs_locator_variableContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Rs_locator_variableContext* rs_locator_variable();

  class  Integer_constant_listContext : public antlr4::ParserRuleContext {
  public:
    Integer_constant_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Integer_constantContext *> integer_constant();
    Integer_constantContext* integer_constant(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Integer_constant_listContext* integer_constant_list();

  class  Integer_constantContext : public antlr4::ParserRuleContext {
  public:
    Integer_constantContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DECIMAL_LITERAL();
    antlr4::tree::TerminalNode *L_ONE();
    antlr4::tree::TerminalNode *L_ZERO();

   
  };

  Integer_constantContext* integer_constant();

  class  Integer_valueContext : public antlr4::ParserRuleContext {
  public:
    Integer_valueContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DECIMAL_LITERAL();
    antlr4::tree::TerminalNode *L_ONE();
    antlr4::tree::TerminalNode *L_ZERO();

   
  };

  Integer_valueContext* integer_value();

  class  Positive_integerContext : public antlr4::ParserRuleContext {
  public:
    Positive_integerContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DECIMAL_LITERAL();

   
  };

  Positive_integerContext* positive_integer();

  class  Bigint_valueContext : public antlr4::ParserRuleContext {
  public:
    Bigint_valueContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DECIMAL_LITERAL();

   
  };

  Bigint_valueContext* bigint_value();

  class  Bigint_constantContext : public antlr4::ParserRuleContext {
  public:
    Bigint_constantContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DECIMAL_LITERAL();

   
  };

  Bigint_constantContext* bigint_constant();

  class  Member_numberContext : public antlr4::ParserRuleContext {
  public:
    Member_numberContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Member_numberContext* member_number();

  class  Version_idContext : public antlr4::ParserRuleContext {
  public:
    Version_idContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Version_idContext* version_id();

  class  Drop_statementContext : public antlr4::ParserRuleContext {
  public:
    Drop_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DROP();
    Alias_designatorContext *alias_designator();
    antlr4::tree::TerminalNode *AUDIT();
    antlr4::tree::TerminalNode *POLICY();
    Policy_nameContext *policy_name();
    antlr4::tree::TerminalNode *BUFFERPOOL();
    Bufferpool_nameContext *bufferpool_name();
    antlr4::tree::TerminalNode *DATABASE();
    antlr4::tree::TerminalNode *PARTITION();
    antlr4::tree::TerminalNode *GROUP();
    Db_partition_group_nameContext *db_partition_group_name();
    antlr4::tree::TerminalNode *EVENT();
    antlr4::tree::TerminalNode *MONITOR();
    Event_monitor_nameContext *event_monitor_name();
    Function_designatorContext *function_designator();
    antlr4::tree::TerminalNode *FUNCTION();
    antlr4::tree::TerminalNode *MAPPING();
    Function_mapping_nameContext *function_mapping_name();
    antlr4::tree::TerminalNode *HISTOGRAM();
    antlr4::tree::TerminalNode *TEMPLATE();
    Template_nameContext *template_name();
    antlr4::tree::TerminalNode *INDEX();
    Index_nameContext *index_name();
    antlr4::tree::TerminalNode *EXTENSION();
    Index_extension_nameContext *index_extension_name();
    antlr4::tree::TerminalNode *RESTRICT();
    antlr4::tree::TerminalNode *MASK();
    Mask_nameContext *mask_name();
    Method_designatorContext *method_designator();
    antlr4::tree::TerminalNode *MODULE();
    Module_nameContext *module_name();
    antlr4::tree::TerminalNode *NICKNAME();
    Nick_nameContext *nick_name();
    antlr4::tree::TerminalNode *PACKAGE();
    Package_nameContext *package_name();
    antlr4::tree::TerminalNode *PERMISSION();
    Permission_nameContext *permission_name();
    Procedure_designatorContext *procedure_designator();
    antlr4::tree::TerminalNode *ROLE();
    Role_nameContext *role_name();
    antlr4::tree::TerminalNode *SCHEMA();
    Schema_nameContext *schema_name();
    antlr4::tree::TerminalNode *SECURITY();
    antlr4::tree::TerminalNode *LABEL();
    Security_label_nameContext *security_label_name();
    antlr4::tree::TerminalNode *COMPONENT();
    Sec_label_comp_nameContext *sec_label_comp_name();
    Security_policy_nameContext *security_policy_name();
    antlr4::tree::TerminalNode *SEQUENCE();
    Sequence_nameContext *sequence_name();
    antlr4::tree::TerminalNode *SERVER();
    Server_nameContext *server_name();
    Service_class_designatorContext *service_class_designator();
    antlr4::tree::TerminalNode *STOGROUP();
    Storagegroup_nameContext *storagegroup_name();
    antlr4::tree::TerminalNode *TABLE();
    Table_nameContext *table_name();
    antlr4::tree::TerminalNode *HIERARCHY();
    Root_table_nameContext *root_table_name();
    Tablespace_name_listContext *tablespace_name_list();
    antlr4::tree::TerminalNode *FOR();
    Type_nameContext *type_name();
    antlr4::tree::TerminalNode *THRESHOLD();
    Threshold_nameContext *threshold_name();
    antlr4::tree::TerminalNode *TRIGGER();
    Trigger_nameContext *trigger_name();
    antlr4::tree::TerminalNode *TRUSTED();
    antlr4::tree::TerminalNode *CONTEXT();
    Context_nameContext *context_name();
    antlr4::tree::TerminalNode *TYPE();
    Type_mapping_nameContext *type_mapping_name();
    antlr4::tree::TerminalNode *USAGE();
    antlr4::tree::TerminalNode *LIST();
    Usage_list_nameContext *usage_list_name();
    std::vector<antlr4::tree::TerminalNode *> USER();
    antlr4::tree::TerminalNode* USER(size_t i);
    antlr4::tree::TerminalNode *VARIABLE();
    Variable_nameContext *variable_name();
    antlr4::tree::TerminalNode *VIEW();
    View_nameContext *view_name();
    Root_view_nameContext *root_view_name();
    antlr4::tree::TerminalNode *WORK();
    antlr4::tree::TerminalNode *ACTION();
    antlr4::tree::TerminalNode *SET();
    Work_action_set_nameContext *work_action_set_name();
    antlr4::tree::TerminalNode *CLASS_();
    Work_class_set_nameContext *work_class_set_name();
    antlr4::tree::TerminalNode *WORKLOAD();
    Workload_nameContext *workload_name();
    antlr4::tree::TerminalNode *WRAPPER();
    Wrapper_nameContext *wrapper_name();
    antlr4::tree::TerminalNode *XSROBJECT();
    Xsrobject_nameContext *xsrobject_name();
    antlr4::tree::TerminalNode *TABLESPACE();
    antlr4::tree::TerminalNode *TABLESPACES();
    antlr4::tree::TerminalNode *TRANSFORM();
    antlr4::tree::TerminalNode *TRANSFORMS();
    antlr4::tree::TerminalNode *ALL();
    Group_nameContext *group_name();
    Authorization_nameContext *authorization_name();
    Version_idContext *version_id();
    antlr4::tree::TerminalNode *IF();
    antlr4::tree::TerminalNode *EXISTS();
    antlr4::tree::TerminalNode *VERSION();

   
  };

  Drop_statementContext* drop_statement();

  class  Alias_designatorContext : public antlr4::ParserRuleContext {
  public:
    Alias_designatorContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALIAS();
    Alias_nameContext *alias_name();
    antlr4::tree::TerminalNode *PUBLIC();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *TABLE();
    antlr4::tree::TerminalNode *MODULE();
    antlr4::tree::TerminalNode *SEQUENCE();

   
  };

  Alias_designatorContext* alias_designator();

  class  Service_class_designatorContext : public antlr4::ParserRuleContext {
  public:
    Service_class_designatorContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SERVICE();
    antlr4::tree::TerminalNode *CLASS_();
    Service_class_nameContext *service_class_name();
    antlr4::tree::TerminalNode *UNDER();
    Service_superclass_nameContext *service_superclass_name();

   
  };

  Service_class_designatorContext* service_class_designator();

  class  Tablespace_name_listContext : public antlr4::ParserRuleContext {
  public:
    Tablespace_name_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Tablespace_nameContext *> tablespace_name();
    Tablespace_nameContext* tablespace_name(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Tablespace_name_listContext* tablespace_name_list();

  class  Associate_locators_statementContext : public antlr4::ParserRuleContext {
  public:
    Associate_locators_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ASSOCIATE();
    std::vector<Rs_locator_variableContext *> rs_locator_variable();
    Rs_locator_variableContext* rs_locator_variable(size_t i);
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *PROCEDURE();
    Procedure_nameContext *procedure_name();
    antlr4::tree::TerminalNode *LOCATOR();
    antlr4::tree::TerminalNode *LOCATORS();
    antlr4::tree::TerminalNode *RESULT();
    antlr4::tree::TerminalNode *SET();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Associate_locators_statementContext* associate_locators_statement();

  class  Audit_statementContext : public antlr4::ParserRuleContext {
  public:
    Audit_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *AUDIT();
    antlr4::tree::TerminalNode *DATABASE();
    antlr4::tree::TerminalNode *TABLE();
    Table_nameContext *table_name();
    antlr4::tree::TerminalNode *TRUSTED();
    antlr4::tree::TerminalNode *CONTEXT();
    Context_nameContext *context_name();
    Authorization_nameContext *authorization_name();
    antlr4::tree::TerminalNode *ACCESSCTRL();
    antlr4::tree::TerminalNode *CREATE_SECURE_OBJECT();
    antlr4::tree::TerminalNode *POLICY();
    Policy_nameContext *policy_name();
    antlr4::tree::TerminalNode *REMOVE();
    antlr4::tree::TerminalNode *USER();
    antlr4::tree::TerminalNode *GROUP();
    antlr4::tree::TerminalNode *ROLE();
    antlr4::tree::TerminalNode *USING();
    antlr4::tree::TerminalNode *REPLACE();

   
  };

  Audit_statementContext* audit_statement();

  class  Begin_declare_section_statementContext : public antlr4::ParserRuleContext {
  public:
    Begin_declare_section_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BEGIN();
    antlr4::tree::TerminalNode *DECLARE();
    antlr4::tree::TerminalNode *SECTION();

   
  };

  Begin_declare_section_statementContext* begin_declare_section_statement();

  class  Call_statementContext : public antlr4::ParserRuleContext {
  public:
    Call_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CALL();
    Procedure_nameContext *procedure_name();
    Arg_list_parenContext *arg_list_paren();

   
  };

  Call_statementContext* call_statement();

  class  Arg_list_parenContext : public antlr4::ParserRuleContext {
  public:
    Arg_list_parenContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    Arg_listContext *arg_list();

   
  };

  Arg_list_parenContext* arg_list_paren();

  class  Arg_listContext : public antlr4::ParserRuleContext {
  public:
    Arg_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<ArgumentContext *> argument();
    ArgumentContext* argument(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Arg_listContext* arg_list();

  class  ArgumentContext : public antlr4::ParserRuleContext {
  public:
    ArgumentContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ExpressionContext *expression();
    antlr4::tree::TerminalNode *DEFAULT();
    antlr4::tree::TerminalNode *NULL_();
    Parameter_nameContext *parameter_name();
    antlr4::tree::TerminalNode *ASSOC();

   
  };

  ArgumentContext* argument();

  class  Case_statementContext : public antlr4::ParserRuleContext {
  public:
    Case_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<antlr4::tree::TerminalNode *> CASE();
    antlr4::tree::TerminalNode* CASE(size_t i);
    antlr4::tree::TerminalNode *END();
    Searched_case_statement_when_clauseContext *searched_case_statement_when_clause();
    Simple_case_statement_when_clauseContext *simple_case_statement_when_clause();

   
  };

  Case_statementContext* case_statement();

  class  Searched_case_statement_when_clauseContext : public antlr4::ParserRuleContext {
  public:
    Searched_case_statement_when_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<antlr4::tree::TerminalNode *> WHEN();
    antlr4::tree::TerminalNode* WHEN(size_t i);
    std::vector<Search_conditionContext *> search_condition();
    Search_conditionContext* search_condition(size_t i);
    std::vector<antlr4::tree::TerminalNode *> THEN();
    antlr4::tree::TerminalNode* THEN(size_t i);
    antlr4::tree::TerminalNode *ELSE();
    std::vector<Sql_procedure_statementContext *> sql_procedure_statement();
    Sql_procedure_statementContext* sql_procedure_statement(size_t i);
    std::vector<antlr4::tree::TerminalNode *> SEMI();
    antlr4::tree::TerminalNode* SEMI(size_t i);

   
  };

  Searched_case_statement_when_clauseContext* searched_case_statement_when_clause();

  class  Simple_case_statement_when_clauseContext : public antlr4::ParserRuleContext {
  public:
    Simple_case_statement_when_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<ExpressionContext *> expression();
    ExpressionContext* expression(size_t i);
    std::vector<antlr4::tree::TerminalNode *> WHEN();
    antlr4::tree::TerminalNode* WHEN(size_t i);
    std::vector<antlr4::tree::TerminalNode *> THEN();
    antlr4::tree::TerminalNode* THEN(size_t i);
    antlr4::tree::TerminalNode *ELSE();
    std::vector<Sql_procedure_statementContext *> sql_procedure_statement();
    Sql_procedure_statementContext* sql_procedure_statement(size_t i);
    std::vector<antlr4::tree::TerminalNode *> SEMI();
    antlr4::tree::TerminalNode* SEMI(size_t i);

   
  };

  Simple_case_statement_when_clauseContext* simple_case_statement_when_clause();

  class  Close_statementContext : public antlr4::ParserRuleContext {
  public:
    Close_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CLOSE();
    Cursor_nameContext *cursor_name();
    Cursor_variable_nameContext *cursor_variable_name();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *RELEASE();

   
  };

  Close_statementContext* close_statement();

  class  Comment_statementContext : public antlr4::ParserRuleContext {
  public:
    Comment_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *COMMENT();
    antlr4::tree::TerminalNode *ON();
    Comment_objectsContext *comment_objects();
    antlr4::tree::TerminalNode *IS();
    String_constantContext *string_constant();
    Table_or_view_nameContext *table_or_view_name();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<Column_commentContext *> column_comment();
    Column_commentContext* column_comment(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Comment_statementContext* comment_statement();

  class  Column_commentContext : public antlr4::ParserRuleContext {
  public:
    Column_commentContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Column_nameContext *column_name();
    antlr4::tree::TerminalNode *IS();
    String_constantContext *string_constant();

   
  };

  Column_commentContext* column_comment();

  class  Comment_objectsContext : public antlr4::ParserRuleContext {
  public:
    Comment_objectsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Alias_designatorContext *alias_designator();
    antlr4::tree::TerminalNode *AUDIT();
    antlr4::tree::TerminalNode *POLICY();
    Policy_nameContext *policy_name();
    antlr4::tree::TerminalNode *COLUMN();
    Table_or_view_nameContext *table_or_view_name();
    antlr4::tree::TerminalNode *DOT();
    Column_nameContext *column_name();
    antlr4::tree::TerminalNode *CONSTRAINT();
    Table_nameContext *table_name();
    Constraint_nameContext *constraint_name();
    antlr4::tree::TerminalNode *DATABASE();
    antlr4::tree::TerminalNode *PARTITION();
    antlr4::tree::TerminalNode *GROUP();
    Db_partition_group_nameContext *db_partition_group_name();
    Function_designatorContext *function_designator();
    antlr4::tree::TerminalNode *FUNCTION();
    antlr4::tree::TerminalNode *MAPPING();
    Function_mapping_nameContext *function_mapping_name();
    antlr4::tree::TerminalNode *HISTOGRAM();
    antlr4::tree::TerminalNode *TEMPLATE();
    Template_nameContext *template_name();
    antlr4::tree::TerminalNode *INDEX();
    Index_nameContext *index_name();
    antlr4::tree::TerminalNode *MASK();
    Mask_nameContext *mask_name();
    antlr4::tree::TerminalNode *MODULE();
    Module_nameContext *module_name();
    antlr4::tree::TerminalNode *NICKNAME();
    Nick_nameContext *nick_name();
    antlr4::tree::TerminalNode *PACKAGE();
    Package_nameContext *package_name();
    Version_idContext *version_id();
    antlr4::tree::TerminalNode *VERSION();
    Procedure_designatorContext *procedure_designator();
    antlr4::tree::TerminalNode *ROLE();
    Role_nameContext *role_name();
    antlr4::tree::TerminalNode *SCHEMA();
    Schema_nameContext *schema_name();
    antlr4::tree::TerminalNode *SECURITY();
    antlr4::tree::TerminalNode *LABEL();
    Security_label_nameContext *security_label_name();
    antlr4::tree::TerminalNode *COMPONENT();
    Sec_label_comp_nameContext *sec_label_comp_name();
    Security_policy_nameContext *security_policy_name();
    antlr4::tree::TerminalNode *SEQUENCE();
    Sequence_nameContext *sequence_name();
    antlr4::tree::TerminalNode *SERVER();
    Server_nameContext *server_name();
    antlr4::tree::TerminalNode *OPTION();
    Server_option_nameContext *server_option_name();
    antlr4::tree::TerminalNode *FOR();
    Remote_serverContext *remote_server();
    Service_class_designatorContext *service_class_designator();
    antlr4::tree::TerminalNode *TABLE();
    antlr4::tree::TerminalNode *TABLESPACE();
    Tablespace_nameContext *tablespace_name();
    antlr4::tree::TerminalNode *THRESHOLD();
    Threshold_nameContext *threshold_name();
    antlr4::tree::TerminalNode *TRIGGER();
    Trigger_nameContext *trigger_name();
    antlr4::tree::TerminalNode *TRUSTED();
    antlr4::tree::TerminalNode *CONTEXT();
    Context_nameContext *context_name();
    antlr4::tree::TerminalNode *TYPE();
    Type_nameContext *type_name();
    Type_mapping_nameContext *type_mapping_name();
    antlr4::tree::TerminalNode *USAGE();
    antlr4::tree::TerminalNode *LIST();
    Usage_list_nameContext *usage_list_name();
    antlr4::tree::TerminalNode *VARIABLE();
    Variable_nameContext *variable_name();
    antlr4::tree::TerminalNode *WORK();
    antlr4::tree::TerminalNode *ACTION();
    antlr4::tree::TerminalNode *SET();
    Work_action_set_nameContext *work_action_set_name();
    antlr4::tree::TerminalNode *CLASS_();
    Work_class_set_nameContext *work_class_set_name();
    antlr4::tree::TerminalNode *WORKLOAD();
    Workload_nameContext *workload_name();
    antlr4::tree::TerminalNode *WRAPPER();
    Wrapper_nameContext *wrapper_name();
    antlr4::tree::TerminalNode *XSROBJECT();
    Xsrobject_nameContext *xsrobject_name();

   
  };

  Comment_objectsContext* comment_objects();

  class  Commit_statementContext : public antlr4::ParserRuleContext {
  public:
    Commit_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *COMMIT();
    antlr4::tree::TerminalNode *WORK();

   
  };

  Commit_statementContext* commit_statement();

  class  Connect_type_1_statementContext : public antlr4::ParserRuleContext {
  public:
    Connect_type_1_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CONNECT();
    antlr4::tree::TerminalNode *TO();
    antlr4::tree::TerminalNode *RESET();
    AuthorizationContext *authorization();
    Server_nameContext *server_name();
    Host_variableContext *host_variable();
    Lock_blockContext *lock_block();

   
  };

  Connect_type_1_statementContext* connect_type_1_statement();

  class  AuthorizationContext : public antlr4::ParserRuleContext {
  public:
    AuthorizationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *USER();
    Authorization_nameContext *authorization_name();
    PasswordsContext *passwords();
    AccesstokenContext *accesstoken();
    antlr4::tree::TerminalNode *APIKEY();
    Api_keyContext *api_key();

   
  };

  AuthorizationContext* authorization();

  class  PasswordsContext : public antlr4::ParserRuleContext {
  public:
    PasswordsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *USING();
    std::vector<Password_Context *> password_();
    Password_Context* password_(size_t i);
    antlr4::tree::TerminalNode *NEW();
    antlr4::tree::TerminalNode *CONFIRM();
    antlr4::tree::TerminalNode *CHANGE();
    antlr4::tree::TerminalNode *PASSWORD();

   
  };

  PasswordsContext* passwords();

  class  Lock_blockContext : public antlr4::ParserRuleContext {
  public:
    Lock_blockContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *IN();
    antlr4::tree::TerminalNode *SHARE();
    antlr4::tree::TerminalNode *MODE();
    antlr4::tree::TerminalNode *EXCLUSIVE();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *SINGLE();
    antlr4::tree::TerminalNode *MEMBER();

   
  };

  Lock_blockContext* lock_block();

  class  AccesstokenContext : public antlr4::ParserRuleContext {
  public:
    AccesstokenContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ACCESSTOKEN();
    TokenContext *token();
    antlr4::tree::TerminalNode *ACCESSTOKENTYPE();
    Token_typeContext *token_type();

   
  };

  AccesstokenContext* accesstoken();

  class  TokenContext : public antlr4::ParserRuleContext {
  public:
    TokenContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TodoContext *todo();

   
  };

  TokenContext* token();

  class  Api_keyContext : public antlr4::ParserRuleContext {
  public:
    Api_keyContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TodoContext *todo();

   
  };

  Api_keyContext* api_key();

  class  Token_typeContext : public antlr4::ParserRuleContext {
  public:
    Token_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TodoContext *todo();

   
  };

  Token_typeContext* token_type();

  class  Declare_cursor_statementContext : public antlr4::ParserRuleContext {
  public:
    Declare_cursor_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DECLARE();
    Cursor_nameContext *cursor_name();
    antlr4::tree::TerminalNode *CURSOR();
    HoldabilityContext *holdability();
    ReturnabilityContext *returnability();
    antlr4::tree::TerminalNode *FOR();
    Select_statementContext *select_statement();
    Statement_nameContext *statement_name();
    antlr4::tree::TerminalNode *ASENSITIVE();
    antlr4::tree::TerminalNode *INSENSITIVE();

   
  };

  Declare_cursor_statementContext* declare_cursor_statement();

  class  Declare_global_temporary_table_statementContext : public antlr4::ParserRuleContext {
  public:
    Declare_global_temporary_table_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DECLARE();
    antlr4::tree::TerminalNode *GLOBAL();
    antlr4::tree::TerminalNode *TEMPORARY();
    antlr4::tree::TerminalNode *TABLE();
    Table_nameContext *table_name();

   
  };

  Declare_global_temporary_table_statementContext* declare_global_temporary_table_statement();

  class  Describe_statementContext : public antlr4::ParserRuleContext {
  public:
    Describe_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DESCRIBE();
    Table_nameContext *table_name();
    Select_statementContext *select_statement();
    Call_statementContext *call_statement();
    antlr4::tree::TerminalNode *XQUERY();
    Xquery_statementContext *xquery_statement();
    antlr4::tree::TerminalNode *TABLE();
    antlr4::tree::TerminalNode *INDEXES();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *PARTITIONS();
    antlr4::tree::TerminalNode *OUTPUT();
    antlr4::tree::TerminalNode *SHOW();
    antlr4::tree::TerminalNode *DETAIL();
    antlr4::tree::TerminalNode *RELATIONAL();
    antlr4::tree::TerminalNode *XML();
    antlr4::tree::TerminalNode *TEXT();
    antlr4::tree::TerminalNode *SEARCH();

   
  };

  Describe_statementContext* describe_statement();

  class  Xquery_statementContext : public antlr4::ParserRuleContext {
  public:
    Xquery_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TodoContext *todo();

   
  };

  Xquery_statementContext* xquery_statement();

  class  Describe_input_statementContext : public antlr4::ParserRuleContext {
  public:
    Describe_input_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DESCRIBE();
    antlr4::tree::TerminalNode *INPUT();
    Statement_nameContext *statement_name();
    antlr4::tree::TerminalNode *INTO();
    Descriptor_nameContext *descriptor_name();

   
  };

  Describe_input_statementContext* describe_input_statement();

  class  Describe_output_statementContext : public antlr4::ParserRuleContext {
  public:
    Describe_output_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DESCRIBE();
    Statement_nameContext *statement_name();
    antlr4::tree::TerminalNode *INTO();
    Descriptor_nameContext *descriptor_name();
    antlr4::tree::TerminalNode *OUTPUT();

   
  };

  Describe_output_statementContext* describe_output_statement();

  class  Disconnect_statementContext : public antlr4::ParserRuleContext {
  public:
    Disconnect_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DISCONNECT();
    Server_nameContext *server_name();
    Host_variableContext *host_variable();
    antlr4::tree::TerminalNode *CURRENT();
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *SQL();

   
  };

  Disconnect_statementContext* disconnect_statement();

  class  End_declare_section_statementContext : public antlr4::ParserRuleContext {
  public:
    End_declare_section_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *END();
    antlr4::tree::TerminalNode *DECLARE();
    antlr4::tree::TerminalNode *SECTION();

   
  };

  End_declare_section_statementContext* end_declare_section_statement();

  class  Execute_statementContext : public antlr4::ParserRuleContext {
  public:
    Execute_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *EXECUTE();
    Statement_nameContext *statement_name();
    antlr4::tree::TerminalNode *INTO();
    antlr4::tree::TerminalNode *USING();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *ROWS();
    std::vector<Assignment_targetContext *> assignment_target();
    Assignment_targetContext* assignment_target(size_t i);
    std::vector<antlr4::tree::TerminalNode *> DESCRIPTOR();
    antlr4::tree::TerminalNode* DESCRIPTOR(size_t i);
    Descriptor_nameContext *descriptor_name();
    std::vector<Host_variable_expressionContext *> host_variable_expression();
    Host_variable_expressionContext* host_variable_expression(size_t i);
    Input_descriptor_nameContext *input_descriptor_name();
    Host_variableContext *host_variable();
    Integer_constantContext *integer_constant();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Execute_statementContext* execute_statement();

  class  Host_variable_expressionContext : public antlr4::ParserRuleContext {
  public:
    Host_variable_expressionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Host_variableContext *host_variable();
    ExpressionContext *expression();

   
  };

  Host_variable_expressionContext* host_variable_expression();

  class  Assignment_targetContext : public antlr4::ParserRuleContext {
  public:
    Assignment_targetContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Global_variable_nameContext *global_variable_name();
    Host_variable_nameContext *host_variable_name();
    Sql_parameter_nameContext *sql_parameter_name();
    Sql_variable_nameContext *sql_variable_name();
    Transition_variable_nameContext *transition_variable_name();
    Array_variable_nameContext *array_variable_name();
    antlr4::tree::TerminalNode *LEFT_SQR_BKT();
    Array_indexContext *array_index();
    antlr4::tree::TerminalNode *RIGHT_SQR_BKT();
    Field_referenceContext *field_reference();

   
  };

  Assignment_targetContext* assignment_target();

  class  Execute_immediate_statementContext : public antlr4::ParserRuleContext {
  public:
    Execute_immediate_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *EXECUTE();
    antlr4::tree::TerminalNode *IMMEDIATE();
    ExpressionContext *expression();

   
  };

  Execute_immediate_statementContext* execute_immediate_statement();

  class  Explain_statementContext : public antlr4::ParserRuleContext {
  public:
    Explain_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *EXPLAIN();
    std::vector<antlr4::tree::TerminalNode *> FOR();
    antlr4::tree::TerminalNode* FOR(size_t i);
    antlr4::tree::TerminalNode *PLAN();
    antlr4::tree::TerminalNode *SELECTION();
    antlr4::tree::TerminalNode *ALL();
    Explainable_sql_statementContext *explainable_sql_statement();
    antlr4::tree::TerminalNode *XQUERY();
    StringContext *string();
    antlr4::tree::TerminalNode *SNAPSHOT();
    std::vector<antlr4::tree::TerminalNode *> WITH();
    antlr4::tree::TerminalNode* WITH(size_t i);
    antlr4::tree::TerminalNode *REOPT();
    antlr4::tree::TerminalNode *ONCE();
    std::vector<antlr4::tree::TerminalNode *> SET();
    antlr4::tree::TerminalNode* SET(size_t i);
    antlr4::tree::TerminalNode *QUERYNO();
    std::vector<antlr4::tree::TerminalNode *> EQ();
    antlr4::tree::TerminalNode* EQ(size_t i);
    Integer_valueContext *integer_value();
    antlr4::tree::TerminalNode *QUERYTAG();
    String_constantContext *string_constant();

   
  };

  Explain_statementContext* explain_statement();

  class  Explainable_sql_statementContext : public antlr4::ParserRuleContext {
  public:
    Explainable_sql_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TodoContext *todo();

   
  };

  Explainable_sql_statementContext* explainable_sql_statement();

  class  Fetch_statementContext : public antlr4::ParserRuleContext {
  public:
    Fetch_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FETCH();
    Cursor_nameContext *cursor_name();
    Cursor_variable_nameContext *cursor_variable_name();
    antlr4::tree::TerminalNode *INTO();
    std::vector<Assignment_targetContext *> assignment_target();
    Assignment_targetContext* assignment_target(size_t i);
    antlr4::tree::TerminalNode *USING();
    antlr4::tree::TerminalNode *DESCRIPTOR();
    Descriptor_nameContext *descriptor_name();
    antlr4::tree::TerminalNode *FROM();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Fetch_statementContext* fetch_statement();

  class  Flush_bufferpools_statementContext : public antlr4::ParserRuleContext {
  public:
    Flush_bufferpools_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FLUSH();
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *BUFFERPOOL();
    antlr4::tree::TerminalNode *BUFFERPOOLS();

   
  };

  Flush_bufferpools_statementContext* flush_bufferpools_statement();

  class  Flush_event_monitor_statementContext : public antlr4::ParserRuleContext {
  public:
    Flush_event_monitor_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FLUSH();
    antlr4::tree::TerminalNode *EVENT();
    antlr4::tree::TerminalNode *MONITOR();
    Event_monitor_nameContext *event_monitor_name();
    antlr4::tree::TerminalNode *BUFFER();

   
  };

  Flush_event_monitor_statementContext* flush_event_monitor_statement();

  class  Flush_federated_cache_statementContext : public antlr4::ParserRuleContext {
  public:
    Flush_federated_cache_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FLUSH();
    antlr4::tree::TerminalNode *FEDERATED();
    antlr4::tree::TerminalNode *CACHE();
    antlr4::tree::TerminalNode *FOR();
    Remote_object_nameContext *remote_object_name();
    Data_source_nameContext *data_source_name();
    std::vector<antlr4::tree::TerminalNode *> DOT();
    antlr4::tree::TerminalNode* DOT(size_t i);
    Schema_nameContext *schema_name();
    std::vector<antlr4::tree::TerminalNode *> STAR();
    antlr4::tree::TerminalNode* STAR(size_t i);
    antlr4::tree::TerminalNode *SERVER();
    antlr4::tree::TerminalNode *ALL();

   
  };

  Flush_federated_cache_statementContext* flush_federated_cache_statement();

  class  Flush_optimization_profile_cache_statementContext : public antlr4::ParserRuleContext {
  public:
    Flush_optimization_profile_cache_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FLUSH();
    antlr4::tree::TerminalNode *OPTIMIZATION();
    antlr4::tree::TerminalNode *PROFILE();
    antlr4::tree::TerminalNode *CACHE();
    antlr4::tree::TerminalNode *ALL();
    Optimization_profile_nameContext *optimization_profile_name();

   
  };

  Flush_optimization_profile_cache_statementContext* flush_optimization_profile_cache_statement();

  class  Flush_package_cache_statementContext : public antlr4::ParserRuleContext {
  public:
    Flush_package_cache_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FLUSH();
    antlr4::tree::TerminalNode *PACKAGE();
    antlr4::tree::TerminalNode *CACHE();
    antlr4::tree::TerminalNode *DYNAMIC();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *EXECUTABLE();
    antlr4::tree::TerminalNode *ID();
    Executable_idContext *executable_id();
    antlr4::tree::TerminalNode *USING();
    antlr4::tree::TerminalNode *HARD();
    antlr4::tree::TerminalNode *INVALIDATION();

   
  };

  Flush_package_cache_statementContext* flush_package_cache_statement();

  class  Flush_authentication_cache_statementContext : public antlr4::ParserRuleContext {
  public:
    Flush_authentication_cache_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FLUSH();
    antlr4::tree::TerminalNode *AUTHENTICATION();
    antlr4::tree::TerminalNode *CACHE();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *ALL();

   
  };

  Flush_authentication_cache_statementContext* flush_authentication_cache_statement();

  class  Free_locator_statementContext : public antlr4::ParserRuleContext {
  public:
    Free_locator_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FREE();
    antlr4::tree::TerminalNode *LOCATOR();
    std::vector<Variable_nameContext *> variable_name();
    Variable_nameContext* variable_name(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Free_locator_statementContext* free_locator_statement();

  class  Get_diagnostics_statementContext : public antlr4::ParserRuleContext {
  public:
    Get_diagnostics_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GET();
    antlr4::tree::TerminalNode *DIAGNOSTICS();
    Statement_informationContext *statement_information();
    Condition_informationContext *condition_information();

   
  };

  Get_diagnostics_statementContext* get_diagnostics_statement();

  class  Statement_informationContext : public antlr4::ParserRuleContext {
  public:
    Statement_informationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    VariableContext *variable();
    antlr4::tree::TerminalNode *EQ();
    antlr4::tree::TerminalNode *DB2_RETURN_STATUS();
    antlr4::tree::TerminalNode *DB2_SQL_NESTING_LEVEL();
    antlr4::tree::TerminalNode *ROW_COUNT();

   
  };

  Statement_informationContext* statement_information();

  class  Condition_informationContext : public antlr4::ParserRuleContext {
  public:
    Condition_informationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *EXCEPTION();
    std::vector<Condition_var_assignmentContext *> condition_var_assignment();
    Condition_var_assignmentContext* condition_var_assignment(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Condition_informationContext* condition_information();

  class  Condition_var_assignmentContext : public antlr4::ParserRuleContext {
  public:
    Condition_var_assignmentContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    VariableContext *variable();
    antlr4::tree::TerminalNode *EQ();
    antlr4::tree::TerminalNode *DB2_TOKEN_STRING();
    antlr4::tree::TerminalNode *MESSAGE_TEXT();

   
  };

  Condition_var_assignmentContext* condition_var_assignment();

  class  Lock_table_statementContext : public antlr4::ParserRuleContext {
  public:
    Lock_table_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LOCK();
    antlr4::tree::TerminalNode *TABLE();
    antlr4::tree::TerminalNode *IN();
    antlr4::tree::TerminalNode *MODE();
    antlr4::tree::TerminalNode *SHARE();
    antlr4::tree::TerminalNode *EXCLUSIVE();
    Table_nameContext *table_name();
    Nick_nameContext *nick_name();

   
  };

  Lock_table_statementContext* lock_table_statement();

  class  Pipe_statementContext : public antlr4::ParserRuleContext {
  public:
    Pipe_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PIPE();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    Row_fullselectContext *row_fullselect();
    Row_expressionContext *row_expression();
    std::vector<ExpressionContext *> expression();
    ExpressionContext* expression(size_t i);
    std::vector<antlr4::tree::TerminalNode *> NULL_();
    antlr4::tree::TerminalNode* NULL_(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Pipe_statementContext* pipe_statement();

  class  Refresh_table_statementContext : public antlr4::ParserRuleContext {
  public:
    Refresh_table_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REFRESH();
    antlr4::tree::TerminalNode *TABLE();

   
  };

  Refresh_table_statementContext* refresh_table_statement();

  class  Release_connection_statementContext : public antlr4::ParserRuleContext {
  public:
    Release_connection_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RELEASE();
    Server_nameContext *server_name();
    Host_variableContext *host_variable();
    antlr4::tree::TerminalNode *CURRENT();
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *SQL();

   
  };

  Release_connection_statementContext* release_connection_statement();

  class  Rename_statementContext : public antlr4::ParserRuleContext {
  public:
    Rename_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RENAME();
    antlr4::tree::TerminalNode *TO();
    Target_identifierContext *target_identifier();
    Source_table_nameContext *source_table_name();
    antlr4::tree::TerminalNode *INDEX();
    Source_index_nameContext *source_index_name();
    antlr4::tree::TerminalNode *TABLE();

   
  };

  Rename_statementContext* rename_statement();

  class  Rename_stogroup_statementContext : public antlr4::ParserRuleContext {
  public:
    Rename_stogroup_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RENAME();
    antlr4::tree::TerminalNode *STOGROUP();
    Source_storagegroup_nameContext *source_storagegroup_name();
    antlr4::tree::TerminalNode *TO();
    Target_storagegroup_nameContext *target_storagegroup_name();

   
  };

  Rename_stogroup_statementContext* rename_stogroup_statement();

  class  Rename_tablespace_statementContext : public antlr4::ParserRuleContext {
  public:
    Rename_tablespace_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RENAME();
    antlr4::tree::TerminalNode *TABLESPACE();
    Source_tablespace_nameContext *source_tablespace_name();
    antlr4::tree::TerminalNode *TO();
    Target_tablespace_nameContext *target_tablespace_name();

   
  };

  Rename_tablespace_statementContext* rename_tablespace_statement();

  class  Set_statementContext : public antlr4::ParserRuleContext {
  public:
    Set_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SET();
    antlr4::tree::TerminalNode *COMPILATION();
    antlr4::tree::TerminalNode *ENVIRONMENT();
    Host_variableContext *host_variable();
    antlr4::tree::TerminalNode *CONNECTION();
    antlr4::tree::TerminalNode *CURRENT();
    antlr4::tree::TerminalNode *DECFLOAT();
    antlr4::tree::TerminalNode *ROUNDING();
    antlr4::tree::TerminalNode *MODE();
    antlr4::tree::TerminalNode *DEFAULT();
    antlr4::tree::TerminalNode *TRANSFORM();
    antlr4::tree::TerminalNode *GROUP();
    Group_nameContext *group_name();
    antlr4::tree::TerminalNode *DEGREE_();
    std::vector<antlr4::tree::TerminalNode *> EXPLAIN();
    antlr4::tree::TerminalNode* EXPLAIN(size_t i);
    antlr4::tree::TerminalNode *SNAPSHOT();
    antlr4::tree::TerminalNode *FEDERATED();
    antlr4::tree::TerminalNode *ASYNCHRONY();
    antlr4::tree::TerminalNode *IMPLICIT();
    antlr4::tree::TerminalNode *XMLPARSE();
    antlr4::tree::TerminalNode *OPTION();
    antlr4::tree::TerminalNode *ISOLATION();
    antlr4::tree::TerminalNode *LOCALE();
    antlr4::tree::TerminalNode *LOCK();
    antlr4::tree::TerminalNode *TIMEOUT();
    antlr4::tree::TerminalNode *MAINTAINED();
    antlr4::tree::TerminalNode *TYPES();
    antlr4::tree::TerminalNode *MDC();
    antlr4::tree::TerminalNode *ROLLOUT();
    antlr4::tree::TerminalNode *OPTIMIZATION();
    antlr4::tree::TerminalNode *PROFILE();
    antlr4::tree::TerminalNode *PACKAGE();
    antlr4::tree::TerminalNode *PATH();
    Pkg_opt_listContext *pkg_opt_list();
    antlr4::tree::TerminalNode *PACKAGESET();
    antlr4::tree::TerminalNode *QUERY();
    antlr4::tree::TerminalNode *REFRESH();
    antlr4::tree::TerminalNode *AGE();
    antlr4::tree::TerminalNode *SQL_CCFLAGS();
    antlr4::tree::TerminalNode *TEMPORAL();
    antlr4::tree::TerminalNode *ENCRYPTION();
    antlr4::tree::TerminalNode *PASSWORD();
    antlr4::tree::TerminalNode *EVENT();
    antlr4::tree::TerminalNode *MONITOR();
    Event_monitor_nameContext *event_monitor_name();
    antlr4::tree::TerminalNode *STATE();
    antlr4::tree::TerminalNode *INTEGRITY();
    antlr4::tree::TerminalNode *PASSTHRU();
    Path_opt_listContext *path_opt_list();
    antlr4::tree::TerminalNode *ROLE();
    Role_nameContext *role_name();
    antlr4::tree::TerminalNode *SCHEMA();
    std::vector<antlr4::tree::TerminalNode *> SERVER();
    antlr4::tree::TerminalNode* SERVER(size_t i);
    Server_option_nameContext *server_option_name();
    antlr4::tree::TerminalNode *TO();
    String_constantContext *string_constant();
    antlr4::tree::TerminalNode *FOR();
    Server_nameContext *server_name();
    antlr4::tree::TerminalNode *USAGE();
    antlr4::tree::TerminalNode *LIST();
    Usage_list_nameContext *usage_list_name();
    antlr4::tree::TerminalNode *UR();
    antlr4::tree::TerminalNode *CS();
    antlr4::tree::TerminalNode *RR();
    antlr4::tree::TerminalNode *RS();
    antlr4::tree::TerminalNode *RESET();
    antlr4::tree::TerminalNode *LC_MESSAGES_();
    antlr4::tree::TerminalNode *LC_TIME_();
    antlr4::tree::TerminalNode *BUSINESS_TIME();
    antlr4::tree::TerminalNode *SYSTEM_TIME();
    antlr4::tree::TerminalNode *ROUND_CEILING();
    antlr4::tree::TerminalNode *ROUND_DOWN();
    antlr4::tree::TerminalNode *ROUND_FLOOR();
    antlr4::tree::TerminalNode *ROUND_HALF_EVEN();
    antlr4::tree::TerminalNode *ROUND_HALF_UP();
    Yes_noContext *yes_no();
    antlr4::tree::TerminalNode *REOPT();
    antlr4::tree::TerminalNode *ANY();
    Integer_constantContext *integer_constant();
    antlr4::tree::TerminalNode *WAIT();
    antlr4::tree::TerminalNode *NULL_();
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *NONE();
    Maintain_opt_listContext *maintain_opt_list();
    antlr4::tree::TerminalNode *IMMEDIATE();
    antlr4::tree::TerminalNode *DEFERRED();
    Optimization_profile_nameContext *optimization_profile_name();
    antlr4::tree::TerminalNode *SINGLE_DIGIT();
    Numeric_constantContext *numeric_constant();
    VariableContext *variable();
    ExpressionContext *expression();
    antlr4::tree::TerminalNode *L_ZERO();
    antlr4::tree::TerminalNode *L_ONE();
    std::vector<Table_nameContext *> table_name();
    Table_nameContext* table_name(size_t i);
    std::vector<Table_checked_options_listContext *> table_checked_options_list();
    Table_checked_options_listContext* table_checked_options_list(size_t i);
    antlr4::tree::TerminalNode *CHECKED();
    std::vector<Table_unchecked_optionsContext *> table_unchecked_options();
    Table_unchecked_optionsContext* table_unchecked_options(size_t i);
    antlr4::tree::TerminalNode *UNCHECKED();
    antlr4::tree::TerminalNode *CURRENT_PATH();
    Schema_nameContext *schema_name();
    antlr4::tree::TerminalNode *USER();
    antlr4::tree::TerminalNode *SESSION_USER();
    antlr4::tree::TerminalNode *SYSTEM_USER();
    antlr4::tree::TerminalNode *CURRENT_USER();
    antlr4::tree::TerminalNode *SESSION();
    antlr4::tree::TerminalNode *AUTHORIZATION();
    Authorization_nameContext *authorization_name();
    antlr4::tree::TerminalNode *ACTIVE();
    antlr4::tree::TerminalNode *INACTIVE();
    antlr4::tree::TerminalNode *RELEASED();
    Var_def_listContext *var_def_list();
    Boolean_variable_nameContext *boolean_variable_name();
    antlr4::tree::TerminalNode *EQ();
    Array_variable_nameContext *array_variable_name();
    antlr4::tree::TerminalNode *LEFT_SQR_BKT();
    Array_indexContext *array_index();
    antlr4::tree::TerminalNode *RIGHT_SQR_BKT();
    Target_cursor_variableContext *target_cursor_variable();
    Target_row_variableContext *target_row_variable();
    antlr4::tree::TerminalNode *RECOMMEND();
    antlr4::tree::TerminalNode *EVALUATE();
    antlr4::tree::TerminalNode *INDEXES();
    antlr4::tree::TerminalNode *PARTITIONINGS();
    antlr4::tree::TerminalNode *TABLE();
    antlr4::tree::TerminalNode *OFF();
    Access_mode_clauseContext *access_mode_clause();
    Cascade_clauseContext *cascade_clause();
    antlr4::tree::TerminalNode *FULL();
    antlr4::tree::TerminalNode *ACCESS();
    antlr4::tree::TerminalNode *PRUNE();
    Search_conditionContext *search_condition();
    antlr4::tree::TerminalNode *TRUE();
    antlr4::tree::TerminalNode *FALSE();
    Cursor_variable_nameContext *cursor_variable_name();
    Cursor_value_constructorContext *cursor_value_constructor();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<Expr_nullContext *> expr_null();
    Expr_nullContext* expr_null(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    Row_fullselectContext *row_fullselect();
    Row_expressionContext *row_expression();
    antlr4::tree::TerminalNode *NORCAC();
    antlr4::tree::TerminalNode *NOT();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);
    Check_optionsContext *check_options();

   
  };

  Set_statementContext* set_statement();

  class  Access_mode_clauseContext : public antlr4::ParserRuleContext {
  public:
    Access_mode_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ACCESS();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *READ();

   
  };

  Access_mode_clauseContext* access_mode_clause();

  class  Cascade_clauseContext : public antlr4::ParserRuleContext {
  public:
    Cascade_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CASCADE();
    antlr4::tree::TerminalNode *IMMEDIATE();
    antlr4::tree::TerminalNode *DEFERRED();
    To_descendent_typesContext *to_descendent_types();

   
  };

  Cascade_clauseContext* cascade_clause();

  class  To_descendent_typesContext : public antlr4::ParserRuleContext {
  public:
    To_descendent_typesContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TO();
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *TABLES();
    Table_type_listContext *table_type_list();

   
  };

  To_descendent_typesContext* to_descendent_types();

  class  Table_type_listContext : public antlr4::ParserRuleContext {
  public:
    Table_type_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Table_typeContext *> table_type();
    Table_typeContext* table_type(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Table_type_listContext* table_type_list();

  class  Table_typeContext : public antlr4::ParserRuleContext {
  public:
    Table_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *MATERIALIZED();
    antlr4::tree::TerminalNode *QUERY();
    antlr4::tree::TerminalNode *TABLES();
    antlr4::tree::TerminalNode *FOREIGN();
    antlr4::tree::TerminalNode *KEY();
    antlr4::tree::TerminalNode *STAGING();

   
  };

  Table_typeContext* table_type();

  class  Table_checked_options_listContext : public antlr4::ParserRuleContext {
  public:
    Table_checked_options_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Table_checked_optionsContext *> table_checked_options();
    Table_checked_optionsContext* table_checked_options(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Table_checked_options_listContext* table_checked_options_list();

  class  Table_checked_optionsContext : public antlr4::ParserRuleContext {
  public:
    Table_checked_optionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Online_optionsContext *online_options();
    antlr4::tree::TerminalNode *GENERATE();
    antlr4::tree::TerminalNode *IDENTITY();
    Query_optimization_optionsContext *query_optimization_options();

   
  };

  Table_checked_optionsContext* table_checked_options();

  class  Online_optionsContext : public antlr4::ParserRuleContext {
  public:
    Online_optionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALLOW();
    antlr4::tree::TerminalNode *ACCESS();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *READ();
    antlr4::tree::TerminalNode *WRITE();

   
  };

  Online_optionsContext* online_options();

  class  Query_optimization_optionsContext : public antlr4::ParserRuleContext {
  public:
    Query_optimization_optionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *USING();
    std::vector<antlr4::tree::TerminalNode *> REFRESH();
    antlr4::tree::TerminalNode* REFRESH(size_t i);
    antlr4::tree::TerminalNode *DEFERRED();
    antlr4::tree::TerminalNode *TABLES();
    antlr4::tree::TerminalNode *ALLOW();
    antlr4::tree::TerminalNode *QUERY();
    antlr4::tree::TerminalNode *OPTIMIZATION();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *AGE();
    antlr4::tree::TerminalNode *ANY();

   
  };

  Query_optimization_optionsContext* query_optimization_options();

  class  Check_optionsContext : public antlr4::ParserRuleContext {
  public:
    Check_optionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Incremental_optionsContext *incremental_options();
    Exception_clauseContext *exception_clause();

   
  };

  Check_optionsContext* check_options();

  class  Incremental_optionsContext : public antlr4::ParserRuleContext {
  public:
    Incremental_optionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INCREMENTAL();
    antlr4::tree::TerminalNode *NOT();

   
  };

  Incremental_optionsContext* incremental_options();

  class  Exception_clauseContext : public antlr4::ParserRuleContext {
  public:
    Exception_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *EXCEPTION();
    std::vector<In_table_use_clauseContext *> in_table_use_clause();
    In_table_use_clauseContext* in_table_use_clause(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Exception_clauseContext* exception_clause();

  class  In_table_use_clauseContext : public antlr4::ParserRuleContext {
  public:
    In_table_use_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *IN();
    std::vector<Table_nameContext *> table_name();
    Table_nameContext* table_name(size_t i);
    antlr4::tree::TerminalNode *USE();

   
  };

  In_table_use_clauseContext* in_table_use_clause();

  class  Table_unchecked_optionsContext : public antlr4::ParserRuleContext {
  public:
    Table_unchecked_optionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Integrity_optionsContext *> integrity_options();
    Integrity_optionsContext* integrity_options(size_t i);
    std::vector<Full_accessContext *> full_access();
    Full_accessContext* full_access(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Table_unchecked_optionsContext* table_unchecked_options();

  class  Full_accessContext : public antlr4::ParserRuleContext {
  public:
    Full_accessContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FULL();
    antlr4::tree::TerminalNode *ACCESS();

   
  };

  Full_accessContext* full_access();

  class  Integrity_optionsContext : public antlr4::ParserRuleContext {
  public:
    Integrity_optionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALL();
    std::vector<Integrity_options_itemContext *> integrity_options_item();
    Integrity_options_itemContext* integrity_options_item(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Integrity_optionsContext* integrity_options();

  class  Integrity_options_itemContext : public antlr4::ParserRuleContext {
  public:
    Integrity_options_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FOREIGN();
    antlr4::tree::TerminalNode *KEY();
    antlr4::tree::TerminalNode *CHECK();
    antlr4::tree::TerminalNode *MATERIALIZED();
    antlr4::tree::TerminalNode *QUERY();
    antlr4::tree::TerminalNode *GENERATED();
    antlr4::tree::TerminalNode *COLUMN();
    antlr4::tree::TerminalNode *STAGING();

   
  };

  Integrity_options_itemContext* integrity_options_item();

  class  Var_def_listContext : public antlr4::ParserRuleContext {
  public:
    Var_def_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Var_defContext *> var_def();
    Var_defContext* var_def(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Var_def_listContext* var_def_list();

  class  Var_defContext : public antlr4::ParserRuleContext {
  public:
    Var_defContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Target_variableContext *> target_variable();
    Target_variableContext* target_variable(size_t i);
    antlr4::tree::TerminalNode *EQ();
    std::vector<Expr_null_defaultContext *> expr_null_default();
    Expr_null_defaultContext* expr_null_default(size_t i);
    std::vector<antlr4::tree::TerminalNode *> LEFT_RND_BKT();
    antlr4::tree::TerminalNode* LEFT_RND_BKT(size_t i);
    std::vector<antlr4::tree::TerminalNode *> RIGHT_RND_BKT();
    antlr4::tree::TerminalNode* RIGHT_RND_BKT(size_t i);
    Row_fullselectContext *row_fullselect();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Var_defContext* var_def();

  class  Expr_nullContext : public antlr4::ParserRuleContext {
  public:
    Expr_nullContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ExpressionContext *expression();
    antlr4::tree::TerminalNode *NULL_();

   
  };

  Expr_nullContext* expr_null();

  class  Expr_null_defaultContext : public antlr4::ParserRuleContext {
  public:
    Expr_null_defaultContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ExpressionContext *expression();
    antlr4::tree::TerminalNode *NULL_();
    antlr4::tree::TerminalNode *DEFAULT();

   
  };

  Expr_null_defaultContext* expr_null_default();

  class  Array_indexContext : public antlr4::ParserRuleContext {
  public:
    Array_indexContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TodoContext *todo();

   
  };

  Array_indexContext* array_index();

  class  Row_fullselectContext : public antlr4::ParserRuleContext {
  public:
    Row_fullselectContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TodoContext *todo();

   
  };

  Row_fullselectContext* row_fullselect();

  class  Target_variableContext : public antlr4::ParserRuleContext {
  public:
    Target_variableContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Global_variable_nameContext *global_variable_name();
    Host_variableContext *host_variable();
    Parameter_markerContext *parameter_marker();
    Sql_parameter_nameContext *sql_parameter_name();
    Field_referenceContext *field_reference();
    Sql_variable_nameContext *sql_variable_name();
    Transition_variable_nameContext *transition_variable_name();
    std::vector<Attribute_nameContext *> attribute_name();
    Attribute_nameContext* attribute_name(size_t i);

   
  };

  Target_variableContext* target_variable();

  class  Target_cursor_variableContext : public antlr4::ParserRuleContext {
  public:
    Target_cursor_variableContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TodoContext *todo();

   
  };

  Target_cursor_variableContext* target_cursor_variable();

  class  Target_row_variableContext : public antlr4::ParserRuleContext {
  public:
    Target_row_variableContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Global_variable_nameContext *global_variable_name();
    Parameter_markerContext *parameter_marker();
    Sql_parameter_nameContext *sql_parameter_name();
    Sql_variable_nameContext *sql_variable_name();
    Row_array_element_specificationContext *row_array_element_specification();
    Row_field_referenceContext *row_field_reference();

   
  };

  Target_row_variableContext* target_row_variable();

  class  Row_array_element_specificationContext : public antlr4::ParserRuleContext {
  public:
    Row_array_element_specificationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TodoContext *todo();

   
  };

  Row_array_element_specificationContext* row_array_element_specification();

  class  Row_field_referenceContext : public antlr4::ParserRuleContext {
  public:
    Row_field_referenceContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TodoContext *todo();

   
  };

  Row_field_referenceContext* row_field_reference();

  class  Field_referenceContext : public antlr4::ParserRuleContext {
  public:
    Field_referenceContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Row_variable_nameContext *row_variable_name();
    antlr4::tree::TerminalNode *DOT();
    Field_nameContext *field_name();

   
  };

  Field_referenceContext* field_reference();

  class  Search_conditionContext : public antlr4::ParserRuleContext {
  public:
    Search_conditionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    PredicateContext *predicate();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *SELECTIVITY();
    Numeric_constantContext *numeric_constant();
    std::vector<Search_conditionContext *> search_condition();
    Search_conditionContext* search_condition(size_t i);
    antlr4::tree::TerminalNode *AND();
    antlr4::tree::TerminalNode *OR();

   
  };

  Search_conditionContext* search_condition();
  Search_conditionContext* search_condition(int precedence);
  class  PredicateContext : public antlr4::ParserRuleContext {
  public:
    Db2Parser::ExpressionContext *e1 = nullptr;
    Db2Parser::ExpressionContext *e = nullptr;
    Db2Parser::StringContext *je = nullptr;
    Db2Parser::StringContext *jp = nullptr;
    Db2Parser::ExpressionContext *me = nullptr;
    Db2Parser::ExpressionContext *pe = nullptr;
    Db2Parser::ExpressionContext *ee = nullptr;
    Db2Parser::ExpressionContext *s1 = nullptr;
    Db2Parser::ExpressionContext *s2 = nullptr;
    Db2Parser::ExpressionContext *e2 = nullptr;
    Db2Parser::ExpressionContext *s = nullptr;
    Db2Parser::ExpressionContext *f = nullptr;
    Db2Parser::ExpressionContext *xe = nullptr;
    PredicateContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<ExpressionContext *> expression();
    ExpressionContext* expression(size_t i);
    Comparison_operatorContext *comparison_operator();
    std::vector<Row_value_expressionContext *> row_value_expression();
    Row_value_expressionContext* row_value_expression(size_t i);
    Some_any_allContext *some_any_all();
    Fullselect_in_parenthesesContext *fullselect_in_parentheses();
    antlr4::tree::TerminalNode *EQ();
    antlr4::tree::TerminalNode *IS();
    antlr4::tree::TerminalNode *TRUE();
    antlr4::tree::TerminalNode *FALSE();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *ARRAY_EXISTS();
    std::vector<antlr4::tree::TerminalNode *> LEFT_RND_BKT();
    antlr4::tree::TerminalNode* LEFT_RND_BKT(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);
    Array_indexContext *array_index();
    std::vector<antlr4::tree::TerminalNode *> RIGHT_RND_BKT();
    antlr4::tree::TerminalNode* RIGHT_RND_BKT(size_t i);
    antlr4::tree::TerminalNode *BETWEEN();
    antlr4::tree::TerminalNode *AND();
    Cursor_variable_nameContext *cursor_variable_name();
    antlr4::tree::TerminalNode *IN();
    antlr4::tree::TerminalNode *FOUND();
    antlr4::tree::TerminalNode *OPEN();
    antlr4::tree::TerminalNode *DISTINCT();
    antlr4::tree::TerminalNode *FROM();
    antlr4::tree::TerminalNode *EXISTS();
    Expression_list_in_parenthesesContext *expression_list_in_parentheses();
    antlr4::tree::TerminalNode *JSON_EXISTS();
    std::vector<StringContext *> string();
    StringContext* string(size_t i);
    antlr4::tree::TerminalNode *ON();
    std::vector<antlr4::tree::TerminalNode *> ERROR();
    antlr4::tree::TerminalNode* ERROR(size_t i);
    antlr4::tree::TerminalNode *FORMAT();
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *UNKNOWN();
    antlr4::tree::TerminalNode *JSON();
    antlr4::tree::TerminalNode *BSON();
    antlr4::tree::TerminalNode *LIKE();
    antlr4::tree::TerminalNode *ESCAPE();
    antlr4::tree::TerminalNode *NULL_();
    antlr4::tree::TerminalNode *OVERLAPS();
    antlr4::tree::TerminalNode *REGEXP_LIKE();
    antlr4::tree::TerminalNode *CODEUNITS32();
    antlr4::tree::TerminalNode *CODEUNITS16();
    antlr4::tree::TerminalNode *OCTETS();
    std::vector<Type_nameContext *> type_name();
    Type_nameContext* type_name(size_t i);
    antlr4::tree::TerminalNode *OF();
    antlr4::tree::TerminalNode *DYNAMIC();
    antlr4::tree::TerminalNode *TYPE();
    std::vector<antlr4::tree::TerminalNode *> ONLY();
    antlr4::tree::TerminalNode* ONLY(size_t i);
    antlr4::tree::TerminalNode *VALIDATED();
    According_to_clauseContext *according_to_clause();

   
  };

  PredicateContext* predicate();

  class  According_to_clauseContext : public antlr4::ParserRuleContext {
  public:
    According_to_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ACCORDING();
    antlr4::tree::TerminalNode *TO();
    antlr4::tree::TerminalNode *XMLSCHEMA();
    Xml_schema_identificationContext *xml_schema_identification();
    antlr4::tree::TerminalNode *IN();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Xml_schema_identification_listContext *xml_schema_identification_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();

   
  };

  According_to_clauseContext* according_to_clause();

  class  Xml_schema_identification_listContext : public antlr4::ParserRuleContext {
  public:
    Xml_schema_identification_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Xml_schema_identificationContext *> xml_schema_identification();
    Xml_schema_identificationContext* xml_schema_identification(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Xml_schema_identification_listContext* xml_schema_identification_list();

  class  Xml_schema_identificationContext : public antlr4::ParserRuleContext {
  public:
    Db2Parser::StringContext *u = nullptr;
    Db2Parser::StringContext *u2 = nullptr;
    Xml_schema_identificationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ID();
    Xml_schema_nameContext *xml_schema_name();
    antlr4::tree::TerminalNode *URI();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *NAMESPACE();
    std::vector<StringContext *> string();
    StringContext* string(size_t i);
    antlr4::tree::TerminalNode *LOCATION();

   
  };

  Xml_schema_identificationContext* xml_schema_identification();

  class  Fullselect_in_parenthesesContext : public antlr4::ParserRuleContext {
  public:
    Fullselect_in_parenthesesContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    FullselectContext *fullselect();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();

   
  };

  Fullselect_in_parenthesesContext* fullselect_in_parentheses();

  class  Some_any_allContext : public antlr4::ParserRuleContext {
  public:
    Some_any_allContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SOME();
    antlr4::tree::TerminalNode *ANY();
    antlr4::tree::TerminalNode *ALL();

   
  };

  Some_any_allContext* some_any_all();

  class  Row_value_expressionContext : public antlr4::ParserRuleContext {
  public:
    Row_value_expressionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Expression_list_in_parenthesesContext *expression_list_in_parentheses();

   
  };

  Row_value_expressionContext* row_value_expression();

  class  Comparison_operatorContext : public antlr4::ParserRuleContext {
  public:
    Comparison_operatorContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *EQ();
    antlr4::tree::TerminalNode *LTGT();
    antlr4::tree::TerminalNode *LT();
    antlr4::tree::TerminalNode *GT();
    antlr4::tree::TerminalNode *LE();
    antlr4::tree::TerminalNode *GE();

   
  };

  Comparison_operatorContext* comparison_operator();

  class  Row_expressionContext : public antlr4::ParserRuleContext {
  public:
    Row_expressionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ExpressionContext *expression();

   
  };

  Row_expressionContext* row_expression();

  class  Path_opt_listContext : public antlr4::ParserRuleContext {
  public:
    Path_opt_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Path_optContext *> path_opt();
    Path_optContext* path_opt(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Path_opt_listContext* path_opt_list();

  class  Path_optContext : public antlr4::ParserRuleContext {
  public:
    Path_optContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Schema_nameContext *schema_name();
    antlr4::tree::TerminalNode *SYSTEM();
    antlr4::tree::TerminalNode *PATH();
    antlr4::tree::TerminalNode *USER();
    antlr4::tree::TerminalNode *CURRENT();
    antlr4::tree::TerminalNode *CURRENT_PATH();
    antlr4::tree::TerminalNode *PACKAGE();
    Host_variableContext *host_variable();
    String_constantContext *string_constant();

   
  };

  Path_optContext* path_opt();

  class  Pkg_opt_listContext : public antlr4::ParserRuleContext {
  public:
    Pkg_opt_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Pkg_optContext *> pkg_opt();
    Pkg_optContext* pkg_opt(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Pkg_opt_listContext* pkg_opt_list();

  class  Pkg_optContext : public antlr4::ParserRuleContext {
  public:
    Pkg_optContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Schema_nameContext *schema_name();
    antlr4::tree::TerminalNode *CURRENT();
    antlr4::tree::TerminalNode *PATH();
    antlr4::tree::TerminalNode *USER();
    antlr4::tree::TerminalNode *PACKAGE();
    antlr4::tree::TerminalNode *CURRENT_PATH();
    antlr4::tree::TerminalNode *CURRENT_USER();
    antlr4::tree::TerminalNode *SESSION_USER();
    antlr4::tree::TerminalNode *SYSTEM_USER();
    Host_variableContext *host_variable();
    String_constantContext *string_constant();

   
  };

  Pkg_optContext* pkg_opt();

  class  Maintain_opt_listContext : public antlr4::ParserRuleContext {
  public:
    Maintain_opt_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Maintain_optContext *> maintain_opt();
    Maintain_optContext* maintain_opt(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Maintain_opt_listContext* maintain_opt_list();

  class  Maintain_optContext : public antlr4::ParserRuleContext {
  public:
    Maintain_optContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FEDERATED_TOOL();
    antlr4::tree::TerminalNode *SYSTEM();
    antlr4::tree::TerminalNode *USER();
    antlr4::tree::TerminalNode *REPLICATION();
    antlr4::tree::TerminalNode *CURRENT();
    antlr4::tree::TerminalNode *MAINTAINED();
    antlr4::tree::TerminalNode *TYPES();
    antlr4::tree::TerminalNode *TABLE();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *OPTIMIZATION();

   
  };

  Maintain_optContext* maintain_opt();

  class  VariableContext : public antlr4::ParserRuleContext {
  public:
    VariableContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  VariableContext* variable();

  class  Host_variableContext : public antlr4::ParserRuleContext {
  public:
    Host_variableContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *COLON();
    Id_Context *id_();

   
  };

  Host_variableContext* host_variable();

  class  Set_integrity_statementContext : public antlr4::ParserRuleContext {
  public:
    Set_integrity_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TodoContext *todo();

   
  };

  Set_integrity_statementContext* set_integrity_statement();

  class  Transfer_ownership_statementContext : public antlr4::ParserRuleContext {
  public:
    Transfer_ownership_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TRANSFER();
    antlr4::tree::TerminalNode *OWNERSHIP();
    antlr4::tree::TerminalNode *OF();
    ObjectsContext *objects();
    antlr4::tree::TerminalNode *TO();
    New_ownerContext *new_owner();
    antlr4::tree::TerminalNode *PRIVILEGES();
    antlr4::tree::TerminalNode *REVOKE();
    antlr4::tree::TerminalNode *PRESERVE();

   
  };

  Transfer_ownership_statementContext* transfer_ownership_statement();

  class  ObjectsContext : public antlr4::ParserRuleContext {
  public:
    ObjectsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Alias_designatorContext *alias_designator();
    antlr4::tree::TerminalNode *CONSTRAINT();
    Table_nameContext *table_name();
    antlr4::tree::TerminalNode *DOT();
    Constraint_nameContext *constraint_name();
    antlr4::tree::TerminalNode *DATABASE();
    antlr4::tree::TerminalNode *PARTITION();
    antlr4::tree::TerminalNode *GROUP();
    Db_partition_group_nameContext *db_partition_group_name();
    antlr4::tree::TerminalNode *EVENT();
    antlr4::tree::TerminalNode *MONITOR();
    Event_monitor_nameContext *event_monitor_name();
    Function_designatorContext *function_designator();
    antlr4::tree::TerminalNode *FUNCTION();
    antlr4::tree::TerminalNode *MAPPING();
    Function_mapping_nameContext *function_mapping_name();
    antlr4::tree::TerminalNode *INDEX();
    Index_nameContext *index_name();
    antlr4::tree::TerminalNode *EXTENSION();
    Index_extension_nameContext *index_extension_name();
    Method_designatorContext *method_designator();
    antlr4::tree::TerminalNode *NICKNAME();
    Nick_nameContext *nick_name();
    antlr4::tree::TerminalNode *PACKAGE();
    Package_nameContext *package_name();
    Version_idContext *version_id();
    antlr4::tree::TerminalNode *VERSION();
    Procedure_designatorContext *procedure_designator();
    antlr4::tree::TerminalNode *SCHEMA();
    Schema_nameContext *schema_name();
    antlr4::tree::TerminalNode *SEQUENCE();
    Sequence_nameContext *sequence_name();
    antlr4::tree::TerminalNode *TABLE();
    antlr4::tree::TerminalNode *HIERARCHY();
    Root_table_nameContext *root_table_name();
    antlr4::tree::TerminalNode *TABLESPACE();
    Tablespace_nameContext *tablespace_name();
    antlr4::tree::TerminalNode *TRIGGER();
    Trigger_nameContext *trigger_name();
    antlr4::tree::TerminalNode *TYPE();
    Type_nameContext *type_name();
    antlr4::tree::TerminalNode *DISTINCT();
    Type_mapping_nameContext *type_mapping_name();
    antlr4::tree::TerminalNode *VARIABLE();
    Variable_nameContext *variable_name();
    antlr4::tree::TerminalNode *VIEW();
    View_nameContext *view_name();
    Root_view_nameContext *root_view_name();
    antlr4::tree::TerminalNode *XSROBJECT();
    Xsrobject_nameContext *xsrobject_name();

   
  };

  ObjectsContext* objects();

  class  Whenever_statementContext : public antlr4::ParserRuleContext {
  public:
    Whenever_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WHENEVER();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *FOUND();
    antlr4::tree::TerminalNode *SQLERROR();
    antlr4::tree::TerminalNode *SQLWARNING();
    antlr4::tree::TerminalNode *CONTINUE();
    antlr4::tree::TerminalNode *COLON();
    Host_labelContext *host_label();
    antlr4::tree::TerminalNode *DO();
    antlr4::tree::TerminalNode *GOTO();
    antlr4::tree::TerminalNode *GO();
    antlr4::tree::TerminalNode *TO();
    Function_nameContext *function_name();
    antlr4::tree::TerminalNode *BREAK();

   
  };

  Whenever_statementContext* whenever_statement();

  class  For_statementContext : public antlr4::ParserRuleContext {
  public:
    For_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<antlr4::tree::TerminalNode *> FOR();
    antlr4::tree::TerminalNode* FOR(size_t i);
    For_loop_nameContext *for_loop_name();
    antlr4::tree::TerminalNode *AS();
    Select_statementContext *select_statement();
    antlr4::tree::TerminalNode *DO();
    Sql_routine_statementContext *sql_routine_statement();
    antlr4::tree::TerminalNode *END();
    std::vector<LabelContext *> label();
    LabelContext* label(size_t i);
    Cursor_nameContext *cursor_name();
    antlr4::tree::TerminalNode *CURSOR();
    antlr4::tree::TerminalNode *WITHOUT();
    antlr4::tree::TerminalNode *HOLD();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *ASENSITIVE();
    antlr4::tree::TerminalNode *INSENSITIVE();

   
  };

  For_statementContext* for_statement();

  class  Goto_statementContext : public antlr4::ParserRuleContext {
  public:
    Goto_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GOTO();
    LabelContext *label();

   
  };

  Goto_statementContext* goto_statement();

  class  If_statementContext : public antlr4::ParserRuleContext {
  public:
    If_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<antlr4::tree::TerminalNode *> IF();
    antlr4::tree::TerminalNode* IF(size_t i);
    std::vector<Search_conditionContext *> search_condition();
    Search_conditionContext* search_condition(size_t i);
    std::vector<antlr4::tree::TerminalNode *> THEN();
    antlr4::tree::TerminalNode* THEN(size_t i);
    std::vector<Sql_routine_statementContext *> sql_routine_statement();
    Sql_routine_statementContext* sql_routine_statement(size_t i);
    antlr4::tree::TerminalNode *END();
    std::vector<antlr4::tree::TerminalNode *> ELSEIF();
    antlr4::tree::TerminalNode* ELSEIF(size_t i);
    antlr4::tree::TerminalNode *ELSE();

   
  };

  If_statementContext* if_statement();

  class  Include_statementContext : public antlr4::ParserRuleContext {
  public:
    Include_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INCLUDE();
    antlr4::tree::TerminalNode *SQLCA();
    antlr4::tree::TerminalNode *SQLDA();
    NameContext *name();

   
  };

  Include_statementContext* include_statement();

  class  Resignal_statementContext : public antlr4::ParserRuleContext {
  public:
    Resignal_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RESIGNAL();
    antlr4::tree::TerminalNode *SQLSTATE();
    Condition_nameContext *condition_name();
    Signal_informationContext *signal_information();
    Sqlstate_string_constantContext *sqlstate_string_constant();
    Sqlstate_string_variableContext *sqlstate_string_variable();
    antlr4::tree::TerminalNode *VALUE();

   
  };

  Resignal_statementContext* resignal_statement();

  class  Signal_informationContext : public antlr4::ParserRuleContext {
  public:
    Signal_informationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SET();
    antlr4::tree::TerminalNode *MESSAGE_TEXT();
    antlr4::tree::TerminalNode *EQ();
    Sql_variable_nameContext *sql_variable_name();
    Sql_parameter_nameContext *sql_parameter_name();
    Diagnostic_string_constantContext *diagnostic_string_constant();

   
  };

  Signal_informationContext* signal_information();

  class  Diagnostic_string_constantContext : public antlr4::ParserRuleContext {
  public:
    Diagnostic_string_constantContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TodoContext *todo();

   
  };

  Diagnostic_string_constantContext* diagnostic_string_constant();

  class  Signal_statementContext : public antlr4::ParserRuleContext {
  public:
    Signal_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SIGNAL();
    antlr4::tree::TerminalNode *SQLSTATE();
    Condition_nameContext *condition_name();
    Signal_information_2Context *signal_information_2();
    Sqlstate_string_constantContext *sqlstate_string_constant();
    Sqlstate_string_variableContext *sqlstate_string_variable();
    antlr4::tree::TerminalNode *VALUE();

   
  };

  Signal_statementContext* signal_statement();

  class  Sqlstate_string_constantContext : public antlr4::ParserRuleContext {
  public:
    Sqlstate_string_constantContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TodoContext *todo();

   
  };

  Sqlstate_string_constantContext* sqlstate_string_constant();

  class  Sqlstate_string_variableContext : public antlr4::ParserRuleContext {
  public:
    Sqlstate_string_variableContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TodoContext *todo();

   
  };

  Sqlstate_string_variableContext* sqlstate_string_variable();

  class  Signal_information_2Context : public antlr4::ParserRuleContext {
  public:
    Signal_information_2Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SET();
    antlr4::tree::TerminalNode *MESSAGE_TEXT();
    antlr4::tree::TerminalNode *EQ();
    Diagnostic_string_expressionContext *diagnostic_string_expression();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();

   
  };

  Signal_information_2Context* signal_information_2();

  class  Diagnostic_string_expressionContext : public antlr4::ParserRuleContext {
  public:
    Diagnostic_string_expressionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TodoContext *todo();

   
  };

  Diagnostic_string_expressionContext* diagnostic_string_expression();

  class  Iterate_statementContext : public antlr4::ParserRuleContext {
  public:
    Iterate_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ITERATE();
    LabelContext *label();

   
  };

  Iterate_statementContext* iterate_statement();

  class  Leave_statementContext : public antlr4::ParserRuleContext {
  public:
    Leave_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEAVE();
    LabelContext *label();

   
  };

  Leave_statementContext* leave_statement();

  class  Loop_statementContext : public antlr4::ParserRuleContext {
  public:
    Loop_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<antlr4::tree::TerminalNode *> LOOP();
    antlr4::tree::TerminalNode* LOOP(size_t i);
    Sql_routine_statementContext *sql_routine_statement();
    antlr4::tree::TerminalNode *END();
    std::vector<LabelContext *> label();
    LabelContext* label(size_t i);
    antlr4::tree::TerminalNode *COLON();

   
  };

  Loop_statementContext* loop_statement();

  class  Open_statementContext : public antlr4::ParserRuleContext {
  public:
    Open_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OPEN();
    Cursor_nameContext *cursor_name();
    Cursor_variable_nameContext *cursor_variable_name();
    antlr4::tree::TerminalNode *USING();
    std::vector<Variable_or_expressionContext *> variable_or_expression();
    Variable_or_expressionContext* variable_or_expression(size_t i);
    antlr4::tree::TerminalNode *DESCRIPTOR();
    Descriptor_nameContext *descriptor_name();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);
    std::vector<ExpressionContext *> expression();
    ExpressionContext* expression(size_t i);

   
  };

  Open_statementContext* open_statement();

  class  Variable_or_expressionContext : public antlr4::ParserRuleContext {
  public:
    Variable_or_expressionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    VariableContext *variable();
    ExpressionContext *expression();

   
  };

  Variable_or_expressionContext* variable_or_expression();

  class  Select_into_statementContext : public antlr4::ParserRuleContext {
  public:
    Select_into_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Select_clauseContext *select_clause();
    antlr4::tree::TerminalNode *INTO();
    std::vector<Assignment_targetContext *> assignment_target();
    Assignment_targetContext* assignment_target(size_t i);
    From_clauseContext *from_clause();
    antlr4::tree::TerminalNode *WITH();
    Common_table_expression_listContext *common_table_expression_list();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);
    Where_clauseContext *where_clause();
    Group_by_clauseContext *group_by_clause();
    Having_clauseContext *having_clause();
    Order_by_clauseContext *order_by_clause();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *READ();
    antlr4::tree::TerminalNode *ONLY();
    antlr4::tree::TerminalNode *UPDATE();
    Isolation_clauseContext *isolation_clause();
    Offset_clauseContext *offset_clause();
    Fetch_clauseContext *fetch_clause();
    antlr4::tree::TerminalNode *OF();
    Column_name_listContext *column_name_list();

   
  };

  Select_into_statementContext* select_into_statement();

  class  Values_into_statementContext : public antlr4::ParserRuleContext {
  public:
    Values_into_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *VALUES();
    antlr4::tree::TerminalNode *INTO();
    std::vector<Assignment_targetContext *> assignment_target();
    Assignment_targetContext* assignment_target(size_t i);
    ExpressionContext *expression();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Expression_listContext *expression_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    Row_expressionContext *row_expression();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Values_into_statementContext* values_into_statement();

  class  Prepare_statementContext : public antlr4::ParserRuleContext {
  public:
    Prepare_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PREPARE();
    Statement_nameContext *statement_name();
    antlr4::tree::TerminalNode *FROM();
    Host_variableContext *host_variable();
    ExpressionContext *expression();
    std::vector<antlr4::tree::TerminalNode *> INTO();
    antlr4::tree::TerminalNode* INTO(size_t i);
    Result_descriptor_nameContext *result_descriptor_name();
    antlr4::tree::TerminalNode *INPUT();
    Input_descriptor_nameContext *input_descriptor_name();
    antlr4::tree::TerminalNode *OUTPUT();

   
  };

  Prepare_statementContext* prepare_statement();

  class  Repeat_statementContext : public antlr4::ParserRuleContext {
  public:
    Repeat_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<antlr4::tree::TerminalNode *> REPEAT();
    antlr4::tree::TerminalNode* REPEAT(size_t i);
    Sql_routine_statementContext *sql_routine_statement();
    antlr4::tree::TerminalNode *UNTIL();
    Search_conditionContext *search_condition();
    antlr4::tree::TerminalNode *END();
    std::vector<LabelContext *> label();
    LabelContext* label(size_t i);

   
  };

  Repeat_statementContext* repeat_statement();

  class  Return_statementContext : public antlr4::ParserRuleContext {
  public:
    Return_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RETURN();
    ExpressionContext *expression();
    antlr4::tree::TerminalNode *NULL_();
    FullselectContext *fullselect();
    antlr4::tree::TerminalNode *WITH();
    Common_table_expression_listContext *common_table_expression_list();

   
  };

  Return_statementContext* return_statement();

  class  While_statementContext : public antlr4::ParserRuleContext {
  public:
    While_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<antlr4::tree::TerminalNode *> WHILE();
    antlr4::tree::TerminalNode* WHILE(size_t i);
    Search_conditionContext *search_condition();
    antlr4::tree::TerminalNode *DO();
    Sql_routine_statementContext *sql_routine_statement();
    antlr4::tree::TerminalNode *END();
    std::vector<LabelContext *> label();
    LabelContext* label(size_t i);

   
  };

  While_statementContext* while_statement();

  class  Sql_routine_statementContext : public antlr4::ParserRuleContext {
  public:
    Sql_routine_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Sql_procedure_statementContext *> sql_procedure_statement();
    Sql_procedure_statementContext* sql_procedure_statement(size_t i);
    std::vector<antlr4::tree::TerminalNode *> SEMI();
    antlr4::tree::TerminalNode* SEMI(size_t i);
    std::vector<Sql_function_statementContext *> sql_function_statement();
    Sql_function_statementContext* sql_function_statement(size_t i);

   
  };

  Sql_routine_statementContext* sql_routine_statement();

  class  Common_table_expressionContext : public antlr4::ParserRuleContext {
  public:
    Common_table_expressionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Table_nameContext *table_name();
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    FullselectContext *fullselect();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    Column_name_list_parenContext *column_name_list_paren();
    antlr4::tree::TerminalNode *WITH();
    Common_table_expression_listContext *common_table_expression_list();

   
  };

  Common_table_expressionContext* common_table_expression();

  class  Create_alias_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_alias_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *ALIAS();
    Table_aliasContext *table_alias();
    Module_aliasContext *module_alias();
    Sequence_aliasContext *sequence_alias();
    Or_replaceContext *or_replace();
    antlr4::tree::TerminalNode *PUBLIC();

   
  };

  Create_alias_statementContext* create_alias_statement();

  class  Declared_alias_nameContext : public antlr4::ParserRuleContext {
  public:
    Declared_alias_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Alias_nameContext *alias_name();

   
  };

  Declared_alias_nameContext* declared_alias_name();

  class  Table_aliasContext : public antlr4::ParserRuleContext {
  public:
    Table_aliasContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Declared_alias_nameContext *declared_alias_name();
    antlr4::tree::TerminalNode *FOR();
    Table_or_view_nameContext *table_or_view_name();
    Nick_nameContext *nick_name();
    Alias_nameContext *alias_name();
    antlr4::tree::TerminalNode *TABLE();

   
  };

  Table_aliasContext* table_alias();

  class  Module_aliasContext : public antlr4::ParserRuleContext {
  public:
    Module_aliasContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Declared_alias_nameContext *declared_alias_name();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *MODULE();
    Module_nameContext *module_name();
    Alias_nameContext *alias_name();

   
  };

  Module_aliasContext* module_alias();

  class  Sequence_aliasContext : public antlr4::ParserRuleContext {
  public:
    Sequence_aliasContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Declared_alias_nameContext *declared_alias_name();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *SEQUENCE();
    Sequence_nameContext *sequence_name();
    Alias_nameContext *alias_name();

   
  };

  Sequence_aliasContext* sequence_alias();

  class  Or_replaceContext : public antlr4::ParserRuleContext {
  public:
    Or_replaceContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OR();
    antlr4::tree::TerminalNode *REPLACE();

   
  };

  Or_replaceContext* or_replace();

  class  Create_audit_policy_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_audit_policy_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *AUDIT();
    antlr4::tree::TerminalNode *POLICY();
    Policy_nameContext *policy_name();
    std::vector<Audit_policy_optsContext *> audit_policy_opts();
    Audit_policy_optsContext* audit_policy_opts(size_t i);

   
  };

  Create_audit_policy_statementContext* create_audit_policy_statement();

  class  Audit_policy_optsContext : public antlr4::ParserRuleContext {
  public:
    Audit_policy_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CATEGORIES();
    std::vector<Audit_policy_categories_optsContext *> audit_policy_categories_opts();
    Audit_policy_categories_optsContext* audit_policy_categories_opts(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);
    antlr4::tree::TerminalNode *ERROR();
    antlr4::tree::TerminalNode *TYPE();
    Normal_auditContext *normal_audit();

   
  };

  Audit_policy_optsContext* audit_policy_opts();

  class  Audit_policy_categories_optsContext : public antlr4::ParserRuleContext {
  public:
    Audit_policy_categories_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Status_specContext *status_spec();
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *AUDIT();
    antlr4::tree::TerminalNode *CHECKING();
    antlr4::tree::TerminalNode *CONTEXT();
    antlr4::tree::TerminalNode *EXECUTE();
    antlr4::tree::TerminalNode *OBJMAINT();
    antlr4::tree::TerminalNode *SECMAINT();
    antlr4::tree::TerminalNode *SYSADMIN();
    antlr4::tree::TerminalNode *VALIDATE();
    With_withoutContext *with_without();
    antlr4::tree::TerminalNode *DATA();

   
  };

  Audit_policy_categories_optsContext* audit_policy_categories_opts();

  class  Create_bufferpool_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_bufferpool_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *BUFFERPOOL();
    Bufferpool_nameContext *bufferpool_name();
    Bufferpool_optsContext *bufferpool_opts();
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *DBPARTITIONNUMS();
    antlr4::tree::TerminalNode *DATABASE();
    antlr4::tree::TerminalNode *PARTITION();
    antlr4::tree::TerminalNode *GROUP();
    std::vector<Db_partition_group_nameContext *> db_partition_group_name();
    Db_partition_group_nameContext* db_partition_group_name(size_t i);
    antlr4::tree::TerminalNode *SIZE();
    antlr4::tree::TerminalNode *IMMEDIATE();
    antlr4::tree::TerminalNode *DEFERRED();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);
    Number_of_pagesContext *number_of_pages();
    antlr4::tree::TerminalNode *AUTOMATIC();

   
  };

  Create_bufferpool_statementContext* create_bufferpool_statement();

  class  Bufferpool_optsContext : public antlr4::ParserRuleContext {
  public:
    Bufferpool_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Except_clauseContext *except_clause();
    antlr4::tree::TerminalNode *NUMBLOCKPAGES();
    std::vector<Number_of_pagesContext *> number_of_pages();
    Number_of_pagesContext* number_of_pages(size_t i);
    antlr4::tree::TerminalNode *BLOCKSIZE();
    antlr4::tree::TerminalNode *PAGESIZE();
    Integer_valueContext *integer_value();
    antlr4::tree::TerminalNode *K();

   
  };

  Bufferpool_optsContext* bufferpool_opts();

  class  Except_clauseContext : public antlr4::ParserRuleContext {
  public:
    Except_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *EXCEPT();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Member_listContext *member_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *MEMBER();
    antlr4::tree::TerminalNode *MEMBERS();

   
  };

  Except_clauseContext* except_clause();

  class  Member_listContext : public antlr4::ParserRuleContext {
  public:
    Member_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Member_list_itemContext *> member_list_item();
    Member_list_itemContext* member_list_item(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Member_listContext* member_list();

  class  Member_list_itemContext : public antlr4::ParserRuleContext {
  public:
    Member_list_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Member_numberContext *> member_number();
    Member_numberContext* member_number(size_t i);
    antlr4::tree::TerminalNode *SIZE();
    Number_of_pagesContext *number_of_pages();
    antlr4::tree::TerminalNode *TO();

   
  };

  Member_list_itemContext* member_list_item();

  class  Create_database_partition_group_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_database_partition_group_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *DATABASE();
    antlr4::tree::TerminalNode *PARTITION();
    antlr4::tree::TerminalNode *GROUP();
    Db_partition_group_nameContext *db_partition_group_name();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *DBPARTITIONNUMS();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Db_partition_number_listContext *db_partition_number_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *DBPARTITIONNUM();

   
  };

  Create_database_partition_group_statementContext* create_database_partition_group_statement();

  class  Create_event_monitor_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_event_monitor_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *EVENT();
    antlr4::tree::TerminalNode *MONITOR();
    Event_monitor_nameContext *event_monitor_name();
    antlr4::tree::TerminalNode *FOR();
    TodoContext *todo();

   
  };

  Create_event_monitor_statementContext* create_event_monitor_statement();

  class  Create_event_monitor_activities_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_event_monitor_activities_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *EVENT();
    antlr4::tree::TerminalNode *MONITOR();
    Event_monitor_nameContext *event_monitor_name();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *ACTIVITIES();
    antlr4::tree::TerminalNode *WRITE();
    antlr4::tree::TerminalNode *TO();
    antlr4::tree::TerminalNode *TABLE();
    antlr4::tree::TerminalNode *PIPE();
    Pipe_nameContext *pipe_name();
    antlr4::tree::TerminalNode *FILE();
    Path_nameContext *path_name();
    File_optionsContext *file_options();
    std::vector<Formatted_event_table_info_3Context *> formatted_event_table_info_3();
    Formatted_event_table_info_3Context* formatted_event_table_info_3(size_t i);

   
  };

  Create_event_monitor_activities_statementContext* create_event_monitor_activities_statement();

  class  Formatted_event_table_info_3Context : public antlr4::ParserRuleContext {
  public:
    Formatted_event_table_info_3Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Evm_groupContext *evm_group();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<Target_table_optionsContext *> target_table_options();
    Target_table_optionsContext* target_table_options(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);
    antlr4::tree::TerminalNode *BLOCKED();

   
  };

  Formatted_event_table_info_3Context* formatted_event_table_info_3();

  class  Create_event_monitor_change_history_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_event_monitor_change_history_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    std::vector<antlr4::tree::TerminalNode *> EVENT();
    antlr4::tree::TerminalNode* EVENT(size_t i);
    antlr4::tree::TerminalNode *MONITOR();
    Event_monitor_nameContext *event_monitor_name();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *CHANGE();
    antlr4::tree::TerminalNode *HISTORY();
    antlr4::tree::TerminalNode *WHERE();
    antlr4::tree::TerminalNode *IN();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *WRITE();
    antlr4::tree::TerminalNode *TO();
    antlr4::tree::TerminalNode *TABLE();
    Formatted_event_table_infoContext *formatted_event_table_info();
    Event_control_listContext *event_control_list();
    Autostart_manualstartContext *autostart_manualstart();

   
  };

  Create_event_monitor_change_history_statementContext* create_event_monitor_change_history_statement();

  class  Event_control_listContext : public antlr4::ParserRuleContext {
  public:
    Event_control_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Event_controlContext *> event_control();
    Event_controlContext* event_control(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Event_control_listContext* event_control_list();

  class  Event_controlContext : public antlr4::ParserRuleContext {
  public:
    Event_controlContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *ADC();
    antlr4::tree::TerminalNode *BACKUP();
    antlr4::tree::TerminalNode *CFGALL();
    antlr4::tree::TerminalNode *DBCFG();
    antlr4::tree::TerminalNode *DBCFGVALUES();
    antlr4::tree::TerminalNode *DBMCFG();
    antlr4::tree::TerminalNode *CBMCFGVALUES();
    antlr4::tree::TerminalNode *DDLALL();
    antlr4::tree::TerminalNode *DDLDATA();
    antlr4::tree::TerminalNode *DDLFEDERATED();
    antlr4::tree::TerminalNode *DDLMONITOR();
    antlr4::tree::TerminalNode *DDLSECURITY();
    antlr4::tree::TerminalNode *DDLSQL();
    antlr4::tree::TerminalNode *DDLSTORAGE();
    antlr4::tree::TerminalNode *DDLWLM();
    antlr4::tree::TerminalNode *DDLXML();
    antlr4::tree::TerminalNode *LOAD();
    antlr4::tree::TerminalNode *MOVETABLE();
    antlr4::tree::TerminalNode *ONLINERECOVERY();
    antlr4::tree::TerminalNode *REDISTRIBUTE();
    antlr4::tree::TerminalNode *REGVAR();
    antlr4::tree::TerminalNode *REGVARVALUES();
    antlr4::tree::TerminalNode *REORG();
    antlr4::tree::TerminalNode *RESTORE();
    antlr4::tree::TerminalNode *ROLLFORWARD();
    antlr4::tree::TerminalNode *RUNSTATS();
    antlr4::tree::TerminalNode *UTILALL();

   
  };

  Event_controlContext* event_control();

  class  Create_event_monitor_locking_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_event_monitor_locking_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    std::vector<antlr4::tree::TerminalNode *> EVENT();
    antlr4::tree::TerminalNode* EVENT(size_t i);
    antlr4::tree::TerminalNode *MONITOR();
    Event_monitor_nameContext *event_monitor_name();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *LOCKING();
    antlr4::tree::TerminalNode *WRITE();
    antlr4::tree::TerminalNode *TO();
    antlr4::tree::TerminalNode *TABLE();
    Formatted_event_table_infoContext *formatted_event_table_info();
    antlr4::tree::TerminalNode *UNFORMATTED();
    Autostart_manualstartContext *autostart_manualstart();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Target_table_optionsContext *target_table_options();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();

   
  };

  Create_event_monitor_locking_statementContext* create_event_monitor_locking_statement();

  class  Create_event_monitor_package_cache_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_event_monitor_package_cache_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    std::vector<antlr4::tree::TerminalNode *> EVENT();
    antlr4::tree::TerminalNode* EVENT(size_t i);
    antlr4::tree::TerminalNode *MONITOR();
    Event_monitor_nameContext *event_monitor_name();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *PACKAGE();
    antlr4::tree::TerminalNode *CACHE();
    Filter_and_collection_optionsContext *filter_and_collection_options();
    antlr4::tree::TerminalNode *WRITE();
    antlr4::tree::TerminalNode *TO();
    antlr4::tree::TerminalNode *TABLE();
    Formatted_event_table_infoContext *formatted_event_table_info();
    antlr4::tree::TerminalNode *UNFORMATTED();
    Autostart_manualstartContext *autostart_manualstart();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Target_table_optionsContext *target_table_options();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();

   
  };

  Create_event_monitor_package_cache_statementContext* create_event_monitor_package_cache_statement();

  class  Filter_and_collection_optionsContext : public antlr4::ParserRuleContext {
  public:
    Filter_and_collection_optionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Where_clauseContext *where_clause();
    antlr4::tree::TerminalNode *COLLECT();
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *BASE();
    antlr4::tree::TerminalNode *DETAILED();

   
  };

  Filter_and_collection_optionsContext* filter_and_collection_options();

  class  Event_conditionContext : public antlr4::ParserRuleContext {
  public:
    Event_conditionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Event_condition_itemContext *> event_condition_item();
    Event_condition_itemContext* event_condition_item(size_t i);
    std::vector<antlr4::tree::TerminalNode *> AND();
    antlr4::tree::TerminalNode* AND(size_t i);

   
  };

  Event_conditionContext* event_condition();

  class  Event_condition_itemContext : public antlr4::ParserRuleContext {
  public:
    Event_condition_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *UPDATED_SINCE_BOUNDARY_TIME();
    Integer_constantContext *integer_constant();
    antlr4::tree::TerminalNode *NUM_EXECUTIONS();
    antlr4::tree::TerminalNode *STMT_EXEC_TIME();
    antlr4::tree::TerminalNode *GT();
    antlr4::tree::TerminalNode *LT();
    antlr4::tree::TerminalNode *LE();
    antlr4::tree::TerminalNode *EQ();
    antlr4::tree::TerminalNode *GE();

   
  };

  Event_condition_itemContext* event_condition_item();

  class  Create_event_monitor_statistics_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_event_monitor_statistics_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *EVENT();
    antlr4::tree::TerminalNode *MONITOR();
    Event_monitor_nameContext *event_monitor_name();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *STATISTICS();
    antlr4::tree::TerminalNode *WRITE();
    antlr4::tree::TerminalNode *TO();
    Event_monitor_statistics_optsContext *event_monitor_statistics_opts();
    antlr4::tree::TerminalNode *TABLE();
    Formatted_event_table_info_2Context *formatted_event_table_info_2();
    antlr4::tree::TerminalNode *PIPE();
    Pipe_nameContext *pipe_name();
    antlr4::tree::TerminalNode *FILE();
    Path_nameContext *path_name();
    File_optionsContext *file_options();

   
  };

  Create_event_monitor_statistics_statementContext* create_event_monitor_statistics_statement();

  class  Event_monitor_statistics_optsContext : public antlr4::ParserRuleContext {
  public:
    Event_monitor_statistics_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *AUTOSTART();
    antlr4::tree::TerminalNode *MANUALSTART();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *MEMBER();
    Member_numberContext *member_number();
    antlr4::tree::TerminalNode *LOCAL();

   
  };

  Event_monitor_statistics_optsContext* event_monitor_statistics_opts();

  class  Create_event_monitor_threshold_violations_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_event_monitor_threshold_violations_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *EVENT();
    antlr4::tree::TerminalNode *MONITOR();
    Event_monitor_nameContext *event_monitor_name();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *THRESHOLD();
    antlr4::tree::TerminalNode *VIOLATIONS();
    antlr4::tree::TerminalNode *WRITE();
    antlr4::tree::TerminalNode *TO();
    antlr4::tree::TerminalNode *TABLE();
    antlr4::tree::TerminalNode *PIPE();
    Pipe_nameContext *pipe_name();
    antlr4::tree::TerminalNode *FILE();
    Path_nameContext *path_name();
    File_optionsContext *file_options();
    std::vector<Event_monitor_threshold_optsContext *> event_monitor_threshold_opts();
    Event_monitor_threshold_optsContext* event_monitor_threshold_opts(size_t i);
    std::vector<Formatted_event_table_info_2Context *> formatted_event_table_info_2();
    Formatted_event_table_info_2Context* formatted_event_table_info_2(size_t i);

   
  };

  Create_event_monitor_threshold_violations_statementContext* create_event_monitor_threshold_violations_statement();

  class  Formatted_event_table_info_2Context : public antlr4::ParserRuleContext {
  public:
    Formatted_event_table_info_2Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Evm_groupContext *evm_group();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<Target_table_optionsContext *> target_table_options();
    Target_table_optionsContext* target_table_options(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);
    antlr4::tree::TerminalNode *BUFFERSIZE();
    PagesContext *pages();
    antlr4::tree::TerminalNode *BLOCKED();
    antlr4::tree::TerminalNode *NONBLOCKED();

   
  };

  Formatted_event_table_info_2Context* formatted_event_table_info_2();

  class  File_optionsContext : public antlr4::ParserRuleContext {
  public:
    File_optionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *MAXFILES();
    antlr4::tree::TerminalNode *NONE();
    Number_of_filesContext *number_of_files();
    antlr4::tree::TerminalNode *MAXFILESIZE();
    PagesContext *pages();
    antlr4::tree::TerminalNode *BUFFERSIZE();
    antlr4::tree::TerminalNode *BLOCKED();
    antlr4::tree::TerminalNode *NONBLOCKED();
    antlr4::tree::TerminalNode *APPEND();
    antlr4::tree::TerminalNode *REPLACE();

   
  };

  File_optionsContext* file_options();

  class  Event_monitor_threshold_optsContext : public antlr4::ParserRuleContext {
  public:
    Event_monitor_threshold_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Autostart_manualstartContext *autostart_manualstart();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *MEMBER();
    Member_numberContext *member_number();
    antlr4::tree::TerminalNode *LOCAL();

   
  };

  Event_monitor_threshold_optsContext* event_monitor_threshold_opts();

  class  PagesContext : public antlr4::ParserRuleContext {
  public:
    PagesContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Integer_valueContext *integer_value();

   
  };

  PagesContext* pages();

  class  Create_event_monitor_unit_of_workContext : public antlr4::ParserRuleContext {
  public:
    Create_event_monitor_unit_of_workContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    std::vector<antlr4::tree::TerminalNode *> EVENT();
    antlr4::tree::TerminalNode* EVENT(size_t i);
    antlr4::tree::TerminalNode *MONITOR();
    Event_monitor_nameContext *event_monitor_name();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *UNIT();
    antlr4::tree::TerminalNode *OF();
    antlr4::tree::TerminalNode *WORK();
    antlr4::tree::TerminalNode *WRITE();
    antlr4::tree::TerminalNode *TO();
    antlr4::tree::TerminalNode *TABLE();
    antlr4::tree::TerminalNode *UNFORMATTED();
    Autostart_manualstartContext *autostart_manualstart();
    Formatted_event_table_infoContext *formatted_event_table_info();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Target_table_optionsContext *target_table_options();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();

   
  };

  Create_event_monitor_unit_of_workContext* create_event_monitor_unit_of_work();

  class  Formatted_event_table_infoContext : public antlr4::ParserRuleContext {
  public:
    Formatted_event_table_infoContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Evm_groupContext *evm_group();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<Target_table_optionsContext *> target_table_options();
    Target_table_optionsContext* target_table_options(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Formatted_event_table_infoContext* formatted_event_table_info();

  class  Autostart_manualstartContext : public antlr4::ParserRuleContext {
  public:
    Autostart_manualstartContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *AUTOSTART();
    antlr4::tree::TerminalNode *MANUALSTART();

   
  };

  Autostart_manualstartContext* autostart_manualstart();

  class  Evm_groupContext : public antlr4::ParserRuleContext {
  public:
    Evm_groupContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TodoContext *todo();

   
  };

  Evm_groupContext* evm_group();

  class  Target_table_optionsContext : public antlr4::ParserRuleContext {
  public:
    Target_table_optionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TABLE();
    Table_nameContext *table_name();
    antlr4::tree::TerminalNode *IN();
    Tablespace_nameContext *tablespace_name();
    antlr4::tree::TerminalNode *PCTDEACTIVATE();
    Integer_valueContext *integer_value();

   
  };

  Target_table_optionsContext* target_table_options();

  class  Create_external_table_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_external_table_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *EXTERNAL();
    antlr4::tree::TerminalNode *TABLE();
    Table_nameContext *table_name();
    antlr4::tree::TerminalNode *USING();
    std::vector<antlr4::tree::TerminalNode *> LEFT_RND_BKT();
    antlr4::tree::TerminalNode* LEFT_RND_BKT(size_t i);
    std::vector<Ext_table_optionContext *> ext_table_option();
    Ext_table_optionContext* ext_table_option(size_t i);
    std::vector<Ext_table_option_valueContext *> ext_table_option_value();
    Ext_table_option_valueContext* ext_table_option_value(size_t i);
    std::vector<antlr4::tree::TerminalNode *> RIGHT_RND_BKT();
    antlr4::tree::TerminalNode* RIGHT_RND_BKT(size_t i);
    std::vector<Column_definitionContext *> column_definition();
    Column_definitionContext* column_definition(size_t i);
    antlr4::tree::TerminalNode *LIKE();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);
    Table_or_view_nameContext *table_or_view_name();
    Nick_nameContext *nick_name();

   
  };

  Create_external_table_statementContext* create_external_table_statement();

  class  Ext_table_optionContext : public antlr4::ParserRuleContext {
  public:
    Ext_table_optionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Ext_table_optionContext* ext_table_option();

  class  Ext_table_option_valueContext : public antlr4::ParserRuleContext {
  public:
    Ext_table_option_valueContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();
    StringContext *string();
    TodoContext *todo();

   
  };

  Ext_table_option_valueContext* ext_table_option_value();

  class  Create_function_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_function_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Create_function_external_scalar_statementContext *create_function_external_scalar_statement();
    Create_function_external_table_statementContext *create_function_external_table_statement();
    Create_function_old_db_external_function_statementContext *create_function_old_db_external_function_statement();
    Create_function_sourced_or_template_statementContext *create_function_sourced_or_template_statement();
    Create_function_sql_scalar_table_or_row_statementContext *create_function_sql_scalar_table_or_row_statement();

   
  };

  Create_function_statementContext* create_function_statement();

  class  Create_function_aggregate_interface_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_function_aggregate_interface_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *FUNCTION();
    Function_nameContext *function_name();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    Aggregate_return_clauseContext *aggregate_return_clause();
    Aggregate_implementation_clauseContext *aggregate_implementation_clause();
    Or_replaceContext *or_replace();
    std::vector<Agg_fn_param_declContext *> agg_fn_param_decl();
    Agg_fn_param_declContext* agg_fn_param_decl(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Create_function_aggregate_interface_statementContext* create_function_aggregate_interface_statement();

  class  Agg_fn_param_declContext : public antlr4::ParserRuleContext {
  public:
    Agg_fn_param_declContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Aggregate_builtin_typeContext *aggregate_builtin_type();
    Parameter_nameContext *parameter_name();
    Routine_default_clauseContext *routine_default_clause();
    antlr4::tree::TerminalNode *IN();

   
  };

  Agg_fn_param_declContext* agg_fn_param_decl();

  class  Aggregate_return_clauseContext : public antlr4::ParserRuleContext {
  public:
    Aggregate_return_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RETURNS();
    Agg_fn_option_listContext *agg_fn_option_list();
    Aggregate_builtin_typeContext *aggregate_builtin_type();
    Aggregate_cast_target_typeContext *aggregate_cast_target_type();
    antlr4::tree::TerminalNode *CAST();
    antlr4::tree::TerminalNode *FROM();

   
  };

  Aggregate_return_clauseContext* aggregate_return_clause();

  class  Aggregate_implementation_clauseContext : public antlr4::ParserRuleContext {
  public:
    Aggregate_implementation_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *AGGREGATE();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<State_variable_declarationContext *> state_variable_declaration();
    State_variable_declarationContext* state_variable_declaration(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *USING();
    antlr4::tree::TerminalNode *INITIALIZE();
    std::vector<Procedure_designatorContext *> procedure_designator();
    Procedure_designatorContext* procedure_designator(size_t i);
    antlr4::tree::TerminalNode *ACCUMULATE();
    antlr4::tree::TerminalNode *MERGE();
    antlr4::tree::TerminalNode *FINALIZE();
    Function_designatorContext *function_designator();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);
    antlr4::tree::TerminalNode *IN();
    antlr4::tree::TerminalNode *MODULE();
    Module_nameContext *module_name();

   
  };

  Aggregate_implementation_clauseContext* aggregate_implementation_clause();

  class  Agg_fn_option_listContext : public antlr4::ParserRuleContext {
  public:
    unsigned options_seen = 0;
    Agg_fn_option_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<antlr4::tree::TerminalNode *> SPECIFIC();
    antlr4::tree::TerminalNode* SPECIFIC(size_t i);
    std::vector<Specific_nameContext *> specific_name();
    Specific_nameContext* specific_name(size_t i);
    std::vector<antlr4::tree::TerminalNode *> DETERMINISTIC();
    antlr4::tree::TerminalNode* DETERMINISTIC(size_t i);
    std::vector<antlr4::tree::TerminalNode *> CALLED();
    antlr4::tree::TerminalNode* CALLED(size_t i);
    std::vector<antlr4::tree::TerminalNode *> ON();
    antlr4::tree::TerminalNode* ON(size_t i);
    std::vector<antlr4::tree::TerminalNode *> NULL_();
    antlr4::tree::TerminalNode* NULL_(size_t i);
    std::vector<antlr4::tree::TerminalNode *> INPUT();
    antlr4::tree::TerminalNode* INPUT(size_t i);
    std::vector<antlr4::tree::TerminalNode *> EXTERNAL();
    antlr4::tree::TerminalNode* EXTERNAL(size_t i);
    std::vector<antlr4::tree::TerminalNode *> ACTION();
    antlr4::tree::TerminalNode* ACTION(size_t i);
    std::vector<antlr4::tree::TerminalNode *> NO();
    antlr4::tree::TerminalNode* NO(size_t i);
    std::vector<antlr4::tree::TerminalNode *> SCRATCHPAD();
    antlr4::tree::TerminalNode* SCRATCHPAD(size_t i);
    std::vector<antlr4::tree::TerminalNode *> FINAL();
    antlr4::tree::TerminalNode* FINAL(size_t i);
    std::vector<antlr4::tree::TerminalNode *> CALL();
    antlr4::tree::TerminalNode* CALL(size_t i);
    std::vector<antlr4::tree::TerminalNode *> ALLOW();
    antlr4::tree::TerminalNode* ALLOW(size_t i);
    std::vector<antlr4::tree::TerminalNode *> PARALLEL();
    antlr4::tree::TerminalNode* PARALLEL(size_t i);
    std::vector<antlr4::tree::TerminalNode *> SECURED();
    antlr4::tree::TerminalNode* SECURED(size_t i);
    std::vector<antlr4::tree::TerminalNode *> NOT();
    antlr4::tree::TerminalNode* NOT(size_t i);

   
  };

  Agg_fn_option_listContext* agg_fn_option_list();

  class  State_variable_declarationContext : public antlr4::ParserRuleContext {
  public:
    State_variable_declarationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Aggregate_builtin_typeContext *aggregate_builtin_type();
    Variable_nameContext *variable_name();

   
  };

  State_variable_declarationContext* state_variable_declaration();

  class  Aggregate_cast_target_typeContext : public antlr4::ParserRuleContext {
  public:
    Aggregate_cast_target_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Aggregate_builtin_typeContext *aggregate_builtin_type();
    Inferred_builtin_typeContext *inferred_builtin_type();

   
  };

  Aggregate_cast_target_typeContext* aggregate_cast_target_type();

  class  Aggregate_builtin_typeContext : public antlr4::ParserRuleContext {
  public:
    Aggregate_builtin_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SMALLINT();
    antlr4::tree::TerminalNode *INT();
    antlr4::tree::TerminalNode *INTEGER();
    antlr4::tree::TerminalNode *BIGINT();
    antlr4::tree::TerminalNode *DEC();
    antlr4::tree::TerminalNode *DECIMAL();
    antlr4::tree::TerminalNode *NUMERIC();
    antlr4::tree::TerminalNode *NUM();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<Method_integer_attributeContext *> method_integer_attribute();
    Method_integer_attributeContext* method_integer_attribute(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *COMMA();
    antlr4::tree::TerminalNode *DECFLOAT();
    antlr4::tree::TerminalNode *FLOAT();
    antlr4::tree::TerminalNode *REAL();
    antlr4::tree::TerminalNode *DOUBLE();
    antlr4::tree::TerminalNode *PRECISION();
    Char_characterContext *char_character();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *BIT();
    antlr4::tree::TerminalNode *DATA();
    Octets_codeunitsContext *octets_codeunits();
    antlr4::tree::TerminalNode *VARCHAR();
    antlr4::tree::TerminalNode *VARYING();
    antlr4::tree::TerminalNode *CLOB();
    antlr4::tree::TerminalNode *LARGE();
    antlr4::tree::TerminalNode *OBJECT();
    KmgContext *kmg();
    antlr4::tree::TerminalNode *GRAPHIC();
    CodeunitsContext *codeunits();
    antlr4::tree::TerminalNode *VARGRAPHIC();
    antlr4::tree::TerminalNode *DBCLOB();
    antlr4::tree::TerminalNode *NCHAR();
    antlr4::tree::TerminalNode *NATIONAL();
    antlr4::tree::TerminalNode *NVARCHAR();
    antlr4::tree::TerminalNode *NCLOB();
    antlr4::tree::TerminalNode *CHARACTER();
    antlr4::tree::TerminalNode *BINARY();
    antlr4::tree::TerminalNode *VARBINARY();
    antlr4::tree::TerminalNode *BLOB();
    antlr4::tree::TerminalNode *DATE();
    antlr4::tree::TerminalNode *TIME();
    antlr4::tree::TerminalNode *TIMESTAMP();
    antlr4::tree::TerminalNode *XML();
    antlr4::tree::TerminalNode *DB2SECURITYLABEL();
    antlr4::tree::TerminalNode *SYSPROC();
    antlr4::tree::TerminalNode *DOT();

   
  };

  Aggregate_builtin_typeContext* aggregate_builtin_type();

  class  Create_function_external_scalar_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_function_external_scalar_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *FUNCTION();
    Function_nameContext *function_name();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *RETURNS();
    Ext_scalar_option_listContext *ext_scalar_option_list();
    External_scalar_ordinary_return_typeContext *external_scalar_ordinary_return_type();
    Data_type_3Context *data_type_3();
    antlr4::tree::TerminalNode *CAST();
    antlr4::tree::TerminalNode *FROM();
    External_scalar_cast_source_typeContext *external_scalar_cast_source_type();
    Or_replaceContext *or_replace();
    std::vector<Ext_scalar_param_declContext *> ext_scalar_param_decl();
    Ext_scalar_param_declContext* ext_scalar_param_decl(size_t i);
    As_locatorContext *as_locator();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Create_function_external_scalar_statementContext* create_function_external_scalar_statement();

  class  External_scalar_ordinary_return_typeContext : public antlr4::ParserRuleContext {
  public:
    External_scalar_ordinary_return_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Data_typeContext *data_type();
    Method_reference_typeContext *method_reference_type();
    Procedure_external_xml_typeContext *procedure_external_xml_type();

   
  };

  External_scalar_ordinary_return_typeContext* external_scalar_ordinary_return_type();

  class  External_scalar_cast_source_typeContext : public antlr4::ParserRuleContext {
  public:
    External_scalar_cast_source_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Data_typeContext *data_type();
    Method_reference_typeContext *method_reference_type();

   
  };

  External_scalar_cast_source_typeContext* external_scalar_cast_source_type();

  class  Ext_scalar_param_declContext : public antlr4::ParserRuleContext {
  public:
    Ext_scalar_param_declContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Data_typeContext *data_type();
    Method_reference_typeContext *method_reference_type();
    Procedure_external_xml_typeContext *procedure_external_xml_type();
    Parameter_nameContext *parameter_name();
    Routine_default_clauseContext *routine_default_clause();
    As_locatorContext *as_locator();
    In_out_inoutContext *in_out_inout();

   
  };

  Ext_scalar_param_declContext* ext_scalar_param_decl();

  class  Ext_scalar_option_listContext : public antlr4::ParserRuleContext {
  public:
    Ext_scalar_option_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Ext_scalar_option_list_itemContext *> ext_scalar_option_list_item();
    Ext_scalar_option_list_itemContext* ext_scalar_option_list_item(size_t i);

   
  };

  Ext_scalar_option_listContext* ext_scalar_option_list();

  class  Ext_scalar_option_list_itemContext : public antlr4::ParserRuleContext {
  public:
    Ext_scalar_option_list_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LANGUAGE();
    antlr4::tree::TerminalNode *C_();
    antlr4::tree::TerminalNode *JAVA();
    antlr4::tree::TerminalNode *CLR();
    antlr4::tree::TerminalNode *OLE();
    antlr4::tree::TerminalNode *CPP();
    antlr4::tree::TerminalNode *PYTHON();
    antlr4::tree::TerminalNode *SPECIFIC();
    Specific_nameContext *specific_name();
    antlr4::tree::TerminalNode *EXTERNAL();
    antlr4::tree::TerminalNode *NAME();
    StringContext *string();
    Id_Context *id_();
    antlr4::tree::TerminalNode *PARAMETER();
    antlr4::tree::TerminalNode *STYLE();
    antlr4::tree::TerminalNode *DB2GENERAL();
    antlr4::tree::TerminalNode *SQL();
    antlr4::tree::TerminalNode *NPSGENERIC();
    antlr4::tree::TerminalNode *CCSID();
    Ascii_unicodeContext *ascii_unicode();
    antlr4::tree::TerminalNode *DETERMINISTIC();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *FENCED();
    antlr4::tree::TerminalNode *THREADSAFE();
    antlr4::tree::TerminalNode *RETURNS();
    std::vector<antlr4::tree::TerminalNode *> NULL_();
    antlr4::tree::TerminalNode* NULL_(size_t i);
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *INPUT();
    antlr4::tree::TerminalNode *CALLED();
    antlr4::tree::TerminalNode *READS();
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *CONTAINS();
    antlr4::tree::TerminalNode *STATIC();
    antlr4::tree::TerminalNode *DISPATCH();
    antlr4::tree::TerminalNode *ACTION();
    antlr4::tree::TerminalNode *SCRATCHPAD();
    LengthContext *length();
    antlr4::tree::TerminalNode *FINAL();
    antlr4::tree::TerminalNode *CALL();
    Allow_disallowContext *allow_disallow();
    antlr4::tree::TerminalNode *PARALLEL();
    antlr4::tree::TerminalNode *DBINFO();
    antlr4::tree::TerminalNode *TRANSFORM();
    antlr4::tree::TerminalNode *GROUP();
    Group_nameContext *group_name();
    antlr4::tree::TerminalNode *PREDICATES();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Predicate_specificationContext *predicate_specification();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *INHERIT();
    antlr4::tree::TerminalNode *SPECIAL();
    antlr4::tree::TerminalNode *REGISTERS();
    antlr4::tree::TerminalNode *SECURED();
    antlr4::tree::TerminalNode *STAY();
    antlr4::tree::TerminalNode *RESIDENT();

   
  };

  Ext_scalar_option_list_itemContext* ext_scalar_option_list_item();

  class  Predicate_specificationContext : public antlr4::ParserRuleContext {
  public:
    Predicate_specificationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WHEN();
    antlr4::tree::TerminalNode *EQ();
    antlr4::tree::TerminalNode *LTGT();
    antlr4::tree::TerminalNode *LT();
    antlr4::tree::TerminalNode *GT();
    antlr4::tree::TerminalNode *LE();
    antlr4::tree::TerminalNode *GE();
    Constant_Context *constant_();
    antlr4::tree::TerminalNode *EXPRESSION();
    antlr4::tree::TerminalNode *AS();
    Expression_nameContext *expression_name();
    Data_filterContext *data_filter();
    Index_exploitationContext *index_exploitation();

   
  };

  Predicate_specificationContext* predicate_specification();

  class  Data_filterContext : public antlr4::ParserRuleContext {
  public:
    Data_filterContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FILTER();
    antlr4::tree::TerminalNode *USING();
    Function_invocationContext *function_invocation();
    Case_expressionContext *case_expression();

   
  };

  Data_filterContext* data_filter();

  class  Index_exploitationContext : public antlr4::ParserRuleContext {
  public:
    Index_exploitationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SEARCH();
    antlr4::tree::TerminalNode *BY();
    antlr4::tree::TerminalNode *INDEX();
    antlr4::tree::TerminalNode *EXTENSION();
    Index_extension_nameContext *index_extension_name();
    antlr4::tree::TerminalNode *EXACT();
    std::vector<Exploitation_ruleContext *> exploitation_rule();
    Exploitation_ruleContext* exploitation_rule(size_t i);

   
  };

  Index_exploitationContext* index_exploitation();

  class  Exploitation_ruleContext : public antlr4::ParserRuleContext {
  public:
    Exploitation_ruleContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WHEN();
    antlr4::tree::TerminalNode *KEY();
    std::vector<antlr4::tree::TerminalNode *> LEFT_RND_BKT();
    antlr4::tree::TerminalNode* LEFT_RND_BKT(size_t i);
    std::vector<Parameter_nameContext *> parameter_name();
    Parameter_nameContext* parameter_name(size_t i);
    std::vector<antlr4::tree::TerminalNode *> RIGHT_RND_BKT();
    antlr4::tree::TerminalNode* RIGHT_RND_BKT(size_t i);
    antlr4::tree::TerminalNode *USE();
    Search_method_nameContext *search_method_name();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Exploitation_ruleContext* exploitation_rule();

  class  Create_function_external_table_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_function_external_table_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *FUNCTION();
    Function_nameContext *function_name();
    std::vector<antlr4::tree::TerminalNode *> LEFT_RND_BKT();
    antlr4::tree::TerminalNode* LEFT_RND_BKT(size_t i);
    std::vector<antlr4::tree::TerminalNode *> RIGHT_RND_BKT();
    antlr4::tree::TerminalNode* RIGHT_RND_BKT(size_t i);
    antlr4::tree::TerminalNode *RETURNS();
    Ext_table_option_listContext *ext_table_option_list();
    antlr4::tree::TerminalNode *TABLE();
    std::vector<Column_nameContext *> column_name();
    Column_nameContext* column_name(size_t i);
    std::vector<External_table_return_typeContext *> external_table_return_type();
    External_table_return_typeContext* external_table_return_type(size_t i);
    antlr4::tree::TerminalNode *GENERIC();
    Or_replaceContext *or_replace();
    Ext_table_param_decl_listContext *ext_table_param_decl_list();
    std::vector<As_locatorContext *> as_locator();
    As_locatorContext* as_locator(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Create_function_external_table_statementContext* create_function_external_table_statement();

  class  Ext_table_param_decl_listContext : public antlr4::ParserRuleContext {
  public:
    Ext_table_param_decl_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Ext_table_param_declContext *> ext_table_param_decl();
    Ext_table_param_declContext* ext_table_param_decl(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Ext_table_param_decl_listContext* ext_table_param_decl_list();

  class  Ext_table_param_declContext : public antlr4::ParserRuleContext {
  public:
    Ext_table_param_declContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Data_typeContext *data_type();
    Method_reference_typeContext *method_reference_type();
    Procedure_external_xml_typeContext *procedure_external_xml_type();
    Parameter_nameContext *parameter_name();
    Routine_default_clauseContext *routine_default_clause();
    As_locatorContext *as_locator();

   
  };

  Ext_table_param_declContext* ext_table_param_decl();

  class  External_table_return_typeContext : public antlr4::ParserRuleContext {
  public:
    External_table_return_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Data_typeContext *data_type();
    Method_reference_typeContext *method_reference_type();
    Procedure_external_xml_typeContext *procedure_external_xml_type();

   
  };

  External_table_return_typeContext* external_table_return_type();

  class  Ext_table_option_listContext : public antlr4::ParserRuleContext {
  public:
    Ext_table_option_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Ext_table_option_list_itemContext *> ext_table_option_list_item();
    Ext_table_option_list_itemContext* ext_table_option_list_item(size_t i);

   
  };

  Ext_table_option_listContext* ext_table_option_list();

  class  Ext_table_option_list_itemContext : public antlr4::ParserRuleContext {
  public:
    Ext_table_option_list_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LANGUAGE();
    antlr4::tree::TerminalNode *C_();
    antlr4::tree::TerminalNode *JAVA();
    antlr4::tree::TerminalNode *CLR();
    antlr4::tree::TerminalNode *OLE();
    antlr4::tree::TerminalNode *CPP();
    antlr4::tree::TerminalNode *SPECIFIC();
    Specific_nameContext *specific_name();
    antlr4::tree::TerminalNode *EXTERNAL();
    antlr4::tree::TerminalNode *NAME();
    StringContext *string();
    Id_Context *id_();
    antlr4::tree::TerminalNode *PARAMETER();
    antlr4::tree::TerminalNode *STYLE();
    antlr4::tree::TerminalNode *DB2GENERAL();
    antlr4::tree::TerminalNode *SQL();
    antlr4::tree::TerminalNode *NPSGENERIC();
    antlr4::tree::TerminalNode *CCSID();
    Ascii_unicodeContext *ascii_unicode();
    antlr4::tree::TerminalNode *DETERMINISTIC();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *FENCED();
    antlr4::tree::TerminalNode *THREADSAFE();
    antlr4::tree::TerminalNode *RETURNS();
    std::vector<antlr4::tree::TerminalNode *> NULL_();
    antlr4::tree::TerminalNode* NULL_(size_t i);
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *INPUT();
    antlr4::tree::TerminalNode *CALLED();
    antlr4::tree::TerminalNode *READS();
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *CONTAINS();
    antlr4::tree::TerminalNode *STATIC();
    antlr4::tree::TerminalNode *DISPATCH();
    antlr4::tree::TerminalNode *ACTION();
    antlr4::tree::TerminalNode *SCRATCHPAD();
    LengthContext *length();
    antlr4::tree::TerminalNode *FINAL();
    antlr4::tree::TerminalNode *CALL();
    antlr4::tree::TerminalNode *DISALLOW();
    antlr4::tree::TerminalNode *PARALLEL();
    antlr4::tree::TerminalNode *ALLOW();
    antlr4::tree::TerminalNode *EXECUTE();
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *RESULT();
    antlr4::tree::TerminalNode *TABLE();
    antlr4::tree::TerminalNode *DISTRIBUTED();
    antlr4::tree::TerminalNode *DATABASE();
    antlr4::tree::TerminalNode *PARTITIONS();
    antlr4::tree::TerminalNode *DBINFO();
    antlr4::tree::TerminalNode *CARDINALITY();
    Integer_valueContext *integer_value();
    antlr4::tree::TerminalNode *TRANSFORM();
    antlr4::tree::TerminalNode *GROUP();
    Group_nameContext *group_name();
    antlr4::tree::TerminalNode *INHERIT();
    antlr4::tree::TerminalNode *SPECIAL();
    antlr4::tree::TerminalNode *REGISTERS();
    antlr4::tree::TerminalNode *SECURED();
    antlr4::tree::TerminalNode *STAY();
    antlr4::tree::TerminalNode *RESIDENT();

   
  };

  Ext_table_option_list_itemContext* ext_table_option_list_item();

  class  Create_function_old_db_external_function_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_function_old_db_external_function_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *FUNCTION();
    Function_nameContext *function_name();
    std::vector<antlr4::tree::TerminalNode *> LEFT_RND_BKT();
    antlr4::tree::TerminalNode* LEFT_RND_BKT(size_t i);
    std::vector<antlr4::tree::TerminalNode *> RIGHT_RND_BKT();
    antlr4::tree::TerminalNode* RIGHT_RND_BKT(size_t i);
    antlr4::tree::TerminalNode *RETURNS();
    antlr4::tree::TerminalNode *TABLE();
    std::vector<Column_nameContext *> column_name();
    Column_nameContext* column_name(size_t i);
    std::vector<Oledb_return_typeContext *> oledb_return_type();
    Oledb_return_typeContext* oledb_return_type(size_t i);
    Oledb_option_listContext *oledb_option_list();
    Param_decl_list_3Context *param_decl_list_3();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Create_function_old_db_external_function_statementContext* create_function_old_db_external_function_statement();

  class  Oledb_return_typeContext : public antlr4::ParserRuleContext {
  public:
    Oledb_return_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Data_typeContext *data_type();
    Method_reference_typeContext *method_reference_type();

   
  };

  Oledb_return_typeContext* oledb_return_type();

  class  Oledb_option_listContext : public antlr4::ParserRuleContext {
  public:
    Oledb_option_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Oledb_option_list_itemContext *> oledb_option_list_item();
    Oledb_option_list_itemContext* oledb_option_list_item(size_t i);

   
  };

  Oledb_option_listContext* oledb_option_list();

  class  Oledb_option_list_itemContext : public antlr4::ParserRuleContext {
  public:
    Oledb_option_list_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LANGUAGE();
    antlr4::tree::TerminalNode *OLEDB();
    antlr4::tree::TerminalNode *SPECIFIC();
    Specific_nameContext *specific_name();
    antlr4::tree::TerminalNode *EXTERNAL();
    antlr4::tree::TerminalNode *NAME();
    StringContext *string();
    antlr4::tree::TerminalNode *DETERMINISTIC();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *STATIC();
    antlr4::tree::TerminalNode *DISPATCH();
    antlr4::tree::TerminalNode *RETURNS();
    std::vector<antlr4::tree::TerminalNode *> NULL_();
    antlr4::tree::TerminalNode* NULL_(size_t i);
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *INPUT();
    antlr4::tree::TerminalNode *CALLED();
    antlr4::tree::TerminalNode *CARDINALITY();
    Integer_valueContext *integer_value();
    antlr4::tree::TerminalNode *SECURED();
    antlr4::tree::TerminalNode *ACTION();
    antlr4::tree::TerminalNode *NO();

   
  };

  Oledb_option_list_itemContext* oledb_option_list_item();

  class  Create_function_sourced_or_template_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_function_sourced_or_template_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *FUNCTION();
    Function_nameContext *function_name();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    Fn_return_optsContext *fn_return_opts();
    Sourced_function_param_decl_listContext *sourced_function_param_decl_list();

   
  };

  Create_function_sourced_or_template_statementContext* create_function_sourced_or_template_statement();

  class  Sourced_function_param_decl_listContext : public antlr4::ParserRuleContext {
  public:
    Sourced_function_param_decl_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Sourced_function_param_declContext *> sourced_function_param_decl();
    Sourced_function_param_declContext* sourced_function_param_decl(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Sourced_function_param_decl_listContext* sourced_function_param_decl_list();

  class  Sourced_function_param_declContext : public antlr4::ParserRuleContext {
  public:
    Sourced_function_param_declContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Sourced_function_parameter_typeContext *sourced_function_parameter_type();
    Parameter_nameContext *parameter_name();
    Routine_default_clauseContext *routine_default_clause();

   
  };

  Sourced_function_param_declContext* sourced_function_param_decl();

  class  Sourced_function_parameter_typeContext : public antlr4::ParserRuleContext {
  public:
    Sourced_function_parameter_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Data_typeContext *data_type();
    Inferred_builtin_typeContext *inferred_builtin_type();

   
  };

  Sourced_function_parameter_typeContext* sourced_function_parameter_type();

  class  Sourced_function_return_typeContext : public antlr4::ParserRuleContext {
  public:
    Sourced_function_return_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Data_typeContext *data_type();
    Inferred_builtin_typeContext *inferred_builtin_type();

   
  };

  Sourced_function_return_typeContext* sourced_function_return_type();

  class  Template_function_return_typeContext : public antlr4::ParserRuleContext {
  public:
    Template_function_return_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Data_typeContext *data_type();

   
  };

  Template_function_return_typeContext* template_function_return_type();

  class  Fn_return_optsContext : public antlr4::ParserRuleContext {
  public:
    Fn_return_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Fn_return_opts_itemContext *fn_return_opts_item();

   
  };

  Fn_return_optsContext* fn_return_opts();

  class  Fn_return_opts_itemContext : public antlr4::ParserRuleContext {
  public:
    Fn_return_opts_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RETURNS();
    Sourced_function_return_typeContext *sourced_function_return_type();
    antlr4::tree::TerminalNode *SOURCE();
    std::vector<antlr4::tree::TerminalNode *> SPECIFIC();
    antlr4::tree::TerminalNode* SPECIFIC(size_t i);
    std::vector<Specific_nameContext *> specific_name();
    Specific_nameContext* specific_name(size_t i);
    Function_nameContext *function_name();
    antlr4::tree::TerminalNode *PARAMETER();
    antlr4::tree::TerminalNode *CCSID();
    Ascii_unicodeContext *ascii_unicode();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    Designator_data_type_listContext *designator_data_type_list();
    Template_function_return_typeContext *template_function_return_type();
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *TEMPLATE();
    Template_optsContext *template_opts();

   
  };

  Fn_return_opts_itemContext* fn_return_opts_item();

  class  Template_optsContext : public antlr4::ParserRuleContext {
  public:
    Template_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Template_opts_itemContext *> template_opts_item();
    Template_opts_itemContext* template_opts_item(size_t i);

   
  };

  Template_optsContext* template_opts();

  class  Template_opts_itemContext : public antlr4::ParserRuleContext {
  public:
    Template_opts_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DETERMINISTIC();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *EXTERNAL();
    antlr4::tree::TerminalNode *ACTION();
    antlr4::tree::TerminalNode *NO();

   
  };

  Template_opts_itemContext* template_opts_item();

  class  Ascii_unicodeContext : public antlr4::ParserRuleContext {
  public:
    Ascii_unicodeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ASCII();
    antlr4::tree::TerminalNode *UNICODE();

   
  };

  Ascii_unicodeContext* ascii_unicode();

  class  Param_decl_list_3Context : public antlr4::ParserRuleContext {
  public:
    Param_decl_list_3Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Param_decl_3Context *> param_decl_3();
    Param_decl_3Context* param_decl_3(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Param_decl_list_3Context* param_decl_list_3();

  class  Param_decl_3Context : public antlr4::ParserRuleContext {
  public:
    Param_decl_3Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Data_typeContext *data_type();
    Method_reference_typeContext *method_reference_type();
    Parameter_nameContext *parameter_name();
    Routine_default_clauseContext *routine_default_clause();

   
  };

  Param_decl_3Context* param_decl_3();

  class  Create_function_sql_scalar_table_or_row_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_function_sql_scalar_table_or_row_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *FUNCTION();
    Function_nameContext *function_name();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *RETURNS();
    Option_listContext *option_list();
    Sql_function_bodyContext *sql_function_body();
    Data_type_1Context *data_type_1();
    antlr4::tree::TerminalNode *ROW();
    Sql_function_return_row_columnsContext *sql_function_return_row_columns();
    antlr4::tree::TerminalNode *TABLE();
    Or_replaceContext *or_replace();
    Param_decl_list_2Context *param_decl_list_2();
    Sql_function_return_table_columnsContext *sql_function_return_table_columns();
    Row_type_nameContext *row_type_name();
    Anchored_row_data_typeContext *anchored_row_data_type();
    antlr4::tree::TerminalNode *ELEMENT();
    antlr4::tree::TerminalNode *OF();
    Array_type_nameContext *array_type_name();

   
  };

  Create_function_sql_scalar_table_or_row_statementContext* create_function_sql_scalar_table_or_row_statement();

  class  Sql_function_return_builtin_typeContext : public antlr4::ParserRuleContext {
  public:
    Sql_function_return_builtin_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Method_builtin_typeContext *method_builtin_type();
    antlr4::tree::TerminalNode *CURSOR();

   
  };

  Sql_function_return_builtin_typeContext* sql_function_return_builtin_type();

  class  Sql_function_return_named_typeContext : public antlr4::ParserRuleContext {
  public:
    Sql_function_return_named_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Method_named_typeContext *method_named_type();

   
  };

  Sql_function_return_named_typeContext* sql_function_return_named_type();

  class  Sql_function_return_data_typeContext : public antlr4::ParserRuleContext {
  public:
    Sql_function_return_data_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Sql_function_return_builtin_typeContext *sql_function_return_builtin_type();
    Sql_function_return_named_typeContext *sql_function_return_named_type();
    antlr4::tree::TerminalNode *REF();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();

   
  };

  Sql_function_return_data_typeContext* sql_function_return_data_type();

  class  Sql_function_return_columnContext : public antlr4::ParserRuleContext {
  public:
    Sql_function_return_columnContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Column_nameContext *column_name();
    Sql_function_return_data_typeContext *sql_function_return_data_type();

   
  };

  Sql_function_return_columnContext* sql_function_return_column();

  class  Sql_function_return_table_columnsContext : public antlr4::ParserRuleContext {
  public:
    Sql_function_return_table_columnsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<Sql_function_return_columnContext *> sql_function_return_column();
    Sql_function_return_columnContext* sql_function_return_column(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Sql_function_return_table_columnsContext* sql_function_return_table_columns();

  class  Sql_function_return_row_columnsContext : public antlr4::ParserRuleContext {
  public:
    Sql_function_return_row_columnsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<Sql_function_return_columnContext *> sql_function_return_column();
    Sql_function_return_columnContext* sql_function_return_column(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();

   
  };

  Sql_function_return_row_columnsContext* sql_function_return_row_columns();

  class  Param_decl_list_2Context : public antlr4::ParserRuleContext {
  public:
    Param_decl_list_2Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Param_decl_2Context *> param_decl_2();
    Param_decl_2Context* param_decl_2(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Param_decl_list_2Context* param_decl_list_2();

  class  Param_decl_2Context : public antlr4::ParserRuleContext {
  public:
    Param_decl_2Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Parameter_nameContext *parameter_name();
    Data_type_1Context *data_type_1();
    In_out_inoutContext *in_out_inout();
    Routine_default_clauseContext *routine_default_clause();

   
  };

  Param_decl_2Context* param_decl_2();

  class  Sql_function_bodyContext : public antlr4::ParserRuleContext {
  public:
    Sql_function_bodyContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Return_statementContext *return_statement();
    Compound_sql_compiledContext *compound_sql_compiled();
    Compound_sql_inlinedContext *compound_sql_inlined();

   
  };

  Sql_function_bodyContext* sql_function_body();

  class  Create_function_mapping_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_function_mapping_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *FUNCTION();
    antlr4::tree::TerminalNode *MAPPING();
    antlr4::tree::TerminalNode *FOR();
    Function_nameContext *function_name();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Data_type_listContext *data_type_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *SPECIFIC();
    Specific_nameContext *specific_name();
    antlr4::tree::TerminalNode *SERVER();
    Server_nameContext *server_name();
    antlr4::tree::TerminalNode *TYPE();
    Server_typeContext *server_type();
    Function_mapping_nameContext *function_mapping_name();
    Function_optionsContext *function_options();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *INFIX();
    antlr4::tree::TerminalNode *VERSION();
    Server_versionContext *server_version();
    antlr4::tree::TerminalNode *WRAPPER();
    Wrapper_nameContext *wrapper_name();

   
  };

  Create_function_mapping_statementContext* create_function_mapping_statement();

  class  Function_optionsContext : public antlr4::ParserRuleContext {
  public:
    Function_optionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OPTIONS();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<Function_option_nameContext *> function_option_name();
    Function_option_nameContext* function_option_name(size_t i);
    std::vector<String_constantContext *> string_constant();
    String_constantContext* string_constant(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Function_optionsContext* function_options();

  class  Function_option_nameContext : public antlr4::ParserRuleContext {
  public:
    Function_option_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Function_option_nameContext* function_option_name();

  class  Create_global_temporary_table_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_global_temporary_table_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *GLOBAL();
    antlr4::tree::TerminalNode *TEMPORARY();
    antlr4::tree::TerminalNode *TABLE();
    Table_nameContext *table_name();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<Column_definitionContext *> column_definition();
    Column_definitionContext* column_definition(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *LIKE();
    Table_or_view_nameContext *table_or_view_name();
    antlr4::tree::TerminalNode *AS();
    FullselectContext *fullselect();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *DATA();
    Create_global_temporary_table_optsContext *create_global_temporary_table_opts();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);
    Copy_optionsContext *copy_options();

   
  };

  Create_global_temporary_table_statementContext* create_global_temporary_table_statement();

  class  Create_global_temporary_table_optsContext : public antlr4::ParserRuleContext {
  public:
    Create_global_temporary_table_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Create_global_temporary_table_itemContext *> create_global_temporary_table_item();
    Create_global_temporary_table_itemContext* create_global_temporary_table_item(size_t i);

   
  };

  Create_global_temporary_table_optsContext* create_global_temporary_table_opts();

  class  Create_global_temporary_table_itemContext : public antlr4::ParserRuleContext {
  public:
    Create_global_temporary_table_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *COMMIT();
    Delete_preserveContext *delete_preserve();
    antlr4::tree::TerminalNode *ROWS();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *LOGGED();
    antlr4::tree::TerminalNode *ROLLBACK();
    antlr4::tree::TerminalNode *IN();
    Tablespace_nameContext *tablespace_name();
    Distribution_clauseContext *distribution_clause();

   
  };

  Create_global_temporary_table_itemContext* create_global_temporary_table_item();

  class  Delete_preserveContext : public antlr4::ParserRuleContext {
  public:
    Delete_preserveContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DELETE();
    antlr4::tree::TerminalNode *PRESERVE();

   
  };

  Delete_preserveContext* delete_preserve();

  class  Create_histogram_template_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_histogram_template_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *HISTOGRAM();
    antlr4::tree::TerminalNode *TEMPLATE();
    Template_nameContext *template_name();
    antlr4::tree::TerminalNode *HIGH();
    antlr4::tree::TerminalNode *BIN();
    antlr4::tree::TerminalNode *VALUE();
    Bigint_constantContext *bigint_constant();

   
  };

  Create_histogram_template_statementContext* create_histogram_template_statement();

  class  Create_index_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_index_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *INDEX();
    Index_nameContext *index_name();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Index_col_optsContext *index_col_opts();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    Table_nameContext *table_name();
    Nick_nameContext *nick_name();
    antlr4::tree::TerminalNode *UNIQUE();
    Index_cluster_clauseContext *index_cluster_clause();

   
  };

  Create_index_statementContext* create_index_statement();

  class  Index_cluster_clauseContext : public antlr4::ParserRuleContext {
  public:
    Index_cluster_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CLUSTER();

   
  };

  Index_cluster_clauseContext* index_cluster_clause();

  class  Index_col_optsContext : public antlr4::ParserRuleContext {
  public:
    Index_col_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Index_col_opts_itemContext *> index_col_opts_item();
    Index_col_opts_itemContext* index_col_opts_item(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Index_col_optsContext* index_col_opts();

  class  Index_col_opts_itemContext : public antlr4::ParserRuleContext {
  public:
    Index_col_opts_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Column_nameContext *column_name();
    Key_expressionContext *key_expression();
    antlr4::tree::TerminalNode *ASC();
    antlr4::tree::TerminalNode *DESC();
    antlr4::tree::TerminalNode *RANDOM();
    antlr4::tree::TerminalNode *BUSINESS_TIME();
    antlr4::tree::TerminalNode *WITHOUT();
    antlr4::tree::TerminalNode *OVERLAPS();

   
  };

  Index_col_opts_itemContext* index_col_opts_item();

  class  Key_expressionContext : public antlr4::ParserRuleContext {
  public:
    Key_expressionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ExpressionContext *expression();

   
  };

  Key_expressionContext* key_expression();

  class  Create_index_extension_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_index_extension_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *INDEX();
    antlr4::tree::TerminalNode *EXTENSION();
    Index_extension_nameContext *index_extension_name();
    Index_maintenanceContext *index_maintenance();
    Index_searchContext *index_search();
    Param_listContext *param_list();

   
  };

  Create_index_extension_statementContext* create_index_extension_statement();

  class  Param_listContext : public antlr4::ParserRuleContext {
  public:
    Param_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Parameter_nameContext *> parameter_name();
    Parameter_nameContext* parameter_name(size_t i);
    std::vector<Data_typeContext *> data_type();
    Data_typeContext* data_type(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Param_listContext* param_list();

  class  Index_maintenanceContext : public antlr4::ParserRuleContext {
  public:
    Index_maintenanceContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FROM();
    antlr4::tree::TerminalNode *SOURCE();
    std::vector<antlr4::tree::TerminalNode *> KEY();
    antlr4::tree::TerminalNode* KEY(size_t i);
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Parameter_nameContext *parameter_name();
    Data_typeContext *data_type();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *GENERATE();
    antlr4::tree::TerminalNode *USING();
    Table_function_invocationContext *table_function_invocation();

   
  };

  Index_maintenanceContext* index_maintenance();

  class  Table_function_invocationContext : public antlr4::ParserRuleContext {
  public:
    Table_function_invocationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TodoContext *todo();

   
  };

  Table_function_invocationContext* table_function_invocation();

  class  Index_searchContext : public antlr4::ParserRuleContext {
  public:
    Index_searchContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *TARGET();
    antlr4::tree::TerminalNode *KEY();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Param_listContext *param_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *SEARCH();
    antlr4::tree::TerminalNode *METHOD();
    std::vector<Search_method_definitionContext *> search_method_definition();
    Search_method_definitionContext* search_method_definition(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Index_searchContext* index_search();

  class  Search_method_definitionContext : public antlr4::ParserRuleContext {
  public:
    Search_method_definitionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WHEN();
    Method_nameContext *method_name();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Param_listContext *param_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *RANGE();
    antlr4::tree::TerminalNode *THROUGH();
    Range_producing_funciton_invocationContext *range_producing_funciton_invocation();
    antlr4::tree::TerminalNode *FILTER();
    antlr4::tree::TerminalNode *USING();
    Index_filtering_function_invocationContext *index_filtering_function_invocation();
    Case_expressionContext *case_expression();

   
  };

  Search_method_definitionContext* search_method_definition();

  class  Create_mask_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_mask_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *MASK();
    Mask_nameContext *mask_name();
    antlr4::tree::TerminalNode *ON();
    Table_nameContext *table_name();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *COLUMN();
    Column_nameContext *column_name();
    antlr4::tree::TerminalNode *RETURN();
    Case_expressionContext *case_expression();
    Or_replaceContext *or_replace();
    Correlation_nameContext *correlation_name();
    Enable_disableContext *enable_disable();
    antlr4::tree::TerminalNode *AS();

   
  };

  Create_mask_statementContext* create_mask_statement();

  class  Case_expressionContext : public antlr4::ParserRuleContext {
  public:
    Case_expressionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CASE();

   
  };

  Case_expressionContext* case_expression();

  class  Range_producing_funciton_invocationContext : public antlr4::ParserRuleContext {
  public:
    Range_producing_funciton_invocationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TodoContext *todo();

   
  };

  Range_producing_funciton_invocationContext* range_producing_funciton_invocation();

  class  Index_filtering_function_invocationContext : public antlr4::ParserRuleContext {
  public:
    Index_filtering_function_invocationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TodoContext *todo();

   
  };

  Index_filtering_function_invocationContext* index_filtering_function_invocation();

  class  Create_method_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_method_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *METHOD();
    antlr4::tree::TerminalNode *FOR();
    Type_nameContext *type_name();
    antlr4::tree::TerminalNode *SPECIFIC();
    Specific_nameContext *specific_name();
    antlr4::tree::TerminalNode *INHERIT();
    antlr4::tree::TerminalNode *ISOLATION();
    antlr4::tree::TerminalNode *LEVEL();
    With_withoutContext *with_without();
    antlr4::tree::TerminalNode *LOCK();
    antlr4::tree::TerminalNode *REQUEST();
    Sql_method_bodyContext *sql_method_body();
    Method_nameContext *method_name();
    Method_signatureContext *method_signature();
    Method_optsContext *method_opts();

   
  };

  Create_method_statementContext* create_method_statement();

  class  Method_optsContext : public antlr4::ParserRuleContext {
  public:
    Method_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Method_opts_itemContext *> method_opts_item();
    Method_opts_itemContext* method_opts_item(size_t i);

   
  };

  Method_optsContext* method_opts();

  class  Method_opts_itemContext : public antlr4::ParserRuleContext {
  public:
    Method_opts_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *EXTERNAL();
    antlr4::tree::TerminalNode *NAME();
    StringContext *string();
    Id_Context *id_();
    antlr4::tree::TerminalNode *TRANSFORM();
    antlr4::tree::TerminalNode *GROUP();
    Group_nameContext *group_name();

   
  };

  Method_opts_itemContext* method_opts_item();

  class  Method_signatureContext : public antlr4::ParserRuleContext {
  public:
    Method_signatureContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Method_nameContext *method_name();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    Method_param_listContext *method_param_list();
    antlr4::tree::TerminalNode *RETURNS();
    Method_ordinary_return_typeContext *method_ordinary_return_type();
    Method_cast_target_typeContext *method_cast_target_type();
    antlr4::tree::TerminalNode *CAST();
    antlr4::tree::TerminalNode *FROM();
    Method_cast_source_typeContext *method_cast_source_type();
    As_locatorContext *as_locator();

   
  };

  Method_signatureContext* method_signature();

  class  Method_ordinary_return_typeContext : public antlr4::ParserRuleContext {
  public:
    Method_ordinary_return_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Method_spec_parameter_typeContext *method_spec_parameter_type();

   
  };

  Method_ordinary_return_typeContext* method_ordinary_return_type();

  class  Method_reference_typeContext : public antlr4::ParserRuleContext {
  public:
    Method_reference_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REF();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Method_named_typeContext *method_named_type();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();

   
  };

  Method_reference_typeContext* method_reference_type();

  class  Method_cast_non_xml_typeContext : public antlr4::ParserRuleContext {
  public:
    Method_cast_non_xml_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Method_cast_non_xml_builtin_typeContext *method_cast_non_xml_builtin_type();
    Anchored_data_typeContext *anchored_data_type();
    Row_type_nameContext *row_type_name();
    Array_type_nameContext *array_type_name();
    Method_reference_typeContext *method_reference_type();

   
  };

  Method_cast_non_xml_typeContext* method_cast_non_xml_type();

  class  Method_cast_non_xml_builtin_typeContext : public antlr4::ParserRuleContext {
  public:
    Method_cast_non_xml_builtin_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SMALLINT();
    antlr4::tree::TerminalNode *INT();
    antlr4::tree::TerminalNode *INTEGER();
    antlr4::tree::TerminalNode *BIGINT();
    antlr4::tree::TerminalNode *DEC();
    antlr4::tree::TerminalNode *DECIMAL();
    antlr4::tree::TerminalNode *NUMERIC();
    antlr4::tree::TerminalNode *NUM();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<Integer_valueContext *> integer_value();
    Integer_valueContext* integer_value(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *COMMA();
    antlr4::tree::TerminalNode *DECFLOAT();
    antlr4::tree::TerminalNode *FLOAT();
    antlr4::tree::TerminalNode *REAL();
    antlr4::tree::TerminalNode *DOUBLE();
    antlr4::tree::TerminalNode *PRECISION();
    Char_characterContext *char_character();
    antlr4::tree::TerminalNode *VARCHAR();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *BIT();
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *VARYING();
    Octets_codeunitsContext *octets_codeunits();
    antlr4::tree::TerminalNode *CLOB();
    antlr4::tree::TerminalNode *LARGE();
    antlr4::tree::TerminalNode *OBJECT();
    KmgContext *kmg();
    antlr4::tree::TerminalNode *GRAPHIC();
    antlr4::tree::TerminalNode *VARGRAPHIC();
    CodeunitsContext *codeunits();
    antlr4::tree::TerminalNode *DBCLOB();
    antlr4::tree::TerminalNode *NCHAR();
    antlr4::tree::TerminalNode *NATIONAL();
    Integer_parenContext *integer_paren();
    antlr4::tree::TerminalNode *NVARCHAR();
    antlr4::tree::TerminalNode *NCLOB();
    antlr4::tree::TerminalNode *CHARACTER();
    Integer_kmg_parenContext *integer_kmg_paren();
    antlr4::tree::TerminalNode *BINARY();
    antlr4::tree::TerminalNode *VARBINARY();
    antlr4::tree::TerminalNode *BLOB();
    antlr4::tree::TerminalNode *DATE();
    antlr4::tree::TerminalNode *TIME();
    antlr4::tree::TerminalNode *TIMESTAMP();
    antlr4::tree::TerminalNode *BOOLEAN();
    antlr4::tree::TerminalNode *CURSOR();

   
  };

  Method_cast_non_xml_builtin_typeContext* method_cast_non_xml_builtin_type();

  class  Method_cast_source_typeContext : public antlr4::ParserRuleContext {
  public:
    Method_cast_source_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Method_cast_non_xml_typeContext *method_cast_non_xml_type();

   
  };

  Method_cast_source_typeContext* method_cast_source_type();

  class  Method_cast_target_typeContext : public antlr4::ParserRuleContext {
  public:
    Method_cast_target_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Method_cast_non_xml_typeContext *method_cast_non_xml_type();
    Inferred_builtin_typeContext *inferred_builtin_type();

   
  };

  Method_cast_target_typeContext* method_cast_target_type();

  class  Method_param_listContext : public antlr4::ParserRuleContext {
  public:
    Method_param_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Method_signature_parameter_declContext *> method_signature_parameter_decl();
    Method_signature_parameter_declContext* method_signature_parameter_decl(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Method_param_listContext* method_param_list();

  class  Method_signature_parameter_declContext : public antlr4::ParserRuleContext {
  public:
    Method_signature_parameter_declContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Method_parameter_typeContext *method_parameter_type();
    Parameter_nameContext *parameter_name();
    As_locatorContext *as_locator();

   
  };

  Method_signature_parameter_declContext* method_signature_parameter_decl();

  class  Method_parameter_typeContext : public antlr4::ParserRuleContext {
  public:
    Method_parameter_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Method_builtin_typeContext *method_builtin_type();
    Method_named_typeContext *method_named_type();
    antlr4::tree::TerminalNode *REF();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();

   
  };

  Method_parameter_typeContext* method_parameter_type();

  class  Method_builtin_typeContext : public antlr4::ParserRuleContext {
  public:
    Method_builtin_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Method_non_xml_builtin_typeContext *method_non_xml_builtin_type();
    antlr4::tree::TerminalNode *XML();

   
  };

  Method_builtin_typeContext* method_builtin_type();

  class  Method_non_xml_builtin_typeContext : public antlr4::ParserRuleContext {
  public:
    Method_non_xml_builtin_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SMALLINT();
    antlr4::tree::TerminalNode *INT();
    antlr4::tree::TerminalNode *INTEGER();
    antlr4::tree::TerminalNode *BIGINT();
    antlr4::tree::TerminalNode *DEC();
    antlr4::tree::TerminalNode *DECIMAL();
    antlr4::tree::TerminalNode *NUMERIC();
    antlr4::tree::TerminalNode *NUM();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<Method_integer_attributeContext *> method_integer_attribute();
    Method_integer_attributeContext* method_integer_attribute(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *COMMA();
    antlr4::tree::TerminalNode *DECFLOAT();
    antlr4::tree::TerminalNode *FLOAT();
    antlr4::tree::TerminalNode *REAL();
    antlr4::tree::TerminalNode *DOUBLE();
    antlr4::tree::TerminalNode *PRECISION();
    Char_characterContext *char_character();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *BIT();
    antlr4::tree::TerminalNode *DATA();
    Octets_codeunitsContext *octets_codeunits();
    antlr4::tree::TerminalNode *VARCHAR();
    antlr4::tree::TerminalNode *VARYING();
    antlr4::tree::TerminalNode *CLOB();
    antlr4::tree::TerminalNode *LARGE();
    antlr4::tree::TerminalNode *OBJECT();
    KmgContext *kmg();
    antlr4::tree::TerminalNode *GRAPHIC();
    CodeunitsContext *codeunits();
    antlr4::tree::TerminalNode *VARGRAPHIC();
    antlr4::tree::TerminalNode *DBCLOB();
    antlr4::tree::TerminalNode *NCHAR();
    antlr4::tree::TerminalNode *NATIONAL();
    antlr4::tree::TerminalNode *NVARCHAR();
    antlr4::tree::TerminalNode *NCLOB();
    antlr4::tree::TerminalNode *CHARACTER();
    antlr4::tree::TerminalNode *BINARY();
    antlr4::tree::TerminalNode *VARBINARY();
    antlr4::tree::TerminalNode *BLOB();
    antlr4::tree::TerminalNode *DATE();
    antlr4::tree::TerminalNode *TIME();
    antlr4::tree::TerminalNode *TIMESTAMP();
    antlr4::tree::TerminalNode *BOOLEAN();
    antlr4::tree::TerminalNode *DB2SECURITYLABEL();
    antlr4::tree::TerminalNode *SYSPROC();
    antlr4::tree::TerminalNode *DOT();

   
  };

  Method_non_xml_builtin_typeContext* method_non_xml_builtin_type();

  class  Method_named_typeContext : public antlr4::ParserRuleContext {
  public:
    Method_named_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Type_nameContext *type_name();

   
  };

  Method_named_typeContext* method_named_type();

  class  Method_integer_attributeContext : public antlr4::ParserRuleContext {
  public:
    Method_integer_attributeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Integer_valueContext *integer_value();
    antlr4::tree::TerminalNode *L_ONE();
    antlr4::tree::TerminalNode *L_ZERO();

   
  };

  Method_integer_attributeContext* method_integer_attribute();

  class  Data_type_3Context : public antlr4::ParserRuleContext {
  public:
    Data_type_3Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Data_typeContext *data_type();
    Inferred_builtin_typeContext *inferred_builtin_type();

   
  };

  Data_type_3Context* data_type_3();

  class  Sql_method_bodyContext : public antlr4::ParserRuleContext {
  public:
    Sql_method_bodyContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RETURN();
    Compound_sql_inlinedContext *compound_sql_inlined();

   
  };

  Sql_method_bodyContext* sql_method_body();

  class  Compound_sql_inlinedContext : public antlr4::ParserRuleContext {
  public:
    Compound_sql_inlinedContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BEGIN();
    antlr4::tree::TerminalNode *END();
    std::vector<LabelContext *> label();
    LabelContext* label(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COLON();
    antlr4::tree::TerminalNode* COLON(size_t i);
    antlr4::tree::TerminalNode *ATOMIC();
    std::vector<Declare_variable_statementContext *> declare_variable_statement();
    Declare_variable_statementContext* declare_variable_statement(size_t i);
    std::vector<antlr4::tree::TerminalNode *> SEMI();
    antlr4::tree::TerminalNode* SEMI(size_t i);
    std::vector<Compound_body_statementContext *> compound_body_statement();
    Compound_body_statementContext* compound_body_statement(size_t i);
    antlr4::tree::TerminalNode *NOT();

   
  };

  Compound_sql_inlinedContext* compound_sql_inlined();

  class  Declare_variable_statementContext : public antlr4::ParserRuleContext {
  public:
    Declare_variable_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DECLARE();
    std::vector<Variable_nameContext *> variable_name();
    Variable_nameContext* variable_name(size_t i);
    Data_typeContext *data_type();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);
    Declare_variable_default_clauseContext *declare_variable_default_clause();

   
  };

  Declare_variable_statementContext* declare_variable_statement();

  class  Assignment_statementContext : public antlr4::ParserRuleContext {
  public:
    Assignment_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SET();
    VariableContext *variable();
    antlr4::tree::TerminalNode *EQ();
    ExpressionContext *expression();

   
  };

  Assignment_statementContext* assignment_statement();

  class  Declare_variable_default_clauseContext : public antlr4::ParserRuleContext {
  public:
    Declare_variable_default_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DEFAULT();
    Default_valuesContext *default_values();

   
  };

  Declare_variable_default_clauseContext* declare_variable_default_clause();

  class  Compound_body_statementContext : public antlr4::ParserRuleContext {
  public:
    Compound_body_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Sql_constrol_statementContext *sql_constrol_statement();
    Assignment_statementContext *assignment_statement();
    Insert_statementContext *insert_statement();
    Merge_statementContext *merge_statement();
    Get_diagnostics_statementContext *get_diagnostics_statement();
    FullselectContext *fullselect();
    antlr4::tree::TerminalNode *WITH();
    Common_table_expression_listContext *common_table_expression_list();

   
  };

  Compound_body_statementContext* compound_body_statement();

  class  Sql_statement_inlinedContext : public antlr4::ParserRuleContext {
  public:
    Sql_statement_inlinedContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Call_statementContext *call_statement();
    For_statementContext *for_statement();
    FullselectContext *fullselect();
    antlr4::tree::TerminalNode *WITH();
    Common_table_expression_listContext *common_table_expression_list();
    Get_diagnostics_statementContext *get_diagnostics_statement();
    If_statementContext *if_statement();
    Insert_statementContext *insert_statement();
    Iterate_statementContext *iterate_statement();
    Leave_statementContext *leave_statement();
    Merge_statementContext *merge_statement();
    Return_statementContext *return_statement();
    Set_statementContext *set_statement();
    While_statementContext *while_statement();

   
  };

  Sql_statement_inlinedContext* sql_statement_inlined();

  class  Compound_sql_compiledContext : public antlr4::ParserRuleContext {
  public:
    Compound_sql_compiledContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BEGIN();
    std::vector<antlr4::tree::TerminalNode *> COMPOUND();
    antlr4::tree::TerminalNode* COMPOUND(size_t i);
    antlr4::tree::TerminalNode *ATOMIC();
    antlr4::tree::TerminalNode *STATIC();
    antlr4::tree::TerminalNode *END();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *STOP();
    antlr4::tree::TerminalNode *AFTER();
    antlr4::tree::TerminalNode *FIRST();
    Host_variableContext *host_variable();
    antlr4::tree::TerminalNode *STATEMENTS();
    std::vector<Sql_statement_compiledContext *> sql_statement_compiled();
    Sql_statement_compiledContext* sql_statement_compiled(size_t i);
    std::vector<antlr4::tree::TerminalNode *> SEMI();
    antlr4::tree::TerminalNode* SEMI(size_t i);

   
  };

  Compound_sql_compiledContext* compound_sql_compiled();

  class  Sql_statement_compiledContext : public antlr4::ParserRuleContext {
  public:
    Sql_statement_compiledContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TodoContext *todo();

   
  };

  Sql_statement_compiledContext* sql_statement_compiled();

  class  Create_module_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_module_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *MODULE();
    Module_nameContext *module_name();
    Or_replaceContext *or_replace();

   
  };

  Create_module_statementContext* create_module_statement();

  class  Create_nickname_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_nickname_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *NICKNAME();
    Nick_nameContext *nick_name();
    antlr4::tree::TerminalNode *FOR();
    Remote_object_nameContext *remote_object_name();
    Non_relational_data_definitionContext *non_relational_data_definition();
    Or_replaceContext *or_replace();
    antlr4::tree::TerminalNode *OPTIONS();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<Nick_name_option_nameContext *> nick_name_option_name();
    Nick_name_option_nameContext* nick_name_option_name(size_t i);
    std::vector<String_constantContext *> string_constant();
    String_constantContext* string_constant(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Create_nickname_statementContext* create_nickname_statement();

  class  Nick_name_option_nameContext : public antlr4::ParserRuleContext {
  public:
    Nick_name_option_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Nick_name_option_nameContext* nick_name_option_name();

  class  Remote_object_nameContext : public antlr4::ParserRuleContext {
  public:
    Remote_object_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Remote_object_nameContext* remote_object_name();

  class  Non_relational_data_definitionContext : public antlr4::ParserRuleContext {
  public:
    Non_relational_data_definitionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Nick_name_column_listContext *nick_name_column_list();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *SERVER();
    Server_nameContext *server_name();

   
  };

  Non_relational_data_definitionContext* non_relational_data_definition();

  class  Nick_name_column_listContext : public antlr4::ParserRuleContext {
  public:
    Nick_name_column_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<Nick_name_column_list_itemContext *> nick_name_column_list_item();
    Nick_name_column_list_itemContext* nick_name_column_list_item(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Nick_name_column_listContext* nick_name_column_list();

  class  Nick_name_column_list_itemContext : public antlr4::ParserRuleContext {
  public:
    Nick_name_column_list_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Nick_name_column_definitionContext *nick_name_column_definition();
    Unique_constraintContext *unique_constraint();
    Referential_constraintContext *referential_constraint();
    Check_constraintContext *check_constraint();

   
  };

  Nick_name_column_list_itemContext* nick_name_column_list_item();

  class  Nick_name_column_definitionContext : public antlr4::ParserRuleContext {
  public:
    Nick_name_column_definitionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Column_nameContext *column_name();
    Local_data_typeContext *local_data_type();
    std::vector<Nick_name_column_optionsContext *> nick_name_column_options();
    Nick_name_column_optionsContext* nick_name_column_options(size_t i);

   
  };

  Nick_name_column_definitionContext* nick_name_column_definition();

  class  Nick_name_column_optionsContext : public antlr4::ParserRuleContext {
  public:
    Nick_name_column_optionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *NULL_();
    Constraint_attributesContext *constraint_attributes();
    References_clauseContext *references_clause();
    antlr4::tree::TerminalNode *CHECK();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Check_conditionContext *check_condition();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *CONSTRAINT();
    Constraint_nameContext *constraint_name();
    antlr4::tree::TerminalNode *PRIMARY();
    antlr4::tree::TerminalNode *KEY();
    antlr4::tree::TerminalNode *UNIQUE();
    Federated_column_optionsContext *federated_column_options();

   
  };

  Nick_name_column_optionsContext* nick_name_column_options();

  class  Federated_column_optionsContext : public antlr4::ParserRuleContext {
  public:
    Federated_column_optionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OPTIONS();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<Column_option_nameContext *> column_option_name();
    Column_option_nameContext* column_option_name(size_t i);
    std::vector<String_constantContext *> string_constant();
    String_constantContext* string_constant(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Federated_column_optionsContext* federated_column_options();

  class  Column_option_nameContext : public antlr4::ParserRuleContext {
  public:
    Column_option_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Column_option_nameContext* column_option_name();

  class  Create_permission_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_permission_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *PERMISSION();
    Permission_nameContext *permission_name();
    antlr4::tree::TerminalNode *ON();
    Table_nameContext *table_name();
    std::vector<antlr4::tree::TerminalNode *> FOR();
    antlr4::tree::TerminalNode* FOR(size_t i);
    antlr4::tree::TerminalNode *ROWS();
    Where_clauseContext *where_clause();
    antlr4::tree::TerminalNode *ENFORCED();
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *ACCESS();
    Or_replaceContext *or_replace();
    Correlation_nameContext *correlation_name();
    Enable_disableContext *enable_disable();
    antlr4::tree::TerminalNode *AS();

   
  };

  Create_permission_statementContext* create_permission_statement();

  class  Create_procedure_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_procedure_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Create_procedure_external_statementContext *create_procedure_external_statement();
    Create_procedure_sourced_statementContext *create_procedure_sourced_statement();
    Create_procedure_sql_statementContext *create_procedure_sql_statement();

   
  };

  Create_procedure_statementContext* create_procedure_statement();

  class  Create_procedure_external_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_procedure_external_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *PROCEDURE();
    Procedure_nameContext *procedure_name();
    Or_replaceContext *or_replace();
    Proc_ext_param_listContext *proc_ext_param_list();
    Option_list_2Context *option_list_2();

   
  };

  Create_procedure_external_statementContext* create_procedure_external_statement();

  class  Proc_ext_param_listContext : public antlr4::ParserRuleContext {
  public:
    Proc_ext_param_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    std::vector<Proc_ext_paramContext *> proc_ext_param();
    Proc_ext_paramContext* proc_ext_param(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Proc_ext_param_listContext* proc_ext_param_list();

  class  Proc_ext_paramContext : public antlr4::ParserRuleContext {
  public:
    Proc_ext_paramContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Data_typeContext *data_type();
    Procedure_external_xml_typeContext *procedure_external_xml_type();
    In_out_inoutContext *in_out_inout();
    Parameter_nameContext *parameter_name();
    Routine_default_clauseContext *routine_default_clause();

   
  };

  Proc_ext_paramContext* proc_ext_param();

  class  Procedure_external_xml_typeContext : public antlr4::ParserRuleContext {
  public:
    Procedure_external_xml_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *XML();
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *CLOB();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Integer_valueContext *integer_value();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    KmgContext *kmg();

   
  };

  Procedure_external_xml_typeContext* procedure_external_xml_type();

  class  Option_list_2Context : public antlr4::ParserRuleContext {
  public:
    Option_list_2Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Option_list_2_itemContext *> option_list_2_item();
    Option_list_2_itemContext* option_list_2_item(size_t i);

   
  };

  Option_list_2Context* option_list_2();

  class  Option_list_2_itemContext : public antlr4::ParserRuleContext {
  public:
    Option_list_2_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LANGUAGE();
    antlr4::tree::TerminalNode *C_();
    antlr4::tree::TerminalNode *JAVA();
    antlr4::tree::TerminalNode *COBOL();
    antlr4::tree::TerminalNode *CLR();
    antlr4::tree::TerminalNode *OLE();
    antlr4::tree::TerminalNode *SPECIFIC();
    Specific_nameContext *specific_name();
    antlr4::tree::TerminalNode *DYNAMIC();
    antlr4::tree::TerminalNode *RESULT();
    antlr4::tree::TerminalNode *SETS();
    Integer_valueContext *integer_value();
    antlr4::tree::TerminalNode *SQL();
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *MODIFIES();
    antlr4::tree::TerminalNode *READS();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *CONTAINS();
    antlr4::tree::TerminalNode *DETERMINISTIC();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *CALLED();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *NULL_();
    antlr4::tree::TerminalNode *INPUT();
    Old_newContext *old_new();
    antlr4::tree::TerminalNode *SAVEPOINT();
    antlr4::tree::TerminalNode *LEVEL();
    antlr4::tree::TerminalNode *EXTERNAL();
    antlr4::tree::TerminalNode *NAME();
    StringContext *string();
    Id_Context *id_();
    antlr4::tree::TerminalNode *FENCED();
    antlr4::tree::TerminalNode *THREADSAFE();
    antlr4::tree::TerminalNode *COMMIT();
    antlr4::tree::TerminalNode *RETURN();
    Yes_noContext *yes_no();
    antlr4::tree::TerminalNode *AUTONOMOUS();
    antlr4::tree::TerminalNode *ACTION();
    antlr4::tree::TerminalNode *INHERIT();
    antlr4::tree::TerminalNode *SPECIAL();
    antlr4::tree::TerminalNode *REGISTERS();
    antlr4::tree::TerminalNode *PARAMETER();
    antlr4::tree::TerminalNode *STYLE();
    antlr4::tree::TerminalNode *DB2GENERAL();
    antlr4::tree::TerminalNode *DB2SQL();
    antlr4::tree::TerminalNode *GENERAL();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *RULES();
    antlr4::tree::TerminalNode *CCSID();
    antlr4::tree::TerminalNode *ASCII();
    antlr4::tree::TerminalNode *UNICODE();
    antlr4::tree::TerminalNode *PROGRAM();
    antlr4::tree::TerminalNode *TYPE();
    antlr4::tree::TerminalNode *SUB();
    antlr4::tree::TerminalNode *MAIN();
    antlr4::tree::TerminalNode *DBINFO();
    antlr4::tree::TerminalNode *STAY();
    antlr4::tree::TerminalNode *RESIDENT();

   
  };

  Option_list_2_itemContext* option_list_2_item();

  class  Create_procedure_sourced_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_procedure_sourced_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *PROCEDURE();
    Procedure_nameContext *procedure_name();
    Source_procedure_clauseContext *source_procedure_clause();
    Or_replaceContext *or_replace();
    Option_list_1Context *option_list_1();

   
  };

  Create_procedure_sourced_statementContext* create_procedure_sourced_statement();

  class  Source_procedure_clauseContext : public antlr4::ParserRuleContext {
  public:
    Source_procedure_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SOURCE();
    Source_object_nameContext *source_object_name();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *SERVER();
    Server_nameContext *server_name();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *NUMBER();
    antlr4::tree::TerminalNode *OF();
    antlr4::tree::TerminalNode *PARAMETERS();
    Integer_valueContext *integer_value();
    antlr4::tree::TerminalNode *UNIQUE();
    antlr4::tree::TerminalNode *ID();
    Unique_idContext *unique_id();

   
  };

  Source_procedure_clauseContext* source_procedure_clause();

  class  Source_object_nameContext : public antlr4::ParserRuleContext {
  public:
    Source_object_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Source_schema_nameContext *source_schema_name();
    std::vector<antlr4::tree::TerminalNode *> DOT();
    antlr4::tree::TerminalNode* DOT(size_t i);
    Source_procedure_nameContext *source_procedure_name();
    Source_package_nameContext *source_package_name();

   
  };

  Source_object_nameContext* source_object_name();

  class  Option_list_1Context : public antlr4::ParserRuleContext {
  public:
    Option_list_1Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Option_list_1_itemContext *> option_list_1_item();
    Option_list_1_itemContext* option_list_1_item(size_t i);

   
  };

  Option_list_1Context* option_list_1();

  class  Option_list_1_itemContext : public antlr4::ParserRuleContext {
  public:
    Option_list_1_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SPECIFIC();
    Specific_nameContext *specific_name();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *RETURN();
    antlr4::tree::TerminalNode *TO();
    antlr4::tree::TerminalNode *CALLER();
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<Result_set_element_numberContext *> result_set_element_number();
    Result_set_element_numberContext* result_set_element_number(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *SQL();
    antlr4::tree::TerminalNode *CONTAINS();
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *MODIFIES();
    antlr4::tree::TerminalNode *READS();
    antlr4::tree::TerminalNode *DETERMINISTIC();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *EXTERNAL();
    antlr4::tree::TerminalNode *ACTION();

   
  };

  Option_list_1_itemContext* option_list_1_item();

  class  Result_set_element_numberContext : public antlr4::ParserRuleContext {
  public:
    Result_set_element_numberContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Integer_constantContext *integer_constant();

   
  };

  Result_set_element_numberContext* result_set_element_number();

  class  Unique_idContext : public antlr4::ParserRuleContext {
  public:
    Unique_idContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    String_constantContext *string_constant();

   
  };

  Unique_idContext* unique_id();

  class  Create_procedure_sql_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_procedure_sql_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *PROCEDURE();
    Procedure_nameContext *procedure_name();
    Sql_procedure_bodyContext *sql_procedure_body();
    Or_replaceContext *or_replace();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    Option_listContext *option_list();
    Proc_parameter_listContext *proc_parameter_list();

   
  };

  Create_procedure_sql_statementContext* create_procedure_sql_statement();

  class  Proc_parameter_listContext : public antlr4::ParserRuleContext {
  public:
    Proc_parameter_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Proc_parameter_list_itemContext *> proc_parameter_list_item();
    Proc_parameter_list_itemContext* proc_parameter_list_item(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Proc_parameter_listContext* proc_parameter_list();

  class  Proc_parameter_list_itemContext : public antlr4::ParserRuleContext {
  public:
    Proc_parameter_list_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Parameter_nameContext *parameter_name();
    Data_typeContext *data_type();
    In_out_inoutContext *in_out_inout();
    Routine_default_clauseContext *routine_default_clause();

   
  };

  Proc_parameter_list_itemContext* proc_parameter_list_item();

  class  In_out_inoutContext : public antlr4::ParserRuleContext {
  public:
    In_out_inoutContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *IN();
    antlr4::tree::TerminalNode *OUT();
    antlr4::tree::TerminalNode *INOUT();

   
  };

  In_out_inoutContext* in_out_inout();

  class  Option_listContext : public antlr4::ParserRuleContext {
  public:
    Option_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Option_list_itemContext *> option_list_item();
    Option_list_itemContext* option_list_item(size_t i);

   
  };

  Option_listContext* option_list();

  class  Option_list_itemContext : public antlr4::ParserRuleContext {
  public:
    Option_list_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LANGUAGE();
    antlr4::tree::TerminalNode *SQL();
    antlr4::tree::TerminalNode *SPECIFIC();
    Specific_nameContext *specific_name();
    antlr4::tree::TerminalNode *DYNAMIC();
    antlr4::tree::TerminalNode *RESULT();
    antlr4::tree::TerminalNode *SETS();
    Integer_valueContext *integer_value();
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *MODIFIES();
    antlr4::tree::TerminalNode *READS();
    antlr4::tree::TerminalNode *CONTAINS();
    antlr4::tree::TerminalNode *DETERMINISTIC();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *CALLED();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *NULL_();
    antlr4::tree::TerminalNode *INPUT();
    antlr4::tree::TerminalNode *COMMIT();
    antlr4::tree::TerminalNode *RETURN();
    Yes_noContext *yes_no();
    antlr4::tree::TerminalNode *AUTONOMOUS();
    antlr4::tree::TerminalNode *INHERIT();
    antlr4::tree::TerminalNode *SPECIAL();
    antlr4::tree::TerminalNode *REGISTERS();
    Old_newContext *old_new();
    antlr4::tree::TerminalNode *SAVEPOINT();
    antlr4::tree::TerminalNode *LEVEL();
    antlr4::tree::TerminalNode *EXTERNAL();
    antlr4::tree::TerminalNode *ACTION();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *PARAMETER();
    antlr4::tree::TerminalNode *CCSID();
    antlr4::tree::TerminalNode *ASCII();
    antlr4::tree::TerminalNode *UNICODE();

   
  };

  Option_list_itemContext* option_list_item();

  class  Sql_procedure_bodyContext : public antlr4::ParserRuleContext {
  public:
    Sql_procedure_bodyContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Compound_sql_inlinedContext *compound_sql_inlined();
    Sql_procedure_statementContext *sql_procedure_statement();

   
  };

  Sql_procedure_bodyContext* sql_procedure_body();

  class  Create_role_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_role_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *ROLE();
    Role_nameContext *role_name();

   
  };

  Create_role_statementContext* create_role_statement();

  class  Create_schema_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_schema_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *SCHEMA();
    Schema_nameContext *schema_name();
    antlr4::tree::TerminalNode *AUTHORIZATION();
    Authorization_nameContext *authorization_name();
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *CAPTURE();
    None_changesContext *none_changes();
    std::vector<Schema_sql_statementContext *> schema_sql_statement();
    Schema_sql_statementContext* schema_sql_statement(size_t i);

   
  };

  Create_schema_statementContext* create_schema_statement();

  class  Schema_sql_statementContext : public antlr4::ParserRuleContext {
  public:
    Schema_sql_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Create_table_statementContext *create_table_statement();
    Create_view_statementContext *create_view_statement();
    Create_index_statementContext *create_index_statement();
    Comment_statementContext *comment_statement();

   
  };

  Schema_sql_statementContext* schema_sql_statement();

  class  Create_security_label_component_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_security_label_component_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *SECURITY();
    antlr4::tree::TerminalNode *LABEL();
    antlr4::tree::TerminalNode *COMPONENT();
    Component_nameContext *component_name();
    Array_clauseContext *array_clause();
    Set_clauseContext *set_clause();
    Tree_clauseContext *tree_clause();

   
  };

  Create_security_label_component_statementContext* create_security_label_component_statement();

  class  Array_clauseContext : public antlr4::ParserRuleContext {
  public:
    Array_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ARRAY();
    antlr4::tree::TerminalNode *LEFT_SQR_BKT();
    std::vector<String_constantContext *> string_constant();
    String_constantContext* string_constant(size_t i);
    antlr4::tree::TerminalNode *RIGHT_SQR_BKT();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Array_clauseContext* array_clause();

  class  Set_clauseContext : public antlr4::ParserRuleContext {
  public:
    Set_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SET();
    antlr4::tree::TerminalNode *LEFT_CURLY_BKT();
    std::vector<String_constantContext *> string_constant();
    String_constantContext* string_constant(size_t i);
    antlr4::tree::TerminalNode *RIGHT_CURLY_BKT();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Set_clauseContext* set_clause();

  class  Tree_clauseContext : public antlr4::ParserRuleContext {
  public:
    Tree_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TREE();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    String_constantContext *string_constant();
    antlr4::tree::TerminalNode *ROOT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    std::vector<Tree_clause_itemContext *> tree_clause_item();
    Tree_clause_itemContext* tree_clause_item(size_t i);

   
  };

  Tree_clauseContext* tree_clause();

  class  Tree_clause_itemContext : public antlr4::ParserRuleContext {
  public:
    Tree_clause_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *COMMA();
    std::vector<String_constantContext *> string_constant();
    String_constantContext* string_constant(size_t i);
    antlr4::tree::TerminalNode *UNDER();

   
  };

  Tree_clause_itemContext* tree_clause_item();

  class  Create_security_label_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_security_label_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *SECURITY();
    antlr4::tree::TerminalNode *LABEL();
    Security_label_nameContext *security_label_name();
    std::vector<Create_security_label_itemContext *> create_security_label_item();
    Create_security_label_itemContext* create_security_label_item(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Create_security_label_statementContext* create_security_label_statement();

  class  Create_security_label_itemContext : public antlr4::ParserRuleContext {
  public:
    Create_security_label_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *COMPONENT();
    Component_nameContext *component_name();
    std::vector<String_constantContext *> string_constant();
    String_constantContext* string_constant(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Create_security_label_itemContext* create_security_label_item();

  class  Create_security_policy_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_security_policy_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    std::vector<antlr4::tree::TerminalNode *> SECURITY();
    antlr4::tree::TerminalNode* SECURITY(size_t i);
    antlr4::tree::TerminalNode *POLICY();
    Security_policy_nameContext *security_policy_name();
    antlr4::tree::TerminalNode *COMPONENTS();
    std::vector<Component_nameContext *> component_name();
    Component_nameContext* component_name(size_t i);
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *DB2LBACRULES();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *AUTHORIZED();
    antlr4::tree::TerminalNode *WRITE();
    antlr4::tree::TerminalNode *LABEL();
    antlr4::tree::TerminalNode *OVERRIDE_();
    antlr4::tree::TerminalNode *RESTRICT();

   
  };

  Create_security_policy_statementContext* create_security_policy_statement();

  class  Create_sequence_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_sequence_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *SEQUENCE();
    Sequence_nameContext *sequence_name();
    Or_replaceContext *or_replace();
    antlr4::tree::TerminalNode *AS();
    Create_sequence_optsContext *create_sequence_opts();
    antlr4::tree::TerminalNode *INTEGER();
    Data_typeContext *data_type();

   
  };

  Create_sequence_statementContext* create_sequence_statement();

  class  Create_sequence_optsContext : public antlr4::ParserRuleContext {
  public:
    Create_sequence_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Create_sequence_opts_itemContext *> create_sequence_opts_item();
    Create_sequence_opts_itemContext* create_sequence_opts_item(size_t i);

   
  };

  Create_sequence_optsContext* create_sequence_opts();

  class  Create_sequence_opts_itemContext : public antlr4::ParserRuleContext {
  public:
    Create_sequence_opts_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *START();
    antlr4::tree::TerminalNode *WITH();
    Numeric_constantContext *numeric_constant();
    antlr4::tree::TerminalNode *INCREMENT();
    antlr4::tree::TerminalNode *BY();
    antlr4::tree::TerminalNode *MINVALUE();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *MAXVALUE();
    antlr4::tree::TerminalNode *CYCLE();
    antlr4::tree::TerminalNode *CACHE();
    antlr4::tree::TerminalNode *ORDER();

   
  };

  Create_sequence_opts_itemContext* create_sequence_opts_item();

  class  Create_service_class_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_service_class_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *SERVICE();
    antlr4::tree::TerminalNode *CLASS_();
    Service_class_nameContext *service_class_name();
    Histogram_templace_clauseContext *histogram_templace_clause();
    antlr4::tree::TerminalNode *UNDER();
    Service_superclass_nameContext *service_superclass_name();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *WORKLOAD();
    antlr4::tree::TerminalNode *TYPE();
    std::vector<antlr4::tree::TerminalNode *> RESOURCE();
    antlr4::tree::TerminalNode* RESOURCE(size_t i);
    std::vector<antlr4::tree::TerminalNode *> SHARES();
    antlr4::tree::TerminalNode* SHARES(size_t i);
    std::vector<Integer_constantContext *> integer_constant();
    Integer_constantContext* integer_constant(size_t i);
    std::vector<antlr4::tree::TerminalNode *> CPU();
    antlr4::tree::TerminalNode* CPU(size_t i);
    antlr4::tree::TerminalNode *LIMIT();
    antlr4::tree::TerminalNode *MINIMUM();
    antlr4::tree::TerminalNode *SHARE();
    antlr4::tree::TerminalNode *PERCENT();
    antlr4::tree::TerminalNode *ADMISSION();
    antlr4::tree::TerminalNode *QUEUE();
    antlr4::tree::TerminalNode *ORDER();
    std::vector<antlr4::tree::TerminalNode *> DEGREE_();
    antlr4::tree::TerminalNode* DEGREE_(size_t i);
    antlr4::tree::TerminalNode *SCALEBACK();
    antlr4::tree::TerminalNode *MAXIMUM();
    antlr4::tree::TerminalNode *PREFETCH();
    std::vector<antlr4::tree::TerminalNode *> PRIORITY();
    antlr4::tree::TerminalNode* PRIORITY(size_t i);
    antlr4::tree::TerminalNode *OUTBOUND();
    antlr4::tree::TerminalNode *CORRELATOR();
    antlr4::tree::TerminalNode *BUFFERPOOL();
    std::vector<antlr4::tree::TerminalNode *> COLLECT();
    antlr4::tree::TerminalNode* COLLECT(size_t i);
    std::vector<antlr4::tree::TerminalNode *> AGGREGATE();
    antlr4::tree::TerminalNode* AGGREGATE(size_t i);
    antlr4::tree::TerminalNode *ACTIVITY();
    std::vector<antlr4::tree::TerminalNode *> DATA();
    antlr4::tree::TerminalNode* DATA(size_t i);
    std::vector<antlr4::tree::TerminalNode *> REQUEST();
    antlr4::tree::TerminalNode* REQUEST(size_t i);
    antlr4::tree::TerminalNode *UNIT();
    antlr4::tree::TerminalNode *OF();
    antlr4::tree::TerminalNode *WORK();
    antlr4::tree::TerminalNode *METRICS();
    Enable_disableContext *enable_disable();
    antlr4::tree::TerminalNode *CUSTOM();
    antlr4::tree::TerminalNode *BATCH();
    antlr4::tree::TerminalNode *INTERACTIVE();
    antlr4::tree::TerminalNode *MIXED();
    antlr4::tree::TerminalNode *FIFO();
    antlr4::tree::TerminalNode *LATENCY();
    std::vector<antlr4::tree::TerminalNode *> NONE();
    antlr4::tree::TerminalNode* NONE(size_t i);
    std::vector<antlr4::tree::TerminalNode *> DEFAULT();
    antlr4::tree::TerminalNode* DEFAULT(size_t i);
    On_offContext *on_off();
    DegreeContext *degree();
    std::vector<High_medium_lowContext *> high_medium_low();
    High_medium_lowContext* high_medium_low(size_t i);
    String_constantContext *string_constant();
    std::vector<Soft_hardContext *> soft_hard();
    Soft_hardContext* soft_hard(size_t i);
    std::vector<antlr4::tree::TerminalNode *> BASE();
    antlr4::tree::TerminalNode* BASE(size_t i);
    std::vector<antlr4::tree::TerminalNode *> EXTENDED();
    antlr4::tree::TerminalNode* EXTENDED(size_t i);

   
  };

  Create_service_class_statementContext* create_service_class_statement();

  class  High_medium_lowContext : public antlr4::ParserRuleContext {
  public:
    High_medium_lowContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *HIGH();
    antlr4::tree::TerminalNode *MEDIUM();
    antlr4::tree::TerminalNode *LOW();

   
  };

  High_medium_lowContext* high_medium_low();

  class  On_offContext : public antlr4::ParserRuleContext {
  public:
    On_offContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *OFF();

   
  };

  On_offContext* on_off();

  class  Soft_hardContext : public antlr4::ParserRuleContext {
  public:
    Soft_hardContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SOFT();
    antlr4::tree::TerminalNode *HARD();

   
  };

  Soft_hardContext* soft_hard();

  class  Create_server_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_server_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *SERVER();
    Server_nameContext *server_name();
    antlr4::tree::TerminalNode *WRAPPER();
    Wrapper_nameContext *wrapper_name();
    antlr4::tree::TerminalNode *VERSION();
    Server_versionContext *server_version();
    antlr4::tree::TerminalNode *TYPE();
    Server_typeContext *server_type();
    antlr4::tree::TerminalNode *AUTHORIZATION();
    Authorization_nameContext *authorization_name();
    antlr4::tree::TerminalNode *PASSWORD();
    Password_Context *password_();
    antlr4::tree::TerminalNode *OPTIONS();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<Server_option_nameContext *> server_option_name();
    Server_option_nameContext* server_option_name(size_t i);
    std::vector<String_constantContext *> string_constant();
    String_constantContext* string_constant(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Create_server_statementContext* create_server_statement();

  class  Password_Context : public antlr4::ParserRuleContext {
  public:
    Password_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TodoContext *todo();

   
  };

  Password_Context* password_();

  class  Create_stogroup_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_stogroup_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *STOGROUP();
    Storagegroup_nameContext *storagegroup_name();
    antlr4::tree::TerminalNode *ON();
    std::vector<StringContext *> string();
    StringContext* string(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);
    std::vector<Create_stogroup_optsContext *> create_stogroup_opts();
    Create_stogroup_optsContext* create_stogroup_opts(size_t i);

   
  };

  Create_stogroup_statementContext* create_stogroup_statement();

  class  Create_stogroup_optsContext : public antlr4::ParserRuleContext {
  public:
    Create_stogroup_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OVERHEAD();
    Number_of_millisecondsContext *number_of_milliseconds();
    antlr4::tree::TerminalNode *DEVICE();
    antlr4::tree::TerminalNode *READ();
    antlr4::tree::TerminalNode *RATE();
    Number_megabytes_per_secondContext *number_megabytes_per_second();
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *TAG();
    Integer_constantContext *integer_constant();
    antlr4::tree::TerminalNode *NONE();
    antlr4::tree::TerminalNode *SET();
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *DEFAULT();

   
  };

  Create_stogroup_optsContext* create_stogroup_opts();

  class  Create_synonym_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_synonym_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();

   
  };

  Create_synonym_statementContext* create_synonym_statement();

  class  Create_table_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_table_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *TABLE();
    Table_nameContext *table_name();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Element_listContext *element_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *OF();
    Type_nameContext *type_name();
    antlr4::tree::TerminalNode *LIKE();
    As_result_tableContext *as_result_table();
    Materialized_query_optionsContext *materialized_query_options();
    Staging_table_definitionContext *staging_table_definition();
    If_not_existsContext *if_not_exists();
    antlr4::tree::TerminalNode *ORGANIZED();
    antlr4::tree::TerminalNode *BY();
    std::vector<Create_table_optsContext *> create_table_opts();
    Create_table_optsContext* create_table_opts(size_t i);
    Table_or_view_nameContext *table_or_view_name();
    Nick_nameContext *nick_name();
    Typed_table_optionsContext *typed_table_options();
    Copy_optionsContext *copy_options();
    antlr4::tree::TerminalNode *ROW();
    antlr4::tree::TerminalNode *COLUMN();
    Dimensions_clauseContext *dimensions_clause();
    antlr4::tree::TerminalNode *KEY();
    antlr4::tree::TerminalNode *SEQUENCE();
    Sequence_key_specContext *sequence_key_spec();
    antlr4::tree::TerminalNode *INSERT();
    antlr4::tree::TerminalNode *TIME();
    antlr4::tree::TerminalNode *USING();

   
  };

  Create_table_statementContext* create_table_statement();

  class  Create_table_optsContext : public antlr4::ParserRuleContext {
  public:
    Create_table_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *CAPTURE();
    None_changesContext *none_changes();
    Tablespace_clausesContext *tablespace_clauses();
    Distribution_clauseContext *distribution_clause();
    Partitioning_clauseContext *partitioning_clause();
    antlr4::tree::TerminalNode *COMPRESS();
    antlr4::tree::TerminalNode *YES();
    antlr4::tree::TerminalNode *STATIC();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *ADAPTIVE();
    antlr4::tree::TerminalNode *VALUE();
    antlr4::tree::TerminalNode *COMPRESSION();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *RESTRICT();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *DROP();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *LOGGED();
    antlr4::tree::TerminalNode *INITIALLY();
    antlr4::tree::TerminalNode *CCSID();
    antlr4::tree::TerminalNode *ASCII();
    antlr4::tree::TerminalNode *UNICODE();
    antlr4::tree::TerminalNode *SECURITY();
    antlr4::tree::TerminalNode *POLICY();
    Policy_nameContext *policy_name();
    antlr4::tree::TerminalNode *OPTIONS();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Table_option_listContext *table_option_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();

   
  };

  Create_table_optsContext* create_table_opts();

  class  Table_option_listContext : public antlr4::ParserRuleContext {
  public:
    Table_option_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Table_option_list_itemContext *> table_option_list_item();
    Table_option_list_itemContext* table_option_list_item(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Table_option_listContext* table_option_list();

  class  Table_option_list_itemContext : public antlr4::ParserRuleContext {
  public:
    Table_option_list_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Table_option_nameContext *table_option_name();
    String_constantContext *string_constant();

   
  };

  Table_option_list_itemContext* table_option_list_item();

  class  Table_option_nameContext : public antlr4::ParserRuleContext {
  public:
    Table_option_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    StringContext *string();

   
  };

  Table_option_nameContext* table_option_name();

  class  Element_listContext : public antlr4::ParserRuleContext {
  public:
    Element_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Element_list_itemContext *> element_list_item();
    Element_list_itemContext* element_list_item(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Element_listContext* element_list();

  class  Element_list_itemContext : public antlr4::ParserRuleContext {
  public:
    Element_list_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Column_definitionContext *column_definition();
    Period_definitionContext *period_definition();
    Unique_constraintContext *unique_constraint();
    Referential_constraintContext *referential_constraint();
    Check_constraintContext *check_constraint();

   
  };

  Element_list_itemContext* element_list_item();

  class  Column_definitionContext : public antlr4::ParserRuleContext {
  public:
    Column_definitionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Column_nameContext *column_name();
    Data_typeContext *data_type();
    Column_optionsContext *column_options();

   
  };

  Column_definitionContext* column_definition();

  class  Period_definitionContext : public antlr4::ParserRuleContext {
  public:
    Period_definitionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PERIOD();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<Column_nameContext *> column_name();
    Column_nameContext* column_name(size_t i);
    antlr4::tree::TerminalNode *COMMA();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *SYSTEM_TIME();
    antlr4::tree::TerminalNode *BUSINESS_TIME();

   
  };

  Period_definitionContext* period_definition();

  class  Unique_constraintContext : public antlr4::ParserRuleContext {
  public:
    Unique_constraintContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Column_name_listContext *column_name_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *UNIQUE();
    antlr4::tree::TerminalNode *PRIMARY();
    antlr4::tree::TerminalNode *KEY();
    antlr4::tree::TerminalNode *CONSTRAINT();
    Constraint_nameContext *constraint_name();
    antlr4::tree::TerminalNode *COMMA();
    antlr4::tree::TerminalNode *BUSINESS_TIME();
    antlr4::tree::TerminalNode *WITHOUT();
    antlr4::tree::TerminalNode *OVERLAPS();
    Constraint_attributesContext *constraint_attributes();

   
  };

  Unique_constraintContext* unique_constraint();

  class  Referential_constraintContext : public antlr4::ParserRuleContext {
  public:
    Referential_constraintContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FOREIGN();
    antlr4::tree::TerminalNode *KEY();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Column_name_listContext *column_name_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    References_clauseContext *references_clause();
    antlr4::tree::TerminalNode *CONSTRAINT();
    Constraint_nameContext *constraint_name();

   
  };

  Referential_constraintContext* referential_constraint();

  class  Check_constraintContext : public antlr4::ParserRuleContext {
  public:
    Check_constraintContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CHECK();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Check_conditionContext *check_condition();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *CONSTRAINT();
    Constraint_nameContext *constraint_name();
    Constraint_attributesContext *constraint_attributes();

   
  };

  Check_constraintContext* check_constraint();

  class  Column_optionsContext : public antlr4::ParserRuleContext {
  public:
    Column_optionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Column_options_itemContext *column_options_item();

   
  };

  Column_optionsContext* column_options();

  class  Column_options_itemContext : public antlr4::ParserRuleContext {
  public:
    Column_options_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *NULL_();
    Lob_optionsContext *lob_options();
    antlr4::tree::TerminalNode *SCOPE();
    Typed_table_nameContext *typed_table_name();
    Typed_view_nameContext *typed_view_name();
    References_clauseContext *references_clause();
    antlr4::tree::TerminalNode *CHECK();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Check_conditionContext *check_condition();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *CONSTRAINT();
    Constraint_nameContext *constraint_name();
    Constraint_attributesContext *constraint_attributes();
    antlr4::tree::TerminalNode *PRIMARY();
    antlr4::tree::TerminalNode *KEY();
    antlr4::tree::TerminalNode *UNIQUE();
    Default_clauseContext *default_clause();
    Generated_clauseContext *generated_clause();
    antlr4::tree::TerminalNode *INLINE();
    antlr4::tree::TerminalNode *LENGTH();
    Integer_valueContext *integer_value();
    antlr4::tree::TerminalNode *COMPRESS();
    antlr4::tree::TerminalNode *SYSTEM();
    antlr4::tree::TerminalNode *DEFAULT();
    antlr4::tree::TerminalNode *SECURED();
    antlr4::tree::TerminalNode *WITH();
    Security_label_nameContext *security_label_name();
    antlr4::tree::TerminalNode *COLUMN();
    antlr4::tree::TerminalNode *HIDDEN_();
    antlr4::tree::TerminalNode *IMPLICITLY();

   
  };

  Column_options_itemContext* column_options_item();

  class  References_clauseContext : public antlr4::ParserRuleContext {
  public:
    References_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REFERENCES();
    Table_nameContext *table_name();
    Nick_nameContext *nick_name();
    Column_name_list_parenContext *column_name_list_paren();
    Rule_clauseContext *rule_clause();
    Constraint_attributesContext *constraint_attributes();

   
  };

  References_clauseContext* references_clause();

  class  Rule_clauseContext : public antlr4::ParserRuleContext {
  public:
    Rule_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *DELETE();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *ACTION();
    antlr4::tree::TerminalNode *RESTRICT();
    antlr4::tree::TerminalNode *CASCADE();
    antlr4::tree::TerminalNode *SET();
    antlr4::tree::TerminalNode *NULL_();
    antlr4::tree::TerminalNode *UPDATE();

   
  };

  Rule_clauseContext* rule_clause();

  class  Constraint_attributesContext : public antlr4::ParserRuleContext {
  public:
    Constraint_attributesContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ENFORCED();
    std::vector<antlr4::tree::TerminalNode *> NOT();
    antlr4::tree::TerminalNode* NOT(size_t i);
    antlr4::tree::TerminalNode *TRUSTED();
    Enable_disableContext *enable_disable();
    antlr4::tree::TerminalNode *QUERY();
    antlr4::tree::TerminalNode *OPTIMIZATION();

   
  };

  Constraint_attributesContext* constraint_attributes();

  class  Routine_default_clauseContext : public antlr4::ParserRuleContext {
  public:
    Routine_default_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DEFAULT();
    Routine_default_valueContext *routine_default_value();

   
  };

  Routine_default_clauseContext* routine_default_clause();

  class  Routine_default_valueContext : public antlr4::ParserRuleContext {
  public:
    Routine_default_valueContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NULL_();
    Constant_Context *constant_();
    Routine_default_special_registerContext *routine_default_special_register();
    Global_variableContext *global_variable();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    ExpressionContext *expression();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();

   
  };

  Routine_default_valueContext* routine_default_value();

  class  Routine_default_special_registerContext : public antlr4::ParserRuleContext {
  public:
    Routine_default_special_registerContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CURRENT();
    antlr4::tree::TerminalNode *DATE();
    antlr4::tree::TerminalNode *TIME();
    antlr4::tree::TerminalNode *USER();
    antlr4::tree::TerminalNode *SCHEMA();
    antlr4::tree::TerminalNode *MEMBER();
    antlr4::tree::TerminalNode *SQLID();
    antlr4::tree::TerminalNode *SESSION_USER();
    antlr4::tree::TerminalNode *SYSTEM_USER();
    antlr4::tree::TerminalNode *CURRENT_DATE();
    antlr4::tree::TerminalNode *CURRENT_TIME();
    antlr4::tree::TerminalNode *CURRENT_USER();
    antlr4::tree::TerminalNode *CURRENT_SCHEMA();
    antlr4::tree::TerminalNode *CURRENT_SQLID();
    antlr4::tree::TerminalNode *SYSDATE();
    Routine_default_timestamp_registerContext *routine_default_timestamp_register();

   
  };

  Routine_default_special_registerContext* routine_default_special_register();

  class  Routine_default_timestamp_registerContext : public antlr4::ParserRuleContext {
  public:
    Routine_default_timestamp_registerContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CURRENT();
    antlr4::tree::TerminalNode *TIMESTAMP();
    antlr4::tree::TerminalNode *CURRENT_TIMESTAMP();
    antlr4::tree::TerminalNode *LOCALTIMESTAMP();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    Routine_default_integer_attributeContext *routine_default_integer_attribute();

   
  };

  Routine_default_timestamp_registerContext* routine_default_timestamp_register();

  class  Routine_default_integer_attributeContext : public antlr4::ParserRuleContext {
  public:
    Routine_default_integer_attributeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Integer_valueContext *integer_value();
    antlr4::tree::TerminalNode *L_ONE();
    antlr4::tree::TerminalNode *L_ZERO();

   
  };

  Routine_default_integer_attributeContext* routine_default_integer_attribute();

  class  Default_clauseContext : public antlr4::ParserRuleContext {
  public:
    Default_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DEFAULT();
    antlr4::tree::TerminalNode *WITH();
    Default_valuesContext *default_values();

   
  };

  Default_clauseContext* default_clause();

  class  Default_valuesContext : public antlr4::ParserRuleContext {
  public:
    Default_valuesContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Constant_Context *constant_();
    Datetime_special_registerContext *datetime_special_register();
    User_special_registerContext *user_special_register();
    antlr4::tree::TerminalNode *CURRENT();
    antlr4::tree::TerminalNode *SCHEMA();
    antlr4::tree::TerminalNode *MEMBER();
    antlr4::tree::TerminalNode *NULL_();
    Cast_functionContext *cast_function();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *EMPTY_CLOB();
    antlr4::tree::TerminalNode *EMPTY_DBCLOB();
    antlr4::tree::TerminalNode *EMPTY_NCLOB();
    antlr4::tree::TerminalNode *EMPTY_BLOB();

   
  };

  Default_valuesContext* default_values();

  class  Generated_clauseContext : public antlr4::ParserRuleContext {
  public:
    Generated_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GENERATED();
    Identity_optionsContext *identity_options();
    As_row_change_timestamp_clauseContext *as_row_change_timestamp_clause();
    antlr4::tree::TerminalNode *ALWAYS();
    antlr4::tree::TerminalNode *BY();
    antlr4::tree::TerminalNode *DEFAULT();
    As_generated_expression_clauseContext *as_generated_expression_clause();
    As_row_transaction_timestamp_clauseContext *as_row_transaction_timestamp_clause();
    As_row_transaction_start_id_clauseContext *as_row_transaction_start_id_clause();

   
  };

  Generated_clauseContext* generated_clause();

  class  Datetime_special_registerContext : public antlr4::ParserRuleContext {
  public:
    Datetime_special_registerContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CURRENT();
    antlr4::tree::TerminalNode *DATE();
    antlr4::tree::TerminalNode *TIME();
    antlr4::tree::TerminalNode *TIMESTAMP();

   
  };

  Datetime_special_registerContext* datetime_special_register();

  class  User_special_registerContext : public antlr4::ParserRuleContext {
  public:
    User_special_registerContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CURRENT();
    antlr4::tree::TerminalNode *USER();
    antlr4::tree::TerminalNode *SESSION_USER();
    antlr4::tree::TerminalNode *SYSTEM_USER();

   
  };

  User_special_registerContext* user_special_register();

  class  Cast_functionContext : public antlr4::ParserRuleContext {
  public:
    Cast_functionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TodoContext *todo();

   
  };

  Cast_functionContext* cast_function();

  class  Identity_optionsContext : public antlr4::ParserRuleContext {
  public:
    Identity_optionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *IDENTITY();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    std::vector<Identity_options_itemContext *> identity_options_item();
    Identity_options_itemContext* identity_options_item(size_t i);

   
  };

  Identity_optionsContext* identity_options();

  class  Identity_options_itemContext : public antlr4::ParserRuleContext {
  public:
    Identity_options_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *START();
    antlr4::tree::TerminalNode *WITH();
    Numeric_constantContext *numeric_constant();
    antlr4::tree::TerminalNode *INCREMENT();
    antlr4::tree::TerminalNode *BY();
    antlr4::tree::TerminalNode *MINVALUE();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *MAXVALUE();
    antlr4::tree::TerminalNode *CYCLE();
    antlr4::tree::TerminalNode *CACHE();
    Integer_constantContext *integer_constant();
    antlr4::tree::TerminalNode *ORDER();

   
  };

  Identity_options_itemContext* identity_options_item();

  class  As_row_change_timestamp_clauseContext : public antlr4::ParserRuleContext {
  public:
    As_row_change_timestamp_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *EACH();
    std::vector<antlr4::tree::TerminalNode *> ROW();
    antlr4::tree::TerminalNode* ROW(size_t i);
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *UPDATE();
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *CHANGE();
    antlr4::tree::TerminalNode *TIMESTAMP();

   
  };

  As_row_change_timestamp_clauseContext* as_row_change_timestamp_clause();

  class  As_generated_expression_clauseContext : public antlr4::ParserRuleContext {
  public:
    As_generated_expression_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Generation_expressionContext *generation_expression();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();

   
  };

  As_generated_expression_clauseContext* as_generated_expression_clause();

  class  Generation_expressionContext : public antlr4::ParserRuleContext {
  public:
    Generation_expressionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TodoContext *todo();

   
  };

  Generation_expressionContext* generation_expression();

  class  As_row_transaction_timestamp_clauseContext : public antlr4::ParserRuleContext {
  public:
    As_row_transaction_timestamp_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *ROW();
    antlr4::tree::TerminalNode *BEGIN();
    antlr4::tree::TerminalNode *END();

   
  };

  As_row_transaction_timestamp_clauseContext* as_row_transaction_timestamp_clause();

  class  As_row_transaction_start_id_clauseContext : public antlr4::ParserRuleContext {
  public:
    As_row_transaction_start_id_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *TRANSACTION();
    antlr4::tree::TerminalNode *START();
    antlr4::tree::TerminalNode *ID();

   
  };

  As_row_transaction_start_id_clauseContext* as_row_transaction_start_id_clause();

  class  Oid_column_definitionContext : public antlr4::ParserRuleContext {
  public:
    Oid_column_definitionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REF();
    antlr4::tree::TerminalNode *IS();
    Oid_column_nameContext *oid_column_name();
    antlr4::tree::TerminalNode *USE();
    antlr4::tree::TerminalNode *GENERATED();

   
  };

  Oid_column_definitionContext* oid_column_definition();

  class  Range_partition_specContext : public antlr4::ParserRuleContext {
  public:
    Range_partition_specContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<antlr4::tree::TerminalNode *> LEFT_RND_BKT();
    antlr4::tree::TerminalNode* LEFT_RND_BKT(size_t i);
    Partition_expression_listContext *partition_expression_list();
    std::vector<antlr4::tree::TerminalNode *> RIGHT_RND_BKT();
    antlr4::tree::TerminalNode* RIGHT_RND_BKT(size_t i);
    Partition_element_listContext *partition_element_list();

   
  };

  Range_partition_specContext* range_partition_spec();

  class  Partition_expression_listContext : public antlr4::ParserRuleContext {
  public:
    Partition_expression_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Partition_expressionContext *> partition_expression();
    Partition_expressionContext* partition_expression(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Partition_expression_listContext* partition_expression_list();

  class  Partition_expressionContext : public antlr4::ParserRuleContext {
  public:
    Partition_expressionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Column_nameContext *column_name();
    antlr4::tree::TerminalNode *NULLS();
    First_lastContext *first_last();

   
  };

  Partition_expressionContext* partition_expression();

  class  Partition_element_listContext : public antlr4::ParserRuleContext {
  public:
    Partition_element_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Partition_elementContext *> partition_element();
    Partition_elementContext* partition_element(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Partition_element_listContext* partition_element_list();

  class  Partition_elementContext : public antlr4::ParserRuleContext {
  public:
    Partition_elementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Boundary_specContext *boundary_spec();
    antlr4::tree::TerminalNode *PARTITION();
    Partition_nameContext *partition_name();
    Partition_tablespace_optionsContext *partition_tablespace_options();
    antlr4::tree::TerminalNode *EVERY();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Constant_Context *constant_();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    Duration_labelContext *duration_label();

   
  };

  Partition_elementContext* partition_element();

  class  Boundary_specContext : public antlr4::ParserRuleContext {
  public:
    Boundary_specContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Starting_clauseContext *starting_clause();
    Ending_clauseContext *ending_clause();

   
  };

  Boundary_specContext* boundary_spec();

  class  Partition_tablespace_optionsContext : public antlr4::ParserRuleContext {
  public:
    Partition_tablespace_optionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LONG();
    std::vector<antlr4::tree::TerminalNode *> IN();
    antlr4::tree::TerminalNode* IN(size_t i);
    std::vector<Tablespace_nameContext *> tablespace_name();
    Tablespace_nameContext* tablespace_name(size_t i);
    antlr4::tree::TerminalNode *INDEX();

   
  };

  Partition_tablespace_optionsContext* partition_tablespace_options();

  class  Duration_labelContext : public antlr4::ParserRuleContext {
  public:
    Duration_labelContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *YEAR();
    antlr4::tree::TerminalNode *YEARS();
    antlr4::tree::TerminalNode *MONTH();
    antlr4::tree::TerminalNode *MONTHS();
    antlr4::tree::TerminalNode *DAY();
    antlr4::tree::TerminalNode *DAYS();
    antlr4::tree::TerminalNode *HOUR();
    antlr4::tree::TerminalNode *HOURS();
    antlr4::tree::TerminalNode *MINUTE();
    antlr4::tree::TerminalNode *MINUTES();
    antlr4::tree::TerminalNode *SECOND();
    antlr4::tree::TerminalNode *SECONDS();
    antlr4::tree::TerminalNode *MICROSECOND();
    antlr4::tree::TerminalNode *MICROSECONDS();

   
  };

  Duration_labelContext* duration_label();

  class  Starting_clauseContext : public antlr4::ParserRuleContext {
  public:
    Starting_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *STARTING();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Const_min_max_listContext *const_min_max_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    Const_min_maxContext *const_min_max();
    antlr4::tree::TerminalNode *FROM();

   
  };

  Starting_clauseContext* starting_clause();

  class  Const_min_max_listContext : public antlr4::ParserRuleContext {
  public:
    Const_min_max_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Const_min_maxContext *> const_min_max();
    Const_min_maxContext* const_min_max(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Const_min_max_listContext* const_min_max_list();

  class  Const_min_maxContext : public antlr4::ParserRuleContext {
  public:
    Const_min_maxContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Constant_Context *constant_();
    antlr4::tree::TerminalNode *MINVALUE();
    antlr4::tree::TerminalNode *MAXVALUE();

   
  };

  Const_min_maxContext* const_min_max();

  class  Ending_clauseContext : public antlr4::ParserRuleContext {
  public:
    Ending_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ENDING();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Const_min_max_listContext *const_min_max_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    Const_min_maxContext *const_min_max();
    antlr4::tree::TerminalNode *AT();
    antlr4::tree::TerminalNode *INCLUSIVE();
    antlr4::tree::TerminalNode *EXCLUSIVE();

   
  };

  Ending_clauseContext* ending_clause();

  class  Typed_table_optionsContext : public antlr4::ParserRuleContext {
  public:
    Typed_table_optionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Typed_element_listContext *typed_element_list();
    antlr4::tree::TerminalNode *HIERARCHY();
    Hierarchy_nameContext *hierarchy_name();
    Under_clauseContext *under_clause();

   
  };

  Typed_table_optionsContext* typed_table_options();

  class  Typed_element_listContext : public antlr4::ParserRuleContext {
  public:
    Typed_element_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<Typed_element_list_itemContext *> typed_element_list_item();
    Typed_element_list_itemContext* typed_element_list_item(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Typed_element_listContext* typed_element_list();

  class  Typed_element_list_itemContext : public antlr4::ParserRuleContext {
  public:
    Typed_element_list_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Oid_column_nameContext *oid_column_name();
    With_optionsContext *with_options();
    Unique_constraintContext *unique_constraint();
    Check_constraintContext *check_constraint();

   
  };

  Typed_element_list_itemContext* typed_element_list_item();

  class  As_result_tableContext : public antlr4::ParserRuleContext {
  public:
    As_result_tableContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *AS();
    Fullselect_in_parenthesesContext *fullselect_in_parentheses();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *DATA();
    Column_name_list_parenContext *column_name_list_paren();
    antlr4::tree::TerminalNode *NO();

   
  };

  As_result_tableContext* as_result_table();

  class  Copy_optionsContext : public antlr4::ParserRuleContext {
  public:
    Copy_optionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DEFAULTS();
    antlr4::tree::TerminalNode *INCLUDING();
    antlr4::tree::TerminalNode *EXCLUDING();
    antlr4::tree::TerminalNode *COLUMN();

   
  };

  Copy_optionsContext* copy_options();

  class  Materialized_query_optionsContext : public antlr4::ParserRuleContext {
  public:
    Materialized_query_optionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *AS();
    Fullselect_in_parenthesesContext *fullselect_in_parentheses();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *DATA();
    Column_name_list_parenContext *column_name_list_paren();
    antlr4::tree::TerminalNode *NO();

   
  };

  Materialized_query_optionsContext* materialized_query_options();

  class  Staging_table_definitionContext : public antlr4::ParserRuleContext {
  public:
    Staging_table_definitionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FOR();
    Table_nameContext *table_name();
    antlr4::tree::TerminalNode *PROPAGATE();
    antlr4::tree::TerminalNode *IMMEDIATE();
    Column_name_list_parenContext *column_name_list_paren();

   
  };

  Staging_table_definitionContext* staging_table_definition();

  class  Dimensions_clauseContext : public antlr4::ParserRuleContext {
  public:
    Dimensions_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<Col_namesContext *> col_names();
    Col_namesContext* col_names(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *DIMENSIONS();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Dimensions_clauseContext* dimensions_clause();

  class  Col_namesContext : public antlr4::ParserRuleContext {
  public:
    Col_namesContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Column_nameContext *column_name();
    Column_name_list_parenContext *column_name_list_paren();

   
  };

  Col_namesContext* col_names();

  class  Sequence_key_specContext : public antlr4::ParserRuleContext {
  public:
    Sequence_key_specContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Sequence_key_spec_listContext *sequence_key_spec_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *OVERFLOW();
    antlr4::tree::TerminalNode *ALLOW();
    antlr4::tree::TerminalNode *DISALLOW();
    antlr4::tree::TerminalNode *PCTFREE();
    Integer_valueContext *integer_value();

   
  };

  Sequence_key_specContext* sequence_key_spec();

  class  Sequence_key_spec_listContext : public antlr4::ParserRuleContext {
  public:
    Sequence_key_spec_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Sequence_key_spec_list_itemContext *> sequence_key_spec_list_item();
    Sequence_key_spec_list_itemContext* sequence_key_spec_list_item(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Sequence_key_spec_listContext* sequence_key_spec_list();

  class  Sequence_key_spec_list_itemContext : public antlr4::ParserRuleContext {
  public:
    Sequence_key_spec_list_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Column_nameContext *column_name();
    antlr4::tree::TerminalNode *ENDING();
    std::vector<Constant_Context *> constant_();
    Constant_Context* constant_(size_t i);
    antlr4::tree::TerminalNode *STARTING();
    antlr4::tree::TerminalNode *AT();
    antlr4::tree::TerminalNode *FROM();

   
  };

  Sequence_key_spec_list_itemContext* sequence_key_spec_list_item();

  class  Tablespace_clausesContext : public antlr4::ParserRuleContext {
  public:
    Tablespace_clausesContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Tablespace_name_listContext *> tablespace_name_list();
    Tablespace_name_listContext* tablespace_name_list(size_t i);
    antlr4::tree::TerminalNode *CYCLE();
    std::vector<antlr4::tree::TerminalNode *> IN();
    antlr4::tree::TerminalNode* IN(size_t i);
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *INDEX();
    Tablespace_nameContext *tablespace_name();
    antlr4::tree::TerminalNode *LONG();

   
  };

  Tablespace_clausesContext* tablespace_clauses();

  class  Distribution_clauseContext : public antlr4::ParserRuleContext {
  public:
    Distribution_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DISTRIBUTE();
    antlr4::tree::TerminalNode *BY();
    antlr4::tree::TerminalNode *HASH();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Column_name_listContext *column_name_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *REPLICATION();
    antlr4::tree::TerminalNode *RANDOM();

   
  };

  Distribution_clauseContext* distribution_clause();

  class  Partitioning_clauseContext : public antlr4::ParserRuleContext {
  public:
    Partitioning_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PARTITION();
    antlr4::tree::TerminalNode *BY();
    Range_partition_specContext *range_partition_spec();
    antlr4::tree::TerminalNode *RANGE();

   
  };

  Partitioning_clauseContext* partitioning_clause();

  class  If_not_existsContext : public antlr4::ParserRuleContext {
  public:
    If_not_existsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *IF();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *EXISTS();

   
  };

  If_not_existsContext* if_not_exists();

  class  Create_tablespace_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_tablespace_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *TABLESPACE();
    Tablespace_nameContext *tablespace_name();
    antlr4::tree::TerminalNode *LARGE();
    antlr4::tree::TerminalNode *REGULAR();
    antlr4::tree::TerminalNode *TEMPORARY();
    antlr4::tree::TerminalNode *IN();
    Db_partition_group_nameContext *db_partition_group_name();
    antlr4::tree::TerminalNode *PAGESIZE();
    std::vector<Integer_valueContext *> integer_value();
    Integer_valueContext* integer_value(size_t i);
    antlr4::tree::TerminalNode *MANAGED();
    antlr4::tree::TerminalNode *BY();
    antlr4::tree::TerminalNode *EXTENTSIZE();
    antlr4::tree::TerminalNode *PREFETCHSIZE();
    antlr4::tree::TerminalNode *BUFFERPOOL();
    Bufferpool_nameContext *bufferpool_name();
    antlr4::tree::TerminalNode *OVERHEAD();
    antlr4::tree::TerminalNode *FILE();
    std::vector<antlr4::tree::TerminalNode *> SYSTEM();
    antlr4::tree::TerminalNode* SYSTEM(size_t i);
    antlr4::tree::TerminalNode *CACHING();
    antlr4::tree::TerminalNode *TRANSFERRATE();
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *TAG();
    antlr4::tree::TerminalNode *DROPPED();
    antlr4::tree::TerminalNode *TABLE();
    antlr4::tree::TerminalNode *RECOVERY();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *OFF();
    std::vector<antlr4::tree::TerminalNode *> AUTOMATIC();
    antlr4::tree::TerminalNode* AUTOMATIC(size_t i);
    antlr4::tree::TerminalNode *STORAGE();
    std::vector<antlr4::tree::TerminalNode *> DATABASE();
    antlr4::tree::TerminalNode* DATABASE(size_t i);
    std::vector<Number_of_pagesContext *> number_of_pages();
    Number_of_pagesContext* number_of_pages(size_t i);
    std::vector<Number_of_millisecondsContext *> number_of_milliseconds();
    Number_of_millisecondsContext* number_of_milliseconds(size_t i);
    std::vector<antlr4::tree::TerminalNode *> INHERIT();
    antlr4::tree::TerminalNode* INHERIT(size_t i);
    Integer_constantContext *integer_constant();
    antlr4::tree::TerminalNode *NONE();
    antlr4::tree::TerminalNode *PARTITION();
    antlr4::tree::TerminalNode *GROUP();
    std::vector<antlr4::tree::TerminalNode *> K();
    antlr4::tree::TerminalNode* K(size_t i);
    std::vector<antlr4::tree::TerminalNode *> M();
    antlr4::tree::TerminalNode* M(size_t i);
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *USER();
    Storage_groupContext *storage_group();
    Size_attributesContext *size_attributes();
    std::vector<System_containersContext *> system_containers();
    System_containersContext* system_containers(size_t i);
    std::vector<Database_containersContext *> database_containers();
    Database_containersContext* database_containers(size_t i);

   
  };

  Create_tablespace_statementContext* create_tablespace_statement();

  class  Storage_groupContext : public antlr4::ParserRuleContext {
  public:
    Storage_groupContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *USING();
    antlr4::tree::TerminalNode *STOGROUP();
    Storagegroup_nameContext *storagegroup_name();

   
  };

  Storage_groupContext* storage_group();

  class  Size_attributesContext : public antlr4::ParserRuleContext {
  public:
    Size_attributesContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INITIALSIZE();
    std::vector<Integer_valueContext *> integer_value();
    Integer_valueContext* integer_value(size_t i);
    std::vector<KmgContext *> kmg();
    KmgContext* kmg(size_t i);
    antlr4::tree::TerminalNode *AUTORESIZE();
    Yes_noContext *yes_no();
    antlr4::tree::TerminalNode *INCREASESIZE();
    antlr4::tree::TerminalNode *MAXSIZE();
    antlr4::tree::TerminalNode *PERCENT();
    antlr4::tree::TerminalNode *NONE();

   
  };

  Size_attributesContext* size_attributes();

  class  System_containersContext : public antlr4::ParserRuleContext {
  public:
    System_containersContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *USING();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Container_string_listContext *container_string_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    On_db_partitions_clauseContext *on_db_partitions_clause();

   
  };

  System_containersContext* system_containers();

  class  Container_string_listContext : public antlr4::ParserRuleContext {
  public:
    Container_string_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<StringContext *> string();
    StringContext* string(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Container_string_listContext* container_string_list();

  class  Database_containersContext : public antlr4::ParserRuleContext {
  public:
    Database_containersContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *USING();
    Container_clauseContext *container_clause();
    On_db_partitions_clauseContext *on_db_partitions_clause();

   
  };

  Database_containersContext* database_containers();

  class  Container_clauseContext : public antlr4::ParserRuleContext {
  public:
    Container_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Container_clause_listContext *container_clause_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();

   
  };

  Container_clauseContext* container_clause();

  class  Container_clause_listContext : public antlr4::ParserRuleContext {
  public:
    Container_clause_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Container_clause_list_itemContext *> container_clause_list_item();
    Container_clause_list_itemContext* container_clause_list_item(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Container_clause_listContext* container_clause_list();

  class  Container_clause_list_itemContext : public antlr4::ParserRuleContext {
  public:
    Container_clause_list_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    File_deviceContext *file_device();
    StringContext *string();
    Number_of_pagesContext *number_of_pages();
    Integer_valueContext *integer_value();
    KmgContext *kmg();

   
  };

  Container_clause_list_itemContext* container_clause_list_item();

  class  On_db_partitions_clauseContext : public antlr4::ParserRuleContext {
  public:
    On_db_partitions_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ON();
    Db_partition_num_numsContext *db_partition_num_nums();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Db_partition_number_listContext *db_partition_number_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();

   
  };

  On_db_partitions_clauseContext* on_db_partitions_clause();

  class  Db_partition_number_listContext : public antlr4::ParserRuleContext {
  public:
    Db_partition_number_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Db_partition_number_list_itemContext *> db_partition_number_list_item();
    Db_partition_number_list_itemContext* db_partition_number_list_item(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Db_partition_number_listContext* db_partition_number_list();

  class  Db_partition_number_list_itemContext : public antlr4::ParserRuleContext {
  public:
    Db_partition_number_list_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Db_partition_numberContext *> db_partition_number();
    Db_partition_numberContext* db_partition_number(size_t i);
    antlr4::tree::TerminalNode *TO();

   
  };

  Db_partition_number_list_itemContext* db_partition_number_list_item();

  class  Db_partition_numberContext : public antlr4::ParserRuleContext {
  public:
    Db_partition_numberContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Integer_valueContext *integer_value();

   
  };

  Db_partition_numberContext* db_partition_number();

  class  Number_of_pagesContext : public antlr4::ParserRuleContext {
  public:
    Number_of_pagesContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Integer_valueContext *integer_value();

   
  };

  Number_of_pagesContext* number_of_pages();

  class  Number_of_filesContext : public antlr4::ParserRuleContext {
  public:
    Number_of_filesContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Integer_valueContext *integer_value();

   
  };

  Number_of_filesContext* number_of_files();

  class  Number_of_millisecondsContext : public antlr4::ParserRuleContext {
  public:
    Number_of_millisecondsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Integer_valueContext *integer_value();

   
  };

  Number_of_millisecondsContext* number_of_milliseconds();

  class  Number_megabytes_per_secondContext : public antlr4::ParserRuleContext {
  public:
    Number_megabytes_per_secondContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Integer_valueContext *integer_value();

   
  };

  Number_megabytes_per_secondContext* number_megabytes_per_second();

  class  Create_threshold_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_threshold_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *THRESHOLD();
    Threshold_nameContext *threshold_name();
    antlr4::tree::TerminalNode *FOR();
    Threshold_domainContext *threshold_domain();
    Enable_disableContext *enable_disable();
    antlr4::tree::TerminalNode *WHEN();
    Threshold_predicateContext *threshold_predicate();
    Threshold_exceeded_actions_2Context *threshold_exceeded_actions_2();
    antlr4::tree::TerminalNode *ENFORCEMENT();
    antlr4::tree::TerminalNode *DEFAULT();
    Enforcement_scopeContext *enforcement_scope();
    antlr4::tree::TerminalNode *ACTIVITIES();

   
  };

  Create_threshold_statementContext* create_threshold_statement();

  class  Threshold_domainContext : public antlr4::ParserRuleContext {
  public:
    Threshold_domainContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DATABASE();
    antlr4::tree::TerminalNode *SERVICE();
    antlr4::tree::TerminalNode *CLASS_();
    std::vector<Service_class_nameContext *> service_class_name();
    Service_class_nameContext* service_class_name(size_t i);
    antlr4::tree::TerminalNode *UNDER();
    antlr4::tree::TerminalNode *STATEMENT();
    antlr4::tree::TerminalNode *TEXT();
    Statement_textContext *statement_text();
    antlr4::tree::TerminalNode *REFERENCE();
    Executable_idContext *executable_id();
    antlr4::tree::TerminalNode *WORKLOAD();
    Workload_nameContext *workload_name();

   
  };

  Threshold_domainContext* threshold_domain();

  class  Statement_textContext : public antlr4::ParserRuleContext {
  public:
    Statement_textContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TodoContext *todo();

   
  };

  Statement_textContext* statement_text();

  class  Executable_idContext : public antlr4::ParserRuleContext {
  public:
    Executable_idContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TodoContext *todo();

   
  };

  Executable_idContext* executable_id();

  class  Enforcement_scopeContext : public antlr4::ParserRuleContext {
  public:
    Enforcement_scopeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DATABASE();
    antlr4::tree::TerminalNode *MEMBER();
    antlr4::tree::TerminalNode *WORKLOAD();
    antlr4::tree::TerminalNode *OCCURENCE();

   
  };

  Enforcement_scopeContext* enforcement_scope();

  class  Threshold_predicateContext : public antlr4::ParserRuleContext {
  public:
    Threshold_predicateContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TOTALMEMBERCONNECTIONS();
    std::vector<antlr4::tree::TerminalNode *> GT();
    antlr4::tree::TerminalNode* GT(size_t i);
    std::vector<Integer_valueContext *> integer_value();
    Integer_valueContext* integer_value(size_t i);
    antlr4::tree::TerminalNode *TOTALSCMEMBERCONNECTIONS();
    antlr4::tree::TerminalNode *AND();
    antlr4::tree::TerminalNode *QUEUEDCONNECTIONS();
    antlr4::tree::TerminalNode *UNBOUNDED();
    antlr4::tree::TerminalNode *CONNECTIONIDLETIME();
    Day_to_minutesContext *day_to_minutes();
    antlr4::tree::TerminalNode *CONCURRENTWORKLOADOCCURRENCES();
    antlr4::tree::TerminalNode *CONCURRENTWORKLOADACTIVITIES();
    antlr4::tree::TerminalNode *CONCURRENTDBCOORDACTIVITIES();
    antlr4::tree::TerminalNode *QUEUEDACTIVITIES();
    antlr4::tree::TerminalNode *ESTIMATEDSQLCOST();
    antlr4::tree::TerminalNode *SQLROWSRETURNED();
    Day_to_secondsContext *day_to_seconds();
    antlr4::tree::TerminalNode *ACTIVITYTOTALTIME();
    antlr4::tree::TerminalNode *UOWTOTALTIME();
    KmgContext *kmg();
    antlr4::tree::TerminalNode *SQLTEMPSPACE();
    antlr4::tree::TerminalNode *AGGSQLTEMPSPACE();
    Bigint_valueContext *bigint_value();
    antlr4::tree::TerminalNode *SQLROWSREAD();
    antlr4::tree::TerminalNode *SQLROWSREADINSC();
    Checking_everyContext *checking_every();
    Hour_to_secondsContext *hour_to_seconds();
    antlr4::tree::TerminalNode *CPUTIME();
    antlr4::tree::TerminalNode *CPUTIMEINSC();
    antlr4::tree::TerminalNode *ACTIVITYTOTALRUNTIME();
    antlr4::tree::TerminalNode *ACTIVITYTOTALRUNTIMEINALLSC();
    antlr4::tree::TerminalNode *SORTSHRHEAPUTIL();
    antlr4::tree::TerminalNode *PERCENT();
    antlr4::tree::TerminalNode *BLOCKING();
    antlr4::tree::TerminalNode *ADMISSION();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *DATATAGINSC();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Integer_constant_listContext *integer_constant_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *IN();
    antlr4::tree::TerminalNode *NOT();

   
  };

  Threshold_predicateContext* threshold_predicate();

  class  Checking_everyContext : public antlr4::ParserRuleContext {
  public:
    Checking_everyContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CHECKING();
    antlr4::tree::TerminalNode *EVERY();
    Integer_valueContext *integer_value();
    Second_secondsContext *second_seconds();

   
  };

  Checking_everyContext* checking_every();

  class  Hour_to_secondsContext : public antlr4::ParserRuleContext {
  public:
    Hour_to_secondsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *HOUR();
    antlr4::tree::TerminalNode *HOURS();
    antlr4::tree::TerminalNode *MINUTE();
    antlr4::tree::TerminalNode *MINUTES();
    antlr4::tree::TerminalNode *SECOND();
    antlr4::tree::TerminalNode *SECONDS();

   
  };

  Hour_to_secondsContext* hour_to_seconds();

  class  Day_to_minutesContext : public antlr4::ParserRuleContext {
  public:
    Day_to_minutesContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DAY();
    antlr4::tree::TerminalNode *DAYS();
    antlr4::tree::TerminalNode *HOUR();
    antlr4::tree::TerminalNode *HOURS();
    antlr4::tree::TerminalNode *MINUTE();
    antlr4::tree::TerminalNode *MINUTES();

   
  };

  Day_to_minutesContext* day_to_minutes();

  class  Day_to_secondsContext : public antlr4::ParserRuleContext {
  public:
    Day_to_secondsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DAY();
    antlr4::tree::TerminalNode *DAYS();
    antlr4::tree::TerminalNode *HOUR();
    antlr4::tree::TerminalNode *HOURS();
    antlr4::tree::TerminalNode *MINUTE();
    antlr4::tree::TerminalNode *MINUTES();
    antlr4::tree::TerminalNode *SECONDS();

   
  };

  Day_to_secondsContext* day_to_seconds();

  class  Threshold_exceeded_actions_2Context : public antlr4::ParserRuleContext {
  public:
    Threshold_exceeded_actions_2Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *STOP();
    antlr4::tree::TerminalNode *EXECUTION();
    antlr4::tree::TerminalNode *CONTINUE();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *APPLICATION();
    Remap_activity_actionContext *remap_activity_action();
    antlr4::tree::TerminalNode *COLLECT();
    antlr4::tree::TerminalNode *ACTIVITY();
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *WITHOUT();
    antlr4::tree::TerminalNode *DETAILS();
    antlr4::tree::TerminalNode *WITH();
    Details_sectionContext *details_section();
    TodoContext *todo();
    antlr4::tree::TerminalNode *AND();
    antlr4::tree::TerminalNode *VALUES();

   
  };

  Threshold_exceeded_actions_2Context* threshold_exceeded_actions_2();

  class  Details_sectionContext : public antlr4::ParserRuleContext {
  public:
    Details_sectionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DETAILS();
    antlr4::tree::TerminalNode *COMMA();
    antlr4::tree::TerminalNode *SECTION();

   
  };

  Details_sectionContext* details_section();

  class  Remap_activity_actionContext : public antlr4::ParserRuleContext {
  public:
    Remap_activity_actionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REMAP();
    antlr4::tree::TerminalNode *ACTIVITY();
    antlr4::tree::TerminalNode *TO();
    Service_subclass_nameContext *service_subclass_name();
    antlr4::tree::TerminalNode *EVENT();
    antlr4::tree::TerminalNode *MONITOR();
    antlr4::tree::TerminalNode *RECORD();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *LOG();

   
  };

  Remap_activity_actionContext* remap_activity_action();

  class  Create_transform_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_transform_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *FOR();
    Type_nameContext *type_name();
    Tranform_listContext *tranform_list();
    antlr4::tree::TerminalNode *TRANSFORM();
    antlr4::tree::TerminalNode *TRANSFORMS();

   
  };

  Create_transform_statementContext* create_transform_statement();

  class  Tranform_listContext : public antlr4::ParserRuleContext {
  public:
    Tranform_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Tranform_list_itemContext *> tranform_list_item();
    Tranform_list_itemContext* tranform_list_item(size_t i);

   
  };

  Tranform_listContext* tranform_list();

  class  Tranform_list_itemContext : public antlr4::ParserRuleContext {
  public:
    Tranform_list_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Group_nameContext *group_name();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Transform_group_listContext *transform_group_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();

   
  };

  Tranform_list_itemContext* tranform_list_item();

  class  Transform_group_listContext : public antlr4::ParserRuleContext {
  public:
    Transform_group_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Transform_group_list_itemContext *> transform_group_list_item();
    Transform_group_list_itemContext* transform_group_list_item(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Transform_group_listContext* transform_group_list();

  class  Transform_group_list_itemContext : public antlr4::ParserRuleContext {
  public:
    Transform_group_list_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SQL();
    antlr4::tree::TerminalNode *WITH();
    Function_designatorContext *function_designator();
    antlr4::tree::TerminalNode *TO();
    antlr4::tree::TerminalNode *FROM();

   
  };

  Transform_group_list_itemContext* transform_group_list_item();

  class  Create_trigger_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_trigger_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *TRIGGER();
    Trigger_nameContext *trigger_name();
    Trigger_eventContext *trigger_event();
    antlr4::tree::TerminalNode *ON();
    Table_or_view_nameContext *table_or_view_name();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *EACH();
    Triggered_actionContext *triggered_action();
    antlr4::tree::TerminalNode *ROW();
    antlr4::tree::TerminalNode *STATEMENT();
    antlr4::tree::TerminalNode *BEFORE();
    antlr4::tree::TerminalNode *AFTER();
    antlr4::tree::TerminalNode *INSTEAD();
    antlr4::tree::TerminalNode *OF();
    Or_replaceContext *or_replace();
    antlr4::tree::TerminalNode *REFERENCING();
    Ref_listContext *ref_list();
    antlr4::tree::TerminalNode *SECURED();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *CASCADE();
    antlr4::tree::TerminalNode *NOT();

   
  };

  Create_trigger_statementContext* create_trigger_statement();

  class  Ref_listContext : public antlr4::ParserRuleContext {
  public:
    Ref_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Ref_list_itemContext *> ref_list_item();
    Ref_list_itemContext* ref_list_item(size_t i);

   
  };

  Ref_listContext* ref_list();

  class  Ref_list_itemContext : public antlr4::ParserRuleContext {
  public:
    Ref_list_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Old_newContext *old_new();
    Correlation_nameContext *correlation_name();
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *TABLE();
    IdentifierContext *identifier();

   
  };

  Ref_list_itemContext* ref_list_item();

  class  Old_newContext : public antlr4::ParserRuleContext {
  public:
    Old_newContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OLD();
    antlr4::tree::TerminalNode *NEW();

   
  };

  Old_newContext* old_new();

  class  Correlation_nameContext : public antlr4::ParserRuleContext {
  public:
    Correlation_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Correlation_nameContext* correlation_name();

  class  IdentifierContext : public antlr4::ParserRuleContext {
  public:
    IdentifierContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  IdentifierContext* identifier();

  class  Trigger_eventContext : public antlr4::ParserRuleContext {
  public:
    Trigger_eventContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OR();
    antlr4::tree::TerminalNode *INSERT();
    antlr4::tree::TerminalNode *DELETE();
    antlr4::tree::TerminalNode *UPDATE();
    antlr4::tree::TerminalNode *OF();
    Column_name_listContext *column_name_list();

   
  };

  Trigger_eventContext* trigger_event();

  class  Triggered_actionContext : public antlr4::ParserRuleContext {
  public:
    Triggered_actionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Sql_procedure_statementContext *sql_procedure_statement();
    antlr4::tree::TerminalNode *WHEN();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Search_conditionContext *search_condition();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    LabelContext *label();

   
  };

  Triggered_actionContext* triggered_action();

  class  Sql_procedure_statementContext : public antlr4::ParserRuleContext {
  public:
    Sql_procedure_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Compound_body_statementContext *compound_body_statement();

   
  };

  Sql_procedure_statementContext* sql_procedure_statement();

  class  Sql_function_statementContext : public antlr4::ParserRuleContext {
  public:
    Sql_function_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CALL();
    antlr4::tree::TerminalNode *FOR();
    FullselectContext *fullselect();
    antlr4::tree::TerminalNode *WITH();
    Common_table_expression_listContext *common_table_expression_list();
    antlr4::tree::TerminalNode *GET();
    antlr4::tree::TerminalNode *DIAGNOSTICS();
    antlr4::tree::TerminalNode *IF();
    antlr4::tree::TerminalNode *INSERT();
    antlr4::tree::TerminalNode *ITERATE();
    antlr4::tree::TerminalNode *LEAVE();
    antlr4::tree::TerminalNode *MERGE();
    Searched_delete_statementContext *searched_delete_statement();
    Searched_update_statementContext *searched_update_statement();
    antlr4::tree::TerminalNode *SET();
    VariableContext *variable();
    antlr4::tree::TerminalNode *SIGNAL();
    antlr4::tree::TerminalNode *WHILE();

   
  };

  Sql_function_statementContext* sql_function_statement();

  class  Create_trusted_context_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_trusted_context_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *TRUSTED();
    antlr4::tree::TerminalNode *CONTEXT();
    Context_nameContext *context_name();
    antlr4::tree::TerminalNode *BASE();
    antlr4::tree::TerminalNode *UPON();
    antlr4::tree::TerminalNode *CONNECTION();
    antlr4::tree::TerminalNode *USING();
    antlr4::tree::TerminalNode *SYSTEM();
    antlr4::tree::TerminalNode *AUTHID();
    Authorization_nameContext *authorization_name();
    antlr4::tree::TerminalNode *ATTRIBUTES();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Attr_listContext *attr_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *USE();
    antlr4::tree::TerminalNode *FOR();
    Auth_listContext *auth_list();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *DEFAULT();
    antlr4::tree::TerminalNode *ROLE();
    Role_nameContext *role_name();
    Enable_disableContext *enable_disable();

   
  };

  Create_trusted_context_statementContext* create_trusted_context_statement();

  class  Attr_listContext : public antlr4::ParserRuleContext {
  public:
    Attr_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Attr_list_itemContext *> attr_list_item();
    Attr_list_itemContext* attr_list_item(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Attr_listContext* attr_list();

  class  Attr_list_itemContext : public antlr4::ParserRuleContext {
  public:
    Attr_list_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ADDRESS();
    Address_valueContext *address_value();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *ENCRYPTION();
    Encryption_valueContext *encryption_value();

   
  };

  Attr_list_itemContext* attr_list_item();

  class  Auth_listContext : public antlr4::ParserRuleContext {
  public:
    Auth_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Auth_list_itemContext *> auth_list_item();
    Auth_list_itemContext* auth_list_item(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Auth_listContext* auth_list();

  class  Auth_list_itemContext : public antlr4::ParserRuleContext {
  public:
    Auth_list_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Authorization_nameContext *authorization_name();
    antlr4::tree::TerminalNode *PUBLIC();
    With_withoutContext *with_without();
    antlr4::tree::TerminalNode *AUTHENTICATION();
    antlr4::tree::TerminalNode *ROLE();
    Role_nameContext *role_name();

   
  };

  Auth_list_itemContext* auth_list_item();

  class  Address_valueContext : public antlr4::ParserRuleContext {
  public:
    Address_valueContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    String_constantContext *string_constant();

   
  };

  Address_valueContext* address_value();

  class  Encryption_valueContext : public antlr4::ParserRuleContext {
  public:
    Encryption_valueContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    String_constantContext *string_constant();

   
  };

  Encryption_valueContext* encryption_value();

  class  Create_type_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_type_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Create_type_array_statementContext *create_type_array_statement();
    Create_type_cursor_statementContext *create_type_cursor_statement();
    Create_type_distinct_statementContext *create_type_distinct_statement();
    Create_type_row_statementContext *create_type_row_statement();
    Create_type_structured_statementContext *create_type_structured_statement();

   
  };

  Create_type_statementContext* create_type_statement();

  class  Create_type_array_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_type_array_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *TYPE();
    Type_nameContext *type_name();
    antlr4::tree::TerminalNode *AS();
    Data_typeContext *data_type();
    antlr4::tree::TerminalNode *ARRAY();
    antlr4::tree::TerminalNode *LEFT_SQR_BKT();
    antlr4::tree::TerminalNode *RIGHT_SQR_BKT();
    Or_replaceContext *or_replace();
    Integer_constantContext *integer_constant();
    Data_type_2Context *data_type_2();

   
  };

  Create_type_array_statementContext* create_type_array_statement();

  class  Create_type_cursor_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_type_cursor_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *TYPE();
    Type_nameContext *type_name();
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *CURSOR();
    Or_replaceContext *or_replace();
    Anchored_row_data_typeContext *anchored_row_data_type();
    Row_type_nameContext *row_type_name();

   
  };

  Create_type_cursor_statementContext* create_type_cursor_statement();

  class  Create_type_distinct_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_type_distinct_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    std::vector<antlr4::tree::TerminalNode *> TYPE();
    antlr4::tree::TerminalNode* TYPE(size_t i);
    Distinct_type_nameContext *distinct_type_name();
    antlr4::tree::TerminalNode *AS();
    Source_data_typeContext *source_data_type();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *STRONG();
    antlr4::tree::TerminalNode *RULES();
    antlr4::tree::TerminalNode *WEAK();
    Data_type_constrainstContext *data_type_constrainst();

   
  };

  Create_type_distinct_statementContext* create_type_distinct_statement();

  class  Create_type_row_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_type_row_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *TYPE();
    Type_nameContext *type_name();
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *ROW();
    Field_definition_list_parenContext *field_definition_list_paren();
    Anchored_row_data_typeContext *anchored_row_data_type();
    Or_replaceContext *or_replace();

   
  };

  Create_type_row_statementContext* create_type_row_statement();

  class  Field_definition_list_parenContext : public antlr4::ParserRuleContext {
  public:
    Field_definition_list_parenContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Field_definition_listContext *field_definition_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();

   
  };

  Field_definition_list_parenContext* field_definition_list_paren();

  class  Field_definition_listContext : public antlr4::ParserRuleContext {
  public:
    Field_definition_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Field_definitionContext *> field_definition();
    Field_definitionContext* field_definition(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Field_definition_listContext* field_definition_list();

  class  Field_definitionContext : public antlr4::ParserRuleContext {
  public:
    Field_definitionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Field_nameContext *field_name();
    Data_typeContext *data_type();

   
  };

  Field_definitionContext* field_definition();

  class  Create_type_structured_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_type_structured_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *TYPE();
    Type_nameContext *type_name();
    antlr4::tree::TerminalNode *UNDER();
    Supertype_nameContext *supertype_name();
    antlr4::tree::TerminalNode *AS();
    Attribute_definition_list_parenContext *attribute_definition_list_paren();
    std::vector<Structured_type_seqContext *> structured_type_seq();
    Structured_type_seqContext* structured_type_seq(size_t i);
    Method_specification_listContext *method_specification_list();

   
  };

  Create_type_structured_statementContext* create_type_structured_statement();

  class  Structured_type_seqContext : public antlr4::ParserRuleContext {
  public:
    Structured_type_seqContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INSTANTIABLE();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *CAST();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *SOURCE();
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *REF();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *WITH();
    Function_nameContext *function_name();
    antlr4::tree::TerminalNode *INLINE();
    antlr4::tree::TerminalNode *LENGTH();
    Integer_valueContext *integer_value();
    antlr4::tree::TerminalNode *WITHOUT();
    antlr4::tree::TerminalNode *COMPARISONS();
    antlr4::tree::TerminalNode *FINAL();
    antlr4::tree::TerminalNode *MODE();
    antlr4::tree::TerminalNode *DB2SQL();
    antlr4::tree::TerminalNode *FUNCTION();
    antlr4::tree::TerminalNode *ACCESS();
    antlr4::tree::TerminalNode *USING();
    Rep_typeContext *rep_type();

   
  };

  Structured_type_seqContext* structured_type_seq();

  class  Attribute_definition_list_parenContext : public antlr4::ParserRuleContext {
  public:
    Attribute_definition_list_parenContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Attribute_definition_listContext *attribute_definition_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();

   
  };

  Attribute_definition_list_parenContext* attribute_definition_list_paren();

  class  Attribute_definition_listContext : public antlr4::ParserRuleContext {
  public:
    Attribute_definition_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Attribute_definitionContext *> attribute_definition();
    Attribute_definitionContext* attribute_definition(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Attribute_definition_listContext* attribute_definition_list();

  class  Attribute_definitionContext : public antlr4::ParserRuleContext {
  public:
    Attribute_definitionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Attribute_nameContext *attribute_name();
    Data_typeContext *data_type();
    Lob_optionsContext *lob_options();

   
  };

  Attribute_definitionContext* attribute_definition();

  class  Method_specification_listContext : public antlr4::ParserRuleContext {
  public:
    Method_specification_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Method_specificationContext *> method_specification();
    Method_specificationContext* method_specification(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Method_specification_listContext* method_specification_list();

  class  Method_specificationContext : public antlr4::ParserRuleContext {
  public:
    Method_specificationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *METHOD();
    Method_nameContext *method_name();
    Method_spec_parameter_list_parenContext *method_spec_parameter_list_paren();
    Method_specification_return_clauseContext *method_specification_return_clause();
    antlr4::tree::TerminalNode *OVERRIDING();
    std::vector<Method_specification_seqContext *> method_specification_seq();
    Method_specification_seqContext* method_specification_seq(size_t i);

   
  };

  Method_specificationContext* method_specification();

  class  Method_specification_return_clauseContext : public antlr4::ParserRuleContext {
  public:
    Method_specification_return_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RETURNS();
    Method_ordinary_return_typeContext *method_ordinary_return_type();
    Method_cast_target_typeContext *method_cast_target_type();
    antlr4::tree::TerminalNode *CAST();
    antlr4::tree::TerminalNode *FROM();
    Method_cast_source_typeContext *method_cast_source_type();
    As_locatorContext *as_locator();

   
  };

  Method_specification_return_clauseContext* method_specification_return_clause();

  class  Method_specification_seqContext : public antlr4::ParserRuleContext {
  public:
    Method_specification_seqContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SPECIFIC();
    Specific_nameContext *specific_name();
    antlr4::tree::TerminalNode *SELF();
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *RESULT();
    Sql_routine_characteristicsContext *sql_routine_characteristics();
    External_routine_characteristicsContext *external_routine_characteristics();

   
  };

  Method_specification_seqContext* method_specification_seq();

  class  As_locatorContext : public antlr4::ParserRuleContext {
  public:
    As_locatorContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *LOCATOR();

   
  };

  As_locatorContext* as_locator();

  class  Method_spec_parameter_list_parenContext : public antlr4::ParserRuleContext {
  public:
    Method_spec_parameter_list_parenContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    Method_spec_parameter_listContext *method_spec_parameter_list();

   
  };

  Method_spec_parameter_list_parenContext* method_spec_parameter_list_paren();

  class  Method_spec_parameter_listContext : public antlr4::ParserRuleContext {
  public:
    Method_spec_parameter_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Method_spec_parameter_declContext *> method_spec_parameter_decl();
    Method_spec_parameter_declContext* method_spec_parameter_decl(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Method_spec_parameter_listContext* method_spec_parameter_list();

  class  Method_spec_parameter_declContext : public antlr4::ParserRuleContext {
  public:
    Method_spec_parameter_declContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Method_spec_parameter_typeContext *method_spec_parameter_type();
    Parameter_nameContext *parameter_name();
    As_locatorContext *as_locator();

   
  };

  Method_spec_parameter_declContext* method_spec_parameter_decl();

  class  Method_spec_parameter_typeContext : public antlr4::ParserRuleContext {
  public:
    Method_spec_parameter_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Method_non_xml_builtin_typeContext *method_non_xml_builtin_type();
    Method_named_typeContext *method_named_type();
    antlr4::tree::TerminalNode *REF();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();

   
  };

  Method_spec_parameter_typeContext* method_spec_parameter_type();

  class  Param_decl_list_parenContext : public antlr4::ParserRuleContext {
  public:
    Param_decl_list_parenContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Param_decl_listContext *param_decl_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();

   
  };

  Param_decl_list_parenContext* param_decl_list_paren();

  class  Param_decl_listContext : public antlr4::ParserRuleContext {
  public:
    Param_decl_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Param_declContext *> param_decl();
    Param_declContext* param_decl(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Param_decl_listContext* param_decl_list();

  class  Param_declContext : public antlr4::ParserRuleContext {
  public:
    Param_declContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Data_type_2Context *data_type_2();
    Parameter_nameContext *parameter_name();
    As_locatorContext *as_locator();

   
  };

  Param_declContext* param_decl();

  class  Sql_routine_characteristicsContext : public antlr4::ParserRuleContext {
  public:
    Sql_routine_characteristicsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LANGUAGE();
    antlr4::tree::TerminalNode *SQL();
    antlr4::tree::TerminalNode *PARAMETER();
    antlr4::tree::TerminalNode *CCSID();
    antlr4::tree::TerminalNode *ASCII();
    antlr4::tree::TerminalNode *UNICODE();
    antlr4::tree::TerminalNode *DETERMINISTIC();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *EXTERNAL();
    antlr4::tree::TerminalNode *ACTION();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *READS();
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *CONTAINS();
    antlr4::tree::TerminalNode *CALLED();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *NULL_();
    antlr4::tree::TerminalNode *INPUT();
    antlr4::tree::TerminalNode *INHERIT();
    antlr4::tree::TerminalNode *SPECIAL();
    antlr4::tree::TerminalNode *REGISTERS();

   
  };

  Sql_routine_characteristicsContext* sql_routine_characteristics();

  class  External_routine_characteristicsContext : public antlr4::ParserRuleContext {
  public:
    External_routine_characteristicsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LANGUAGE();
    antlr4::tree::TerminalNode *C_();
    antlr4::tree::TerminalNode *JAVA();
    antlr4::tree::TerminalNode *OLE();
    antlr4::tree::TerminalNode *PARAMETER();
    antlr4::tree::TerminalNode *STYLE();
    antlr4::tree::TerminalNode *DB2GENERAL();
    antlr4::tree::TerminalNode *SQL();
    antlr4::tree::TerminalNode *CCSID();
    antlr4::tree::TerminalNode *ASCII();
    antlr4::tree::TerminalNode *UNICODE();
    antlr4::tree::TerminalNode *DETERMINISTIC();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *FENCED();
    antlr4::tree::TerminalNode *THREADSAFE();
    antlr4::tree::TerminalNode *CALLED();
    antlr4::tree::TerminalNode *ON();
    std::vector<antlr4::tree::TerminalNode *> NULL_();
    antlr4::tree::TerminalNode* NULL_(size_t i);
    antlr4::tree::TerminalNode *INPUT();
    antlr4::tree::TerminalNode *RETURNS();
    antlr4::tree::TerminalNode *READS();
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *NO();
    antlr4::tree::TerminalNode *CONTAINS();
    antlr4::tree::TerminalNode *EXTERNAL();
    antlr4::tree::TerminalNode *ACTION();
    antlr4::tree::TerminalNode *SCRATCHPAD();
    LengthContext *length();
    antlr4::tree::TerminalNode *FINAL();
    antlr4::tree::TerminalNode *CALL();
    antlr4::tree::TerminalNode *PARALLEL();
    antlr4::tree::TerminalNode *ALLOW();
    antlr4::tree::TerminalNode *DISALLOW();
    antlr4::tree::TerminalNode *DBINFO();
    antlr4::tree::TerminalNode *INHERIT();
    antlr4::tree::TerminalNode *SPECIAL();
    antlr4::tree::TerminalNode *REGISTERS();

   
  };

  External_routine_characteristicsContext* external_routine_characteristics();

  class  LengthContext : public antlr4::ParserRuleContext {
  public:
    LengthContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Integer_valueContext *integer_value();

   
  };

  LengthContext* length();

  class  Rep_typeContext : public antlr4::ParserRuleContext {
  public:
    Rep_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SMALLINT();
    antlr4::tree::TerminalNode *INTEGER();
    antlr4::tree::TerminalNode *INT();
    antlr4::tree::TerminalNode *BIGINT();
    antlr4::tree::TerminalNode *DECIMAL();
    antlr4::tree::TerminalNode *DEC();
    antlr4::tree::TerminalNode *NUMERIC();
    antlr4::tree::TerminalNode *NUM();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<Integer_valueContext *> integer_value();
    Integer_valueContext* integer_value(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *COMMA();
    antlr4::tree::TerminalNode *DECFLOAT();
    Char_characterContext *char_character();
    Integer_parenContext *integer_paren();
    For_bit_dataContext *for_bit_data();
    VarcharsContext *varchars();
    antlr4::tree::TerminalNode *BINARY();
    VarbinariesContext *varbinaries();
    antlr4::tree::TerminalNode *GRAPHIC();
    antlr4::tree::TerminalNode *VARGRAPHIC();

   
  };

  Rep_typeContext* rep_type();

  class  VarcharsContext : public antlr4::ParserRuleContext {
  public:
    VarcharsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *VARCHAR();
    Char_characterContext *char_character();
    antlr4::tree::TerminalNode *VARYING();

   
  };

  VarcharsContext* varchars();

  class  VarbinariesContext : public antlr4::ParserRuleContext {
  public:
    VarbinariesContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *VARBINARY();
    antlr4::tree::TerminalNode *BINARY();
    antlr4::tree::TerminalNode *VARYING();

   
  };

  VarbinariesContext* varbinaries();

  class  For_bit_dataContext : public antlr4::ParserRuleContext {
  public:
    For_bit_dataContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *BIT();
    antlr4::tree::TerminalNode *DATA();

   
  };

  For_bit_dataContext* for_bit_data();

  class  Lob_optionsContext : public antlr4::ParserRuleContext {
  public:
    Lob_optionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LOGGED();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *COMPACT();

   
  };

  Lob_optionsContext* lob_options();

  class  Create_type_mapping_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_type_mapping_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    std::vector<antlr4::tree::TerminalNode *> TYPE();
    antlr4::tree::TerminalNode* TYPE(size_t i);
    antlr4::tree::TerminalNode *MAPPING();
    std::vector<From_toContext *> from_to();
    From_toContext* from_to(size_t i);
    Local_data_typeContext *local_data_type();
    Remote_serverContext *remote_server();
    Data_source_data_typeContext *data_source_data_type();
    For_bit_data_precisionContext *for_bit_data_precision();
    Type_mapping_nameContext *type_mapping_name();
    antlr4::tree::TerminalNode *LOCAL();
    antlr4::tree::TerminalNode *REMOTE();

   
  };

  Create_type_mapping_statementContext* create_type_mapping_statement();

  class  For_bit_data_precisionContext : public antlr4::ParserRuleContext {
  public:
    For_bit_data_precisionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    For_bit_dataContext *for_bit_data();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    std::vector<PrecisionContext *> precision();
    PrecisionContext* precision(size_t i);
    std::vector<antlr4::tree::TerminalNode *> DOT_DOT();
    antlr4::tree::TerminalNode* DOT_DOT(size_t i);
    antlr4::tree::TerminalNode *COMMA();
    Precision_scale_compContext *precision_scale_comp();
    std::vector<ScaleContext *> scale();
    ScaleContext* scale(size_t i);

   
  };

  For_bit_data_precisionContext* for_bit_data_precision();

  class  PrecisionContext : public antlr4::ParserRuleContext {
  public:
    PrecisionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Integer_valueContext *integer_value();

   
  };

  PrecisionContext* precision();

  class  ScaleContext : public antlr4::ParserRuleContext {
  public:
    ScaleContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Integer_valueContext *integer_value();

   
  };

  ScaleContext* scale();

  class  Precision_scale_compContext : public antlr4::ParserRuleContext {
  public:
    Precision_scale_compContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *P_();
    antlr4::tree::TerminalNode *S_();
    antlr4::tree::TerminalNode *EQ();
    antlr4::tree::TerminalNode *GT();
    antlr4::tree::TerminalNode *LT();
    antlr4::tree::TerminalNode *GE();
    antlr4::tree::TerminalNode *LE();
    antlr4::tree::TerminalNode *LTGT();

   
  };

  Precision_scale_compContext* precision_scale_comp();

  class  From_toContext : public antlr4::ParserRuleContext {
  public:
    From_toContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FROM();
    antlr4::tree::TerminalNode *TO();

   
  };

  From_toContext* from_to();

  class  Data_source_data_typeContext : public antlr4::ParserRuleContext {
  public:
    Data_source_data_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TodoContext *todo();

   
  };

  Data_source_data_typeContext* data_source_data_type();

  class  Local_data_typeContext : public antlr4::ParserRuleContext {
  public:
    Local_data_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Built_in_typeContext *built_in_type();
    Type_nameContext *type_name();

   
  };

  Local_data_typeContext* local_data_type();

  class  Remote_serverContext : public antlr4::ParserRuleContext {
  public:
    Remote_serverContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SERVER();
    Server_nameContext *server_name();
    antlr4::tree::TerminalNode *TYPE();
    Server_typeContext *server_type();
    antlr4::tree::TerminalNode *VERSION();
    Server_versionContext *server_version();
    antlr4::tree::TerminalNode *WRAPPER();
    Wrapper_nameContext *wrapper_name();

   
  };

  Remote_serverContext* remote_server();

  class  Server_versionContext : public antlr4::ParserRuleContext {
  public:
    Server_versionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    VersionContext *version();
    std::vector<antlr4::tree::TerminalNode *> DOT();
    antlr4::tree::TerminalNode* DOT(size_t i);
    ReleaseContext *release();
    ModContext *mod();
    String_constantContext *string_constant();

   
  };

  Server_versionContext* server_version();

  class  Server_typeContext : public antlr4::ParserRuleContext {
  public:
    Server_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Server_typeContext* server_type();

  class  VersionContext : public antlr4::ParserRuleContext {
  public:
    VersionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Integer_valueContext *integer_value();

   
  };

  VersionContext* version();

  class  ReleaseContext : public antlr4::ParserRuleContext {
  public:
    ReleaseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Integer_valueContext *integer_value();

   
  };

  ReleaseContext* release();

  class  ModContext : public antlr4::ParserRuleContext {
  public:
    ModContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Integer_valueContext *integer_value();

   
  };

  ModContext* mod();

  class  Create_usage_list_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_usage_list_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *USAGE();
    std::vector<antlr4::tree::TerminalNode *> LIST();
    antlr4::tree::TerminalNode* LIST(size_t i);
    Usage_list_nameContext *usage_list_name();
    antlr4::tree::TerminalNode *FOR();
    Object_nameContext *object_name();
    antlr4::tree::TerminalNode *TABLE();
    antlr4::tree::TerminalNode *INDEX();
    antlr4::tree::TerminalNode *SIZE();
    Integer_valueContext *integer_value();
    antlr4::tree::TerminalNode *WHEN();
    antlr4::tree::TerminalNode *FULL();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *START();
    antlr4::tree::TerminalNode *DATABASE();
    antlr4::tree::TerminalNode *WRAP();
    antlr4::tree::TerminalNode *DEACTIVATE();
    antlr4::tree::TerminalNode *INACTIVE();
    antlr4::tree::TerminalNode *ACTIVE();

   
  };

  Create_usage_list_statementContext* create_usage_list_statement();

  class  Create_user_mapping_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_user_mapping_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    std::vector<antlr4::tree::TerminalNode *> USER();
    antlr4::tree::TerminalNode* USER(size_t i);
    antlr4::tree::TerminalNode *MAPPING();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *SERVER();
    Server_nameContext *server_name();
    antlr4::tree::TerminalNode *OPTIONS();
    User_mapping_options_parenContext *user_mapping_options_paren();
    Authorization_nameContext *authorization_name();
    antlr4::tree::TerminalNode *PUBLIC();

   
  };

  Create_user_mapping_statementContext* create_user_mapping_statement();

  class  User_mapping_options_parenContext : public antlr4::ParserRuleContext {
  public:
    User_mapping_options_parenContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<User_mapping_optionsContext *> user_mapping_options();
    User_mapping_optionsContext* user_mapping_options(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  User_mapping_options_parenContext* user_mapping_options_paren();

  class  User_mapping_optionsContext : public antlr4::ParserRuleContext {
  public:
    User_mapping_optionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    User_mapping_option_nameContext *user_mapping_option_name();
    StringContext *string();

   
  };

  User_mapping_optionsContext* user_mapping_options();

  class  Create_variable_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_variable_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *VARIABLE();
    Data_type_1Context *data_type_1();
    Variable_nameContext *variable_name();
    Schema_qualified_variable_declaration_nameContext *schema_qualified_variable_declaration_name();
    Or_replaceContext *or_replace();
    antlr4::tree::TerminalNode *NULL_();
    antlr4::tree::TerminalNode *DEFAULT();
    antlr4::tree::TerminalNode *CONSTANT();
    Constant_Context *constant_();
    Special_registerContext *special_register();
    Global_variableContext *global_variable();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Cursor_value_constructorContext *cursor_value_constructor();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    ExpressionContext *expression();

   
  };

  Create_variable_statementContext* create_variable_statement();

  class  Constant_Context : public antlr4::ParserRuleContext {
  public:
    Constant_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Integer_constantContext *integer_constant();
    Bigint_constantContext *bigint_constant();
    antlr4::tree::TerminalNode *FLOAT_LITERAL();
    antlr4::tree::TerminalNode *REAL_LITERAL();
    StringContext *string();
    antlr4::tree::TerminalNode *NULL_();
    antlr4::tree::TerminalNode *L_ONE();
    antlr4::tree::TerminalNode *L_ZERO();
    Signed_numeric_constantContext *signed_numeric_constant();
    Decfloat_special_constantContext *decfloat_special_constant();
    Signed_decfloat_special_constantContext *signed_decfloat_special_constant();
    Boolean_constantContext *boolean_constant();
    Typed_datetime_constantContext *typed_datetime_constant();
    Prefixed_string_constantContext *prefixed_string_constant();
    Unicode_string_constantContext *unicode_string_constant();

   
  };

  Constant_Context* constant_();

  class  Signed_numeric_constantContext : public antlr4::ParserRuleContext {
  public:
    Signed_numeric_constantContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PLUS();
    antlr4::tree::TerminalNode *MINUS();
    Integer_constantContext *integer_constant();
    Bigint_constantContext *bigint_constant();
    antlr4::tree::TerminalNode *FLOAT_LITERAL();
    antlr4::tree::TerminalNode *REAL_LITERAL();
    antlr4::tree::TerminalNode *L_ONE();
    antlr4::tree::TerminalNode *L_ZERO();

   
  };

  Signed_numeric_constantContext* signed_numeric_constant();

  class  Decfloat_special_constantContext : public antlr4::ParserRuleContext {
  public:
    Decfloat_special_constantContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *INF();
    antlr4::tree::TerminalNode *INFINITY();
    antlr4::tree::TerminalNode *NAN();
    antlr4::tree::TerminalNode *SNAN();

   
  };

  Decfloat_special_constantContext* decfloat_special_constant();

  class  Signed_decfloat_special_constantContext : public antlr4::ParserRuleContext {
  public:
    Signed_decfloat_special_constantContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Decfloat_special_constantContext *decfloat_special_constant();
    antlr4::tree::TerminalNode *PLUS();
    antlr4::tree::TerminalNode *MINUS();

   
  };

  Signed_decfloat_special_constantContext* signed_decfloat_special_constant();

  class  Boolean_constantContext : public antlr4::ParserRuleContext {
  public:
    Boolean_constantContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TRUE();
    antlr4::tree::TerminalNode *FALSE();

   
  };

  Boolean_constantContext* boolean_constant();

  class  Typed_datetime_constantContext : public antlr4::ParserRuleContext {
  public:
    Typed_datetime_constantContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    StringContext *string();
    antlr4::tree::TerminalNode *DATE();
    antlr4::tree::TerminalNode *TIME();
    antlr4::tree::TerminalNode *TIMESTAMP();

   
  };

  Typed_datetime_constantContext* typed_datetime_constant();

  class  Unicode_string_constantContext : public antlr4::ParserRuleContext {
  public:
    Unicode_string_constantContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *UNICODE_STRING_LITERAL();
    Unicode_escape_clauseContext *unicode_escape_clause();

   
  };

  Unicode_string_constantContext* unicode_string_constant();

  class  Unicode_escape_clauseContext : public antlr4::ParserRuleContext {
  public:
    Unicode_escape_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *UESCAPE();
    antlr4::tree::TerminalNode *STRING_LITERAL();

   
  };

  Unicode_escape_clauseContext* unicode_escape_clause();

  class  Prefixed_string_constantContext : public antlr4::ParserRuleContext {
  public:
    Prefixed_string_constantContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Hex_character_string_constantContext *hex_character_string_constant();
    Graphic_string_constantContext *graphic_string_constant();
    Hex_graphic_string_constantContext *hex_graphic_string_constant();
    Binary_string_constantContext *binary_string_constant();
    Utf16be_graphic_string_constantContext *utf16be_graphic_string_constant();

   
  };

  Prefixed_string_constantContext* prefixed_string_constant();

  class  Hex_character_string_constantContext : public antlr4::ParserRuleContext {
  public:
    Hex_character_string_constantContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *HEX_CHARACTER_LITERAL();

   
  };

  Hex_character_string_constantContext* hex_character_string_constant();

  class  Graphic_string_constantContext : public antlr4::ParserRuleContext {
  public:
    Graphic_string_constantContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GRAPHIC_G_LITERAL();
    antlr4::tree::TerminalNode *GRAPHIC_N_LITERAL();

   
  };

  Graphic_string_constantContext* graphic_string_constant();

  class  Hex_graphic_string_constantContext : public antlr4::ParserRuleContext {
  public:
    Hex_graphic_string_constantContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *HEX_GRAPHIC_LITERAL();

   
  };

  Hex_graphic_string_constantContext* hex_graphic_string_constant();

  class  Binary_string_constantContext : public antlr4::ParserRuleContext {
  public:
    Binary_string_constantContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BINARY_STRING_LITERAL();

   
  };

  Binary_string_constantContext* binary_string_constant();

  class  Utf16be_graphic_string_constantContext : public antlr4::ParserRuleContext {
  public:
    Utf16be_graphic_string_constantContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *UTF16BE_GRAPHIC_LITERAL();

   
  };

  Utf16be_graphic_string_constantContext* utf16be_graphic_string_constant();

  class  Special_registerContext : public antlr4::ParserRuleContext {
  public:
    Special_registerContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();
    Native_special_registerContext *native_special_register();

   
  };

  Special_registerContext* special_register();

  class  Native_special_registerContext : public antlr4::ParserRuleContext {
  public:
    Native_special_registerContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CURRENT();
    antlr4::tree::TerminalNode *CLIENT_ACCTNG();
    antlr4::tree::TerminalNode *CLIENT_APPLNAME();
    antlr4::tree::TerminalNode *CLIENT_USERID();
    antlr4::tree::TerminalNode *CLIENT_WRKSTNNAME();
    antlr4::tree::TerminalNode *DATE();
    antlr4::tree::TerminalNode *DBPARTITIONNUM();
    antlr4::tree::TerminalNode *DECFLOAT();
    antlr4::tree::TerminalNode *ROUNDING();
    antlr4::tree::TerminalNode *MODE();
    antlr4::tree::TerminalNode *DEFAULT();
    antlr4::tree::TerminalNode *TRANSFORM();
    antlr4::tree::TerminalNode *GROUP();
    antlr4::tree::TerminalNode *DEGREE_();
    antlr4::tree::TerminalNode *EXPLAIN();
    antlr4::tree::TerminalNode *SNAPSHOT();
    antlr4::tree::TerminalNode *FEDERATED();
    antlr4::tree::TerminalNode *ASYNCHRONY();
    antlr4::tree::TerminalNode *IMPLICIT();
    antlr4::tree::TerminalNode *XMLPARSE();
    antlr4::tree::TerminalNode *OPTION();
    antlr4::tree::TerminalNode *ISOLATION();
    antlr4::tree::TerminalNode *LOCALE();
    antlr4::tree::TerminalNode *LOCK();
    antlr4::tree::TerminalNode *TIMEOUT();
    antlr4::tree::TerminalNode *MAINTAINED();
    antlr4::tree::TerminalNode *TABLE();
    antlr4::tree::TerminalNode *TYPES();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *OPTIMIZATION();
    antlr4::tree::TerminalNode *MDC();
    antlr4::tree::TerminalNode *ROLLOUT();
    antlr4::tree::TerminalNode *MEMBER();
    antlr4::tree::TerminalNode *PROFILE();
    antlr4::tree::TerminalNode *PACKAGE();
    antlr4::tree::TerminalNode *PATH();
    antlr4::tree::TerminalNode *QUERY();
    antlr4::tree::TerminalNode *REFRESH();
    antlr4::tree::TerminalNode *AGE();
    antlr4::tree::TerminalNode *SCHEMA();
    antlr4::tree::TerminalNode *SERVER();
    antlr4::tree::TerminalNode *SQL_CCFLAGS();
    antlr4::tree::TerminalNode *SQLID();
    antlr4::tree::TerminalNode *TEMPORAL();
    antlr4::tree::TerminalNode *TIME();
    antlr4::tree::TerminalNode *TIMESTAMP();
    antlr4::tree::TerminalNode *TIMEZONE();
    antlr4::tree::TerminalNode *USER();
    antlr4::tree::TerminalNode *LC_MESSAGES_();
    antlr4::tree::TerminalNode *LC_TIME_();
    antlr4::tree::TerminalNode *BUSINESS_TIME();
    antlr4::tree::TerminalNode *SYSTEM_TIME();
    antlr4::tree::TerminalNode *FUNCTION();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    Integer_valueContext *integer_value();
    antlr4::tree::TerminalNode *CLIENT();
    antlr4::tree::TerminalNode *ACCTNG();
    antlr4::tree::TerminalNode *APPLNAME();
    antlr4::tree::TerminalNode *USERID();
    antlr4::tree::TerminalNode *WRKSTNNAME();
    antlr4::tree::TerminalNode *SESSION_USER();
    antlr4::tree::TerminalNode *SYSTEM_USER();
    antlr4::tree::TerminalNode *CURRENT_DATE();
    antlr4::tree::TerminalNode *CURRENT_TIME();
    antlr4::tree::TerminalNode *CURRENT_TIMESTAMP();
    antlr4::tree::TerminalNode *CURRENT_USER();
    antlr4::tree::TerminalNode *CURRENT_SCHEMA();
    antlr4::tree::TerminalNode *CURRENT_SQLID();
    antlr4::tree::TerminalNode *CURRENT_PATH();
    antlr4::tree::TerminalNode *CURRENT_SERVER();
    antlr4::tree::TerminalNode *CURRENT_TIMEZONE();
    antlr4::tree::TerminalNode *SYSDATE();
    antlr4::tree::TerminalNode *LOCALTIMESTAMP();
    Timestamp_register_invocationContext *timestamp_register_invocation();

   
  };

  Native_special_registerContext* native_special_register();

  class  Timestamp_register_invocationContext : public antlr4::ParserRuleContext {
  public:
    Timestamp_register_invocationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *CURRENT_TIMESTAMP();
    antlr4::tree::TerminalNode *LOCALTIMESTAMP();
    Integer_valueContext *integer_value();

   
  };

  Timestamp_register_invocationContext* timestamp_register_invocation();

  class  Schema_qualified_variable_declaration_nameContext : public antlr4::ParserRuleContext {
  public:
    Schema_qualified_variable_declaration_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Schema_nameContext *schema_name();
    antlr4::tree::TerminalNode *DOT();
    Variable_nameContext *variable_name();

   
  };

  Schema_qualified_variable_declaration_nameContext* schema_qualified_variable_declaration_name();

  class  Global_variableContext : public antlr4::ParserRuleContext {
  public:
    Global_variableContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();
    Qualified_global_variable_referenceContext *qualified_global_variable_reference();

   
  };

  Global_variableContext* global_variable();

  class  Qualified_global_variable_referenceContext : public antlr4::ParserRuleContext {
  public:
    Qualified_global_variable_referenceContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Id_Context *> id_();
    Id_Context* id_(size_t i);
    std::vector<antlr4::tree::TerminalNode *> DOT();
    antlr4::tree::TerminalNode* DOT(size_t i);

   
  };

  Qualified_global_variable_referenceContext* qualified_global_variable_reference();

  class  Data_type_1Context : public antlr4::ParserRuleContext {
  public:
    Data_type_1Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Built_in_typeContext *built_in_type();
    Anchored_variable_data_typeContext *anchored_variable_data_type();
    Array_type_nameContext *array_type_name();
    Cursor_type_nameContext *cursor_type_name();
    Distinct_type_nameContext *distinct_type_name();
    antlr4::tree::TerminalNode *REF();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Type_nameContext *type_name();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    Row_type_nameContext *row_type_name();

   
  };

  Data_type_1Context* data_type_1();

  class  Cursor_value_constructorContext : public antlr4::ParserRuleContext {
  public:
    Cursor_value_constructorContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CURSOR();
    antlr4::tree::TerminalNode *FOR();
    Select_statementContext *select_statement();
    Param_decl_list_parenContext *param_decl_list_paren();
    HoldabilityContext *holdability();
    antlr4::tree::TerminalNode *ASENSITIVE();
    antlr4::tree::TerminalNode *INSENSITIVE();

   
  };

  Cursor_value_constructorContext* cursor_value_constructor();

  class  Anchored_variable_data_typeContext : public antlr4::ParserRuleContext {
  public:
    Anchored_variable_data_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ANCHOR();
    Variable_nameContext *variable_name();
    Table_nameContext *table_name();
    std::vector<antlr4::tree::TerminalNode *> DOT();
    antlr4::tree::TerminalNode* DOT(size_t i);
    Column_nameContext *column_name();
    Qualified_global_variable_referenceContext *qualified_global_variable_reference();
    antlr4::tree::TerminalNode *ROW();
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *TYPE();
    antlr4::tree::TerminalNode *TO();
    Table_or_view_nameContext *table_or_view_name();
    Cursor_variable_nameContext *cursor_variable_name();
    Schema_nameContext *schema_name();
    antlr4::tree::TerminalNode *OF();

   
  };

  Anchored_variable_data_typeContext* anchored_variable_data_type();

  class  HoldabilityContext : public antlr4::ParserRuleContext {
  public:
    HoldabilityContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *HOLD();
    antlr4::tree::TerminalNode *WITHOUT();
    antlr4::tree::TerminalNode *WITH();

   
  };

  HoldabilityContext* holdability();

  class  ReturnabilityContext : public antlr4::ParserRuleContext {
  public:
    ReturnabilityContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WITHOUT();
    antlr4::tree::TerminalNode *RETURN();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *TO();
    antlr4::tree::TerminalNode *CALLER();
    antlr4::tree::TerminalNode *CLIENT();

   
  };

  ReturnabilityContext* returnability();

  class  Create_view_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_view_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *VIEW();
    View_nameContext *view_name();
    antlr4::tree::TerminalNode *AS();
    FullselectContext *fullselect();
    Or_replaceContext *or_replace();
    Column_name_list_parenContext *column_name_list_paren();
    antlr4::tree::TerminalNode *OF();
    Type_nameContext *type_name();
    antlr4::tree::TerminalNode *WITH();
    Common_table_expression_listContext *common_table_expression_list();
    std::vector<Create_view_seqContext *> create_view_seq();
    Create_view_seqContext* create_view_seq(size_t i);
    Root_view_definitionContext *root_view_definition();
    Subview_definitionContext *subview_definition();

   
  };

  Create_view_statementContext* create_view_statement();

  class  Create_view_seqContext : public antlr4::ParserRuleContext {
  public:
    Create_view_seqContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *CHECK();
    antlr4::tree::TerminalNode *OPTION();
    antlr4::tree::TerminalNode *CASCADED();
    antlr4::tree::TerminalNode *LOCAL();
    antlr4::tree::TerminalNode *ROW();
    antlr4::tree::TerminalNode *MOVEMENT();
    antlr4::tree::TerminalNode *NO();

   
  };

  Create_view_seqContext* create_view_seq();

  class  FullselectContext : public antlr4::ParserRuleContext {
  public:
    FullselectContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<SubselectContext *> subselect();
    SubselectContext* subselect(size_t i);
    std::vector<Fullselect_in_parenthesesContext *> fullselect_in_parentheses();
    Fullselect_in_parenthesesContext* fullselect_in_parentheses(size_t i);
    std::vector<Values_clauseContext *> values_clause();
    Values_clauseContext* values_clause(size_t i);
    Order_by_clauseContext *order_by_clause();
    Offset_clauseContext *offset_clause();
    Fetch_clauseContext *fetch_clause();
    Isolation_clauseContext *isolation_clause();
    std::vector<antlr4::tree::TerminalNode *> UNION();
    antlr4::tree::TerminalNode* UNION(size_t i);
    std::vector<antlr4::tree::TerminalNode *> EXCEPT();
    antlr4::tree::TerminalNode* EXCEPT(size_t i);
    std::vector<antlr4::tree::TerminalNode *> INTERSECT();
    antlr4::tree::TerminalNode* INTERSECT(size_t i);
    std::vector<antlr4::tree::TerminalNode *> ALL();
    antlr4::tree::TerminalNode* ALL(size_t i);

   
  };

  FullselectContext* fullselect();

  class  SubselectContext : public antlr4::ParserRuleContext {
  public:
    SubselectContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Select_clauseContext *select_clause();
    From_clauseContext *from_clause();
    Where_clauseContext *where_clause();
    Group_by_clauseContext *group_by_clause();
    Having_clauseContext *having_clause();
    Order_by_clauseContext *order_by_clause();
    Offset_clauseContext *offset_clause();
    Fetch_clauseContext *fetch_clause();
    Isolation_clauseContext *isolation_clause();

   
  };

  SubselectContext* subselect();

  class  Select_clauseContext : public antlr4::ParserRuleContext {
  public:
    Select_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SELECT();
    std::vector<Select_clause_itemContext *> select_clause_item();
    Select_clause_itemContext* select_clause_item(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *DISTINCT();

   
  };

  Select_clauseContext* select_clause();

  class  Select_clause_itemContext : public antlr4::ParserRuleContext {
  public:
    Select_clause_itemContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *STAR();
    ExpressionContext *expression();
    Column_nameContext *column_name();
    antlr4::tree::TerminalNode *AS();
    Exposed_nameContext *exposed_name();
    antlr4::tree::TerminalNode *DOT();

   
  };

  Select_clause_itemContext* select_clause_item();

  class  From_clauseContext : public antlr4::ParserRuleContext {
  public:
    From_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FROM();
    Table_reference_listContext *table_reference_list();

   
  };

  From_clauseContext* from_clause();

  class  Table_referenceContext : public antlr4::ParserRuleContext {
  public:
    Table_referenceContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Singles_table_referenceContext *singles_table_reference();
    Single_view_referenceContext *single_view_reference();
    Single_nickname_referenceContext *single_nickname_reference();
    Only_table_referenceContext *only_table_reference();
    Outer_table_referenceContext *outer_table_reference();
    Analyze_table_referenceContext *analyze_table_reference();
    Nested_table_referenceContext *nested_table_reference();
    Data_change_table_referenceContext *data_change_table_reference();
    Table_function_referenceContext *table_function_reference();
    Collection_derived_tableContext *collection_derived_table();
    Xmltable_expressionContext *xmltable_expression();
    External_table_referenceContext *external_table_reference();

   
  };

  Table_referenceContext* table_reference();

  class  Table_reference_listContext : public antlr4::ParserRuleContext {
  public:
    Table_reference_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Table_referenceContext *> table_reference();
    Table_referenceContext* table_reference(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Table_reference_listContext* table_reference_list();

  class  Singles_table_referenceContext : public antlr4::ParserRuleContext {
  public:
    Singles_table_referenceContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Table_reference_nameContext *table_reference_name();
    std::vector<Period_specificationContext *> period_specification();
    Period_specificationContext* period_specification(size_t i);
    Correlation_clauseContext *correlation_clause();
    Tablesample_clauseContext *tablesample_clause();

   
  };

  Singles_table_referenceContext* singles_table_reference();

  class  Table_reference_nameContext : public antlr4::ParserRuleContext {
  public:
    Table_reference_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Table_nameContext *table_name();
    Table_reference_schemaContext *table_reference_schema();
    std::vector<antlr4::tree::TerminalNode *> DOT();
    antlr4::tree::TerminalNode* DOT(size_t i);
    Data_source_nameContext *data_source_name();

   
  };

  Table_reference_nameContext* table_reference_name();

  class  Table_reference_schemaContext : public antlr4::ParserRuleContext {
  public:
    Table_reference_schemaContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Schema_nameContext *schema_name();
    antlr4::tree::TerminalNode *SESSION();

   
  };

  Table_reference_schemaContext* table_reference_schema();

  class  Period_specificationContext : public antlr4::ParserRuleContext {
  public:
    Period_specificationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *SYSTEM_TIME();
    antlr4::tree::TerminalNode *BUSINESS_TIME();
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *OF();
    std::vector<ValueContext *> value();
    ValueContext* value(size_t i);
    antlr4::tree::TerminalNode *FROM();
    antlr4::tree::TerminalNode *TO();
    antlr4::tree::TerminalNode *BETWEEN();
    antlr4::tree::TerminalNode *AND();

   
  };

  Period_specificationContext* period_specification();

  class  ValueContext : public antlr4::ParserRuleContext {
  public:
    ValueContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TodoContext *todo();

   
  };

  ValueContext* value();

  class  Correlation_clauseContext : public antlr4::ParserRuleContext {
  public:
    Correlation_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Correlation_nameContext *correlation_name();
    antlr4::tree::TerminalNode *AS();
    Column_name_list_parenContext *column_name_list_paren();

   
  };

  Correlation_clauseContext* correlation_clause();

  class  Tablesample_clauseContext : public antlr4::ParserRuleContext {
  public:
    Tablesample_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TABLESAMPLE();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<Numeric_expressionContext *> numeric_expression();
    Numeric_expressionContext* numeric_expression(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *BERNOULLI();
    antlr4::tree::TerminalNode *SYSTEM();
    antlr4::tree::TerminalNode *REPEATABLE();

   
  };

  Tablesample_clauseContext* tablesample_clause();

  class  Numeric_expressionContext : public antlr4::ParserRuleContext {
  public:
    Numeric_expressionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ExpressionContext *expression();

   
  };

  Numeric_expressionContext* numeric_expression();

  class  Single_view_referenceContext : public antlr4::ParserRuleContext {
  public:
    Single_view_referenceContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    View_nameContext *view_name();
    std::vector<Period_specificationContext *> period_specification();
    Period_specificationContext* period_specification(size_t i);
    Correlation_clauseContext *correlation_clause();

   
  };

  Single_view_referenceContext* single_view_reference();

  class  Single_nickname_referenceContext : public antlr4::ParserRuleContext {
  public:
    Single_nickname_referenceContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Nick_nameContext *nick_name();
    Correlation_clauseContext *correlation_clause();

   
  };

  Single_nickname_referenceContext* single_nickname_reference();

  class  Only_table_referenceContext : public antlr4::ParserRuleContext {
  public:
    Only_table_referenceContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ONLY();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Table_or_view_nameContext *table_or_view_name();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    Correlation_clauseContext *correlation_clause();

   
  };

  Only_table_referenceContext* only_table_reference();

  class  Outer_table_referenceContext : public antlr4::ParserRuleContext {
  public:
    Outer_table_referenceContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OUTER();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Table_or_view_nameContext *table_or_view_name();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    Correlation_clauseContext *correlation_clause();

   
  };

  Outer_table_referenceContext* outer_table_reference();

  class  Analyze_table_referenceContext : public antlr4::ParserRuleContext {
  public:
    Analyze_table_referenceContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Table_or_view_nameContext *table_or_view_name();
    antlr4::tree::TerminalNode *ANALYZE();
    antlr4::tree::TerminalNode *TABLE();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Implementation_clauseContext *implementation_clause();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();

   
  };

  Analyze_table_referenceContext* analyze_table_reference();

  class  Implementation_clauseContext : public antlr4::ParserRuleContext {
  public:
    Implementation_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *IMPLEMENTATION();
    StringContext *string();

   
  };

  Implementation_clauseContext* implementation_clause();

  class  Nested_table_referenceContext : public antlr4::ParserRuleContext {
  public:
    Nested_table_referenceContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    FullselectContext *fullselect();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *LATERAL();
    antlr4::tree::TerminalNode *WITH();
    Common_table_expression_listContext *common_table_expression_list();
    Correlation_clauseContext *correlation_clause();
    Continue_handlerContext *continue_handler();
    antlr4::tree::TerminalNode *WITHIN();

   
  };

  Nested_table_referenceContext* nested_table_reference();

  class  Continue_handlerContext : public antlr4::ParserRuleContext {
  public:
    Continue_handlerContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RETURN();
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *UNTIL();
    std::vector<Specific_condition_valueContext *> specific_condition_value();
    Specific_condition_valueContext* specific_condition_value(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Continue_handlerContext* continue_handler();

  class  Specific_condition_valueContext : public antlr4::ParserRuleContext {
  public:
    Specific_condition_valueContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FEDERATED();
    antlr4::tree::TerminalNode *SQLSTATE();
    String_constantContext *string_constant();
    antlr4::tree::TerminalNode *VALUE();
    antlr4::tree::TerminalNode *SQLCODE();
    std::vector<Integer_constantContext *> integer_constant();
    Integer_constantContext* integer_constant(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Specific_condition_valueContext* specific_condition_value();

  class  Data_change_table_referenceContext : public antlr4::ParserRuleContext {
  public:
    Data_change_table_referenceContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Final_newContext *final_new();
    antlr4::tree::TerminalNode *TABLE();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Insert_statementContext *insert_statement();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    Final_new_oldContext *final_new_old();
    Searched_update_statementContext *searched_update_statement();
    antlr4::tree::TerminalNode *OLD();
    Searched_delete_statementContext *searched_delete_statement();
    Correlation_clauseContext *correlation_clause();

   
  };

  Data_change_table_referenceContext* data_change_table_reference();

  class  Searched_update_statementContext : public antlr4::ParserRuleContext {
  public:
    Searched_update_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TodoContext *todo();

   
  };

  Searched_update_statementContext* searched_update_statement();

  class  Searched_delete_statementContext : public antlr4::ParserRuleContext {
  public:
    Searched_delete_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TodoContext *todo();

   
  };

  Searched_delete_statementContext* searched_delete_statement();

  class  Final_newContext : public antlr4::ParserRuleContext {
  public:
    Final_newContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FINAL();
    antlr4::tree::TerminalNode *NEW();

   
  };

  Final_newContext* final_new();

  class  Final_new_oldContext : public antlr4::ParserRuleContext {
  public:
    Final_new_oldContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FINAL();
    antlr4::tree::TerminalNode *NEW();
    antlr4::tree::TerminalNode *OLD();

   
  };

  Final_new_oldContext* final_new_old();

  class  Table_function_referenceContext : public antlr4::ParserRuleContext {
  public:
    Table_function_referenceContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TABLE();
    std::vector<antlr4::tree::TerminalNode *> LEFT_RND_BKT();
    antlr4::tree::TerminalNode* LEFT_RND_BKT(size_t i);
    Function_nameContext *function_name();
    std::vector<ExpressionContext *> expression();
    ExpressionContext* expression(size_t i);
    std::vector<antlr4::tree::TerminalNode *> RIGHT_RND_BKT();
    antlr4::tree::TerminalNode* RIGHT_RND_BKT(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);
    Table_udf_cardinality_clauseContext *table_udf_cardinality_clause();
    Correlation_clauseContext *correlation_clause();
    Typed_correlation_clauseContext *typed_correlation_clause();

   
  };

  Table_function_referenceContext* table_function_reference();

  class  Table_udf_cardinality_clauseContext : public antlr4::ParserRuleContext {
  public:
    Table_udf_cardinality_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CARDINALITY();
    Integer_constantContext *integer_constant();
    antlr4::tree::TerminalNode *MULTIPLIER();
    Numeric_constantContext *numeric_constant();

   
  };

  Table_udf_cardinality_clauseContext* table_udf_cardinality_clause();

  class  Typed_correlation_clauseContext : public antlr4::ParserRuleContext {
  public:
    Typed_correlation_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Correlation_nameContext *correlation_name();
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<Column_name_data_typeContext *> column_name_data_type();
    Column_name_data_typeContext* column_name_data_type(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Typed_correlation_clauseContext* typed_correlation_clause();

  class  Column_name_data_typeContext : public antlr4::ParserRuleContext {
  public:
    Column_name_data_typeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Column_nameContext *column_name();
    Data_typeContext *data_type();

   
  };

  Column_name_data_typeContext* column_name_data_type();

  class  Collection_derived_tableContext : public antlr4::ParserRuleContext {
  public:
    Collection_derived_tableContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *UNNEST();
    Table_functionContext *table_function();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *ORDINALITY();
    Correlation_clauseContext *correlation_clause();

   
  };

  Collection_derived_tableContext* collection_derived_table();

  class  Table_functionContext : public antlr4::ParserRuleContext {
  public:
    Table_functionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TodoContext *todo();

   
  };

  Table_functionContext* table_function();

  class  Xmltable_expressionContext : public antlr4::ParserRuleContext {
  public:
    Xmltable_expressionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Xmltable_functionContext *xmltable_function();
    Correlation_clauseContext *correlation_clause();

   
  };

  Xmltable_expressionContext* xmltable_expression();

  class  Xmltable_functionContext : public antlr4::ParserRuleContext {
  public:
    Xmltable_functionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TodoContext *todo();

   
  };

  Xmltable_functionContext* xmltable_function();

  class  Joined_tableContext : public antlr4::ParserRuleContext {
  public:
    Joined_tableContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Table_referenceContext *> table_reference();
    Table_referenceContext* table_reference(size_t i);
    antlr4::tree::TerminalNode *JOIN();
    antlr4::tree::TerminalNode *ON();
    Join_conditionContext *join_condition();
    antlr4::tree::TerminalNode *USING();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Column_name_listContext *column_name_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *INNER();
    OuterContext *outer();
    antlr4::tree::TerminalNode *CROSS();
    Joined_tableContext *joined_table();

   
  };

  Joined_tableContext* joined_table();

  class  Join_conditionContext : public antlr4::ParserRuleContext {
  public:
    Join_conditionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Search_conditionContext *search_condition();

   
  };

  Join_conditionContext* join_condition();

  class  OuterContext : public antlr4::ParserRuleContext {
  public:
    OuterContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT();
    antlr4::tree::TerminalNode *RIGHT();
    antlr4::tree::TerminalNode *FULL();
    antlr4::tree::TerminalNode *OUTER();

   
  };

  OuterContext* outer();

  class  External_table_referenceContext : public antlr4::ParserRuleContext {
  public:
    External_table_referenceContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *EXTERNAL();
    File_nameContext *file_name();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<Column_definition_2Context *> column_definition_2();
    Column_definition_2Context* column_definition_2(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *LIKE();
    Correlation_nameContext *correlation_name();
    Table_or_view_nameContext *table_or_view_name();
    Nick_nameContext *nick_name();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);
    antlr4::tree::TerminalNode *AS();

   
  };

  External_table_referenceContext* external_table_reference();

  class  Column_definition_2Context : public antlr4::ParserRuleContext {
  public:
    Column_definition_2Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Column_nameContext *column_name();
    Built_in_typeContext *built_in_type();
    antlr4::tree::TerminalNode *NOT();
    antlr4::tree::TerminalNode *NULL_();

   
  };

  Column_definition_2Context* column_definition_2();

  class  File_nameContext : public antlr4::ParserRuleContext {
  public:
    File_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    TodoContext *todo();

   
  };

  File_nameContext* file_name();

  class  Where_clauseContext : public antlr4::ParserRuleContext {
  public:
    Where_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WHERE();
    Search_conditionContext *search_condition();

   
  };

  Where_clauseContext* where_clause();

  class  Group_by_clauseContext : public antlr4::ParserRuleContext {
  public:
    Group_by_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GROUP();
    antlr4::tree::TerminalNode *BY();
    std::vector<Group_by_clause_optsContext *> group_by_clause_opts();
    Group_by_clause_optsContext* group_by_clause_opts(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Group_by_clauseContext* group_by_clause();

  class  Group_by_clause_optsContext : public antlr4::ParserRuleContext {
  public:
    Group_by_clause_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Grouping_expressionContext *grouping_expression();
    Grouping_setsContext *grouping_sets();
    Super_groupsContext *super_groups();

   
  };

  Group_by_clause_optsContext* group_by_clause_opts();

  class  Grouping_expressionContext : public antlr4::ParserRuleContext {
  public:
    Grouping_expressionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ExpressionContext *expression();

   
  };

  Grouping_expressionContext* grouping_expression();

  class  Grouping_setsContext : public antlr4::ParserRuleContext {
  public:
    Grouping_setsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *GROUPING();
    antlr4::tree::TerminalNode *SETS();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();

   
  };

  Grouping_setsContext* grouping_sets();

  class  Super_groupsContext : public antlr4::ParserRuleContext {
  public:
    Super_groupsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ROLLUP();
    antlr4::tree::TerminalNode *CUBE();
    Grant_totalContext *grant_total();

   
  };

  Super_groupsContext* super_groups();

  class  Grant_totalContext : public antlr4::ParserRuleContext {
  public:
    Grant_totalContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();

   
  };

  Grant_totalContext* grant_total();

  class  Having_clauseContext : public antlr4::ParserRuleContext {
  public:
    Having_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Search_conditionContext *search_condition();

   
  };

  Having_clauseContext* having_clause();

  class  Order_by_clauseContext : public antlr4::ParserRuleContext {
  public:
    Order_by_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ORDER();
    antlr4::tree::TerminalNode *BY();
    std::vector<Order_by_clause_optsContext *> order_by_clause_opts();
    Order_by_clause_optsContext* order_by_clause_opts(size_t i);
    antlr4::tree::TerminalNode *INPUT();
    antlr4::tree::TerminalNode *SEQUENCE();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Order_by_clauseContext* order_by_clause();

  class  Order_by_clause_optsContext : public antlr4::ParserRuleContext {
  public:
    Order_by_clause_optsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Sort_keyContext *sort_key();
    Asc_descContext *asc_desc();
    antlr4::tree::TerminalNode *NULLS();
    First_lastContext *first_last();
    antlr4::tree::TerminalNode *ORDER();
    antlr4::tree::TerminalNode *OF();
    Table_designatorContext *table_designator();

   
  };

  Order_by_clause_optsContext* order_by_clause_opts();

  class  Table_designatorContext : public antlr4::ParserRuleContext {
  public:
    Table_designatorContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Table_designatorContext* table_designator();

  class  Asc_descContext : public antlr4::ParserRuleContext {
  public:
    Asc_descContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ASC();
    antlr4::tree::TerminalNode *DESC();

   
  };

  Asc_descContext* asc_desc();

  class  First_lastContext : public antlr4::ParserRuleContext {
  public:
    First_lastContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FIRST();
    antlr4::tree::TerminalNode *LAST();

   
  };

  First_lastContext* first_last();

  class  Sort_keyContext : public antlr4::ParserRuleContext {
  public:
    Sort_keyContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Simple_column_nameContext *simple_column_name();
    Simple_integerContext *simple_integer();
    Sork_key_expressionContext *sork_key_expression();

   
  };

  Sort_keyContext* sort_key();

  class  Simple_column_nameContext : public antlr4::ParserRuleContext {
  public:
    Simple_column_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Simple_column_nameContext* simple_column_name();

  class  Simple_integerContext : public antlr4::ParserRuleContext {
  public:
    Simple_integerContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DECIMAL_LITERAL();

   
  };

  Simple_integerContext* simple_integer();

  class  Sork_key_expressionContext : public antlr4::ParserRuleContext {
  public:
    Sork_key_expressionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ExpressionContext *expression();

   
  };

  Sork_key_expressionContext* sork_key_expression();

  class  Offset_clauseContext : public antlr4::ParserRuleContext {
  public:
    Offset_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OFFSET();
    Offset_row_countContext *offset_row_count();
    Row_rowsContext *row_rows();

   
  };

  Offset_clauseContext* offset_clause();

  class  Offset_row_countContext : public antlr4::ParserRuleContext {
  public:
    Offset_row_countContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DECIMAL_LITERAL();

   
  };

  Offset_row_countContext* offset_row_count();

  class  Fetch_clauseContext : public antlr4::ParserRuleContext {
  public:
    Fetch_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FETCH();
    antlr4::tree::TerminalNode *NEXT();
    Row_rowsContext *row_rows();
    antlr4::tree::TerminalNode *ONLY();
    Fetch_row_countContext *fetch_row_count();

   
  };

  Fetch_clauseContext* fetch_clause();

  class  Fetch_row_countContext : public antlr4::ParserRuleContext {
  public:
    Fetch_row_countContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DECIMAL_LITERAL();

   
  };

  Fetch_row_countContext* fetch_row_count();

  class  Row_rowsContext : public antlr4::ParserRuleContext {
  public:
    Row_rowsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ROW();
    antlr4::tree::TerminalNode *ROWS();

   
  };

  Row_rowsContext* row_rows();

  class  Isolation_clauseContext : public antlr4::ParserRuleContext {
  public:
    Isolation_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *RR();
    antlr4::tree::TerminalNode *RS();
    antlr4::tree::TerminalNode *CS();
    antlr4::tree::TerminalNode *UR();
    Lock_request_clauseContext *lock_request_clause();

   
  };

  Isolation_clauseContext* isolation_clause();

  class  Lock_request_clauseContext : public antlr4::ParserRuleContext {
  public:
    Lock_request_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *USE();
    antlr4::tree::TerminalNode *AND();
    antlr4::tree::TerminalNode *KEEP();
    antlr4::tree::TerminalNode *LOCKS();
    antlr4::tree::TerminalNode *SHARE();
    antlr4::tree::TerminalNode *UPDATE();
    antlr4::tree::TerminalNode *EXCLUSIVE();

   
  };

  Lock_request_clauseContext* lock_request_clause();

  class  Values_clauseContext : public antlr4::ParserRuleContext {
  public:
    Values_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *VALUES();
    std::vector<Values_rowContext *> values_row();
    Values_rowContext* values_row(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Values_clauseContext* values_clause();

  class  Values_rowContext : public antlr4::ParserRuleContext {
  public:
    Values_rowContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    ExpressionContext *expression();
    antlr4::tree::TerminalNode *NULL_();
    Row_expressionContext *row_expression();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<Expr_nullContext *> expr_null();
    Expr_nullContext* expr_null(size_t i);
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Values_rowContext* values_row();

  class  Root_view_definitionContext : public antlr4::ParserRuleContext {
  public:
    Root_view_definitionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *MODE();
    antlr4::tree::TerminalNode *DB2SQL();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Oid_columnContext *oid_column();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *COMMA();
    With_optionsContext *with_options();

   
  };

  Root_view_definitionContext* root_view_definition();

  class  Subview_definitionContext : public antlr4::ParserRuleContext {
  public:
    Subview_definitionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *MODE();
    antlr4::tree::TerminalNode *DB2SQL();
    Under_clauseContext *under_clause();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    With_optionsContext *with_options();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *EXTEND();

   
  };

  Subview_definitionContext* subview_definition();

  class  Oid_columnContext : public antlr4::ParserRuleContext {
  public:
    Oid_columnContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *REF();
    antlr4::tree::TerminalNode *IS();
    Oid_column_nameContext *oid_column_name();
    antlr4::tree::TerminalNode *USER();
    antlr4::tree::TerminalNode *GENERATED();
    antlr4::tree::TerminalNode *UNCHECKED();

   
  };

  Oid_columnContext* oid_column();

  class  With_optionsContext : public antlr4::ParserRuleContext {
  public:
    With_optionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<With_option_defContext *> with_option_def();
    With_option_defContext* with_option_def(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  With_optionsContext* with_options();

  class  With_option_defContext : public antlr4::ParserRuleContext {
  public:
    With_option_defContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Column_nameContext *column_name();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *OPTIONS();
    antlr4::tree::TerminalNode *READ();
    antlr4::tree::TerminalNode *ONLY();
    std::vector<With_option_scope_defContext *> with_option_scope_def();
    With_option_scope_defContext* with_option_scope_def(size_t i);

   
  };

  With_option_defContext* with_option_def();

  class  With_option_scope_defContext : public antlr4::ParserRuleContext {
  public:
    With_option_scope_defContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SCOPE();
    Typed_table_nameContext *typed_table_name();
    Typed_view_nameContext *typed_view_name();

   
  };

  With_option_scope_defContext* with_option_scope_def();

  class  Under_clauseContext : public antlr4::ParserRuleContext {
  public:
    Under_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *UNDER();
    Superview_nameContext *superview_name();
    antlr4::tree::TerminalNode *INHERIT();
    antlr4::tree::TerminalNode *SELECT();
    antlr4::tree::TerminalNode *PRIVILEGES();

   
  };

  Under_clauseContext* under_clause();

  class  Create_work_action_set_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_work_action_set_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    std::vector<antlr4::tree::TerminalNode *> WORK();
    antlr4::tree::TerminalNode* WORK(size_t i);
    antlr4::tree::TerminalNode *ACTION();
    std::vector<antlr4::tree::TerminalNode *> SET();
    antlr4::tree::TerminalNode* SET(size_t i);
    Work_action_set_nameContext *work_action_set_name();
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *USING();
    std::vector<antlr4::tree::TerminalNode *> CLASS_();
    antlr4::tree::TerminalNode* CLASS_(size_t i);
    Work_class_set_nameContext *work_class_set_name();
    antlr4::tree::TerminalNode *DATABASE();
    antlr4::tree::TerminalNode *SERVICE();
    Service_superclass_nameContext *service_superclass_name();
    antlr4::tree::TerminalNode *WORKLOAD();
    Workload_nameContext *workload_name();
    Work_action_definition_list_parenContext *work_action_definition_list_paren();

   
  };

  Create_work_action_set_statementContext* create_work_action_set_statement();

  class  Work_action_definition_list_parenContext : public antlr4::ParserRuleContext {
  public:
    Work_action_definition_list_parenContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Work_action_definition_listContext *work_action_definition_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();

   
  };

  Work_action_definition_list_parenContext* work_action_definition_list_paren();

  class  Work_action_definition_listContext : public antlr4::ParserRuleContext {
  public:
    Work_action_definition_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Work_action_definitionContext *> work_action_definition();
    Work_action_definitionContext* work_action_definition(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Work_action_definition_listContext* work_action_definition_list();

  class  Work_action_definitionContext : public antlr4::ParserRuleContext {
  public:
    Work_action_definitionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<antlr4::tree::TerminalNode *> WORK();
    antlr4::tree::TerminalNode* WORK(size_t i);
    antlr4::tree::TerminalNode *ACTION();
    Work_action_nameContext *work_action_name();
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *CLASS_();
    Work_class_nameContext *work_class_name();
    Action_types_clauseContext *action_types_clause();
    std::vector<Histogram_templace_clauseContext *> histogram_templace_clause();
    Histogram_templace_clauseContext* histogram_templace_clause(size_t i);
    Enable_disableContext *enable_disable();

   
  };

  Work_action_definitionContext* work_action_definition();

  class  Action_types_clauseContext : public antlr4::ParserRuleContext {
  public:
    Action_types_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *MAP();
    antlr4::tree::TerminalNode *ACTIVITY();
    antlr4::tree::TerminalNode *TO();
    Service_subclass_nameContext *service_subclass_name();
    With_withoutContext *with_without();
    antlr4::tree::TerminalNode *NESTED();
    antlr4::tree::TerminalNode *WHEN();
    Threshold_types_clauseContext *threshold_types_clause();
    Threshold_exceeded_actionsContext *threshold_exceeded_actions();
    antlr4::tree::TerminalNode *PREVENT();
    antlr4::tree::TerminalNode *EXECUTION();
    antlr4::tree::TerminalNode *COLLECT();
    antlr4::tree::TerminalNode *DATA();
    Collect_activity_data_clauseContext *collect_activity_data_clause();
    antlr4::tree::TerminalNode *AGGREGATE();
    antlr4::tree::TerminalNode *BASE();
    antlr4::tree::TerminalNode *EXTENDED();

   
  };

  Action_types_clauseContext* action_types_clause();

  class  Threshold_types_clauseContext : public antlr4::ParserRuleContext {
  public:
    Threshold_types_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CONCURRENTDBCOORDACTIVITIES();
    std::vector<antlr4::tree::TerminalNode *> GT();
    antlr4::tree::TerminalNode* GT(size_t i);
    std::vector<Integer_valueContext *> integer_value();
    Integer_valueContext* integer_value(size_t i);
    antlr4::tree::TerminalNode *AND();
    antlr4::tree::TerminalNode *QUEUEDACTIVITIES();
    antlr4::tree::TerminalNode *UNBOUNDED();
    antlr4::tree::TerminalNode *SQLTEMPSPACE();
    KmgContext *kmg();
    antlr4::tree::TerminalNode *SQLROWSRETURNED();
    antlr4::tree::TerminalNode *ESTIMATEDSQLCOST();
    Bigint_valueContext *bigint_value();
    antlr4::tree::TerminalNode *CPUTIME();
    Hours_minutesContext *hours_minutes();
    antlr4::tree::TerminalNode *CHECKING();
    antlr4::tree::TerminalNode *EVERY();
    Second_secondsContext *second_seconds();
    antlr4::tree::TerminalNode *SQLROWSREAD();
    antlr4::tree::TerminalNode *SORTSHRHEAPUTIL();
    antlr4::tree::TerminalNode *PERCENT();
    antlr4::tree::TerminalNode *BLOCKING();
    antlr4::tree::TerminalNode *ADMISSION();
    antlr4::tree::TerminalNode *FOR();
    Dt_units_with_secondsContext *dt_units_with_seconds();
    antlr4::tree::TerminalNode *ACTIVITYTOTALTIME();
    antlr4::tree::TerminalNode *ACTIVITYTOTALRUNTIME();

   
  };

  Threshold_types_clauseContext* threshold_types_clause();

  class  Second_secondsContext : public antlr4::ParserRuleContext {
  public:
    Second_secondsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *SECOND();
    antlr4::tree::TerminalNode *SECONDS();

   
  };

  Second_secondsContext* second_seconds();

  class  Hours_minutesContext : public antlr4::ParserRuleContext {
  public:
    Hours_minutesContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *HOUR();
    antlr4::tree::TerminalNode *HOURS();
    antlr4::tree::TerminalNode *MINUTE();
    antlr4::tree::TerminalNode *MINUTES();

   
  };

  Hours_minutesContext* hours_minutes();

  class  Threshold_exceeded_actionsContext : public antlr4::ParserRuleContext {
  public:
    Threshold_exceeded_actionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *STOP();
    antlr4::tree::TerminalNode *EXECUTION();
    antlr4::tree::TerminalNode *CONTINUE();
    antlr4::tree::TerminalNode *COLLECT();
    antlr4::tree::TerminalNode *ACTIVITY();
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *NONE();
    Collect_activity_data_clauseContext *collect_activity_data_clause();

   
  };

  Threshold_exceeded_actionsContext* threshold_exceeded_actions();

  class  Collect_activity_data_clauseContext : public antlr4::ParserRuleContext {
  public:
    Collect_activity_data_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *COORDINATOR();
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *WITHOUT();
    antlr4::tree::TerminalNode *DETAILS();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *SECTION();
    antlr4::tree::TerminalNode *MEMBER();
    antlr4::tree::TerminalNode *MEMBERS();
    antlr4::tree::TerminalNode *AND();
    antlr4::tree::TerminalNode *VALUES();
    antlr4::tree::TerminalNode *INCLUDE();
    antlr4::tree::TerminalNode *ACTUALS();
    antlr4::tree::TerminalNode *BASE();

   
  };

  Collect_activity_data_clauseContext* collect_activity_data_clause();

  class  With_withoutContext : public antlr4::ParserRuleContext {
  public:
    With_withoutContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *WITHOUT();

   
  };

  With_withoutContext* with_without();

  class  Histogram_templace_clauseContext : public antlr4::ParserRuleContext {
  public:
    Histogram_templace_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ACTIVITY();
    antlr4::tree::TerminalNode *HISTOGRAM();
    antlr4::tree::TerminalNode *TEMPLATE();
    antlr4::tree::TerminalNode *LIFETIME();
    antlr4::tree::TerminalNode *QUEUETIME();
    antlr4::tree::TerminalNode *EXECUTETIME();
    antlr4::tree::TerminalNode *ESTIMATEDCOST();
    antlr4::tree::TerminalNode *INTERARRIVALTIME();
    antlr4::tree::TerminalNode *SYSDEFAULTHISTOGRAM();
    Template_nameContext *template_name();

   
  };

  Histogram_templace_clauseContext* histogram_templace_clause();

  class  Create_work_class_set_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_work_class_set_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *WORK();
    antlr4::tree::TerminalNode *CLASS_();
    antlr4::tree::TerminalNode *SET();
    Work_class_set_nameContext *work_class_set_name();
    Work_class_definition_list_parenContext *work_class_definition_list_paren();

   
  };

  Create_work_class_set_statementContext* create_work_class_set_statement();

  class  Work_class_definition_list_parenContext : public antlr4::ParserRuleContext {
  public:
    Work_class_definition_list_parenContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Work_class_definition_listContext *work_class_definition_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();

   
  };

  Work_class_definition_list_parenContext* work_class_definition_list_paren();

  class  Work_class_definition_listContext : public antlr4::ParserRuleContext {
  public:
    Work_class_definition_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Work_class_definitionContext *> work_class_definition();
    Work_class_definitionContext* work_class_definition(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Work_class_definition_listContext* work_class_definition_list();

  class  Work_class_definitionContext : public antlr4::ParserRuleContext {
  public:
    Work_class_definitionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Work_class_nameContext *work_class_name();
    Work_attributesContext *work_attributes();
    Position_clauseContext *position_clause();
    antlr4::tree::TerminalNode *WORK();
    antlr4::tree::TerminalNode *CLASS_();

   
  };

  Work_class_definitionContext* work_class_definition();

  class  Work_attributesContext : public antlr4::ParserRuleContext {
  public:
    Work_attributesContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WORK();
    antlr4::tree::TerminalNode *TYPE();
    antlr4::tree::TerminalNode *CALL();
    antlr4::tree::TerminalNode *DDL();
    antlr4::tree::TerminalNode *LOAD();
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *READ();
    antlr4::tree::TerminalNode *WRITE();
    antlr4::tree::TerminalNode *DML();
    Schema_clauseContext *schema_clause();
    For_from_to_clauseContext *for_from_to_clause();
    Data_tag_clauseContext *data_tag_clause();

   
  };

  Work_attributesContext* work_attributes();

  class  Position_clauseContext : public antlr4::ParserRuleContext {
  public:
    Position_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *POSITION();
    antlr4::tree::TerminalNode *LAST();
    Work_class_nameContext *work_class_name();
    antlr4::tree::TerminalNode *BEFORE();
    antlr4::tree::TerminalNode *AFTER();
    antlr4::tree::TerminalNode *AT();
    Position_Context *position_();

   
  };

  Position_clauseContext* position_clause();

  class  Position_Context : public antlr4::ParserRuleContext {
  public:
    Position_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Positive_integerContext *positive_integer();

   
  };

  Position_Context* position_();

  class  For_from_to_clauseContext : public antlr4::ParserRuleContext {
  public:
    For_from_to_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *FROM();
    From_valueContext *from_value();
    antlr4::tree::TerminalNode *TIMERONCOST();
    antlr4::tree::TerminalNode *CARDINALITY();
    antlr4::tree::TerminalNode *TO();
    antlr4::tree::TerminalNode *UNBOUNDED();
    To_valueContext *to_value();

   
  };

  For_from_to_clauseContext* for_from_to_clause();

  class  From_valueContext : public antlr4::ParserRuleContext {
  public:
    From_valueContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FLOAT_LITERAL();

   
  };

  From_valueContext* from_value();

  class  To_valueContext : public antlr4::ParserRuleContext {
  public:
    To_valueContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FLOAT_LITERAL();

   
  };

  To_valueContext* to_value();

  class  Data_tag_clauseContext : public antlr4::ParserRuleContext {
  public:
    Data_tag_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DATA();
    antlr4::tree::TerminalNode *TAG();
    antlr4::tree::TerminalNode *LIST();
    antlr4::tree::TerminalNode *CONTAINS();
    Integer_constantContext *integer_constant();

   
  };

  Data_tag_clauseContext* data_tag_clause();

  class  Schema_clauseContext : public antlr4::ParserRuleContext {
  public:
    Schema_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ROUTINES();
    antlr4::tree::TerminalNode *IN();
    antlr4::tree::TerminalNode *SCHEMA();
    Schema_nameContext *schema_name();

   
  };

  Schema_clauseContext* schema_clause();

  class  Create_workload_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_workload_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *WORKLOAD();
    Workload_nameContext *workload_name();
    Workload_attributesContext *workload_attributes();
    Histogram_templace_clauseContext *histogram_templace_clause();
    std::vector<Connection_attributesContext *> connection_attributes();
    Connection_attributesContext* connection_attributes(size_t i);
    Position_clause_2Context *position_clause_2();
    antlr4::tree::TerminalNode *PRIORITY();
    std::vector<antlr4::tree::TerminalNode *> COLLECT();
    antlr4::tree::TerminalNode* COLLECT(size_t i);
    std::vector<antlr4::tree::TerminalNode *> ACTIVITY();
    antlr4::tree::TerminalNode* ACTIVITY(size_t i);
    antlr4::tree::TerminalNode *METRICS();
    std::vector<antlr4::tree::TerminalNode *> DATA();
    antlr4::tree::TerminalNode* DATA(size_t i);
    std::vector<antlr4::tree::TerminalNode *> AGGREGATE();
    antlr4::tree::TerminalNode* AGGREGATE(size_t i);
    std::vector<antlr4::tree::TerminalNode *> UNIT();
    antlr4::tree::TerminalNode* UNIT(size_t i);
    std::vector<antlr4::tree::TerminalNode *> OF();
    antlr4::tree::TerminalNode* OF(size_t i);
    std::vector<antlr4::tree::TerminalNode *> WORK();
    antlr4::tree::TerminalNode* WORK(size_t i);
    std::vector<antlr4::tree::TerminalNode *> LOCK();
    antlr4::tree::TerminalNode* LOCK(size_t i);
    antlr4::tree::TerminalNode *TIMEOUT();
    antlr4::tree::TerminalNode *DEADLOCK();
    antlr4::tree::TerminalNode *WAIT();
    Collect_lock_wait_optionsContext *collect_lock_wait_options();
    antlr4::tree::TerminalNode *CRITICAL();
    antlr4::tree::TerminalNode *HIGH();
    antlr4::tree::TerminalNode *MEDIUM();
    antlr4::tree::TerminalNode *LOW();
    std::vector<antlr4::tree::TerminalNode *> NONE();
    antlr4::tree::TerminalNode* NONE(size_t i);
    Collect_on_clauseContext *collect_on_clause();
    Collect_details_clauseContext *collect_details_clause();
    std::vector<antlr4::tree::TerminalNode *> BASE();
    antlr4::tree::TerminalNode* BASE(size_t i);
    std::vector<antlr4::tree::TerminalNode *> WITH();
    antlr4::tree::TerminalNode* WITH(size_t i);
    std::vector<antlr4::tree::TerminalNode *> HISTORY();
    antlr4::tree::TerminalNode* HISTORY(size_t i);
    std::vector<antlr4::tree::TerminalNode *> EXTENDED();
    antlr4::tree::TerminalNode* EXTENDED(size_t i);
    std::vector<antlr4::tree::TerminalNode *> AND();
    antlr4::tree::TerminalNode* AND(size_t i);
    std::vector<antlr4::tree::TerminalNode *> VALUES();
    antlr4::tree::TerminalNode* VALUES(size_t i);
    antlr4::tree::TerminalNode *INCLUDE();
    Pkg_exec_seqContext *pkg_exec_seq();

   
  };

  Create_workload_statementContext* create_workload_statement();

  class  Pkg_exec_seqContext : public antlr4::ParserRuleContext {
  public:
    Pkg_exec_seqContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PACKAGE();
    std::vector<antlr4::tree::TerminalNode *> LIST();
    antlr4::tree::TerminalNode* LIST(size_t i);
    antlr4::tree::TerminalNode *COMMA();
    antlr4::tree::TerminalNode *EXECUTABLE();

   
  };

  Pkg_exec_seqContext* pkg_exec_seq();

  class  Position_clause_2Context : public antlr4::ParserRuleContext {
  public:
    Position_clause_2Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *POSITION();
    antlr4::tree::TerminalNode *LAST();
    Workload_nameContext *workload_name();
    antlr4::tree::TerminalNode *BEFORE();
    antlr4::tree::TerminalNode *AFTER();
    antlr4::tree::TerminalNode *AT();
    Position_Context *position_();

   
  };

  Position_clause_2Context* position_clause_2();

  class  Connection_attributesContext : public antlr4::ParserRuleContext {
  public:
    Connection_attributesContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ADDRESS();
    String_list_parenContext *string_list_paren();
    antlr4::tree::TerminalNode *APPLNAME();
    antlr4::tree::TerminalNode *SYSTEM_USER();
    antlr4::tree::TerminalNode *SESSION_USER();
    antlr4::tree::TerminalNode *GROUP();
    antlr4::tree::TerminalNode *ROLE();
    antlr4::tree::TerminalNode *CURRENT();
    antlr4::tree::TerminalNode *CLIENT_USERID();
    antlr4::tree::TerminalNode *CLIENT_APPLNAME();
    antlr4::tree::TerminalNode *CLIENT_WRKSTNNAME();
    antlr4::tree::TerminalNode *CLIENT_ACCTNG();

   
  };

  Connection_attributesContext* connection_attributes();

  class  String_listContext : public antlr4::ParserRuleContext {
  public:
    String_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<StringContext *> string();
    StringContext* string(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  String_listContext* string_list();

  class  String_list_parenContext : public antlr4::ParserRuleContext {
  public:
    String_list_parenContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    std::vector<StringContext *> string();
    StringContext* string(size_t i);

   
  };

  String_list_parenContext* string_list_paren();

  class  Workload_attributesContext : public antlr4::ParserRuleContext {
  public:
    Workload_attributesContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Enable_disableContext *enable_disable();
    Allow_disallowContext *allow_disallow();
    antlr4::tree::TerminalNode *DB();
    antlr4::tree::TerminalNode *ACCESS();
    antlr4::tree::TerminalNode *MAXIMUM();
    antlr4::tree::TerminalNode *DEGREE_();
    antlr4::tree::TerminalNode *SERVICE();
    antlr4::tree::TerminalNode *CLASS_();
    antlr4::tree::TerminalNode *DEFAULT();
    DegreeContext *degree();
    antlr4::tree::TerminalNode *SYSDEFAULTUSERCLASS();
    Service_class_nameContext *service_class_name();
    antlr4::tree::TerminalNode *UNDER();
    Service_superclass_nameContext *service_superclass_name();

   
  };

  Workload_attributesContext* workload_attributes();

  class  DegreeContext : public antlr4::ParserRuleContext {
  public:
    DegreeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *DECIMAL_LITERAL();

   
  };

  DegreeContext* degree();

  class  Allow_disallowContext : public antlr4::ParserRuleContext {
  public:
    Allow_disallowContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALLOW();
    antlr4::tree::TerminalNode *DISALLOW();

   
  };

  Allow_disallowContext* allow_disallow();

  class  Collect_on_clauseContext : public antlr4::ParserRuleContext {
  public:
    Collect_on_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ON();
    antlr4::tree::TerminalNode *COORDINATOR();
    antlr4::tree::TerminalNode *MEMBER();
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *MEMBERS();

   
  };

  Collect_on_clauseContext* collect_on_clause();

  class  Collect_details_clauseContext : public antlr4::ParserRuleContext {
  public:
    Collect_details_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *WITHOUT();
    antlr4::tree::TerminalNode *DETAILS();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *SECTION();
    antlr4::tree::TerminalNode *AND();
    antlr4::tree::TerminalNode *VALUES();
    antlr4::tree::TerminalNode *INCLUDE();
    antlr4::tree::TerminalNode *ACTUALS();
    antlr4::tree::TerminalNode *BASE();

   
  };

  Collect_details_clauseContext* collect_details_clause();

  class  Collect_lock_wait_optionsContext : public antlr4::ParserRuleContext {
  public:
    Collect_lock_wait_optionsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FOR();
    antlr4::tree::TerminalNode *LOCKS();
    antlr4::tree::TerminalNode *WAITING();
    antlr4::tree::TerminalNode *MORE_();
    antlr4::tree::TerminalNode *THAN();
    Wait_timeContext *wait_time();
    antlr4::tree::TerminalNode *L_ONE();
    antlr4::tree::TerminalNode *SECOND();
    antlr4::tree::TerminalNode *SECONDS();
    antlr4::tree::TerminalNode *MICROSECONDS();
    antlr4::tree::TerminalNode *WITHOUT();
    antlr4::tree::TerminalNode *HISTORY();
    antlr4::tree::TerminalNode *WITH();
    antlr4::tree::TerminalNode *AND();
    antlr4::tree::TerminalNode *VALUES();

   
  };

  Collect_lock_wait_optionsContext* collect_lock_wait_options();

  class  Wait_timeContext : public antlr4::ParserRuleContext {
  public:
    Wait_timeContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Integer_valueContext *integer_value();

   
  };

  Wait_timeContext* wait_time();

  class  Create_wrapper_statementContext : public antlr4::ParserRuleContext {
  public:
    Create_wrapper_statementContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CREATE();
    antlr4::tree::TerminalNode *WRAPPER();
    Wrapper_nameContext *wrapper_name();
    antlr4::tree::TerminalNode *LIBRARY();
    Library_nameContext *library_name();
    antlr4::tree::TerminalNode *OPTIONS();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Wrapper_option_listContext *wrapper_option_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();

   
  };

  Create_wrapper_statementContext* create_wrapper_statement();

  class  Wrapper_option_listContext : public antlr4::ParserRuleContext {
  public:
    Wrapper_option_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Wrapper_optionContext *> wrapper_option();
    Wrapper_optionContext* wrapper_option(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Wrapper_option_listContext* wrapper_option_list();

  class  Wrapper_optionContext : public antlr4::ParserRuleContext {
  public:
    Wrapper_optionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Wrapper_option_nameContext *wrapper_option_name();
    StringContext *string();

   
  };

  Wrapper_optionContext* wrapper_option();

  class  ExpressionContext : public antlr4::ParserRuleContext {
  public:
    ExpressionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Timestamp_register_invocationContext *timestamp_register_invocation();
    Function_invocationContext *function_invocation();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Expression_listContext *expression_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    Constant_Context *constant_();
    Column_nameContext *column_name();
    VariableContext *variable();
    Special_registerContext *special_register();
    Scalar_fullselectContext *scalar_fullselect();
    Case_expressionContext *case_expression();
    Cast_specificationContext *cast_specification();
    Field_referenceContext *field_reference();
    Xmlcast_specificationContext *xmlcast_specification();
    Array_element_specificationContext *array_element_specification();
    Array_constructorContext *array_constructor();
    Method_invocationContext *method_invocation();
    Olap_specificationContext *olap_specification();
    Row_change_expressionContext *row_change_expression();
    Sequence_referenceContext *sequence_reference();
    Subtype_treatmentContext *subtype_treatment();
    std::vector<ExpressionContext *> expression();
    ExpressionContext* expression(size_t i);
    antlr4::tree::TerminalNode *STAR_STAR();
    antlr4::tree::TerminalNode *STAR();
    antlr4::tree::TerminalNode *DIVIDE();
    antlr4::tree::TerminalNode *MODULO();
    antlr4::tree::TerminalNode *CONCAT();
    antlr4::tree::TerminalNode *PIPE_PIPE();
    antlr4::tree::TerminalNode *PLUS();
    antlr4::tree::TerminalNode *MINUS();
    antlr4::tree::TerminalNode *ARROW();
    Id_Context *id_();
    Expression_list_in_parenthesesContext *expression_list_in_parentheses();
    antlr4::tree::TerminalNode *YEAR();
    antlr4::tree::TerminalNode *YEARS();
    antlr4::tree::TerminalNode *MONTH();
    antlr4::tree::TerminalNode *MONTHS();
    Day_to_secondsContext *day_to_seconds();
    antlr4::tree::TerminalNode *MICROSECOND();
    antlr4::tree::TerminalNode *MICROSECONDS();

   
  };

  ExpressionContext* expression();
  ExpressionContext* expression(int precedence);
  class  Function_invocationContext : public antlr4::ParserRuleContext {
  public:
    Function_invocationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Function_nameContext *function_name();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Arg_listContext *arg_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    All_distinctContext *all_distinct();

   
  };

  Function_invocationContext* function_invocation();

  class  All_distinctContext : public antlr4::ParserRuleContext {
  public:
    All_distinctContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ALL();
    antlr4::tree::TerminalNode *DISTINCT();

   
  };

  All_distinctContext* all_distinct();

  class  Scalar_fullselectContext : public antlr4::ParserRuleContext {
  public:
    Scalar_fullselectContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Fullselect_in_parenthesesContext *fullselect_in_parentheses();

   
  };

  Scalar_fullselectContext* scalar_fullselect();

  class  Cast_specificationContext : public antlr4::ParserRuleContext {
  public:
    Cast_specificationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *CAST();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *AS();
    Data_typeContext *data_type();
    Cursor_cast_specificationContext *cursor_cast_specification();
    Row_cast_specificationContext *row_cast_specification();
    Interval_cast_specificationContext *interval_cast_specification();
    ExpressionContext *expression();
    antlr4::tree::TerminalNode *NULL_();
    Parameter_markerContext *parameter_marker();
    antlr4::tree::TerminalNode *SCOPE();
    Typed_table_nameContext *typed_table_name();
    Typed_view_nameContext *typed_view_name();

   
  };

  Cast_specificationContext* cast_specification();

  class  Cursor_cast_specificationContext : public antlr4::ParserRuleContext {
  public:
    Cursor_cast_specificationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Parameter_markerContext *parameter_marker();
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *CURSOR();
    Cursor_type_nameContext *cursor_type_name();

   
  };

  Cursor_cast_specificationContext* cursor_cast_specification();

  class  Row_cast_specificationContext : public antlr4::ParserRuleContext {
  public:
    Row_cast_specificationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *AS();
    Row_type_nameContext *row_type_name();
    Row_expressionContext *row_expression();
    antlr4::tree::TerminalNode *NULL_();
    Parameter_markerContext *parameter_marker();

   
  };

  Row_cast_specificationContext* row_cast_specification();

  class  Interval_cast_specificationContext : public antlr4::ParserRuleContext {
  public:
    Interval_cast_specificationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    String_constantContext *string_constant();
    antlr4::tree::TerminalNode *AS();
    antlr4::tree::TerminalNode *INTERVAL();

   
  };

  Interval_cast_specificationContext* interval_cast_specification();

  class  Xmlcast_specificationContext : public antlr4::ParserRuleContext {
  public:
    Xmlcast_specificationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *XMLCAST();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *AS();
    Data_typeContext *data_type();
    ExpressionContext *expression();
    antlr4::tree::TerminalNode *NULL_();
    Parameter_markerContext *parameter_marker();

   
  };

  Xmlcast_specificationContext* xmlcast_specification();

  class  Array_element_specificationContext : public antlr4::ParserRuleContext {
  public:
    Db2Parser::Id_Context *av = nullptr;
    Array_element_specificationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_SQR_BKT();
    ExpressionContext *expression();
    antlr4::tree::TerminalNode *RIGHT_SQR_BKT();
    Id_Context *id_();
    antlr4::tree::TerminalNode *CAST();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Parameter_markerContext *parameter_marker();
    antlr4::tree::TerminalNode *AS();
    Array_type_nameContext *array_type_name();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();

   
  };

  Array_element_specificationContext* array_element_specification();

  class  Array_constructorContext : public antlr4::ParserRuleContext {
  public:
    Array_constructorContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ARRAY();
    antlr4::tree::TerminalNode *LEFT_SQR_BKT();
    antlr4::tree::TerminalNode *RIGHT_SQR_BKT();
    FullselectContext *fullselect();
    Expression_listContext *expression_list();
    antlr4::tree::TerminalNode *WITH();
    Common_table_expression_listContext *common_table_expression_list();

   
  };

  Array_constructorContext* array_constructor();

  class  Method_invocationContext : public antlr4::ParserRuleContext {
  public:
    Method_invocationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Id_Context *> id_();
    Id_Context* id_(size_t i);
    antlr4::tree::TerminalNode *DOT_DOT();
    Expression_list_in_parenthesesContext *expression_list_in_parentheses();

   
  };

  Method_invocationContext* method_invocation();

  class  Olap_specificationContext : public antlr4::ParserRuleContext {
  public:
    Olap_specificationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Ordered_olap_specificationContext *ordered_olap_specification();
    Numbering_specificationContext *numbering_specification();
    Aggregation_specificationContext *aggregation_specification();

   
  };

  Olap_specificationContext* olap_specification();

  class  Ordered_olap_specificationContext : public antlr4::ParserRuleContext {
  public:
    Db2Parser::Id_Context *f = nullptr;
    Ordered_olap_specificationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OVER();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Window_order_clauseContext *window_order_clause();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    Id_Context *id_();
    Window_partition_clauseContext *window_partition_clause();

   
  };

  Ordered_olap_specificationContext* ordered_olap_specification();

  class  Window_partition_clauseContext : public antlr4::ParserRuleContext {
  public:
    Window_partition_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *PARTITION();
    antlr4::tree::TerminalNode *BY();
    Expression_listContext *expression_list();

   
  };

  Window_partition_clauseContext* window_partition_clause();

  class  Window_order_clauseContext : public antlr4::ParserRuleContext {
  public:
    Window_order_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ORDER();
    antlr4::tree::TerminalNode *BY();
    Order_by_clause_optsContext *order_by_clause_opts();

   
  };

  Window_order_clauseContext* window_order_clause();

  class  Numbering_specificationContext : public antlr4::ParserRuleContext {
  public:
    Numbering_specificationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ROW_NUMBER();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *OVER();
    Window_partition_clauseContext *window_partition_clause();
    Window_order_clauseContext *window_order_clause();

   
  };

  Numbering_specificationContext* numbering_specification();

  class  Aggregation_specificationContext : public antlr4::ParserRuleContext {
  public:
    Db2Parser::Id_Context *af = nullptr;
    Aggregation_specificationContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *OVER();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    Olap_aggregate_functionContext *olap_aggregate_function();
    antlr4::tree::TerminalNode *RANGE();
    antlr4::tree::TerminalNode *BETWEEN();
    std::vector<antlr4::tree::TerminalNode *> UNBOUNDED();
    antlr4::tree::TerminalNode* UNBOUNDED(size_t i);
    antlr4::tree::TerminalNode *PRECEDING();
    antlr4::tree::TerminalNode *AND();
    antlr4::tree::TerminalNode *FOLLOWING();
    Window_order_clauseContext *window_order_clause();
    Id_Context *id_();
    Window_partition_clauseContext *window_partition_clause();
    antlr4::tree::TerminalNode *CURRENT();
    antlr4::tree::TerminalNode *ROW();
    Window_aggregation_group_clauseContext *window_aggregation_group_clause();

   
  };

  Aggregation_specificationContext* aggregation_specification();

  class  Olap_aggregate_functionContext : public antlr4::ParserRuleContext {
  public:
    Olap_aggregate_functionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    First_value_functionContext *first_value_function();
    Last_value_functionContext *last_value_function();
    Nth_value_functionContext *nth_value_function();
    Ratio_to_report_functionContext *ratio_to_report_function();

   
  };

  Olap_aggregate_functionContext* olap_aggregate_function();

  class  First_value_functionContext : public antlr4::ParserRuleContext {
  public:
    First_value_functionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FIRST_VALUE();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    ExpressionContext *expression();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *COMMA();
    Ignore_respect_nullsContext *ignore_respect_nulls();

   
  };

  First_value_functionContext* first_value_function();

  class  Last_value_functionContext : public antlr4::ParserRuleContext {
  public:
    Last_value_functionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LAST_VALUE();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    ExpressionContext *expression();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    antlr4::tree::TerminalNode *COMMA();
    Ignore_respect_nullsContext *ignore_respect_nulls();

   
  };

  Last_value_functionContext* last_value_function();

  class  Nth_value_functionContext : public antlr4::ParserRuleContext {
  public:
    Db2Parser::ExpressionContext *nth_row = nullptr;
    Nth_value_functionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NTH_VALUE();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    std::vector<ExpressionContext *> expression();
    ExpressionContext* expression(size_t i);
    antlr4::tree::TerminalNode *COMMA();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();
    Ignore_respect_nullsContext *ignore_respect_nulls();
    From_first_lastContext *from_first_last();

   
  };

  Nth_value_functionContext* nth_value_function();

  class  Ratio_to_report_functionContext : public antlr4::ParserRuleContext {
  public:
    Ratio_to_report_functionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *RATION_TO_REPORT();
    Expression_list_in_parenthesesContext *expression_list_in_parentheses();

   
  };

  Ratio_to_report_functionContext* ratio_to_report_function();

  class  Ignore_respect_nullsContext : public antlr4::ParserRuleContext {
  public:
    Ignore_respect_nullsContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *NULLS();
    antlr4::tree::TerminalNode *IGNORE();
    antlr4::tree::TerminalNode *RESPECT();

   
  };

  Ignore_respect_nullsContext* ignore_respect_nulls();

  class  From_first_lastContext : public antlr4::ParserRuleContext {
  public:
    From_first_lastContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *FROM();
    First_lastContext *first_last();

   
  };

  From_first_lastContext* from_first_last();

  class  Window_aggregation_group_clauseContext : public antlr4::ParserRuleContext {
  public:
    Window_aggregation_group_clauseContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ROWS();
    antlr4::tree::TerminalNode *RANGE();
    Group_startContext *group_start();
    Group_betweenContext *group_between();
    Group_endContext *group_end();

   
  };

  Window_aggregation_group_clauseContext* window_aggregation_group_clause();

  class  Group_startContext : public antlr4::ParserRuleContext {
  public:
    Group_startContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *UNBOUNDED();
    antlr4::tree::TerminalNode *PRECEDING();
    Constant_Context *constant_();

   
  };

  Group_startContext* group_start();

  class  Group_betweenContext : public antlr4::ParserRuleContext {
  public:
    Group_betweenContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *BETWEEN();
    Group_bound1Context *group_bound1();
    antlr4::tree::TerminalNode *AND();
    Group_bound2Context *group_bound2();

   
  };

  Group_betweenContext* group_between();

  class  Group_bound1Context : public antlr4::ParserRuleContext {
  public:
    Group_bound1Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *UNBOUNDED();
    antlr4::tree::TerminalNode *PRECEDING();
    Constant_Context *constant_();
    antlr4::tree::TerminalNode *FOLLOWING();

   
  };

  Group_bound1Context* group_bound1();

  class  Group_bound2Context : public antlr4::ParserRuleContext {
  public:
    Group_bound2Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *UNBOUNDED();
    antlr4::tree::TerminalNode *FOLLOWING();
    Constant_Context *constant_();
    antlr4::tree::TerminalNode *PRECEDING();

   
  };

  Group_bound2Context* group_bound2();

  class  Group_endContext : public antlr4::ParserRuleContext {
  public:
    Group_endContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *UNBOUNDED();
    antlr4::tree::TerminalNode *FOLLOWING();
    Constant_Context *constant_();

   
  };

  Group_endContext* group_end();

  class  Row_change_expressionContext : public antlr4::ParserRuleContext {
  public:
    Row_change_expressionContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ROW();
    antlr4::tree::TerminalNode *CHANGE();
    antlr4::tree::TerminalNode *FOR();
    Table_designatorContext *table_designator();
    antlr4::tree::TerminalNode *TOKEN();
    antlr4::tree::TerminalNode *TIMESTAMP();

   
  };

  Row_change_expressionContext* row_change_expression();

  class  Sequence_referenceContext : public antlr4::ParserRuleContext {
  public:
    Sequence_referenceContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *VALUE();
    antlr4::tree::TerminalNode *FOR();
    Sequence_nameContext *sequence_name();
    antlr4::tree::TerminalNode *NEXT();
    antlr4::tree::TerminalNode *PREVIOUS();

   
  };

  Sequence_referenceContext* sequence_reference();

  class  Subtype_treatmentContext : public antlr4::ParserRuleContext {
  public:
    Subtype_treatmentContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *TREAT();
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    antlr4::tree::TerminalNode *AS();
    Data_typeContext *data_type();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();

   
  };

  Subtype_treatmentContext* subtype_treatment();

  class  Expression_listContext : public antlr4::ParserRuleContext {
  public:
    Expression_listContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<ExpressionContext *> expression();
    ExpressionContext* expression(size_t i);
    std::vector<antlr4::tree::TerminalNode *> COMMA();
    antlr4::tree::TerminalNode* COMMA(size_t i);

   
  };

  Expression_listContext* expression_list();

  class  Expression_list_in_parenthesesContext : public antlr4::ParserRuleContext {
  public:
    Expression_list_in_parenthesesContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *LEFT_RND_BKT();
    Expression_listContext *expression_list();
    antlr4::tree::TerminalNode *RIGHT_RND_BKT();

   
  };

  Expression_list_in_parenthesesContext* expression_list_in_parentheses();

  class  Id_Context : public antlr4::ParserRuleContext {
  public:
    Id_Context(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    antlr4::tree::TerminalNode *ID();
    antlr4::tree::TerminalNode *VALUE();
    antlr4::tree::TerminalNode *K();
    antlr4::tree::TerminalNode *M();
    antlr4::tree::TerminalNode *G();
    antlr4::tree::TerminalNode *DB2SECURITYLABEL();
    antlr4::tree::TerminalNode *SYSPROC();
    antlr4::tree::TerminalNode *UESCAPE();
    antlr4::tree::TerminalNode *C_();
    antlr4::tree::TerminalNode *CLUSTER();
    antlr4::tree::TerminalNode *MODULE();
    antlr4::tree::TerminalNode *DOUBLE_QUOTE_ID();
    antlr4::tree::TerminalNode *ACCTNG();
    antlr4::tree::TerminalNode *USERID();
    antlr4::tree::TerminalNode *WRKSTNNAME();
    antlr4::tree::TerminalNode *TIMEZONE();
    antlr4::tree::TerminalNode *SQLID();
    antlr4::tree::TerminalNode *CURRENT_DATE();
    antlr4::tree::TerminalNode *CURRENT_TIME();
    antlr4::tree::TerminalNode *CURRENT_TIMESTAMP();
    antlr4::tree::TerminalNode *CURRENT_SCHEMA();
    antlr4::tree::TerminalNode *CURRENT_SQLID();
    antlr4::tree::TerminalNode *CURRENT_SERVER();
    antlr4::tree::TerminalNode *CURRENT_TIMEZONE();
    antlr4::tree::TerminalNode *SYSDATE();
    antlr4::tree::TerminalNode *LOCALTIMESTAMP();
    antlr4::tree::TerminalNode *INF();
    antlr4::tree::TerminalNode *INFINITY();
    antlr4::tree::TerminalNode *NAN();
    antlr4::tree::TerminalNode *SNAN();

   
  };

  Id_Context* id_();

  class  Exposed_nameContext : public antlr4::ParserRuleContext {
  public:
    Exposed_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Table_nameContext *table_name();
    View_nameContext *view_name();
    Nick_nameContext *nick_name();
    Correlation_nameContext *correlation_name();

   
  };

  Exposed_nameContext* exposed_name();

  class  NameContext : public antlr4::ParserRuleContext {
  public:
    NameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  NameContext* name();

  class  LabelContext : public antlr4::ParserRuleContext {
  public:
    LabelContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  LabelContext* label();

  class  Host_labelContext : public antlr4::ParserRuleContext {
  public:
    Host_labelContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Host_labelContext* host_label();

  class  Library_nameContext : public antlr4::ParserRuleContext {
  public:
    Library_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Library_nameContext* library_name();

  class  Array_type_nameContext : public antlr4::ParserRuleContext {
  public:
    Array_type_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Id_Context *> id_();
    Id_Context* id_(size_t i);
    std::vector<antlr4::tree::TerminalNode *> DOT();
    antlr4::tree::TerminalNode* DOT(size_t i);

   
  };

  Array_type_nameContext* array_type_name();

  class  Attribute_nameContext : public antlr4::ParserRuleContext {
  public:
    Attribute_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Attribute_nameContext* attribute_name();

  class  Row_type_nameContext : public antlr4::ParserRuleContext {
  public:
    Row_type_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Id_Context *> id_();
    Id_Context* id_(size_t i);
    std::vector<antlr4::tree::TerminalNode *> DOT();
    antlr4::tree::TerminalNode* DOT(size_t i);

   
  };

  Row_type_nameContext* row_type_name();

  class  Authorization_nameContext : public antlr4::ParserRuleContext {
  public:
    Authorization_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Authorization_nameContext* authorization_name();

  class  Boolean_variable_nameContext : public antlr4::ParserRuleContext {
  public:
    Boolean_variable_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Boolean_variable_nameContext* boolean_variable_name();

  class  Array_variable_nameContext : public antlr4::ParserRuleContext {
  public:
    Array_variable_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Array_variable_nameContext* array_variable_name();

  class  Column_nameContext : public antlr4::ParserRuleContext {
  public:
    Column_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Column_nameContext* column_name();

  class  Constraint_nameContext : public antlr4::ParserRuleContext {
  public:
    Constraint_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Constraint_nameContext* constraint_name();

  class  Descriptor_nameContext : public antlr4::ParserRuleContext {
  public:
    Descriptor_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Descriptor_nameContext* descriptor_name();

  class  Distinct_type_nameContext : public antlr4::ParserRuleContext {
  public:
    Distinct_type_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Unqualified_type_nameContext *unqualified_type_name();
    Schema_nameContext *schema_name();
    antlr4::tree::TerminalNode *DOT();

   
  };

  Distinct_type_nameContext* distinct_type_name();

  class  Cursor_nameContext : public antlr4::ParserRuleContext {
  public:
    Cursor_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Cursor_nameContext* cursor_name();

  class  Cursor_type_nameContext : public antlr4::ParserRuleContext {
  public:
    Cursor_type_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    std::vector<Id_Context *> id_();
    Id_Context* id_(size_t i);
    std::vector<antlr4::tree::TerminalNode *> DOT();
    antlr4::tree::TerminalNode* DOT(size_t i);

   
  };

  Cursor_type_nameContext* cursor_type_name();

  class  Condition_nameContext : public antlr4::ParserRuleContext {
  public:
    Condition_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Condition_nameContext* condition_name();

  class  Data_source_nameContext : public antlr4::ParserRuleContext {
  public:
    Data_source_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Data_source_nameContext* data_source_name();

  class  Expression_nameContext : public antlr4::ParserRuleContext {
  public:
    Expression_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Expression_nameContext* expression_name();

  class  Group_nameContext : public antlr4::ParserRuleContext {
  public:
    Group_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Group_nameContext* group_name();

  class  Policy_nameContext : public antlr4::ParserRuleContext {
  public:
    Policy_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Policy_nameContext* policy_name();

  class  Bufferpool_nameContext : public antlr4::ParserRuleContext {
  public:
    Bufferpool_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Bufferpool_nameContext* bufferpool_name();

  class  Db_partition_nameContext : public antlr4::ParserRuleContext {
  public:
    Db_partition_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Db_partition_nameContext* db_partition_name();

  class  Database_nameContext : public antlr4::ParserRuleContext {
  public:
    Database_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Database_nameContext* database_name();

  class  Event_monitor_nameContext : public antlr4::ParserRuleContext {
  public:
    Event_monitor_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Event_monitor_nameContext* event_monitor_name();

  class  Field_nameContext : public antlr4::ParserRuleContext {
  public:
    Field_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Field_nameContext* field_name();

  class  For_loop_nameContext : public antlr4::ParserRuleContext {
  public:
    For_loop_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  For_loop_nameContext* for_loop_name();

  class  Function_nameContext : public antlr4::ParserRuleContext {
  public:
    Function_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();
    Schema_nameContext *schema_name();
    antlr4::tree::TerminalNode *DOT();

   
  };

  Function_nameContext* function_name();

  class  Function_mapping_nameContext : public antlr4::ParserRuleContext {
  public:
    Function_mapping_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Function_mapping_nameContext* function_mapping_name();

  class  Global_variable_nameContext : public antlr4::ParserRuleContext {
  public:
    Global_variable_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Global_variable_nameContext* global_variable_name();

  class  Hierarchy_nameContext : public antlr4::ParserRuleContext {
  public:
    Hierarchy_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Hierarchy_nameContext* hierarchy_name();

  class  Host_variable_nameContext : public antlr4::ParserRuleContext {
  public:
    Host_variable_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Host_variable_nameContext* host_variable_name();

  class  Parameter_markerContext : public antlr4::ParserRuleContext {
  public:
    Parameter_markerContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Parameter_markerContext* parameter_marker();

  class  Template_nameContext : public antlr4::ParserRuleContext {
  public:
    Template_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Template_nameContext* template_name();

  class  Index_nameContext : public antlr4::ParserRuleContext {
  public:
    Index_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Index_nameContext* index_name();

  class  Index_extension_nameContext : public antlr4::ParserRuleContext {
  public:
    Index_extension_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Index_extension_nameContext* index_extension_name();

  class  Input_descriptor_nameContext : public antlr4::ParserRuleContext {
  public:
    Input_descriptor_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Input_descriptor_nameContext* input_descriptor_name();

  class  Mask_nameContext : public antlr4::ParserRuleContext {
  public:
    Mask_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Mask_nameContext* mask_name();

  class  Method_nameContext : public antlr4::ParserRuleContext {
  public:
    Method_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Method_nameContext* method_name();

  class  Model_nameContext : public antlr4::ParserRuleContext {
  public:
    Model_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Model_nameContext* model_name();

  class  Module_nameContext : public antlr4::ParserRuleContext {
  public:
    Module_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();
    Schema_nameContext *schema_name();
    antlr4::tree::TerminalNode *DOT();

   
  };

  Module_nameContext* module_name();

  class  New_ownerContext : public antlr4::ParserRuleContext {
  public:
    New_ownerContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  New_ownerContext* new_owner();

  class  Nick_nameContext : public antlr4::ParserRuleContext {
  public:
    Nick_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Nick_nameContext* nick_name();

  class  Object_nameContext : public antlr4::ParserRuleContext {
  public:
    Object_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Object_nameContext* object_name();

  class  Oid_column_nameContext : public antlr4::ParserRuleContext {
  public:
    Oid_column_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Oid_column_nameContext* oid_column_name();

  class  Optimization_profile_nameContext : public antlr4::ParserRuleContext {
  public:
    Optimization_profile_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Optimization_profile_nameContext* optimization_profile_name();

  class  Package_nameContext : public antlr4::ParserRuleContext {
  public:
    Package_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Package_nameContext* package_name();

  class  Partition_nameContext : public antlr4::ParserRuleContext {
  public:
    Partition_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Partition_nameContext* partition_name();

  class  Path_nameContext : public antlr4::ParserRuleContext {
  public:
    Path_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Path_nameContext* path_name();

  class  Permission_nameContext : public antlr4::ParserRuleContext {
  public:
    Permission_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Permission_nameContext* permission_name();

  class  Pipe_nameContext : public antlr4::ParserRuleContext {
  public:
    Pipe_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Pipe_nameContext* pipe_name();

  class  Procedure_nameContext : public antlr4::ParserRuleContext {
  public:
    Procedure_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();
    Schema_nameContext *schema_name();
    antlr4::tree::TerminalNode *DOT();

   
  };

  Procedure_nameContext* procedure_name();

  class  Result_descriptor_nameContext : public antlr4::ParserRuleContext {
  public:
    Result_descriptor_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Result_descriptor_nameContext* result_descriptor_name();

  class  Role_nameContext : public antlr4::ParserRuleContext {
  public:
    Role_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Role_nameContext* role_name();

  class  Root_table_nameContext : public antlr4::ParserRuleContext {
  public:
    Root_table_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Root_table_nameContext* root_table_name();

  class  Root_view_nameContext : public antlr4::ParserRuleContext {
  public:
    Root_view_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Root_view_nameContext* root_view_name();

  class  Row_variable_nameContext : public antlr4::ParserRuleContext {
  public:
    Row_variable_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Row_variable_nameContext* row_variable_name();

  class  Source_schema_nameContext : public antlr4::ParserRuleContext {
  public:
    Source_schema_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Source_schema_nameContext* source_schema_name();

  class  Source_package_nameContext : public antlr4::ParserRuleContext {
  public:
    Source_package_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Source_package_nameContext* source_package_name();

  class  Source_procedure_nameContext : public antlr4::ParserRuleContext {
  public:
    Source_procedure_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Source_procedure_nameContext* source_procedure_name();

  class  Sql_parameter_nameContext : public antlr4::ParserRuleContext {
  public:
    Sql_parameter_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Sql_parameter_nameContext* sql_parameter_name();

  class  Sql_variable_nameContext : public antlr4::ParserRuleContext {
  public:
    Sql_variable_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Sql_variable_nameContext* sql_variable_name();

  class  Transition_variable_nameContext : public antlr4::ParserRuleContext {
  public:
    Transition_variable_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Transition_variable_nameContext* transition_variable_name();

  class  Savepoint_nameContext : public antlr4::ParserRuleContext {
  public:
    Savepoint_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Savepoint_nameContext* savepoint_name();

  class  Specific_nameContext : public antlr4::ParserRuleContext {
  public:
    Specific_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();
    Schema_nameContext *schema_name();
    antlr4::tree::TerminalNode *DOT();

   
  };

  Specific_nameContext* specific_name();

  class  SchemaContext : public antlr4::ParserRuleContext {
  public:
    SchemaContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Schema_nameContext *schema_name();

   
  };

  SchemaContext* schema();

  class  Schema_nameContext : public antlr4::ParserRuleContext {
  public:
    Schema_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Schema_nameContext* schema_name();

  class  Search_method_nameContext : public antlr4::ParserRuleContext {
  public:
    Search_method_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Search_method_nameContext* search_method_name();

  class  Server_nameContext : public antlr4::ParserRuleContext {
  public:
    Server_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Server_nameContext* server_name();

  class  Server_option_nameContext : public antlr4::ParserRuleContext {
  public:
    Server_option_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Server_option_nameContext* server_option_name();

  class  Session_authorization_nameContext : public antlr4::ParserRuleContext {
  public:
    Session_authorization_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Session_authorization_nameContext* session_authorization_name();

  class  Component_nameContext : public antlr4::ParserRuleContext {
  public:
    Component_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Component_nameContext* component_name();

  class  Sec_label_comp_nameContext : public antlr4::ParserRuleContext {
  public:
    Sec_label_comp_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Sec_label_comp_nameContext* sec_label_comp_name();

  class  Security_policy_nameContext : public antlr4::ParserRuleContext {
  public:
    Security_policy_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Security_policy_nameContext* security_policy_name();

  class  Security_label_nameContext : public antlr4::ParserRuleContext {
  public:
    Security_label_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Security_label_nameContext* security_label_name();

  class  Sequence_nameContext : public antlr4::ParserRuleContext {
  public:
    Sequence_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Sequence_nameContext* sequence_name();

  class  Service_class_nameContext : public antlr4::ParserRuleContext {
  public:
    Service_class_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Service_class_nameContext* service_class_name();

  class  Service_superclass_nameContext : public antlr4::ParserRuleContext {
  public:
    Service_superclass_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Service_superclass_nameContext* service_superclass_name();

  class  Storagegroup_nameContext : public antlr4::ParserRuleContext {
  public:
    Storagegroup_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Storagegroup_nameContext* storagegroup_name();

  class  Supertype_nameContext : public antlr4::ParserRuleContext {
  public:
    Supertype_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Supertype_nameContext* supertype_name();

  class  Superview_nameContext : public antlr4::ParserRuleContext {
  public:
    Superview_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Superview_nameContext* superview_name();

  class  Service_subclass_nameContext : public antlr4::ParserRuleContext {
  public:
    Service_subclass_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Service_subclass_nameContext* service_subclass_name();

  class  Statement_nameContext : public antlr4::ParserRuleContext {
  public:
    Statement_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Statement_nameContext* statement_name();

  class  Table_nameContext : public antlr4::ParserRuleContext {
  public:
    Table_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Table_nameContext* table_name();

  class  Tablespace_nameContext : public antlr4::ParserRuleContext {
  public:
    Tablespace_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Tablespace_nameContext* tablespace_name();

  class  Target_identifierContext : public antlr4::ParserRuleContext {
  public:
    Target_identifierContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Target_identifierContext* target_identifier();

  class  Threshold_nameContext : public antlr4::ParserRuleContext {
  public:
    Threshold_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Threshold_nameContext* threshold_name();

  class  Trigger_nameContext : public antlr4::ParserRuleContext {
  public:
    Trigger_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Trigger_nameContext* trigger_name();

  class  Context_nameContext : public antlr4::ParserRuleContext {
  public:
    Context_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Context_nameContext* context_name();

  class  Usage_list_nameContext : public antlr4::ParserRuleContext {
  public:
    Usage_list_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Usage_list_nameContext* usage_list_name();

  class  Unqualified_type_nameContext : public antlr4::ParserRuleContext {
  public:
    Unqualified_type_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Unqualified_type_nameContext* unqualified_type_name();

  class  Type_nameContext : public antlr4::ParserRuleContext {
  public:
    Type_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Unqualified_type_nameContext *unqualified_type_name();
    Schema_nameContext *schema_name();
    antlr4::tree::TerminalNode *DOT();

   
  };

  Type_nameContext* type_name();

  class  Type_mapping_nameContext : public antlr4::ParserRuleContext {
  public:
    Type_mapping_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Type_mapping_nameContext* type_mapping_name();

  class  Typed_table_nameContext : public antlr4::ParserRuleContext {
  public:
    Typed_table_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Typed_table_nameContext* typed_table_name();

  class  Typed_view_nameContext : public antlr4::ParserRuleContext {
  public:
    Typed_view_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Typed_view_nameContext* typed_view_name();

  class  User_mapping_option_nameContext : public antlr4::ParserRuleContext {
  public:
    User_mapping_option_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  User_mapping_option_nameContext* user_mapping_option_name();

  class  View_nameContext : public antlr4::ParserRuleContext {
  public:
    View_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  View_nameContext* view_name();

  class  Variable_nameContext : public antlr4::ParserRuleContext {
  public:
    Variable_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Variable_nameContext* variable_name();

  class  Work_action_set_nameContext : public antlr4::ParserRuleContext {
  public:
    Work_action_set_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Work_action_set_nameContext* work_action_set_name();

  class  Work_class_set_nameContext : public antlr4::ParserRuleContext {
  public:
    Work_class_set_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Work_class_set_nameContext* work_class_set_name();

  class  Workload_nameContext : public antlr4::ParserRuleContext {
  public:
    Workload_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Workload_nameContext* workload_name();

  class  Work_action_nameContext : public antlr4::ParserRuleContext {
  public:
    Work_action_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Work_action_nameContext* work_action_name();

  class  Work_class_nameContext : public antlr4::ParserRuleContext {
  public:
    Work_class_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Work_class_nameContext* work_class_name();

  class  Wrapper_nameContext : public antlr4::ParserRuleContext {
  public:
    Wrapper_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Wrapper_nameContext* wrapper_name();

  class  Wrapper_option_nameContext : public antlr4::ParserRuleContext {
  public:
    Wrapper_option_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Wrapper_option_nameContext* wrapper_option_name();

  class  Xsrobject_nameContext : public antlr4::ParserRuleContext {
  public:
    Xsrobject_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Xsrobject_nameContext* xsrobject_name();

  class  Parameter_nameContext : public antlr4::ParserRuleContext {
  public:
    Parameter_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Parameter_nameContext* parameter_name();

  class  Cursor_variable_nameContext : public antlr4::ParserRuleContext {
  public:
    Cursor_variable_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Cursor_variable_nameContext* cursor_variable_name();

  class  Alias_nameContext : public antlr4::ParserRuleContext {
  public:
    Alias_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Alias_nameContext* alias_name();

  class  Db_partition_group_nameContext : public antlr4::ParserRuleContext {
  public:
    Db_partition_group_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Db_partition_group_nameContext* db_partition_group_name();

  class  Source_index_nameContext : public antlr4::ParserRuleContext {
  public:
    Source_index_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Source_index_nameContext* source_index_name();

  class  Source_table_nameContext : public antlr4::ParserRuleContext {
  public:
    Source_table_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Source_table_nameContext* source_table_name();

  class  Source_storagegroup_nameContext : public antlr4::ParserRuleContext {
  public:
    Source_storagegroup_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Source_storagegroup_nameContext* source_storagegroup_name();

  class  Target_storagegroup_nameContext : public antlr4::ParserRuleContext {
  public:
    Target_storagegroup_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Target_storagegroup_nameContext* target_storagegroup_name();

  class  Source_tablespace_nameContext : public antlr4::ParserRuleContext {
  public:
    Source_tablespace_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Source_tablespace_nameContext* source_tablespace_name();

  class  Target_tablespace_nameContext : public antlr4::ParserRuleContext {
  public:
    Target_tablespace_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Target_tablespace_nameContext* target_tablespace_name();

  class  Unqualified_function_nameContext : public antlr4::ParserRuleContext {
  public:
    Unqualified_function_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Unqualified_function_nameContext* unqualified_function_name();

  class  Unqualified_procedure_nameContext : public antlr4::ParserRuleContext {
  public:
    Unqualified_procedure_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Unqualified_procedure_nameContext* unqualified_procedure_name();

  class  Unqualified_specific_nameContext : public antlr4::ParserRuleContext {
  public:
    Unqualified_specific_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Unqualified_specific_nameContext* unqualified_specific_name();

  class  Period_nameContext : public antlr4::ParserRuleContext {
  public:
    Period_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Period_nameContext* period_name();

  class  History_table_nameContext : public antlr4::ParserRuleContext {
  public:
    History_table_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  History_table_nameContext* history_table_name();

  class  Xml_schema_nameContext : public antlr4::ParserRuleContext {
  public:
    Xml_schema_nameContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  Xml_schema_nameContext* xml_schema_name();

  class  TodoContext : public antlr4::ParserRuleContext {
  public:
    TodoContext(antlr4::ParserRuleContext *parent, size_t invokingState);
    virtual size_t getRuleIndex() const override;
    Id_Context *id_();

   
  };

  TodoContext* todo();


  bool sempred(antlr4::RuleContext *_localctx, size_t ruleIndex, size_t predicateIndex) override;

  bool search_conditionSempred(Search_conditionContext *_localctx, size_t predicateIndex);
  bool agg_fn_option_listSempred(Agg_fn_option_listContext *_localctx, size_t predicateIndex);
  bool expressionSempred(ExpressionContext *_localctx, size_t predicateIndex);

  // By default the static state used to implement the parser is lazily initialized during the first
  // call to the constructor. You can call this function if you wish to initialize the static state
  // ahead of time.
  static void initialize();

private:
};

}  // namespace graphit::antlr::db2
