package wiki

import (
	"context"
	"fmt"
	"log/slog"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

// WikiEmbedConfig controls wiki embedding behavior.
type WikiEmbedConfig struct {
	BatchSize      int
	MaxSourceChars int
	OnProgress     func(done, total int)
}

func DefaultWikiEmbedConfig() WikiEmbedConfig {
	return WikiEmbedConfig{
		BatchSize: 64,
		// A chunk is a whole document now, and 800 characters of it was roughly 200
		// tokens against a 512-token model window — the rest of the window sat unused
		// while three quarters of every document went unrepresented. This fills it:
		// ~1600 characters plus the breadcrumb, title and summary that precede the
		// body in the embedded text. The tokenizer truncates anything over the limit,
		// so overshooting costs nothing but wasted characters.
		MaxSourceChars: 1600,
	}
}

// WikiEmbedder generates vector embeddings for wiki chunks.
type WikiEmbedder struct {
	client ai.EmbeddingClient
	cfg    WikiEmbedConfig
	Logger *slog.Logger
}

func (e *WikiEmbedder) log() *slog.Logger { return slogutil.Resolve(e.Logger) }

// NewWikiEmbedder creates a new wiki embedder.
func NewWikiEmbedder(client ai.EmbeddingClient, cfg WikiEmbedConfig) *WikiEmbedder {
	return &WikiEmbedder{client: client, cfg: cfg}
}

// fitVectorWidth forces vec to exactly dim — see the identical helper and its comment in
// internal/ast/query.go, which this mirrors for the wiki's own vector column.
func fitVectorWidth(vec []float32, dim int) []float32 {
	if len(vec) == dim {
		return vec
	}
	if len(vec) > dim {
		return vec[:dim]
	}
	padded := make([]float32, dim)
	copy(padded, vec)
	return padded
}

// chunkRow is a chunk needing an embedding.
type chunkRow struct {
	// NO ID FIELD, deliberately. It used to be the chunk's SQLite rowid, and when the store
	// changed there was nothing stable to put there — the first port filled it with the loop
	// index, which LOOKS like an identifier and is not: embed one chunk, and the next call
	// hands that same number to a different chunk. A test caught it. The slug is the identity.
	Slug       string
	Title      string
	Summary    string
	Body       string
	DocType    string
	Breadcrumb string
	WordCount  int
}

// RunCycle opens the wiki DB, finds chunks without embeddings, generates them,
// and stores them. Returns the count of newly embedded chunks.
func (e *WikiEmbedder) RunCycle(ctx context.Context, wikiDir string) (int, error) {
	db, err := OpenWikiDB(ctx, wikiDir)
	if err != nil {
		return 0, fmt.Errorf("open wiki db: %w", err)
	}
	defer func() { _ = db.Close() }()

	pending, err := db.PendingEmbeddings(ctx)
	if err != nil {
		return 0, fmt.Errorf("pending embeddings: %w", err)
	}
	if len(pending) == 0 {
		return 0, nil
	}

	if e.cfg.OnProgress != nil {
		e.cfg.OnProgress(0, len(pending))
	}

	done := 0
	for i := 0; i < len(pending); i += e.cfg.BatchSize {
		if ctx.Err() != nil {
			return done, ctx.Err()
		}

		end := i + e.cfg.BatchSize
		if end > len(pending) {
			end = len(pending)
		}
		batch := pending[i:end]

		texts := make([]string, len(batch))
		for j, row := range batch {
			texts[j] = e.buildEmbeddingText(row)
		}

		vectors, err := e.client.EmbedBatch(ctx, texts)
		if err != nil {
			return done, fmt.Errorf("embed batch: %w", err)
		}
		if len(vectors) != len(batch) {
			return done, fmt.Errorf("expected %d vectors, got %d", len(batch), len(vectors))
		}

		for j, row := range batch {
			vec := vectors[j]
			if len(vec) == 0 {
				continue
			}
			if dim := e.client.Dimensions(); len(vec) != dim {
				vec = fitVectorWidth(vec, dim)
			}

			// Keyed by SLUG, not by a row id. The id used to be the chunk's SQLite rowid,
			// which only means something inside one build — so a rebuild between the read
			// and the write attached the vector to whatever now held that number. The slug
			// identifies the page itself.
			if err := db.SetChunkVector(ctx, row.Slug, vec); err != nil {
				e.log().Warn("attach vector", "slug", row.Slug, "error", err)
				continue
			}
			done++
		}

		if e.cfg.OnProgress != nil {
			e.cfg.OnProgress(done, len(pending))
		}
	}

	e.log().Info("wiki embedding cycle", "embedded", done)

	// Vectors were written in WAL mode. Replication copies the database file and
	// deliberately not its log, so fold the log in now or every replica of this wiki
	// gets the chunks without their embeddings.
	if done > 0 {
		db.Checkpoint()
	}

	// Auto-export embeddings to per-file shards for cache persistence.
	if done > 0 {
		if pc, err := NewWikiProcessCache(wikiDir); err == nil {
			exported := pc.ExportAllEmbeddingsFromDB(ctx, db)
			if saveErr := pc.Save(); saveErr != nil {
				e.log().Warn("save embedding shards", "error", saveErr)
			} else if exported > 0 {
				e.log().Info("wiki embedding shards exported", "vectors", exported)
			}
		}
	}

	return done, nil
}

// CountPending returns the number of chunks without embeddings.
func (e *WikiEmbedder) CountPending(ctx context.Context, wikiDir string) int {
	db, err := OpenWikiDB(ctx, wikiDir)
	if err != nil {
		return 0
	}
	defer func() { _ = db.Close() }()

	pending, err := db.PendingEmbeddings(ctx)
	if err != nil {
		return 0
	}
	return len(pending)
}

// buildEmbeddingText creates the text representation for embedding.
func (e *WikiEmbedder) buildEmbeddingText(row chunkRow) string {
	var parts []string

	if row.DocType != "" {
		parts = append(parts, "["+row.DocType+"] "+row.Breadcrumb)
	} else {
		parts = append(parts, row.Breadcrumb)
	}

	parts = append(parts, row.Title)

	if row.Summary != "" {
		parts = append(parts, row.Summary)
	}

	body := row.Body
	if len(body) > e.cfg.MaxSourceChars {
		body = body[:e.cfg.MaxSourceChars]
	}
	if body != "" {
		parts = append(parts, body)
	}

	return strings.Join(parts, "\n")
}
