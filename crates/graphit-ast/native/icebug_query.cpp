#include "native/icebug_query.hpp"
#include "graphit-ast/src/icebug_query.rs.h"
#include "catalog/catalog.h"
#include "catalog/catalog_entry/rel_group_catalog_entry.h"
#include "catalog/catalog_entry/node_table_catalog_entry.h"
#include "common/file_system/file_system.h"
#include "common/exception/interrupt.h"
#include "common/string_utils.h"
#include "common/types/value/nested.h"
#include "common/types/value/node.h"
#include "common/types/value/rel.h"
#include "common/types/value/value.h"
#include "main/client_context.h"
#include "main/connection.h"
#include "main/database.h"
#include "main/prepared_statement.h"
#include "main/query_result.h"
#include "parser/parser.h"
#include "parser/query/regular_query.h"
#include "processor/result/flat_tuple.h"
#include "processor/operator/persistent/reader/parquet/parquet_reader.h"
#include "storage/table/ice_disk_utils.h"
#include "transaction/transaction.h"
#include "common/json_utils.h"
#include "function/cast/vector_cast_functions.h"
#include "storage/table/external_table_hooks.h"
#include "storage/table/ice_disk_node_table.h"
#include "storage/table/ice_disk_rel_table.h"
#include <charconv>
#include <array>
#include <algorithm>
#include <cmath>
#include <cstdio>
#include <limits>
#include <map>
#include <mutex>
#include <set>
#include <thread>

namespace graphit::icebug {
using namespace lbug::common;
using lbug::main::Connection;
using lbug::main::Database;

namespace {
// Borrowed views of the SDK's parsed yyjson document, never another JSON parser.
struct JsonView {
    yyjson_val* value;
    JsonView at(const char* key) const {
        if (!yyjson_is_obj(value)) throw std::runtime_error("JSON object expected");
        auto field=yyjson_obj_get(value,key);
        if (!field) throw std::runtime_error("JSON field missing");
        return {field};
    }
    JsonView operator[](std::size_t index) const {
        if (!yyjson_is_arr(value) || index>=yyjson_arr_size(value)) throw std::runtime_error("JSON array index invalid");
        return {yyjson_arr_get(value,index)};
    }
    template<class T> T get() const {
        if constexpr (std::is_same_v<T,std::string>) {
            if (!yyjson_is_str(value)) throw std::runtime_error("JSON string expected");
            return std::string(yyjson_get_str(value),yyjson_get_len(value));
        } else if constexpr (std::is_same_v<T,std::uint64_t>) {
            if (!yyjson_is_uint(value)) throw std::runtime_error("JSON unsigned integer expected");
            return yyjson_get_uint(value);
        } else if constexpr (std::is_same_v<T,std::int64_t>) {
            if (!yyjson_is_sint(value)) throw std::runtime_error("JSON signed integer expected");
            return yyjson_get_sint(value);
        } else if constexpr (std::is_same_v<T,double>) {
            if (!yyjson_is_real(value)) throw std::runtime_error("JSON float expected");
            return yyjson_get_real(value);
        } else if constexpr (std::is_same_v<T,bool>) {
            if (!yyjson_is_bool(value)) throw std::runtime_error("JSON boolean expected");
            return yyjson_get_bool(value);
        }
    }
    std::vector<JsonView> array() const {
        if (!yyjson_is_arr(value)) throw std::runtime_error("JSON array expected");
        std::vector<JsonView> children;
        std::size_t index,maximum; yyjson_val* child;
        yyjson_arr_foreach(value,index,maximum,child) children.push_back({child});
        return children;
    }
    std::vector<std::pair<std::string,JsonView>> fields() const {
        if (!yyjson_is_obj(value)) throw std::runtime_error("JSON object expected");
        std::vector<std::pair<std::string,JsonView>> children;
        std::size_t index,maximum; yyjson_val *key,*child;
        yyjson_obj_foreach(value,index,maximum,key,child)
            children.emplace_back(std::string(yyjson_get_str(key),yyjson_get_len(key)),JsonView{child});
        return children;
    }
};

[[noreturn]] void denied() { throw std::runtime_error("query filesystem effect or path is not admitted"); }

struct SourceFile final : FileInfo {
    rust::Box<QueryFile> file;
    mutable std::mutex mutex;
    std::uint64_t cursor = 0;
    SourceFile(std::string path, FileSystem* fs, rust::Box<QueryFile> file)
        : FileInfo(std::move(path), fs), file(std::move(file)) {}
};

// Match every path, so the SDK cannot fall back to its host LocalFileSystem.
class CapturedFs final : public FileSystem {
    rust::Box<GraphIo> io;
    std::map<std::string,std::string> aliases;
    const std::string& resolve(const std::string& path) const {
        auto found=aliases.find(path);
        return found==aliases.end()?path:found->second;
    }
    static SourceFile& source(FileInfo& info) { return dynamic_cast<SourceFile&>(info); }
    static const SourceFile& source(const FileInfo& info) { return dynamic_cast<const SourceFile&>(info); }
public:
    CapturedFs(rust::Box<GraphIo> io,std::map<std::string,std::string> aliases)
        : io(std::move(io)),aliases(std::move(aliases)) {}
    bool canHandleFile(std::string_view) const override { return true; }
    std::unique_ptr<FileInfo> openFile(const std::string& path, FileOpenFlags flags,
        lbug::main::ClientContext*) override {
        if (flags.flags != FileFlags::READ_ONLY || flags.lockType == FileLockType::WRITE_LOCK) denied();
        return std::make_unique<SourceFile>(path, this, io->open(resolve(path)));
    }
    std::vector<std::string> glob(lbug::main::ClientContext*, const std::string& path) const override {
        if (!io->contains(resolve(path))) denied();
        return {path};
    }
    bool fileOrPathExists(const std::string& path, lbug::main::ClientContext*) override {
        if (!io->contains(resolve(path))) denied();
        return true;
    }
    std::string expandPath(lbug::main::ClientContext*, const std::string& path) const override {
        if (!io->contains(resolve(path))) denied();
        return path;
    }
    void overwriteFile(const std::string&, const std::string&) override { denied(); }
    void renameFile(const std::string&, const std::string&) override { denied(); }
    void copyFile(const std::string&, const std::string&) override { denied(); }
    void createDir(const std::string&) const override { denied(); }
    void removeFileIfExists(const std::string&, const lbug::main::ClientContext*) override { denied(); }
    void syncFile(const FileInfo&) const override { denied(); }
protected:
    std::uint64_t getFileSize(const FileInfo& info) const override { return source(info).file->len(); }
    void readFromFile(FileInfo& info, void* bytes, std::uint64_t count, std::uint64_t offset) const override {
        if (count > std::numeric_limits<std::size_t>::max()) throw std::runtime_error("captured read exceeds native size");
        auto size = static_cast<std::size_t>(count);
        if (source(info).file->read_at(offset, rust::Slice<std::uint8_t>(static_cast<std::uint8_t*>(bytes), size)) != size)
            throw std::runtime_error("captured file returned incomplete range");
    }
    std::int64_t readFile(FileInfo& info, void* bytes, std::size_t count) const override {
        auto& value = source(info);
        std::lock_guard lock(value.mutex);
        auto left = value.file->len() - value.cursor;
        auto size = static_cast<std::size_t>(std::min<std::uint64_t>(left, count));
        if (size > INT64_MAX) throw std::runtime_error("captured read exceeds native signed size");
        if (size != 0) readFromFile(info, bytes, size, value.cursor);
        value.cursor += size;
        return static_cast<std::int64_t>(size);
    }
    std::int64_t seek(FileInfo& info, std::uint64_t offset, int whence) const override {
        auto& value = source(info);
        std::lock_guard lock(value.mutex);
        auto next = offset;
        if (whence != SEEK_SET) {
            if (whence != SEEK_CUR && whence != SEEK_END) throw std::runtime_error("invalid captured seek");
            auto base = whence == SEEK_CUR ? value.cursor : value.file->len();
            auto delta = static_cast<std::int64_t>(offset);
            if (delta < 0) {
                auto magnitude = static_cast<std::uint64_t>(-(delta + 1)) + 1;
                if (magnitude > base) throw std::runtime_error("captured seek before start");
                next = base - magnitude;
            } else {
                if (base > UINT64_MAX - offset) throw std::runtime_error("captured seek overflow");
                next = base + offset;
            }
        }
        if (next > value.file->len() || next > INT64_MAX) throw std::runtime_error("captured seek beyond file");
        value.cursor = next;
        return static_cast<std::int64_t>(next);
    }
    void reset(FileInfo& info) override { auto& value=source(info); std::lock_guard lock(value.mutex); value.cursor=0; }
    void writeFile(FileInfo&, const std::uint8_t*, std::uint64_t, std::uint64_t) const override { denied(); }
    void truncate(FileInfo&, std::uint64_t) const override { denied(); }
};

struct Observe {
    QuerySignal& signal;
    std::thread thread;
    Observe(QuerySignal& signal, Connection& connection) : signal(signal), thread([&signal,&connection] {
        if (signal.wait_interrupt()) connection.interrupt();
    }) {}
    ~Observe() { signal.finish(); thread.join(); }
};
std::unique_ptr<lbug::main::QueryResult> trusted(Connection& connection, const std::string& query) {
    auto result = connection.query(query);
    if (!result || !result->isSuccess()) throw std::runtime_error(result ? result->getErrorMessage() : "SDK result missing");
    return result;
}
std::string scalar_type(const std::string& type) {
    if (type=="STRING" || type=="INT64" || type=="UINT64" || type=="BOOL") return type;
    throw std::runtime_error("unsupported captured column type: " + type);
}
std::string properties(const std::map<std::string,std::set<std::string>>& columns) {
    std::string result;
    for (const auto& [name,types] : columns) {
        if (!result.empty()) result += ',';
        result += StringUtils::quoteIdentifier(name) + ' ';
        if (types.size()==1) result += scalar_type(*types.begin());
        else {
            result += "UNION(";
            bool first=true;
            for (const auto& type : types) {
                if (!first) result += ',';
                first=false;
                result += StringUtils::quoteIdentifier(type) + ' ' + scalar_type(type);
            }
            result += ')';
        }
    }
    return result;
}
#include "native/icebug_tables.hpp"

// Official extension hooks are process-wide; captured catalogs are selected by
// their StorageManager, never by mutable thread-local or last-query state.
struct CapturedCatalog {
    JsonView schema;
    const std::map<std::string,table_id_t>& nodeIDs;
};
std::mutex catalogsMutex;
std::map<const storage::StorageManager*,CapturedCatalog> catalogs;
CapturedCatalog captured(const storage::StorageManager* manager) {
    std::lock_guard lock(catalogsMutex);
    auto found=catalogs.find(manager);
    if (found==catalogs.end()) throw std::runtime_error("catalog is outside captured Graphit query");
    return found->second;
}
struct BindCatalog {
    const storage::StorageManager* manager;
    BindCatalog(Database& database,JsonView schema,const std::map<std::string,table_id_t>& ids)
        : manager(database.getStorageManager()) {
        static std::once_flag initialized;
        std::call_once(initialized,[]{
            storage::ExternalTableHooks hooks;
            hooks.createNodeTable=[](auto manager,auto entry,auto memory,auto)->std::unique_ptr<storage::Table>{
                auto view=captured(manager);
                for(auto node:view.schema.at("nodes").array()) if(node.at("label").template get<std::string>()==entry->getName())
                    return std::make_unique<ShardNodes>(manager,entry,memory,node);
                throw std::runtime_error("node table is outside captured schema");
            };
            hooks.createRelTable=[](auto entry,auto from,auto to,auto manager,auto memory,auto context)->std::unique_ptr<storage::Table>{
                auto view=captured(manager);std::vector<JsonView> members;
                for(auto member:view.schema.at("members").array())
                    if(member.at("kind").template get<std::string>()==entry->getName() &&
                        view.nodeIDs.at(member.at("from").template get<std::string>())==from &&
                        view.nodeIDs.at(member.at("to").template get<std::string>())==to) members.push_back(member);
                if(members.empty()) throw std::runtime_error("relationship is outside captured schema");
                return std::make_unique<ShardRels>(entry,from,to,manager,memory,context,members);
            };
            hooks.createNodeScanState=[](auto,auto ids,const auto& outputs,auto state,auto memory)->std::unique_ptr<storage::TableScanState>{
                return std::make_unique<ShardNodeState>(*memory,ids,outputs,state);
            };
            hooks.getNumScanMorsels=[](auto tx,auto table){return dynamic_cast<ShardNodes&>(*table).numMorsels(tx);};
            hooks.createRelScanState=[](auto,auto ids,const auto& outputs,auto state,auto memory)->std::unique_ptr<storage::RelTableScanState>{
                return std::make_unique<ShardRelState>(*memory,ids,outputs,state);
            };
            hooks.scanStateRank=3;
            storage::ExternalTableHookRegistry::registerHooks(StorageFormat::LANCE,std::move(hooks));
        });
        std::lock_guard lock(catalogsMutex);catalogs.emplace(manager,CapturedCatalog{schema,ids});
    }
    ~BindCatalog(){std::lock_guard lock(catalogsMutex);catalogs.erase(manager);}
};

struct Node { std::uint64_t table, rows; };
struct Part { std::uint64_t member, start, rows; };
struct Ids {
    std::array<std::uint8_t,32> generation;
    std::map<table_id_t,Node> nodes;
    std::map<table_id_t,std::vector<Part>> rels;
    void emit(ResultSink& sink, const std::string& field, internalID_t value) const {
        auto node=nodes.find(value.tableID);
        if (node!=nodes.end() && value.offset<node->second.rows) {
            sink.identity(field,rust::Slice<const std::uint8_t>(generation.data(),generation.size()),node->second.table,value.offset,false); return;
        }
        auto rel=rels.find(value.tableID);
        if (rel!=rels.end()) for (const auto& part:rel->second)
            if (value.offset>=part.start && value.offset-part.start<part.rows) {
                sink.identity(field,rust::Slice<const std::uint8_t>(generation.data(),generation.size()),part.member,value.offset-part.start,true); return;
            }
        throw std::runtime_error("native identity is outside captured graph");
    }
};

void atom(ResultSink& sink, const std::string& type, const std::string& field, AtomKind kind,
    const std::string& text, std::size_t maxBytes) {
    if (text.size()>maxBytes || type.size()>maxBytes) throw std::runtime_error("query cell exceeds response bytes");
    // rust::String validates UTF-8 before the borrowed Rust string crosses the bridge.
    rust::String checkedType(type),checkedField(field),checkedText(text);
    sink.atom(checkedType,checkedField,kind,checkedText);
}
const Value& checked_value(const Value* value) {
    if (!value) throw std::runtime_error("native result value is missing");
    return *value;
}
void cell(ResultSink& sink,const Value& value,const Ids& ids,const std::string& field,
    std::size_t depth,std::size_t maxDepth,std::size_t maxBytes) {
    if (depth>maxDepth) throw std::runtime_error("query value nesting exceeds response depth");
    auto type=value.getDataType().toString();
    auto text=[&](const std::string& data) { atom(sink,type,field,AtomKind::Text,data,maxBytes); };
    if (value.isNull()) { atom(sink,type,field,AtomKind::Null,"",maxBytes); return; }
    auto id=value.getDataType().getLogicalTypeID();
    switch(id) {
    case LogicalTypeID::BOOL: atom(sink,type,field,AtomKind::Boolean,value.getValue<bool>()?"true":"false",maxBytes); break;
    case LogicalTypeID::INT8: text(std::to_string(value.getValue<std::int8_t>())); break;
    case LogicalTypeID::INT16: text(std::to_string(value.getValue<std::int16_t>())); break;
    case LogicalTypeID::INT32: text(std::to_string(value.getValue<std::int32_t>())); break;
    case LogicalTypeID::INT64: text(std::to_string(value.getValue<std::int64_t>())); break;
    case LogicalTypeID::UINT8: text(std::to_string(value.getValue<std::uint8_t>())); break;
    case LogicalTypeID::UINT16: text(std::to_string(value.getValue<std::uint16_t>())); break;
    case LogicalTypeID::UINT32: text(std::to_string(value.getValue<std::uint32_t>())); break;
    case LogicalTypeID::UINT64: text(std::to_string(value.getValue<std::uint64_t>())); break;
    case LogicalTypeID::INT128: case LogicalTypeID::DECIMAL: case LogicalTypeID::UUID: case LogicalTypeID::BLOB: text(value.toString()); break;
    case LogicalTypeID::STRING: text(value.getValue<std::string>()); break;
    case LogicalTypeID::FLOAT: case LogicalTypeID::DOUBLE: {
        auto number=id==LogicalTypeID::FLOAT?static_cast<double>(value.getValue<float>()):value.getValue<double>();
        if (!std::isfinite(number)) text(std::isnan(number)?"NaN":number>0?"Infinity":"-Infinity");
        else {
            char digits[128];
            auto encoded=std::to_chars(digits,digits+sizeof(digits),number,std::chars_format::general,std::numeric_limits<double>::max_digits10);
            if (encoded.ec!=std::errc{}) throw std::runtime_error("native float conversion failed");
            atom(sink,type,field,AtomKind::Number,std::string(digits,encoded.ptr),maxBytes);
        }
        break;
    }
    case LogicalTypeID::DATE: text(std::to_string(value.getValue<date_t>().days)); break;
    case LogicalTypeID::TIMESTAMP: text(std::to_string(value.getValue<timestamp_t>().value)); break;
    case LogicalTypeID::TIMESTAMP_NS: text(std::to_string(value.getValue<timestamp_ns_t>().value)); break;
    case LogicalTypeID::TIMESTAMP_MS: text(std::to_string(value.getValue<timestamp_ms_t>().value)); break;
    case LogicalTypeID::TIMESTAMP_SEC: text(std::to_string(value.getValue<timestamp_sec_t>().value)); break;
    case LogicalTypeID::TIMESTAMP_TZ: text(std::to_string(value.getValue<timestamp_tz_t>().value)); break;
    case LogicalTypeID::INTERVAL: {
        auto interval=value.getValue<interval_t>();
        sink.begin_cell(type,field,true);
        atom(sink,"INT32","months",AtomKind::Text,std::to_string(interval.months),maxBytes);
        atom(sink,"INT32","days",AtomKind::Text,std::to_string(interval.days),maxBytes);
        atom(sink,"INT64","micros",AtomKind::Text,std::to_string(interval.micros),maxBytes);
        sink.end_cell(); break;
    }
    case LogicalTypeID::INTERNAL_ID: ids.emit(sink,field,value.getValue<internalID_t>()); break;
    case LogicalTypeID::NODE: case LogicalTypeID::REL: {
        bool node=id==LogicalTypeID::NODE;
        auto identity=node?NodeVal::getNodeIDVal(&value):RelVal::getIDVal(&value);
        if (!identity || identity->isNull() || identity->getDataType().getLogicalTypeID()!=LogicalTypeID::INTERNAL_ID)
            throw std::runtime_error("native entity has invalid identity");
        sink.begin_cell(type,field,true);
        ids.emit(sink,"id",identity->getValue<internalID_t>());
        if (!node) {
            cell(sink,checked_value(RelVal::getSrcNodeIDVal(&value)),ids,"from",depth+1,maxDepth,maxBytes);
            cell(sink,checked_value(RelVal::getDstNodeIDVal(&value)),ids,"to",depth+1,maxDepth,maxBytes);
        }
        cell(sink,checked_value(node?NodeVal::getLabelVal(&value):RelVal::getLabelVal(&value)),ids,"label",depth+1,maxDepth,maxBytes);
        sink.begin_cell("PROPERTIES","properties",true);
        auto count=node?NodeVal::getNumProperties(&value):RelVal::getNumProperties(&value);
        for (std::uint64_t i=0;i<count;++i) {
            auto name=node?NodeVal::getPropertyName(&value,i):RelVal::getPropertyName(&value,i);
            auto child=node?NodeVal::getPropertyVal(&value,i):RelVal::getPropertyVal(&value,i);
            cell(sink,checked_value(child),ids,name,depth+1,maxDepth,maxBytes);
        }
        sink.end_cell(); sink.end_cell(); break;
    }
    case LogicalTypeID::STRUCT: case LogicalTypeID::RECURSIVE_REL: case LogicalTypeID::LIST:
    case LogicalTypeID::ARRAY: case LogicalTypeID::MAP: case LogicalTypeID::UNION: {
        auto object=id==LogicalTypeID::STRUCT || id==LogicalTypeID::RECURSIVE_REL;
        sink.begin_cell(type,field,object);
        auto count=NestedVal::getChildrenSize(&value);
        for (std::uint32_t i=0;i<count;++i) {
            auto name=object?StructType::getFields(value.getDataType()).at(i).getName():std::string{};
            cell(sink,checked_value(NestedVal::getChildVal(&value,i)),ids,name,depth+1,maxDepth,maxBytes);
        }
        sink.end_cell(); break;
    }
    default: throw std::runtime_error("unsupported native result type: "+type);
    }
}

Value parameter(JsonView input, std::size_t depth, std::size_t maxDepth) {
    if (depth>maxDepth) throw std::runtime_error("query parameter nesting exceeds input depth");
    if (yyjson_is_null(input.value)) return Value::createNullValue();
    if (yyjson_is_bool(input.value)) return Value(input.get<bool>());
    if (yyjson_is_uint(input.value)) {
        auto integer=input.get<std::uint64_t>();
        if (integer<=static_cast<std::uint64_t>(INT64_MAX)) return Value(static_cast<std::int64_t>(integer));
        return Value(integer);
    }
    if (yyjson_is_sint(input.value)) return Value(input.get<std::int64_t>());
    if (yyjson_is_real(input.value)) return Value(input.get<double>());
    if (yyjson_is_str(input.value)) return Value(input.get<std::string>());
    std::vector<std::unique_ptr<Value>> children;
    if (yyjson_is_arr(input.value)) {
        auto type=LogicalType::ANY();
        for (auto item:input.array()) {
            auto child=parameter(item,depth+1,maxDepth);
            if (type.getLogicalTypeID()==LogicalTypeID::ANY && !child.isNull()) type=child.getDataType().copy();
            if (!child.isNull() && child.getDataType()!=type) throw std::runtime_error("mixed parameter list types are unsupported");
            children.push_back(std::make_unique<Value>(std::move(child)));
        }
        return Value(LogicalType::LIST(std::move(type)),std::move(children));
    }
    if (yyjson_is_obj(input.value)) {
        std::vector<StructField> fields;
        for (const auto& [name,item]:input.fields()) {
            auto child=parameter(item,depth+1,maxDepth);
            fields.emplace_back(name,child.getDataType().copy());
            children.push_back(std::make_unique<Value>(std::move(child)));
        }
        return Value(LogicalType::STRUCT(std::move(fields)),std::move(children));
    }
    throw std::runtime_error("unsupported query parameter");
}
} // namespace

void validate_live_query(rust::Str query) {
    auto statements=lbug::parser::Parser::parseQuery(std::string_view(query.data(),query.size()));
    if (statements.size()!=1 || statements[0]->getStatementType()!=StatementType::QUERY)
        throw std::runtime_error("one read-only query statement is required");
    const auto& regular=statements[0]->constCast<lbug::parser::RegularQuery>();
    auto checked=[](const auto* part) {
        if (part->getNumUpdatingClauses()!=0) throw std::runtime_error("graph writes are not permitted");
        for (std::uint32_t i=0;i<part->getNumReadingClauses();++i) {
            auto type=part->getReadingClause(i)->getClauseType();
            if (type!=ClauseType::MATCH && type!=ClauseType::UNWIND)
                throw std::runtime_error("external data and procedure calls are not permitted");
        }
    };
    for (std::uint64_t i=0;i<regular.getNumSingleQueries();++i) {
        auto single=regular.getSingleQuery(i);
        checked(single);
        for (std::uint32_t part=0;part<single->getNumQueryParts();++part) checked(single->getQueryPart(part));
    }
}

void run_query(rust::Box<GraphIo> io, rust::Box<QuerySignal> signal, rust::Str schemaText,
    rust::Str queryText, rust::Str parameterText, const QueryOptions& options, ResultSink& output) {
    const auto maxRows=options.max_rows;
    const auto maxBytes=options.max_bytes;
    const auto maxDepth=options.max_depth;
    signal->check();
    validate_live_query(queryText);
    auto schemaDoc=lbug::json_extension::stringToJson(std::string(schemaText));
    JsonView schema{yyjson_doc_get_root(schemaDoc.ptr)};
    auto statements=lbug::parser::Parser::parseQuery(std::string_view(queryText.data(),queryText.size()));
    if (statements.size()!=1 || statements[0]->getStatementType()!=StatementType::QUERY)
        throw std::runtime_error("one read-only query statement is required");
    std::map<std::string,table_id_t> nodeIDs;
    std::map<std::string,std::vector<JsonView>> groups;
    Ids ids;
    auto generation=schema.at("generation").array();
    if (generation.size()!=ids.generation.size()) throw std::runtime_error("invalid graph generation");
    for (std::size_t i=0;i<generation.size();++i) {
        auto byte=generation[i].get<std::uint64_t>();
        if (byte>UINT8_MAX) throw std::runtime_error("invalid graph generation byte");
        ids.generation[i]=static_cast<std::uint8_t>(byte);
    }
    lbug::main::SystemConfig config(options.buffer_pool_bytes,options.query_threads);
    // Icebug reads its keys from captured Parquets, not empty internal PK indexes.
    config.enableDefaultHashIndex=false;
    Database database(":memory:",config);
    std::map<std::string,std::string> aliases;
    for(auto member:schema.at("members").array()) {
        auto root="graphit-part://"+std::to_string(member.at("member").get<uint64_t>())+"/";
        auto kind=member.at("kind").get<std::string>();
        aliases.emplace(root+"indices_"+kind+".parquet",member.at("indices").get<std::string>());
        aliases.emplace(root+"indptr_"+kind+".parquet",member.at("indptr").get<std::string>());
    }
    database.registerFileSystem(std::make_unique<CapturedFs>(std::move(io),std::move(aliases)));
    BindCatalog binding(database,schema,nodeIDs);
    Connection connection(&database);
    Observe observe(*signal,connection);
    for (const auto& node:schema.at("nodes").array()) {
        signal->check();
        std::map<std::string,std::set<std::string>> columns;
        for (const auto& column:node.at("columns").array()) columns[column[0].get<std::string>()].insert(column[1].get<std::string>());
        auto ddl="CREATE NODE TABLE "+StringUtils::quoteIdentifier(node.at("label").get<std::string>())+
            '('+properties(columns)+",PRIMARY KEY("+StringUtils::quoteIdentifier(node.at("primary_key").get<std::string>())+
            ")) WITH(storage='"+std::string("graphit-captured://logical.parquet")+"',format='lance')";
        trusted(connection,ddl);
    }
    trusted(connection,"BEGIN TRANSACTION READ ONLY");
    auto transaction=lbug::transaction::Transaction::Get(*connection.getClientContext());
    for (const auto& node:schema.at("nodes").array()) {
        auto label=node.at("label").get<std::string>();
        auto table=database.getCatalog()->getTableCatalogEntry(transaction,label)->getTableID();
        nodeIDs.emplace(label,table);
        ids.nodes.emplace(table,Node{node.at("table").get<std::uint64_t>(),node.at("rows").get<std::uint64_t>()});
    }
    trusted(connection,"COMMIT");
    for (const auto& member:schema.at("members").array()) groups[member.at("kind").get<std::string>()].push_back(member);
    for (const auto& [kind,members]:groups) {
        signal->check();
        std::set<std::pair<std::string,std::string>> pairs;
        std::map<std::string,std::set<std::string>> columns;
        for (const auto& member:members) {
            pairs.emplace(member.at("from").get<std::string>(),member.at("to").get<std::string>());
            for (const auto& column:member.at("properties").array()) columns[column[0].get<std::string>()].insert(column[1].get<std::string>());
        }
        auto ddl="CREATE REL TABLE GROUP "+StringUtils::quoteIdentifier(kind)+'(';
        bool first=true;
        for (const auto& [from,to]:pairs) {
            if (!first) ddl+=',';
            first=false;
            ddl+="FROM "+StringUtils::quoteIdentifier(from)+" TO "+StringUtils::quoteIdentifier(to);
        }
        if (!columns.empty()) ddl+=','+properties(columns);
        ddl+=") WITH(storage='graphit-captured://logical',format='lance')";
        trusted(connection,ddl);
    }
    trusted(connection,"BEGIN TRANSACTION READ ONLY");
    transaction=lbug::transaction::Transaction::Get(*connection.getClientContext());
    for (const auto& [kind,members]:groups) {
        auto entry=dynamic_cast<lbug::catalog::RelGroupCatalogEntry*>(database.getCatalog()->getTableCatalogEntry(transaction,kind));
        if (!entry) throw std::runtime_error("native relationship catalogue mismatch");
        for (const auto& pair:entry->getRelEntryInfos()) {
            std::uint64_t start=0;
            for (const auto& member:members) if (nodeIDs.at(member.at("from").get<std::string>())==pair.nodePair.srcTableID &&
                nodeIDs.at(member.at("to").get<std::string>())==pair.nodePair.dstTableID) {
                auto count=member.at("rows").get<std::uint64_t>();
                if (start>UINT64_MAX-count) throw std::runtime_error("native relationship identity overflows");
                ids.rels[pair.oid].push_back(Part{member.at("member").get<std::uint64_t>(),start,count});
                start+=count;
            }
        }
    }
    auto inputDoc=lbug::json_extension::stringToJson(std::string(parameterText));
    JsonView inputs{yyjson_doc_get_root(inputDoc.ptr)};
    if (!yyjson_is_obj(inputs.value)) throw std::runtime_error("query parameters must be an object");
    std::unordered_map<std::string,std::unique_ptr<Value>> parameters;
    for (const auto& [name,input]:inputs.fields()) {
        auto value=parameter(input,0,maxDepth);
        parameters.emplace(name,std::make_unique<Value>(std::move(value)));
    }
    signal->check();
    connection.setQueryTimeOut(signal->remaining_ms());
    auto prepared=connection.prepareWithParams(std::string_view(queryText.data(),queryText.size()),std::move(parameters));
    if (!prepared || !prepared->isSuccess()) throw std::runtime_error(prepared ? prepared->getErrorMessage() : "SDK prepared statement missing");
    if (!prepared->isReadOnly()) throw std::runtime_error("query writes are not admitted");
    signal->check();
    connection.setQueryTimeOut(signal->remaining_ms());
    auto result=connection.execute(prepared.get());
    if (!result || !result->isSuccess()) throw std::runtime_error(result ? result->getErrorMessage() : "SDK query result missing");
    if (result->getNumTuples()>maxRows) throw std::runtime_error("query result exceeds response rows");
    rust::Vec<rust::String> names;
    for (const auto& name:result->getColumnNames()) names.push_back(rust::String(name));
    output.columns(std::move(names));
    while (result->hasNext()) {
        signal->check();
        auto tuple=result->getNext();
        if (!tuple || tuple->len()!=result->getNumColumns())
            throw std::runtime_error("native row/column shape mismatch");
        output.begin_row();
        for (std::uint64_t i=0;i<result->getNumColumns();++i)
            cell(output,checked_value(tuple->getValue(i)),ids,"",0,maxDepth,maxBytes);
        output.end_row();
    }
    signal->check();
    trusted(connection,"COMMIT");
}
} // namespace graphit::icebug
