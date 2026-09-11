package wiki

// WikiChunk represents a single wiki document chunk stored in the database.
type WikiChunk struct {
	Slug        string
	Title       string
	Body        string
	Summary     string
	DocType     string
	Source      string
	Breadcrumb  string
	ClusterID   int
	ClusterName string
	Confidence  float64
	ContentHash string
	WordCount   int
	Updated     string
	Important   bool
	Mandatory   bool
	Tags        []string

	// Supersession — the same four fields on every wiki, because "this page is an older version
	// of that one" is not a memory concept. A memory's revision chain is one instance of it; an
	// ADR replaced by a later ADR and a spec kept for reference are others.
	//
	// They are columns rather than page frontmatter so that collapsing a chain is a predicate the
	// engine evaluates, not a file read per hit.
	EntityID   string // stable identity of the thing this page is about, shared by all its revisions
	RevisionID string // this revision's own address within the entity; empty on the current one
	Superseded bool   // this page is an older revision, not what the project holds now
	CurrentID  string // the EntityID to read for the current version; set only when Superseded

	// Revision, and the two links that make the chain walkable in both directions. `Previous` is
	// the revision this one replaced, `Next` the one that replaced it — absent on the current
	// revision, which is what defines it as the head.
	//
	// They were page frontmatter, which is where the memory protocol still tells an agent to read
	// them from. That stopped being reachable when page reads moved to the index, because a read
	// returns the body: the chain was in the file's frontmatter and the frontmatter was not
	// indexed. These columns are where it lives now.
	Revision int
	Previous string
	Next     string

	// Created is when the thing the page is about came into existence, as opposed to `Updated`,
	// which is when the source it was compiled from last changed.
	Created string

	// Staleness — the source this page was compiled from, or something it depends on, changed
	// after the page was written. Columns rather than page frontmatter for the same reason as
	// everything else here: the page is no longer written, so frontmatter is not a place a fact
	// can live.
	StaleSince  string
	StaleReason string
}

// BM25Result is one ranked hit, as knowledge_search and memory_search return them.
//
// It outlived the Go BM25 index it was named for — that ranked the markdown pages and was
// deleted with them. The name stays because the ranking is still BM25, now the engine's.
type BM25Result struct {
	Path    string
	Title   string
	DocType string
	Score   float64
	Snippet string

	// Supersession, carried out of the index so a caller can collapse an entity's revisions
	// without a second lookup.
	EntityID   string
	RevisionID string
	Superseded bool
	CurrentID  string
	Mandatory  bool
}

// WikiSearchResult is a single result from a wiki search.
type WikiSearchResult struct {
	Slug       string
	Title      string
	Summary    string
	DocType    string
	Breadcrumb string
	Score      float64
	Snippet    string
	CrossRefs  []string
	Source     string

	EntityID   string
	RevisionID string
	Superseded bool
	CurrentID  string
	Mandatory  bool
}

// BrowseFilter controls Browse() output.
type BrowseFilter struct {
	DocType   string
	ClusterID int // -1 = any
	Important *bool
	Limit     int
}

// BrowseEntry is a single entry returned by Browse.
type BrowseEntry struct {
	Slug        string
	Title       string
	Summary     string
	DocType     string
	Breadcrumb  string
	ClusterName string
	Confidence  float64
	Important   bool
	WordCount   int
}

// XRefResult is a cross-reference returned by FindXRefs.
type XRefResult struct {
	Slug      string
	Title     string
	RefType   string
	Direction string // "outbound" or "inbound"
}

// SyncLogEntry records one wiki sync operation.
type SyncLogEntry struct {
	ID              int
	Timestamp       string
	TotalDocs       int
	ArticlesWritten int
	BacklinksAdded  int
	Added           []string
	Updated         []string
	Deleted         []string
	Details         map[string]LogDocDetails
}

// LogDocDetails holds per-document metadata in sync log entries.
type LogDocDetails struct {
	Title   string `json:"title"`
	Summary string `json:"summary"`
}

// StoredEmbedding is one indexed vector with the source file and content hash it belongs to,
// so callers can inspect or reuse indexed embeddings without a sidecar cache.
type StoredEmbedding struct {
	Source      string
	ContentHash string
	Vector      []float32
}
