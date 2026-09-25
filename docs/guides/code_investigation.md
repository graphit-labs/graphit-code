# Investigate code relationships

![Directed calls around Checkout, with boundaries and source inspection](../site/assets/code-investigation.jpg)

*Fictional Aster Delivery project. Follow RetryCheckout → Checkout → PlaceOrder, then open the indexed implementation.*


Use Code intelligence to understand an implementation, inspect the boundaries it crosses and identify code to review before a change. Start with an indexed project selected in the common header, or open an installed Code context.

## Read structure before following a symbol

1. Open **Relationship map**. On first entry, Graphit loads a bounded index sample unless a search or Cypher execution has already supplied the current result (including an empty or failed request).
2. Choose **Organize by**: Directory for source boundaries, File for individual implementations, Language for a cross-language view, or Configured cluster for project-defined path groups.
3. Select a boundary. Its reader lists entities and directed relationships crossing the boundary, and counts relationships internal to the group. Incoming points from outside into the group; outgoing points from the group to an outside entity.
4. Select an entity to query its direct neighborhood in the selected index, including entities outside the original result. Incoming entities appear on the left, the selected entity in the center and outgoing entities on the right. Each connection retains its relationship type. The query covers every declared incoming and outgoing relationship/type combination. Loading and partial failures appear in the reader; **Retry / load remaining relationships** retries failed branches without discarding successful evidence. On narrow screens these sections stack in reading order.
5. Follow a neighbor to continue exploring; **Back** returns to the preceding entity. **All boundaries** returns to the catalogue. Each selected neighbor triggers its own neighborhood query. The original result catalogue, counts and filters remain unchanged. No physics, camera positioning or color interpretation is needed to read direction.
6. Choose **Inspect source & impact** to open the implementation inside Relationship map. There you can read indexed source and explicitly query incoming, outgoing or potential-impact evidence against the selected index.

The boundary search matches names and paths within the loaded result. Language, entity-type and relationship filters narrow that result; they do not fetch additional repository data. **Clear filters** recovers a filtered-empty view. The common header refresh reloads the current result source (search, Cypher or sample) and the selected neighborhood; exploration history remains available. A new explicit search or query clears the previous map filters and selection. Changing project or imported context resets the exploration.

## Understand what the numbers mean

An **entity** is an indexed node, such as a function, class, file or module. A **relationship** is an indexed occurrence with its own `uid`, source, type and target. Repeated query rows for the same relation UID do not increase the count; two real calls between the same entities remain distinct. Self relationships remain explicit.

Directory and file boundaries come from source paths. Language comes from indexed metadata. A configured cluster is a path-based grouping, **not an inferred community, team owner or architectural guarantee**. Missing path, language and cluster metadata have explicit fallback labels rather than guessed values.

Catalogue and boundary counts describe the **loaded result**, not the entire repository. A selected entity’s Incoming/Outgoing reader is separate: it queries direct indexed relationships beyond that result. Each direction/relationship/entity-type branch loads 100 relations at a time; **Load more relationships** continues by relation UID. Counts in that reader describe the relations loaded so far. If an incremental reindex happens between pages, the reader reloads the selected entity by its indexed UID and restarts the neighborhood on the new index generation. An edited or removed relation may disappear from that refreshed result. The initial sample includes up to 300 initial nodes and the endpoints of up to 1,000 relationships, so it can contain more than 300 entities. It has no completeness guarantee. Catalogue filters remove connections to hidden result endpoints, but do not hide independently queried neighbors. No visible link does not prove independence; dynamic dispatch and unresolved calls may be absent even in a wider query.

## Ask a precise question

Use **Find & inspect** for full-index ranked search and **Query lab** for a reproducible read-only Cypher question. Inspect the available schema and persistent identity before traversing. AI drafting produces an editable query; you decide when to run it. Both **Search index** and **Run query** immediately open **Relationship map**, where loading, errors and results appear. Query lab keeps the editable query when you return. Search matches retain their rank and available descriptions, with no invented relationships; select an entity to resolve its indexed identity and load its neighborhood automatically. Scalar rows appear in **Query rows** in the same workspace and are never converted into invented graph relationships. For mixed results, the graph appears first and Query rows can be expanded below it.

Potential impact follows incoming calls up to two hops and returns up to 100 distinct indexed candidates. Review their source and relevant tests before concluding that a change affects them. See the [AST query and traversal contract](../specs/ast_module.md) and [supported languages](../specs/ast_module.md#supported-languages) for parser-specific boundaries.

## Recover from incomplete evidence

- **No graph entities:** load a sample, inspect the query table or check whether the chosen context has been indexed.
- **No matching entities:** clear filters or use a different name/path. Use full-index search if the entity is outside the sample.
- **No source path:** the entity may represent an external or structural node. The index sample carries its persistent identity when available, so its relationships remain inspectable without a file; source is not fabricated.
- **Request failed:** read the error and use **Retry neighborhood**, the failed-branch retry, or the common header refresh. If the same branches keep failing after the daemon is available, update Graphit and reindex the selected project or context; Retry cannot repair an invalid persisted graph bundle. An error is not evidence of an empty repository.
- **Missing unique identifier:** refresh the result or reindex. Symbol nodes need their indexed `uid`; File/Directory nodes use `path`. Search and sample results carry that identity, and a full node returned by Query lab retains it. The map never guesses from a name, file or line.
- **Missing relationship UID:** reindex the selected context. Older graph bundles cannot support stable relation navigation. Ladybug's internal `ID(r)` identifies a relation in the current mounted snapshot and may change after reindexing.

The [design system](../specs/design_system.md) defines this stable, keyboard-accessible catalogue and evidence-reader pattern.

### Query safety with the current native engine

Icebug 0.19 has a multi-table relationship scan defect. Graphit reads physical
forward members separately for the map and for simple `MATCH (n)-[r]-(m)
RETURN n,r,m` queries, preserving each relation's UID. Other wildcard and
type-alternation forms can still be refused when their endpoints cannot be
verified. For a reached-node investigation, filter an anchor and project the
reached endpoint, for example:

```cypher
MATCH (a:Function)-[:CALLS]->(b:Function)
WHERE a.name = 'Checkout'
RETURN DISTINCT b.name
```

This example uses the fictional demonstration project. Substitute a function
from your own index. A native scan refusal is not an empty search result.
