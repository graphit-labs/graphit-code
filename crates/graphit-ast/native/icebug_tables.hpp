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
    std::unique_ptr<storage::IceDiskNodeTable> table;
    offset_t base, rows;
    node_group_idx_t first, groups;
};
struct ShardNodeState final : storage::ColumnarNodeTableScanState {
    std::unique_ptr<storage::IceDiskNodeTableScanState> child;
    std::unique_ptr<ShardMask> mask;
    size_t part=0;
    using ColumnarNodeTableScanState::ColumnarNodeTableScanState;
};

class ShardNodes final : public storage::ColumnarNodeTableBase {
    const storage::StorageManager* manager;
    const catalog::NodeTableCatalogEntry* entry;
    JsonView node;
    mutable std::once_flag partsReady;
    mutable std::vector<NodePart> parts;
    mutable node_group_idx_t morsels=0;
    offset_t extent=0;
    void ensureParts(const Transaction* tx) const {
        std::call_once(partsReady,[&] {
            auto context=tx->getClientContext();
            std::vector<NodePart> ready;
            node_group_idx_t count=0;
            for (auto shard:node.at("shards").array()) {
                auto child=std::make_unique<catalog::NodeTableCatalogEntry>(entry->getName(),
                    entry->getPrimaryKeyName(),shard.at("file").get<std::string>(),StorageFormat::ICEBUG_DISK);
                child->setOID(entry->getOID()); child->setOwningCatalog(entry->getOwningCatalog());
                for (const auto& property:entry->getProperties()) child->addProperty(property);
                auto table=std::make_unique<storage::IceDiskNodeTable>(manager,child.get(),memoryManager,context);
                lbug::processor::ParquetReader reader(shard.at("file").get<std::string>(),{},context);
                auto rows=shard.at("rows").get<uint64_t>(),base=shard.at("row_base").get<uint64_t>();
                if (reader.getMetadata()->num_rows<0 || static_cast<uint64_t>(reader.getMetadata()->num_rows)!=rows)
                    throw std::runtime_error("captured node shard row count mismatch");
                auto groups=reader.getNumRowGroups();
                if (groups>std::numeric_limits<node_group_idx_t>::max()-count)
                    throw std::runtime_error("captured node shard group count overflow");
                ready.push_back({std::move(child),std::move(table),base,rows,count,groups});
                count+=groups;
            }
            parts=std::move(ready); morsels=count;
        });
    }
    // ScanRelTable0.22.1 creates a plain NodeTableScanState for external source
    // nodes. Own only that operation's child state here; ordinary scans carry it
    // in their hook-created state. Entries are released when a morsel ends.
    mutable std::mutex statesMutex;
    mutable std::map<storage::TableScanState*,std::unique_ptr<ShardNodeState>> sourceStates;
    ShardNodeState& state(storage::TableScanState& parent) const {
        if (auto own=dynamic_cast<ShardNodeState*>(&parent)) return *own;
        std::lock_guard lock(statesMutex);
        auto& own=sourceStates[&parent];
        if (!own) own=std::make_unique<ShardNodeState>(*memoryManager,parent.nodeIDVector,parent.outputVectors,parent.outState);
        return *own;
    }
public:
    ShardNodes(const storage::StorageManager* manager,const catalog::NodeTableCatalogEntry* entry,
        storage::MemoryManager* memory,JsonView node)
        : ColumnarNodeTableBase(manager,entry,memory,nullptr),manager(manager),entry(entry),node(node) {
        for (auto shard:node.at("shards").array()) {
            auto rows=shard.at("rows").get<uint64_t>(),base=shard.at("row_base").get<uint64_t>();
            if (base>UINT64_MAX-rows) throw std::runtime_error("captured node shard row count overflow");
            extent=std::max(extent,base+rows);
        }
    }
    node_group_idx_t numMorsels(const Transaction* tx) const { ensureParts(tx); return morsels; }
    void initializeScanCoordination(const Transaction* tx) override {
        ensureParts(tx);
        for (auto& part:parts) part.table->initializeScanCoordination(tx);
    }
    void initScanState(Transaction* tx,storage::TableScanState& parent,bool=true) const override {
        ensureParts(tx);
        auto& own=state(parent);
        auto it=std::find_if(parts.begin(),parts.end(),[&](const NodePart& part){
            return parent.nodeGroupIdx>=part.first && parent.nodeGroupIdx-part.first<part.groups;
        });
        if (it==parts.end()) { own.child.reset(); return; }
        own.part=it-parts.begin();
        own.child=std::make_unique<storage::IceDiskNodeTableScanState>(*memoryManager,parent.nodeIDVector,parent.outputVectors,parent.outState);
        own.child->setToTable(tx,it->table.get(),parent.columnIDs,copyVector(parent.columnPredicateSets));
        own.mask.reset();
        if (parent.semiMask) {
            own.mask=std::make_unique<ShardMask>(*parent.semiMask,it->base,it->rows);
            own.child->semiMask=own.mask.get();
        }
        it->table->initScanState(tx,*own.child);
    }
    bool scanInternal(Transaction* tx,storage::TableScanState& parent) override {
        // A source-mode scan can be called before its first morsel is admitted.
        if (parent.nodeGroupIdx==INVALID_NODE_GROUP_IDX) return false;
        ensureParts(tx);
        auto& own=state(parent);
        if (!own.child) return false;
        auto& part=parts.at(own.part);
        if (!part.table->scanInternal(tx,*own.child)) {
            own.child.reset();
            if (!dynamic_cast<ShardNodeState*>(&parent)) {std::lock_guard lock(statesMutex);sourceStates.erase(&parent);}
            return false;
        }
        for (auto row:parent.outState->getSelVector().getSelectedPositions()) {
            auto& id=parent.nodeIDVector->getValue<nodeID_t>(row);
            id.offset+=part.base;
        }
        return true;
    }
    bool isVisible(const Transaction* tx,offset_t row) const override {
        ensureParts(tx);
        return std::any_of(parts.begin(),parts.end(),[&](const NodePart& part){return row>=part.base && row-part.base<part.rows;});
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
    row_idx_t getTotalRowCount(const Transaction* tx) const override { ensureParts(tx); return extent; }
};

struct RelPart {
    std::unique_ptr<catalog::RelGroupCatalogEntry> entry;
    std::unique_ptr<storage::IceDiskRelTable> table;
    offset_t source,target,sourceRows,targetRows,start,rows;
    std::vector<std::string> properties;
    std::vector<std::string> types;
};
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
    storage::MemoryManager* memory;
    row_idx_t rows=0;
    mutable std::once_flag csrChecked;
    void ensureCSR(const Transaction* tx) const {
        std::call_once(csrChecked,[&] {
            for (const auto& part:parts) {
                auto expected=part.table->getNumTotalRows(tx);
                row_idx_t observed=0;
                for(auto [source,degree]:part.table->getDegreeEntries(tx,RelDataDirection::FWD)) {
                    if(degree>expected-observed) throw std::runtime_error("captured CSR pointer exceeds edge rows");
                    observed+=degree;
                }
                if(expected!=part.rows || observed!=expected) throw std::runtime_error("captured CSR pointer exceeds or omits edge rows");
            }
        });
    }
    bool prepare(Transaction* tx,ShardRelState& state) const {
        auto& part=parts.at(state.part);
        const auto base=state.direction==RelDataDirection::BWD?part.target:part.source;
        const auto width=state.direction==RelDataDirection::BWD?part.targetRows:part.sourceRows;
        auto& original=state.cachedBoundNodeSelVector;
        if (!std::any_of(original.getSelectedPositions().begin(),original.getSelectedPositions().end(),[&](auto pos) {
            auto row=state.nodeIDVector->getValue<nodeID_t>(pos).offset;
            return row>=base && row-base<width && (state.direction==RelDataDirection::BWD ||
                part.table->getDegreeForOffset(tx,state.direction,row-base)>0);
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
        state.child->setToTable(tx,part.table.get(),columns,{},state.direction);
        part.table->initScanState(tx,*state.child);
        state.initialized=true;
        return true;
    }
public:
    ShardRels(catalog::RelGroupCatalogEntry* entry,table_id_t from,table_id_t to,
        const storage::StorageManager* manager,storage::MemoryManager* memory,
        lbug::main::ClientContext* context,const std::vector<JsonView>& members)
        : ColumnarRelTableBase(entry,from,to,manager,memory),memory(memory) {
        for (auto member:members) {
            auto info=entry->getRelEntryInfo(from,to);
            auto root="graphit-part://"+std::to_string(member.at("member").get<uint64_t>());
            auto child=std::make_unique<catalog::RelGroupCatalogEntry>(entry->getName(),
                RelMultiplicity::MANY,RelMultiplicity::MANY,ExtendDirection::BOTH,
                std::vector<catalog::RelTableCatalogInfo>{*info},root,StorageFormat::ICEBUG_DISK);
            child->setOID(entry->getOID()); child->setOwningCatalog(entry->getOwningCatalog());
            for (const auto& property:entry->getProperties()) child->addProperty(property);
            auto table=std::make_unique<storage::IceDiskRelTable>(child.get(),from,to,manager,memory,context);
            std::vector<std::string> properties,types;
            for (auto property:member.at("properties").array()) {properties.push_back(property[0].get<std::string>());types.push_back(property[1].get<std::string>());}
            auto count=member.at("rows").get<uint64_t>();
            parts.push_back({std::move(child),std::move(table),member.at("source_row_base").get<uint64_t>(),
                member.at("target_row_base").get<uint64_t>(),member.at("source_rows").get<uint64_t>(),
                member.at("target_rows").get<uint64_t>(),rows,count,std::move(properties),std::move(types)});
            if (rows>UINT64_MAX-count) throw std::runtime_error("relationship shard count overflow");
            rows+=count;
        }
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
            if (!part.table->scanInternal(tx,*state.child)) { ++state.part; state.initialized=false; continue; }
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
    row_idx_t getTotalRowCount(const Transaction*) const override { return rows; }
    std::vector<std::pair<offset_t,row_idx_t>> getAllDegreeEntries(const Transaction* tx,RelDataDirection dir) const override {
        ensureCSR(tx);
        std::map<offset_t,row_idx_t> degrees;
        for (auto& part:parts) for(auto [row,count]:part.table->getDegreeEntries(tx,dir)) degrees[row+(dir==RelDataDirection::BWD?part.target:part.source)]+=count;
        return {degrees.begin(),degrees.end()};
    }
    row_idx_t getActiveBoundNodeCount(const Transaction* tx,RelDataDirection dir) const override { return getAllDegreeEntries(tx,dir).size(); }
    row_idx_t getDegreeForOffsetInternal(const Transaction* tx,RelDataDirection dir,offset_t row) const override {
        ensureCSR(tx);
        row_idx_t count=0; for(auto& part:parts){auto base=dir==RelDataDirection::BWD?part.target:part.source;if(row>=base)count+=part.table->getDegreeForOffset(tx,dir,row-base);} return count;
    }
    std::vector<std::pair<offset_t,row_idx_t>> getTopKDegreeEntries(const Transaction* tx,RelDataDirection dir,idx_t k) const override {
        auto degrees=getAllDegreeEntries(tx,dir);std::sort(degrees.begin(),degrees.end(),[](auto a,auto b){return a.second!=b.second?a.second>b.second:a.first<b.first;});if(degrees.size()>k)degrees.resize(k);return degrees;
    }
};
