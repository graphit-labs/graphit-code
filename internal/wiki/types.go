package wiki

// The wiki's vocabulary.
//
// These types outlived the storage engine they were written for. They describe what a wiki IS —
// a page, a search result, a cross-reference, one sync — and nothing in them was ever specific to
// SQLite, which is why the engine could be replaced without any caller changing shape.

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

// EmbeddingCache maps a chunk's content hash to its vector, so a rebuild can restore
// embeddings for unchanged text instead of paying for the model again.
//
// It holds []float32 rather than a serialized blob: the blob was sqlite-vec's wire format,
// and nothing outside the storage layer should have had to know it.
type EmbeddingCache map[string][]float32

// StoredEmbedding is one indexed vector with the source file and content hash it belongs to,
// which is what the process cache keys its shards by.
type StoredEmbedding struct {
	Source      string
	ContentHash string
	Vector      []float32
}
