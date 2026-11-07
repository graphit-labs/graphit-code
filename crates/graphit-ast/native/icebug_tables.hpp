// Graphit composes captured FILE shards; all decoding and CSR traversal remain
// in the official Ladybug tables. Included inside the adapter's anonymous namespace.
namespace storage = lbug::storage;
namespace catalog = lbug::catalog;
using Transaction = lbug::transaction::Transaction;

struct ShardMask final : SemiMask {
    SemiMask& parent;
    offset_t base;
    ShardMask(SemiMask& parent, offset_t base, offset_t rows)
        : SemiMask(rows), parent(parent), base(base) { if (parent.isEnabled()) enable(); }
    void mask(offset_t row) override { parent.mask(base + row); }
    void maskRange(offset_t begin, offset_t end) override { parent.maskRange(base+begin, base+end); }
    bool isMasked(offset_t row) override { return parent.isMasked(base+row); }
    offset_vec_t range(uint32_t begin, uint32_t end) override {
        if (base+end>UINT32_MAX) throw std::runtime_error("Ladybug semimask offset exceeds its public range API");
        auto rows=parent.range(base+begin,base+end);
        for (auto& row:rows) row-=base;
        return rows;
    }
    uint64_t getNumMaskedNodes() const override { return parent.getNumMaskedNodes(); }
    offset_vec_t collectMaskedNodes(uint64_t size) const override {
        auto rows=parent.collectMaskedNodes(size);
        std::erase_if(rows,[&](offset_t row){ return row<base || row-base>=getMaxOffset(); });
        for (auto& row:rows) row-=base;
        return rows;
    }
};

struct NodePart {
    std::unique_ptr<catalog::NodeTableCatalogEntry> entry;
    offset_t base, rows;
    node_group_idx_t first, groups;
};
struct ShardNodeState final : storage::ColumnarNodeTableScanState {
    std::unique_ptr<catalog::NodeTableCatalogEntry> pointEntry;
    std::unique_ptr<storage::IceDiskNodeTable> table;
    std::unique_ptr<storage::IceDiskNodeTableScanState> child;
    std::unique_ptr<ShardMask> mask;
    std::unique_ptr<SemiMask> pointMask;
    offset_t base=0;
    using ColumnarNodeTableScanState::ColumnarNodeTableScanState;
};

class ShardNodes final : public storage::ColumnarNodeTableBase {
    const storage::StorageManager* manager;
    const catalog::NodeTableCatalogEntry* entry;
    const NodeMount& node;
    mutable std::once_flag partsReady;
    mutable std::vector<NodePart> parts;
    mutable node_group_idx_t morsels=0;
    offset_t extent=0;
    void ensureParts(const Transaction* tx) const {
        validateNodeCapture(manager,tx,node,this);
        std::call_once(partsReady,[&] {
            auto context=tx->getClientContext();
            std::vector<NodePart> ready;
            node_group_idx_t count=0;
            std::size_t part=0;
            for (const auto& shard:node.shards) {
                auto path=nodeSlicePath(node,part++);
                auto child=std::make_unique<catalog::NodeTableCatalogEntry>(entry->getName(),
                    entry->getPrimaryKeyName(),path,StorageFormat::ICEBUG_DISK);
                child->setOID(entry->getOID()); child->setOwningCatalog(entry->getOwningCatalog());
                for (const auto& property:entry->getProperties()) child->addProperty(property);
                lbug::processor::ParquetReader reader(path,{},context);
                auto rows=shard.rows,base=shard.row_base;
                if (reader.getMetadata()->num_rows<0 || static_cast<uint64_t>(reader.getMetadata()->num_rows)!=rows)
                    throw std::runtime_error("captured node shard row count mismatch");
                auto groups=reader.getNumRowGroups();
                if (groups>std::numeric_limits<node_group_idx_t>::max()-count)
                    throw std::runtime_error("captured node shard group count overflow");
                ready.push_back({std::move(child),base,rows,count,groups});
                count+=groups;
            }
            parts=std::move(ready); morsels=count;
        });
    }
    // ScanRelTable0.22.1 creates a plain NodeTableScanState for external source
    // nodes. Own only that operation's child state here; ordinary scans carry it
    // in their hook-created state. Entries are released when a morsel ends.
    mutable std::mutex statesMutex;
    using SourceKey=std::pair<const lbug::main::ClientContext*,storage::TableScanState*>;
    mutable std::map<SourceKey,std::unique_ptr<ShardNodeState>> sourceStates;
    mutable std::map<const lbug::main::ClientContext*,std::optional<offset_t>> lookupHints;
    std::optional<std::optional<offset_t>> lookupHint(const lbug::main::ClientContext* context) const {
        std::lock_guard lock(statesMutex);
        if(auto found=lookupHints.find(context);found!=lookupHints.end()) return std::make_optional(found->second);
        return std::nullopt;
    }
    ShardNodeState& state(const Transaction* tx,storage::TableScanState& parent) const {
        if (auto own=dynamic_cast<ShardNodeState*>(&parent)) return *own;
        std::lock_guard lock(statesMutex);
        auto& own=sourceStates[{tx->getClientContext(),&parent}];
        if (!own) own=std::make_unique<ShardNodeState>(*memoryManager,parent.nodeIDVector,parent.outputVectors,parent.outState);
        return *own;
    }
public:
    ShardNodes(const storage::StorageManager* manager,const catalog::NodeTableCatalogEntry* entry,
        storage::MemoryManager* memory,const NodeMount& node)
        : ColumnarNodeTableBase(manager,entry,memory,nullptr),manager(manager),entry(entry),node(node) {
        for (const auto& shard:node.shards) {
            auto rows=shard.rows,base=shard.row_base;
            if (base>UINT64_MAX-rows) throw std::runtime_error("captured node shard row count overflow");
            extent=std::max(extent,base+rows);
        }
    }
    void setLookupHint(const lbug::main::ClientContext* context,std::optional<offset_t> row) const {
        std::lock_guard lock(statesMutex);
        if(!lookupHints.emplace(context,row).second) throw std::runtime_error("duplicate native lookup hint");
    }
    void release(const lbug::main::ClientContext* context) const {
        std::lock_guard lock(statesMutex);
        std::erase_if(sourceStates,[&](const auto& item){return item.first.first==context;});
        lookupHints.erase(context);
    }
    node_group_idx_t numMorsels(const Transaction* tx) const {
        if (auto hint=lookupHint(tx->getClientContext())) {
            validateNodeCapture(manager,tx,node,this);
            return *hint ? 1 : 0;
        }
        ensureParts(tx); return morsels;
    }
    void initializeScanCoordination(const Transaction* tx) override {
        if (lookupHint(tx->getClientContext())) validateNodeCapture(manager,tx,node,this);
        else ensureParts(tx);
    }
    void initScanState(Transaction* tx,storage::TableScanState& parent,bool=true) const override {
        auto hint=lookupHint(tx->getClientContext());
        if (hint) validateNodeCapture(manager,tx,node,this);
        else ensureParts(tx);
        auto& own=state(tx,parent);
        own.child.reset(); own.table.reset(); own.pointEntry.reset();
        own.mask.reset(); own.pointMask.reset();
        const catalog::NodeTableCatalogEntry* selected=nullptr;
        offset_t rows=0;
        node_group_idx_t group=0;
        std::unique_ptr<lbug::processor::ParquetReader> metadataReader;
        if (hint) {
            if (!*hint || parent.nodeGroupIdx!=0) return;
            auto shard=std::find_if(node.shards.begin(),node.shards.end(),[&](const auto& part) {
                return **hint>=part.row_base && **hint-part.row_base<part.rows;
            });
            if (shard==node.shards.end()) throw std::runtime_error("captured lookup row outside node ranges");
            auto path=nodeSlicePath(node,shard-node.shards.begin());
            own.pointEntry=std::make_unique<catalog::NodeTableCatalogEntry>(entry->getName(),
                entry->getPrimaryKeyName(),path,StorageFormat::ICEBUG_DISK);
            own.pointEntry->setOID(entry->getOID());
            own.pointEntry->setOwningCatalog(entry->getOwningCatalog());
            for (const auto& property:entry->getProperties()) own.pointEntry->addProperty(property);
            selected=own.pointEntry.get(); own.base=shard->row_base; rows=shard->rows;
            metadataReader=std::make_unique<lbug::processor::ParquetReader>(path,
                std::vector<bool>{},tx->getClientContext());
            auto metadata=metadataReader->getMetadata();
            if (metadata->num_rows<0 || static_cast<uint64_t>(metadata->num_rows)!=rows)
                throw std::runtime_error("captured node shard row count mismatch");
            auto local=**hint-own.base;
            offset_t begin=0;
            for (;group<metadataReader->getNumRowGroups();++group) {
                auto count=metadata->row_groups.at(group).num_rows;
                if (count<0 || static_cast<uint64_t>(count)>rows-begin)
                    throw std::runtime_error("captured node shard group row count mismatch");
                if (local-begin<static_cast<uint64_t>(count)) break;
                begin+=count;
            }
            if (group==metadataReader->getNumRowGroups())
                throw std::runtime_error("captured lookup row outside node groups");
        } else {
            auto part=std::find_if(parts.begin(),parts.end(),[&](const NodePart& part) {
                return parent.nodeGroupIdx>=part.first && parent.nodeGroupIdx-part.first<part.groups;
            });
            if (part==parts.end()) return;
            selected=part->entry.get(); own.base=part->base; rows=part->rows;
            group=parent.nodeGroupIdx-part->first;
            metadataReader=std::make_unique<lbug::processor::ParquetReader>(selected->getStorage(),
                std::vector<bool>{},tx->getClientContext());
        }
        own.table=std::make_unique<storage::IceDiskNodeTable>(manager,selected,memoryManager,tx->getClientContext());
        own.child=std::make_unique<storage::IceDiskNodeTableScanState>(*memoryManager,parent.nodeIDVector,parent.outputVectors,parent.outState);
        own.child->setToTable(tx,own.table.get(),parent.columnIDs,copyVector(parent.columnPredicateSets));
        auto mask=parent.semiMask;
        if (hint) {
            // SDK range() is uint32-based; the point mask uses shard-local rows.
            own.pointMask=SemiMaskUtil::createMask(rows);
            own.pointMask->enable();
            if (!mask || !mask->isEnabled() || mask->isMasked(**hint))
                own.pointMask->mask(**hint-own.base);
            own.child->semiMask=own.pointMask.get();
        } else if (mask) {
            own.mask=std::make_unique<ShardMask>(*mask,own.base,rows);
            own.child->semiMask=own.mask.get();
        }
        // The external hook assigned this exact rowgroup. SDK initScanState
        // would reset it and consume the table's shared cursor across connections.
        auto& child=*own.child;
        child.source=storage::TableScanSource::COMMITTED;
        child.nodeGroupIdx=group;
        std::vector<bool> skips(metadataReader->getNumColumns(),true);
        for(uint32_t column=0;column<metadataReader->getNumColumns();++column) {
            auto name=metadataReader->getColumnName(column);
            if(!selected->containsProperty(name)) continue;
            auto id=selected->getColumnID(name);
            skips[column]=std::find(parent.columnIDs.begin(),parent.columnIDs.end(),id)==parent.columnIDs.end();
        }
        if(std::any_of(skips.begin(),skips.end(),[](bool skip){return skip;}))
            child.parquetReader=std::make_unique<lbug::processor::ParquetReader>(
                selected->getStorage(),std::move(skips),tx->getClientContext());
        else child.parquetReader=std::move(metadataReader);
        child.initialized=true;
        child.scanCompleted=false;
        std::vector<uint64_t> groups{child.nodeGroupIdx};
        if(child.semiMask && child.semiMask->isEnabled()) {
            auto metadata=child.parquetReader->getMetadata();
            offset_t begin=0;
            for(node_group_idx_t group=0;group<child.nodeGroupIdx;++group)
                begin+=metadata->row_groups.at(group).num_rows;
            const auto end=begin+metadata->row_groups.at(child.nodeGroupIdx).num_rows;
            if(end>UINT32_MAX) throw std::runtime_error("Ladybug semimask offset exceeds its public range API");
            if(child.semiMask->range(begin,end).empty()) {child.scanCompleted=true;groups.clear();}
        }
        child.parquetReader->initializeScan(*child.parquetScanState,groups,
            VirtualFileSystem::GetUnsafe(*tx->getClientContext()));
    }
    bool scanInternal(Transaction* tx,storage::TableScanState& parent) override {
        // A source-mode scan can be called before its first morsel is admitted.
        if (parent.nodeGroupIdx==INVALID_NODE_GROUP_IDX) return false;
        if (lookupHint(tx->getClientContext())) validateNodeCapture(manager,tx,node,this);
        else ensureParts(tx);
        auto& own=state(tx,parent);
        if (!own.child) return false;
        if (!own.table->scanInternal(tx,*own.child)) {
            own.child.reset();
            if (!dynamic_cast<ShardNodeState*>(&parent)) {std::lock_guard lock(statesMutex);sourceStates.erase({tx->getClientContext(),&parent});}
            return false;
        }
        for (auto row:parent.outState->getSelVector().getSelectedPositions()) {
            auto& id=parent.nodeIDVector->getValue<nodeID_t>(row);
            id.offset+=own.base;
        }
        return true;
    }
    bool isVisible(const Transaction* tx,offset_t row) const override {
        validateNodeCapture(manager,tx,node,this);
        return std::any_of(node.shards.begin(),node.shards.end(),[&](const auto& part){return row>=part.row_base && row-part.row_base<part.rows;});
    }
    bool isVisibleNoLock(const Transaction* tx,offset_t row) const override { return isVisible(tx,row); }
    bool lookupPK(const Transaction* tx,ValueVector* key,uint64_t pos,offset_t& row) const override {
        if (key->dataType.getLogicalTypeID()!=LogicalTypeID::INTERNAL_ID || key->isNull(pos)) return false;
        auto id=key->getValue<nodeID_t>(pos);
        if (id.tableID!=getTableID() || !isVisible(tx,id.offset)) return false;
        row=id.offset; return true;
    }
protected:
    std::string getColumnarFormatName() const override { return "Graphit captured Parquet shards"; }
    node_group_idx_t getNumBatches(const Transaction* tx) const override { return numMorsels(tx); }
    row_idx_t getTotalRowCount(const Transaction* tx) const override { validateNodeCapture(manager,tx,node,this); return extent; }
};

struct RelPart {
    std::unique_ptr<catalog::RelGroupCatalogEntry> entry;
    offset_t source,target,sourceRows,targetRows,start,rows;
    std::vector<std::string> properties;
    std::vector<std::string> types;
};
void validateRelCapture(const storage::StorageManager*,const Transaction*,
    const std::vector<RelPart>&,const void*);
struct ShardRelState final : storage::IceDiskRelTableScanState {
    size_t part=0;
    bool initialized=false;
    std::unique_ptr<storage::IceDiskRelTableScanState> child;
    std::unique_ptr<ValueVector> bound;
    std::vector<std::shared_ptr<ValueVector>> vectors;
    using IceDiskRelTableScanState::IceDiskRelTableScanState;
    void setToTable(const Transaction* tx,storage::Table* table,std::vector<column_id_t> columns,
        std::vector<storage::ColumnPredicateSet> predicates,RelDataDirection dir) override {
        IceDiskRelTableScanState::setToTable(tx,table,std::move(columns),std::move(predicates),dir);
        child.reset(); vectors.clear(); bound.reset(); initialized=false; part=0;
    }
};

class ShardRels final : public storage::ColumnarRelTableBase {
    std::vector<RelPart> parts;
    const storage::StorageManager* manager;
    storage::MemoryManager* memory;
    row_idx_t rows=0;
    struct Request {
        std::vector<std::unique_ptr<storage::IceDiskRelTable>> tables;
        std::once_flag csrChecked;
    };
    mutable std::mutex requestsMutex;
    mutable std::map<const lbug::main::ClientContext*,std::unique_ptr<Request>> requests;
    Request& request(const Transaction* tx) const {
        std::lock_guard lock(requestsMutex);
        auto context=tx->getClientContext();
        if(auto found=requests.find(context);found!=requests.end()) return *found->second;
        auto next=std::make_unique<Request>();
        for(const auto& part:parts) next->tables.push_back(std::make_unique<storage::IceDiskRelTable>(
            part.entry.get(),getFromNodeTableID(),getToNodeTableID(),manager,memory,context));
        return *requests.emplace(context,std::move(next)).first->second;
    }
    void ensureCSR(const Transaction* tx) const {
        validateRelCapture(manager,tx,parts,this);
        std::call_once(request(tx).csrChecked,[&] {
            auto context=tx->getClientContext();
            auto vfs=VirtualFileSystem::GetUnsafe(*context);
            for (const auto& part:parts) {
                if(context->interrupted()) throw InterruptException{};
                // Validate raw pointers through the official reader before asking
                // IceDiskRelTable to initialize CSR or summarize degrees.
                const auto paths=storage::IceDiskUtils::constructCSRPaths(
                    part.entry->getStorage(),part.entry->getName(),".parquet");
                lbug::processor::ParquetReader indices(paths.indices,{},context);
                auto edges=indices.getMetadata()->num_rows;
                if(edges<0 || static_cast<uint64_t>(edges)!=part.rows)
                    throw std::runtime_error("captured CSR edge row count mismatch");
                if(part.sourceRows==UINT64_MAX)
                    throw std::runtime_error("captured CSR pointer count overflow");
                lbug::processor::ParquetReader pointers(paths.indptr,{},context);
                auto count=pointers.getMetadata()->num_rows;
                if(count<0 || static_cast<uint64_t>(count)!=part.sourceRows+1)
                    throw std::runtime_error("captured CSR pointer count mismatch");
                uint64_t observed=0,previous=0;
                for(uint64_t group=0;group<pointers.getNumRowGroups();++group) {
                    if(context->interrupted()) throw InterruptException{};
                    lbug::processor::ParquetReaderScanState scan;
                    pointers.initializeScan(scan,{group},vfs);
                    if(pointers.getNumColumns()!=1 ||
                        pointers.getColumnType(0).getLogicalTypeID()!=LogicalTypeID::UINT64)
                        throw std::runtime_error("captured CSR pointer schema must be UINT64");
                    DataChunk chunk(1);
                    chunk.insert(0,std::make_shared<ValueVector>(
                        pointers.getColumnType(0).copy(),pointers.getMemoryManager()));
                    while(pointers.scanInternal(scan,chunk)) {
                        if(context->interrupted()) throw InterruptException{};
                        auto& column=chunk.getValueVector(0);
                        for(auto pos:chunk.state->getSelVector().getSelectedPositions()) {
                            if(column.isNull(pos) || observed>=part.sourceRows+1)
                                throw std::runtime_error("captured CSR pointer count or null invalid");
                            auto pointer=column.getValue<uint64_t>(pos);
                            if(pointer>part.rows)
                                throw std::runtime_error("captured CSR pointer exceeds edge rows");
                            if((observed==0 && pointer!=0) || pointer<previous)
                                throw std::runtime_error("captured CSR pointer order invalid");
                            previous=pointer;
                            ++observed;
                        }
                    }
                }
                if(observed!=part.sourceRows+1 || previous!=part.rows)
                    throw std::runtime_error("captured CSR pointer omits edge rows");
            }
        });
    }
    bool prepare(Transaction* tx,ShardRelState& state) const {
        auto& part=parts.at(state.part);
        auto& table=*request(tx).tables.at(state.part);
        const auto base=state.direction==RelDataDirection::BWD?part.target:part.source;
        const auto width=state.direction==RelDataDirection::BWD?part.targetRows:part.sourceRows;
        auto& original=state.cachedBoundNodeSelVector;
        if (!std::any_of(original.getSelectedPositions().begin(),original.getSelectedPositions().end(),[&](auto pos) {
            auto row=state.nodeIDVector->getValue<nodeID_t>(pos).offset;
            return row>=base && row-base<width && (state.direction==RelDataDirection::BWD ||
                table.getDegreeForOffset(tx,state.direction,row-base)>0);
        })) return false;
        state.bound=std::make_unique<ValueVector>(LogicalType::INTERNAL_ID(),memory);
        state.bound->state=std::make_shared<DataChunkState>();
        auto& selection=state.bound->state->getSelVectorUnsafe(); selection.setToFiltered(0);
        for (auto pos:original.getSelectedPositions()) {
            auto id=state.nodeIDVector->getValue<nodeID_t>(pos);
            if (id.offset<base || id.offset-base>=width) continue;
            id.offset-=base;
            state.bound->setValue(pos,id);
            selection.getMutableBuffer()[selection.getSelSize()]=pos;
            selection.setSelSize(selection.getSelSize()+1);
        }
        state.vectors.clear(); std::vector<ValueVector*> outputs; std::vector<column_id_t> columns;
        for (size_t i=0;i<state.outputVectors.size();++i) {
            auto column=i<state.columnIDs.size()?state.columnIDs[i]:INVALID_COLUMN_ID;
            if (i==0 || column==storage::REL_ID_COLUMN_ID) {
                state.vectors.push_back(std::make_shared<ValueVector>(LogicalType::INTERNAL_ID(),memory,state.outState));
            } else if (column!=INVALID_COLUMN_ID && column!=ROW_IDX_COLUMN_ID && column>=2) {
                auto name=relGroupEntry->getProperty(column-1).getName();
                auto found=std::find(part.properties.begin(),part.properties.end(),name);
                auto type=state.outputVectors[i]->dataType.copy();
                if (found==part.properties.end()) column=INVALID_COLUMN_ID;
                else {
                    auto index=found-part.properties.begin();column=2+index;
                    auto name=part.types.at(index);
                    if(name=="STRING")type=LogicalType::STRING();
                    else if(name=="INT64")type=LogicalType::INT64();
                    else if(name=="UINT64")type=LogicalType::UINT64();
                    else if(name=="BOOL")type=LogicalType::BOOL();
                    else throw std::runtime_error("unsupported captured relationship property type");
                }
                state.vectors.push_back(std::make_shared<ValueVector>(std::move(type),memory,state.outState));
            } else state.vectors.push_back(std::make_shared<ValueVector>(state.outputVectors[i]->dataType.copy(),memory,state.outState));
            outputs.push_back(state.vectors.back().get()); columns.push_back(column);
        }
        state.child=std::make_unique<storage::IceDiskRelTableScanState>(*memory,state.bound.get(),outputs,state.outState);
        state.child->setToTable(tx,&table,columns,{},state.direction);
        table.initScanState(tx,*state.child);
        state.initialized=true;
        return true;
    }
public:
    ShardRels(catalog::RelGroupCatalogEntry* entry,table_id_t from,table_id_t to,
        const storage::StorageManager* manager,storage::MemoryManager* memory,
        const std::vector<const MemberMount*>& members)
        : ColumnarRelTableBase(entry,from,to,manager,memory),manager(manager),memory(memory) {
        for (const auto* member:members) {
            auto info=entry->getRelEntryInfo(from,to);
            auto root="graphit-part://"+std::to_string(member->member);
            auto child=std::make_unique<catalog::RelGroupCatalogEntry>(entry->getName(),
                RelMultiplicity::MANY,RelMultiplicity::MANY,ExtendDirection::BOTH,
                std::vector<catalog::RelTableCatalogInfo>{*info},root,StorageFormat::ICEBUG_DISK);
            child->setOID(entry->getOID()); child->setOwningCatalog(entry->getOwningCatalog());
            for (const auto& property:entry->getProperties()) child->addProperty(property);
            std::vector<std::string> properties,types;
            for (const auto& property:member->properties) {properties.push_back(std::string(property.name));types.push_back(std::string(property.data_type));}
            auto count=member->rows;
            parts.push_back({std::move(child),member->source_row_base,
                member->target_row_base,member->source_rows,
                member->target_rows,rows,count,std::move(properties),std::move(types)});
            if (rows>UINT64_MAX-count) throw std::runtime_error("relationship shard count overflow");
            rows+=count;
        }
    }
    void release(const lbug::main::ClientContext* context) const {
        std::lock_guard lock(requestsMutex);
        requests.erase(context);
    }
    void initScanState(Transaction* tx,storage::TableScanState& parent,bool reset=true) const override {
        ensureCSR(tx);
        auto& state=parent.cast<ShardRelState>(); state.child.reset(); state.initialized=false; state.part=0;
        state.source=storage::TableScanSource::COMMITTED;
        if(reset) {
            auto& input=state.nodeIDVector->state->getSelVector();
            state.cachedBoundNodeSelVector.setToFiltered(input.getSelSize());
            std::copy(input.getSelectedPositions().begin(),input.getSelectedPositions().end(),state.cachedBoundNodeSelVector.getMutableBuffer().data());
        }
    }
    bool scanInternal(Transaction* tx,storage::TableScanState& parent) override {
        auto& state=parent.cast<ShardRelState>();
        while (state.part<parts.size()) {
            if (!state.initialized && !prepare(tx,state)) { ++state.part; continue; }
            for (auto& vector:state.vectors) vector->setAllNull();
            auto& part=parts[state.part];
            if (!request(tx).tables.at(state.part)->scanInternal(tx,*state.child)) { ++state.part; state.initialized=false; continue; }
            const auto base=state.direction==RelDataDirection::BWD?part.source:part.target;
            for (size_t i=0;i<state.outputVectors.size();++i) {
                auto& src=*state.vectors[i]; auto* dst=state.outputVectors[i];
                auto count=state.outState->getSelVector().getSelSize();
                if (i==0 || state.columnIDs[i]==storage::REL_ID_COLUMN_ID) {
                    for (size_t row=0;row<count;++row) {auto id=src.getValue<nodeID_t>(row);id.offset+=i==0?base:part.start;dst->setValue(row,id);}
                } else if (src.dataType==dst->dataType) {
                    for (size_t row=0;row<count;++row) dst->copyFromVectorData(row,&src,row);
                } else if(dst->dataType.getLogicalTypeID()==LogicalTypeID::UNION) {
                    union_field_idx_t tag=0;
                    for(;tag<UnionType::getNumFields(dst->dataType);++tag)
                        if(UnionType::getFieldType(dst->dataType,tag)==src.dataType)break;
                    if(tag==UnionType::getNumFields(dst->dataType)) throw std::runtime_error("captured property is outside declared union");
                    auto* value=UnionVector::getValVector(dst,tag);
                    auto* tags=UnionVector::getTagVector(dst);
                    for(size_t row=0;row<count;++row) {
                        dst->setNull(row,src.isNull(row));
                        tags->setValue<union_field_idx_t>(row,tag);
                        value->copyFromVectorData(row,&src,row);
                    }
                } else {
                    auto cast=lbug::function::CastFunction::bindCastFunction("cast",src.dataType,dst->dataType)->execFunc;
                    std::vector<std::shared_ptr<ValueVector>> inputs{state.vectors[i]};
                    std::vector<SelectionVector*> selections{src.getSelVectorPtr()};
                    cast(inputs,selections,*dst,dst->getSelVectorPtr(),nullptr);
                }
            }
            state.nodeIDVector->state->setToFlat();
            state.nodeIDVector->state->getSelVectorUnsafe().setToFiltered(1);
            state.nodeIDVector->state->getSelVectorUnsafe().getMutableBuffer()[0]=state.bound->state->getSelVector()[0];
            return true;
        }
        return false;
    }
protected:
    std::string getColumnarFormatName() const override { return "Graphit captured CSR shards"; }
    row_idx_t getTotalRowCount(const Transaction* tx) const override { ensureCSR(tx); return rows; }
    std::vector<std::pair<offset_t,row_idx_t>> getAllDegreeEntries(const Transaction* tx,RelDataDirection dir) const override {
        ensureCSR(tx);
        std::map<offset_t,row_idx_t> degrees;
        auto& tables=request(tx).tables;
        for(size_t i=0;i<parts.size();++i) for(auto [row,count]:tables[i]->getDegreeEntries(tx,dir))
            degrees[row+(dir==RelDataDirection::BWD?parts[i].target:parts[i].source)]+=count;
        return {degrees.begin(),degrees.end()};
    }
    row_idx_t getActiveBoundNodeCount(const Transaction* tx,RelDataDirection dir) const override { return getAllDegreeEntries(tx,dir).size(); }
    row_idx_t getDegreeForOffsetInternal(const Transaction* tx,RelDataDirection dir,offset_t row) const override {
        ensureCSR(tx);
        row_idx_t count=0; auto& tables=request(tx).tables;
        for(size_t i=0;i<parts.size();++i) {const auto& part=parts[i];auto base=dir==RelDataDirection::BWD?part.target:part.source;
            if(row>=base)count+=tables[i]->getDegreeForOffset(tx,dir,row-base);}
        return count;
    }
    std::vector<std::pair<offset_t,row_idx_t>> getTopKDegreeEntries(const Transaction* tx,RelDataDirection dir,idx_t k) const override {
        auto degrees=getAllDegreeEntries(tx,dir);std::sort(degrees.begin(),degrees.end(),[](auto a,auto b){return a.second!=b.second?a.second>b.second:a.first<b.first;});if(degrees.size()>k)degrees.resize(k);return degrees;
    }
};
