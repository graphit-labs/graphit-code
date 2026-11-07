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
#include "parser/query/reading_clause/match_clause.h"
#include "parser/expression/parsed_literal_expression.h"
#include "parser/expression/parsed_parameter_expression.h"
#include "parser/expression/parsed_property_expression.h"
#include "parser/expression/parsed_variable_expression.h"
#include "processor/result/flat_tuple.h"
#include "processor/operator/persistent/reader/parquet/parquet_reader.h"
#include "storage/table/ice_disk_utils.h"
#include "transaction/transaction.h"
#include "common/json_utils.h"
#include "function/cast/vector_cast_functions.h"
#include "storage/table/external_table_hooks.h"
#include "storage/storage_manager.h"
#include "storage/table/ice_disk_node_table.h"
#include "storage/table/ice_disk_rel_table.h"
#include "transport/TBufferTransports.h"
#include <charconv>
#include <array>
#include <algorithm>
#include <cmath>
#include <cstring>
#include <limits>
#include <map>
#include <optional>
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
    struct Footer {
        std::uint64_t start;
        std::string bytes;
    };
    std::shared_ptr<const Footer> footer;
    mutable std::mutex mutex;
    std::uint64_t cursor = 0;
    SourceFile(std::string path, FileSystem* fs, rust::Box<QueryFile> file,
        std::shared_ptr<const Footer> footer = {})
        : FileInfo(std::move(path), fs), file(std::move(file)),footer(std::move(footer)) {}
    std::uint64_t length() const { return footer ? footer->start+footer->bytes.size() : file->len(); }
};

struct FileSlice {
    std::string path;
    std::uint64_t offset, rows;
};
std::string nodeSlicePath(const NodeMount& node,std::size_t part) {
    return "graphit-node://"+std::to_string(node.table)+"/"+std::to_string(part)+".parquet";
}
std::map<std::string,FileSlice> fileSlices(const GraphMount& mount) {
    std::map<std::string,FileSlice> slices;
    for(const auto& node:mount.nodes) {
        std::size_t part=0;
        for(const auto& shard:node.shards) slices.emplace(nodeSlicePath(node,part++),
            FileSlice{std::string(shard.file),shard.row_offset,shard.rows});
    }
    for(const auto& member:mount.members) {
        auto root="graphit-part://"+std::to_string(member.member)+"/";
        auto kind=std::string(member.kind);
        slices.emplace(root+"indices_"+kind+".parquet",FileSlice{
            std::string(member.indices),member.indices_row_offset,member.rows});
        if(member.source_rows==UINT64_MAX) throw std::runtime_error("captured CSR pointer count overflow");
        slices.emplace(root+"indptr_"+kind+".parquet",FileSlice{
            std::string(member.indptr),member.indptr_row_offset,member.source_rows+1});
    }
    return slices;
}

// Match every path, so the SDK cannot fall back to its host LocalFileSystem.
class CapturedFs final : public FileSystem {
    mutable std::mutex operationsMutex;
    struct ActiveIo {
        const GraphIo* source;
        std::set<const void*> tables;
        std::set<std::string> files;
    };
    std::map<const lbug::main::ClientContext*,ActiveIo> operations;
    rust::Box<QueryFile> open(lbug::main::ClientContext* context,const std::string& path) {
        std::lock_guard lock(operationsMutex);
        auto found=operations.find(context);
        if(found==operations.end()) denied();
        const auto& relative=resolve(path);
        auto file=found->second.source->open(relative);
        found->second.files.insert(relative);
        return file;
    }
    const GraphIo& io(const lbug::main::ClientContext* context) const {
        std::lock_guard lock(operationsMutex);
        auto found=operations.find(context);
        if(found==operations.end()) denied();
        return *found->second.source;
    }
    std::map<std::string,FileSlice> slices;
    struct Metadata {
        lbug_parquet::format::FileMetaData value;
        std::uint64_t footerStart;
    };
    std::mutex metadataMutex;
    std::map<std::string,Metadata> metadata;
    std::map<std::string,std::shared_ptr<const SourceFile::Footer>> footers;
    const std::string& resolve(const std::string& path) const {
        auto found=slices.find(path);
        return found==slices.end()?path:found->second.path;
    }
    std::shared_ptr<const SourceFile::Footer> project(const std::string& path,
        const FileSlice& slice,lbug::main::ClientContext* context) {
        // Official SDK metadata and Thrift only. Page locations remain physical;
        // no graph values or compressed pages are copied or decoded here.
        std::lock_guard lock(metadataMutex);
        if (auto found=footers.find(path);found!=footers.end()) return found->second;
        auto found=metadata.find(slice.path);
        if (found==metadata.end()) {
            lbug::processor::ParquetReader reader(slice.path,{},context);
            auto file=open(context,slice.path);
            auto length=file->len();
            if(length<12) throw std::runtime_error("captured Parquet is too short");
            std::array<uint8_t,8> trailer;
            file->read_at(length-8,rust::Slice<uint8_t>(trailer.data(),trailer.size()));
            uint64_t size=uint64_t(trailer[0])|(uint64_t(trailer[1])<<8)|
                (uint64_t(trailer[2])<<16)|(uint64_t(trailer[3])<<24);
            if(size>length-12 || std::memcmp(trailer.data()+4,"PAR1",4)!=0)
                throw std::runtime_error("captured Parquet footer span invalid");
            found=metadata.emplace(slice.path,Metadata{*reader.getMetadata(),length-8-size}).first;
        }
        const auto& source=found->second.value;
        if(source.num_rows<0 || slice.offset>static_cast<uint64_t>(source.num_rows) ||
            slice.rows>static_cast<uint64_t>(source.num_rows)-slice.offset ||
            source.__isset.encryption_algorithm || source.__isset.footer_signing_key_metadata)
            throw std::runtime_error("captured Parquet slice bounds or encryption invalid");
        lbug_parquet::format::FileMetaData projected;
        projected.version=source.version;
        projected.schema=source.schema;
        projected.key_value_metadata=source.key_value_metadata;
        projected.created_by=source.created_by;
        projected.column_orders=source.column_orders;
        projected.__isset=source.__isset;
        projected.num_rows=static_cast<int64_t>(slice.rows);
        auto end=slice.offset+slice.rows;
        uint64_t row=0;
        bool beginAligned=slice.offset==0,endAligned=end==0;
        for(const auto& group:source.row_groups) {
            if(group.num_rows<0 || static_cast<uint64_t>(group.num_rows)>UINT64_MAX-row)
                throw std::runtime_error("captured Parquet group rows invalid");
            auto next=row+static_cast<uint64_t>(group.num_rows);
            if(row==slice.offset) beginAligned=true;
            if(next==end) endAligned=true;
            if(row>=slice.offset && next<=end && group.num_rows>0) {
                for(const auto& column:group.columns)
                    if(column.__isset.crypto_metadata || column.__isset.encrypted_column_metadata)
                        throw std::runtime_error("captured Parquet encrypted column invalid");
                projected.row_groups.push_back(group);
            }
            row=next;
        }
        if(slice.rows==0 && slice.offset==row) beginAligned=endAligned=true;
        if(!beginAligned || !endAligned || row!=static_cast<uint64_t>(source.num_rows))
            throw std::runtime_error("captured Parquet slice must align with complete row groups");
        auto buffer=std::make_shared<lbug_apache::thrift::transport::TMemoryBuffer>();
        lbug_apache::thrift::protocol::TCompactProtocol protocol(buffer);
        projected.write(&protocol);
        auto bytes=buffer->getBufferAsString();
        if(bytes.size()>UINT32_MAX) throw std::runtime_error("captured Parquet footer too large");
        auto size=static_cast<uint32_t>(bytes.size());
        for(unsigned shift=0;shift<32;shift+=8) bytes.push_back(static_cast<char>(size>>shift));
        bytes.append("PAR1",4);
        auto footer=std::make_shared<SourceFile::Footer>(SourceFile::Footer{found->second.footerStart,std::move(bytes)});
        if(footer->start>UINT64_MAX-footer->bytes.size()) throw std::runtime_error("captured Parquet virtual size overflow");
        footers.emplace(path,footer);
        return footer;
    }
    static SourceFile& source(FileInfo& info) { return dynamic_cast<SourceFile&>(info); }
    static const SourceFile& source(const FileInfo& info) { return dynamic_cast<const SourceFile&>(info); }
public:
    explicit CapturedFs(std::map<std::string,FileSlice> slices)
        : slices(std::move(slices)) {}
    void reset(std::map<std::string,FileSlice> next) {
        std::lock_guard operationsLock(operationsMutex);
        if(!operations.empty()) throw std::runtime_error("cannot rebind an active query catalogue");
        std::lock_guard metadataLock(metadataMutex);
        slices=std::move(next); metadata.clear(); footers.clear();
    }
    void bind(const lbug::main::ClientContext* context,const GraphIo& source) {
        std::lock_guard lock(operationsMutex);
        if(!operations.emplace(context,ActiveIo{&source,{},{}}).second)
            throw std::runtime_error("query IO already bound");
    }
    template<class Paths>
    void validateTable(const lbug::main::ClientContext* context,const void* table,Paths paths) {
        std::lock_guard lock(operationsMutex);
        auto found=operations.find(context);
        if(found==operations.end()) denied();
        auto& active=found->second;
        if(active.tables.contains(table)) return;
        paths([&](const std::string& path) {
            const auto& relative=resolve(path);
            if (!active.files.contains(relative)) {
                active.source->open(relative)->revalidate();
                active.files.insert(relative);
            }
        });
        active.tables.insert(table);
    }
    void validateUsed(const lbug::main::ClientContext* context) {
        std::lock_guard lock(operationsMutex);
        auto found=operations.find(context);
        if(found==operations.end()) denied();
        for(const auto& relative:found->second.files)
            found->second.source->open(relative)->revalidate();
    }
    void unbind(const lbug::main::ClientContext* context) {
        bool empty;
        {
            std::lock_guard lock(operationsMutex);
            operations.erase(context);
            empty=operations.empty();
        }
        if(empty) {
            // Keep only logical engine state between operations, not captured
            // Parquet footer values or projected buffers from the last request.
            std::lock_guard lock(metadataMutex);
            metadata.clear(); footers.clear();
        }
    }
    bool canHandleFile(std::string_view) const override { return true; }
    std::unique_ptr<FileInfo> openFile(const std::string& path, FileOpenFlags flags,
        lbug::main::ClientContext* context) override {
        if (flags.flags != FileFlags::READ_ONLY || flags.lockType == FileLockType::WRITE_LOCK) denied();
        auto file=open(context,path);
        std::shared_ptr<const SourceFile::Footer> footer;
        if(auto found=slices.find(path);found!=slices.end()) footer=project(path,found->second,context);
        return std::make_unique<SourceFile>(path, this, std::move(file),std::move(footer));
    }
    std::vector<std::string> glob(lbug::main::ClientContext* context, const std::string& path) const override {
        if (!io(context).contains(resolve(path))) denied();
        return {path};
    }
    bool fileOrPathExists(const std::string& path, lbug::main::ClientContext* context) override {
        if (!io(context).contains(resolve(path))) denied();
        return true;
    }
    std::string expandPath(lbug::main::ClientContext* context, const std::string& path) const override {
        if (!io(context).contains(resolve(path))) denied();
        return path;
    }
    void overwriteFile(const std::string&, const std::string&) override { denied(); }
    void renameFile(const std::string&, const std::string&) override { denied(); }
    void copyFile(const std::string&, const std::string&) override { denied(); }
    void createDir(const std::string&) const override { denied(); }
    void removeFileIfExists(const std::string&, const lbug::main::ClientContext*) override { denied(); }
    void syncFile(const FileInfo&) const override { denied(); }
protected:
    std::uint64_t getFileSize(const FileInfo& info) const override { source(info).file->validate_binding(); return source(info).length(); }
    void readFromFile(FileInfo& info, void* bytes, std::uint64_t count, std::uint64_t offset) const override {
        if (count > std::numeric_limits<std::size_t>::max()) throw std::runtime_error("captured read exceeds native size");
        auto& value=source(info);
        value.file->validate_binding();
        if(offset>value.length() || count>value.length()-offset) throw std::runtime_error("captured read exceeds slice size");
        if(value.footer) {
            auto body=offset<value.footer->start ? std::min(count,value.footer->start-offset) : 0;
            if(body && value.file->read_at(offset,rust::Slice<uint8_t>(static_cast<uint8_t*>(bytes),body))!=body)
                throw std::runtime_error("captured file returned incomplete range");
            if(count>body) std::memcpy(static_cast<uint8_t*>(bytes)+body,
                value.footer->bytes.data()+(offset+body-value.footer->start),count-body);
            value.file->validate_binding();
            return;
        }
        auto size = static_cast<std::size_t>(count);
        if (source(info).file->read_at(offset, rust::Slice<std::uint8_t>(static_cast<std::uint8_t*>(bytes), size)) != size)
            throw std::runtime_error("captured file returned incomplete range");
    }
    std::int64_t readFile(FileInfo& info, void* bytes, std::size_t count) const override {
        auto& value = source(info);
        std::lock_guard lock(value.mutex);
        auto left = value.length() - value.cursor;
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
            auto base = whence == SEEK_CUR ? value.cursor : value.length();
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
        if (next > value.length() || next > INT64_MAX) throw std::runtime_error("captured seek beyond file");
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
void validateNodeCapture(const lbug::storage::StorageManager*,
    const lbug::transaction::Transaction*,const NodeMount&,const void*);
#include "native/icebug_tables.hpp"

// Official extension hooks are process-wide; captured catalogs are selected by
// their StorageManager, never by mutable thread-local or last-query state.
struct CapturedCatalog {
    JsonView schema;
    const std::map<std::string,table_id_t>& nodeIDs;
    CapturedFs* filesystem;
    const GraphMount* mount;
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
    BindCatalog(Database& database,JsonView schema,const std::map<std::string,table_id_t>& ids,CapturedFs* filesystem,const GraphMount& mount)
        : manager(database.getStorageManager()) {
        static std::once_flag initialized;
        std::call_once(initialized,[]{
            storage::ExternalTableHooks hooks;
            hooks.createNodeTable=[](auto manager,auto entry,auto memory,auto)->std::unique_ptr<storage::Table>{
                auto view=captured(manager);
                for(auto node:view.schema.at("nodes").array()) if(node.at("label").template get<std::string>()==entry->getName())
                    return std::make_unique<ShardNodes>(manager,entry,memory,view.mount->nodes.at(node.at("table").template get<uint64_t>()));
                throw std::runtime_error("node table is outside captured schema");
            };
            hooks.createRelTable=[](auto entry,auto from,auto to,auto manager,auto memory,auto)->std::unique_ptr<storage::Table>{
                auto view=captured(manager);std::vector<const MemberMount*> members;
                for(const auto& member:view.mount->members)
                    if(std::string(member.kind)==entry->getName() &&
                        view.nodeIDs.at(std::string(member.from_label))==from &&
                        view.nodeIDs.at(std::string(member.to_label))==to) members.push_back(&member);
                if(members.empty()) throw std::runtime_error("relationship is outside captured schema");
                return std::make_unique<ShardRels>(entry,from,to,manager,memory,members);
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
        std::lock_guard lock(catalogsMutex);catalogs.emplace(manager,CapturedCatalog{schema,ids,filesystem,&mount});
    }
    void rebind(const GraphMount& mount) {
        std::lock_guard lock(catalogsMutex);
        catalogs.at(manager).mount=&mount;
    }
    ~BindCatalog(){std::lock_guard lock(catalogsMutex);catalogs.erase(manager);}
};

void validateNodeCapture(const storage::StorageManager* manager,const Transaction* tx,
    const NodeMount& node,const void* table) {
    captured(manager).filesystem->validateTable(tx->getClientContext(),table,[&](auto check) {
        for(const auto& shard:node.shards) check(std::string(shard.file));
    });
}
void validateRelCapture(const storage::StorageManager* manager,const Transaction* tx,
    const std::vector<RelPart>& parts,const void* table) {
    captured(manager).filesystem->validateTable(tx->getClientContext(),table,[&](auto check) {
        for(const auto& part:parts) {
            auto paths=storage::IceDiskUtils::constructCSRPaths(
                part.entry->getStorage(),part.entry->getName(),".parquet");
            check(paths.indices); check(paths.indptr);
        }
    });
}

struct Node { std::uint64_t table, rows; };
struct Part { std::uint64_t member, start, rows; };
struct Ids {
    std::map<table_id_t,Node> nodes;
    std::map<table_id_t,std::vector<Part>> rels;
    void emit(ResultSink& sink, const std::string& field, internalID_t value) const {
        auto node=nodes.find(value.tableID);
        if (node!=nodes.end() && value.offset<node->second.rows) {
            sink.identity(field,node->second.table,value.offset,false); return;
        }
        auto rel=rels.find(value.tableID);
        if (rel!=rels.end()) for (const auto& part:rel->second)
            if (value.offset>=part.start && value.offset-part.start<part.rows) {
                sink.identity(field,part.member,value.offset-part.start,true); return;
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

static std::shared_ptr<lbug::parser::Statement> parse_live_query(rust::Str query) {
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
    return std::move(statements.front());
}

// Owns structural metadata and the official immutable external-table catalogue.
// No GraphIo, QueryFile, QuerySignal or ClientContext survives an operation.
struct NativeCatalog::Impl {
    std::unique_ptr<lbug::json_extension::JsonWrapper> schemaDoc;
    GraphMount mount;
    // Keep old descriptors through a failed partial rebind until Database drops.
    std::unique_ptr<GraphMount> previousMount;
    std::map<std::string,table_id_t> nodeIDs;
    Ids ids;
    std::unique_ptr<Database> database;
    std::unique_ptr<BindCatalog> binding;
    CapturedFs* filesystem=nullptr;
    Impl(rust::Str schema,GraphMount mount,const QueryOptions& options)
        : schemaDoc(std::make_unique<lbug::json_extension::JsonWrapper>(
            lbug::json_extension::stringToJson(std::string(schema)))),mount(std::move(mount)) {
        lbug::main::SystemConfig config(options.buffer_pool_bytes,options.query_threads);
        config.enableDefaultHashIndex=false;
        database=std::make_unique<Database>(":memory:",config);
    }
};
NativeCatalog::NativeCatalog(rust::Str schema,GraphMount mount,const QueryOptions& options)
    : impl(std::make_unique<Impl>(schema,std::move(mount),options)) {}
NativeCatalog::~NativeCatalog() = default;
namespace {
struct Operation {
    const NativeCatalog& catalogue;
    lbug::main::ClientContext* context;
    rust::Box<GraphIo> io;
    Operation(const NativeCatalog& catalogue,Connection& connection,rust::Box<GraphIo> io)
        : catalogue(catalogue),context(connection.getClientContext()),io(std::move(io)) {
        catalogue.impl->filesystem->bind(context,*this->io);
    }
    ~Operation() {
        for(const auto& [table,identity]:catalogue.impl->ids.nodes) {
            if(auto nodes=dynamic_cast<ShardNodes*>(catalogue.impl->database->getStorageManager()->getTable(table)))
                nodes->release(context);
        }
        for(const auto& [table,identities]:catalogue.impl->ids.rels) {
            if(auto rels=dynamic_cast<ShardRels*>(catalogue.impl->database->getStorageManager()->getTable(table)))
                rels->release(context);
        }
        catalogue.impl->filesystem->unbind(context);
    }
};
void mapNodes(NativeCatalog& catalogue,Connection& connection,bool recreate,const QuerySignal& signal) {
    auto& own=*catalogue.impl;
    JsonView schema{yyjson_doc_get_root(own.schemaDoc->ptr)};
    auto transaction=lbug::transaction::Transaction::Get(*connection.getClientContext());
    own.ids.nodes.clear();
    for(const auto& node:own.mount.nodes) {
        signal.check();
        auto label=schema.at("nodes")[node.table].at("label").get<std::string>();
        auto entry=own.database->getCatalog()->getTableCatalogEntry(transaction,label);
        if(recreate) own.database->getStorageManager()->createTable(entry,connection.getClientContext());
        own.nodeIDs.insert_or_assign(label,entry->getTableID());
        own.ids.nodes.emplace(entry->getTableID(),Node{node.table,node.rows});
    }
}
void mapRelations(NativeCatalog& catalogue,Connection& connection,bool recreate,const QuerySignal& signal) {
    auto& own=*catalogue.impl;
    auto transaction=lbug::transaction::Transaction::Get(*connection.getClientContext());
    using Pair=std::pair<table_id_t,table_id_t>;
    std::map<std::string,std::map<Pair,std::vector<const MemberMount*>>> groups;
    for(const auto& member:own.mount.members) {
        signal.check();
        auto from=own.nodeIDs.at(std::string(member.from_label));
        auto to=own.nodeIDs.at(std::string(member.to_label));
        groups[std::string(member.kind)][{from,to}].push_back(&member);
    }
    own.ids.rels.clear();
    for(const auto& [kind,pairs]:groups) {
        signal.check();
        auto entry=dynamic_cast<catalog::RelGroupCatalogEntry*>(own.database->getCatalog()->getTableCatalogEntry(transaction,kind));
        if(!entry) throw std::runtime_error("native relationship catalogue mismatch");
        if(recreate) own.database->getStorageManager()->createTable(entry,connection.getClientContext());
        for(const auto& pair:entry->getRelEntryInfos()) {
            signal.check();
            auto found=pairs.find({pair.nodePair.srcTableID,pair.nodePair.dstTableID});
            if(found==pairs.end()) throw std::runtime_error("native relationship pair is outside captured metadata");
            uint64_t start=0;
            for(const auto* member:found->second) {
                if(start>UINT64_MAX-member->rows) throw std::runtime_error("native relationship identity overflows");
                own.ids.rels[pair.oid].push_back(Part{member->member,start,member->rows});
                start+=member->rows;
            }
        }
    }
}
}
std::unique_ptr<NativeCatalog> prepare_catalog(rust::Box<GraphIo> io,
    rust::Box<QuerySignal> signal,rust::Str schemaText,GraphMount mount,const QueryOptions& options) {
    signal->check();
    auto catalogue=std::make_unique<NativeCatalog>(schemaText,std::move(mount),options);
#ifdef GRAPHIT_QUERY_TEST_SUPPORT
    signal->on_database_created();
#endif
    auto& own=*catalogue->impl;
    JsonView schema{yyjson_doc_get_root(own.schemaDoc->ptr)};
    auto& database=*own.database;
    auto filesystem=std::make_unique<CapturedFs>(fileSlices(own.mount));
    own.filesystem=filesystem.get();
    database.registerFileSystem(std::move(filesystem));
    own.binding=std::make_unique<BindCatalog>(database,schema,own.nodeIDs,own.filesystem,own.mount);
    Connection connection(&database);
    Operation operation(*catalogue,connection,std::move(io));
    Observe observe(*signal,connection);
    for(const auto& node:schema.at("nodes").array()) {
        signal->check();
        std::map<std::string,std::set<std::string>> columns;
        for(const auto& column:node.at("columns").array()) columns[column[0].get<std::string>()].insert(column[1].get<std::string>());
        auto ddl="CREATE NODE TABLE "+StringUtils::quoteIdentifier(node.at("label").get<std::string>())+
            '('+properties(columns)+",PRIMARY KEY("+StringUtils::quoteIdentifier(node.at("primary_key").get<std::string>())+
            ")) WITH(storage='graphit-captured://logical.parquet',format='lance')";
        trusted(connection,ddl);
#ifdef GRAPHIT_QUERY_TEST_SUPPORT
        signal->on_ddl_executed();
#endif
    }
    trusted(connection,"BEGIN TRANSACTION READ ONLY");
    mapNodes(*catalogue,connection,false,*signal);
    trusted(connection,"COMMIT");
    std::map<std::string,std::vector<JsonView>> groups;
    for(const auto& member:schema.at("members").array()) groups[member.at("kind").get<std::string>()].push_back(member);
    for(const auto& [kind,members]:groups) {
        signal->check();
        std::set<std::pair<std::string,std::string>> pairs;
        std::map<std::string,std::set<std::string>> columns;
        for(const auto& member:members) {
            pairs.emplace(member.at("from").get<std::string>(),member.at("to").get<std::string>());
            for(const auto& column:member.at("properties").array()) columns[column[0].get<std::string>()].insert(column[1].get<std::string>());
        }
        auto ddl="CREATE REL TABLE GROUP "+StringUtils::quoteIdentifier(kind)+'(';
        bool first=true;
        for(const auto& [from,to]:pairs) {
            if(!first) ddl+=',';
            first=false;
            ddl+="FROM "+StringUtils::quoteIdentifier(from)+" TO "+StringUtils::quoteIdentifier(to);
        }
        if(!columns.empty()) ddl+=','+properties(columns);
        ddl+=") WITH(storage='graphit-captured://logical',format='lance')";
        trusted(connection,ddl);
#ifdef GRAPHIT_QUERY_TEST_SUPPORT
        signal->on_ddl_executed();
#endif
    }
    trusted(connection,"BEGIN TRANSACTION READ ONLY");
    mapRelations(*catalogue,connection,false,*signal);
    signal->check();
    trusted(connection,"COMMIT");
    own.filesystem->validateUsed(connection.getClientContext());
    signal->check();
    return catalogue;
}
void rebind_catalog(NativeCatalog& catalogue,rust::Box<GraphIo> io,
    rust::Box<QuerySignal> signal,GraphMount mount) {
    signal->check();
    auto& own=*catalogue.impl;
    auto slices=fileSlices(mount);
    own.filesystem->reset(std::move(slices));
    own.previousMount=std::make_unique<GraphMount>(std::move(own.mount));
    own.mount=std::move(mount);
    own.binding->rebind(own.mount);
    auto& database=*own.database;
    Connection connection(&database);
    Operation operation(catalogue,connection,std::move(io));
    Observe observe(*signal,connection);
    trusted(connection,"BEGIN TRANSACTION READ ONLY");
    mapNodes(catalogue,connection,true,*signal);
    signal->check();
    mapRelations(catalogue,connection,true,*signal);
    database.getStorageManager()->clearCachedPlannerTableStats();
    own.filesystem->validateUsed(connection.getClientContext());
    signal->check();
    trusted(connection,"COMMIT");
    own.previousMount.reset();
}


namespace {
using LookupHint=std::pair<table_id_t,std::optional<offset_t>>;
bool sameVariable(const lbug::parser::ParsedExpression& expression,const std::string& name) {
    return expression.getExpressionType()==ExpressionType::VARIABLE && expression.getNumChildren()==0 &&
        expression.constCast<lbug::parser::ParsedVariableExpression>().getVariableName()==name;
}
bool directProjection(const lbug::parser::ParsedExpression& expression,const std::string& name) {
    if(sameVariable(expression,name)) return true;
    return expression.getExpressionType()==ExpressionType::PROPERTY && expression.getNumChildren()==1 &&
        !expression.constCast<lbug::parser::ParsedPropertyExpression>().isStar() &&
        sameVariable(*expression.getChild(0),name);
}
std::optional<LookupHint> pointLookup(const lbug::parser::Statement& statement,
    const NativeCatalog& catalogue,const GraphIo& io,
    const std::unordered_map<std::string,std::unique_ptr<Value>>& parameters) {
    const auto& regular=statement.constCast<lbug::parser::RegularQuery>();
    if(regular.getNumSingleQueries()!=1) return std::nullopt;
    auto single=regular.getSingleQuery(0);
    if(single->getNumQueryParts()!=0 || single->getNumReadingClauses()!=1 || !single->hasReturnClause()) return std::nullopt;
    auto reading=single->getReadingClause(0);
    if(reading->getClauseType()!=ClauseType::MATCH || !reading->hasWherePredicate()) return std::nullopt;
    const auto& match=reading->constCast<lbug::parser::MatchClause>();
    const auto& patterns=match.getPatternElementsRef();
    if(match.getMatchClauseType()!=MatchClauseType::MATCH || match.hasHint() || patterns.size()!=1 ||
        patterns[0].hasPathName() || patterns[0].getNumPatternElementChains()!=0) return std::nullopt;
    auto node=patterns[0].getFirstNodePattern();
    auto labels=node->getTableNames();
    auto variable=node->getVariableName();
    if(variable.empty() || labels.size()!=1 || !node->getPropertyKeyVals().empty()) return std::nullopt;
    auto projection=single->getReturnClause()->getProjectionBody();
    if(projection->hasSkipExpression() || projection->hasLimitExpression() || projection->getProjectionExpressions().empty()) return std::nullopt;
    for(const auto& item:projection->getProjectionExpressions()) if(!directProjection(*item,variable)) return std::nullopt;
    for(const auto& item:projection->getOrderByExpressions()) if(!directProjection(*item,variable)) return std::nullopt;
    const auto& where=*reading->getWherePredicate();
    if(where.getExpressionType()!=ExpressionType::EQUALS || where.getNumChildren()!=2) return std::nullopt;
    const lbug::parser::ParsedExpression* property=nullptr;
    const lbug::parser::ParsedExpression* operand=nullptr;
    for(idx_t side=0;side<2;++side) {
        auto candidate=where.getChild(side);
        if(candidate->getExpressionType()==ExpressionType::PROPERTY && candidate->getNumChildren()==1 &&
            sameVariable(*candidate->getChild(0),variable)) {
            property=candidate; operand=where.getChild(1-side); break;
        }
    }
    if(!property) return std::nullopt;
    std::optional<Value> key;
    if(operand->getExpressionType()==ExpressionType::LITERAL)
        key=operand->constCast<lbug::parser::ParsedLiteralExpression>().getValue();
    else if(operand->getExpressionType()==ExpressionType::PARAMETER) {
        auto name=operand->constCast<lbug::parser::ParsedParameterExpression>().getParameterName();
        auto found=parameters.find(name);
        if(found!=parameters.end()) key=*found->second;
    }
    if(!key || key->isNull()) return std::nullopt;
    auto name=property->constCast<lbug::parser::ParsedPropertyExpression>().getPropertyName();
    auto& own=*catalogue.impl;
    JsonView schema{yyjson_doc_get_root(own.schemaDoc->ptr)};
    for(auto metadata:schema.at("nodes").array()) {
        if(metadata.at("label").get<std::string>()!=labels[0] || metadata.at("primary_key").get<std::string>()!=name) continue;
        std::string type;
        for(auto column:metadata.at("columns").array())
            if(column[0].get<std::string>()==name) type=column[1].get<std::string>();
        auto table=metadata.at("table").get<uint64_t>();
        // Native mask range APIs use uint32 physical offsets. Larger captured
        // shards keep the official scan; logical row bases may still be uint64.
        for(const auto& shard:own.mount.nodes.at(table).shards)
            if(shard.rows>UINT32_MAX) return std::nullopt;
        NodeLookup resolved;
        if(type=="STRING" && key->getDataType().getLogicalTypeID()==LogicalTypeID::STRING)
            resolved=io.lookup_string(labels[0],key->getValue<std::string>(),DEFAULT_VECTOR_CAPACITY);
        else if(type=="INT64" && key->getDataType().getLogicalTypeID()==LogicalTypeID::INT64)
            resolved=io.lookup_int64(labels[0],key->getValue<int64_t>(),DEFAULT_VECTOR_CAPACITY);
        else return std::nullopt;
        if(resolved.found && (resolved.table!=table || resolved.row>=own.mount.nodes.at(table).rows))
            throw std::runtime_error("lookup coordinate is outside captured node metadata");
        return LookupHint{own.nodeIDs.at(labels[0]),resolved.found?std::optional<offset_t>(resolved.row):std::nullopt};
    }
    return std::nullopt;
}
} // namespace

void run_query(const NativeCatalog& catalogue,rust::Box<GraphIo> io,
    rust::Box<QuerySignal> signal,rust::Str queryText,rust::Str parameterText,
    const QueryOptions& options,ResultSink& output) {
    const auto maxRows=options.max_rows;
    const auto maxBytes=options.max_bytes;
    const auto maxDepth=options.max_depth;
    signal->check();
    auto statement=parse_live_query(queryText);
    auto& database=*catalogue.impl->database;
    const auto& ids=catalogue.impl->ids;
    Connection connection(&database);
    connection.setMaxNumThreadForExec(options.query_threads);
    Operation operation(catalogue,connection,std::move(io));
    Observe observe(*signal,connection);
    trusted(connection,"BEGIN TRANSACTION READ ONLY");
    auto inputDoc=lbug::json_extension::stringToJson(std::string(parameterText));
    JsonView inputs{yyjson_doc_get_root(inputDoc.ptr)};
    if (!yyjson_is_obj(inputs.value)) throw std::runtime_error("query parameters must be an object");
    std::unordered_map<std::string,std::unique_ptr<Value>> parameters;
    for (const auto& [name,input]:inputs.fields()) {
        auto value=parameter(input,0,maxDepth);
        parameters.emplace(name,std::make_unique<Value>(std::move(value)));
    }
    signal->check();
    if(auto hint=pointLookup(*statement,catalogue,*operation.io,parameters)) {
        auto table=dynamic_cast<ShardNodes*>(database.getStorageManager()->getTable(hint->first));
        if(!table) throw std::runtime_error("lookup table is outside captured native catalogue");
        table->setLookupHint(connection.getClientContext(),hint->second);
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
    catalogue.impl->filesystem->validateUsed(connection.getClientContext());
    signal->check();
    trusted(connection,"COMMIT");
}
} // namespace graphit::icebug
