package task

import (
	"context"

	"github.com/graphit-labs/graphit-code/internal/lancequery"
)

// StoreTables is every table the Task store owns, in the order a reader should meet them:
// the record first, then its relationships, then its history, then the session layer.
func StoreTables() []string {
	return []string{
		tasksTableName, dependenciesTableName, checksTableName, commentsTableName,
		eventsTableName, specRevisionsTableName, controlTableName,
		sessionsTableName, sessionEventsTableName, sessionCheckpointsTableName, sessionRevisionsTableName,
	}
}

// StorePolicy is what the inspection surface may see.
//
// THE REDACTED SET IS NOT A GUESS. Each entry was found by reading the stored schema and then
// confirmed against this project's real store:
//
//   - tasks.claim_token is the worker fencing token, in its own column.
//   - task_control.token is the scheduler lock token.
//   - task_sessions.snapshot_json is the surprise. There is no claim_token COLUMN on the
//     session table, so redacting by that name would have closed nothing; sessionRow marshals
//     the whole Session — including its ClaimToken field, tagged `json:"claim_token"` — into
//     that blob. Probing the live store for the coordinator token this session holds found it
//     there, in plaintext, and nowhere else. Redacting the blob is what actually closes it.
//
// The cost of that last one is real and worth stating: Session carries ProgressSummary and
// NextStep, which sessionSchema does not give columns of their own, so they live only inside
// the snapshot. Redacting it puts them out of query reach; session_get still returns them.
// Leaking a live coordination token to buy two fields back is not a trade worth making.
func StorePolicy() lancequery.Policy {
	return lancequery.Policy{
		Tables: StoreTables(),
		Redacted: map[string][]string{
			tasksTableName:    {"claim_token"},
			controlTableName:  {"token"},
			sessionsTableName: {"snapshot_json"},
		},
		// Heavy is about the size of an answer, not its secrecy. A task description runs to
		// kilobytes and search_text concatenates several of them; returning those by default
		// would reproduce the very cost this surface exists to avoid. Name one to get it.
		Heavy: map[string][]string{
			tasksTableName: {
				"description", "progress_summary", "next_step", "search_text",
				"checks_json", "last_event_json", "last_comment_json",
			},
			checksTableName:             {"text", "evidence"},
			commentsTableName:           {"body"},
			eventsTableName:             {"summary", "next_step"},
			specRevisionsTableName:      {"before_json", "after_json"},
			sessionsTableName:           {"description", "strategy", "search_text"},
			sessionEventsTableName:      {"body_json", "search_text"},
			sessionCheckpointsTableName: {"body_json", "search_text"},
			sessionRevisionsTableName:   {"body_json", "search_text"},
		},
	}
}

// DescribeStore reports the shape of the Task tables. Passing names in `only` narrows it.
func (s *Service) DescribeStore(ctx context.Context, only []string) (lancequery.Schema, error) {
	var out lancequery.Schema
	// withTables is reused rather than opening the store directly: it resolves project
	// identity and authorizes a remote store, and skipping either would make inspection the
	// one read path in this module with different rules from every other.
	err := s.withTables(ctx, func(t *tables) error {
		schema, err := lancequery.Describe(ctx, t.store, StorePolicy(), only)
		out = schema
		return err
	})
	return out, err
}

// QueryStore runs one predicate query against one Task table. It never writes and never
// returns a redacted column.
func (s *Service) QueryStore(ctx context.Context, req lancequery.Request) (lancequery.Result, error) {
	var out lancequery.Result
	err := s.withTables(ctx, func(t *tables) error {
		result, err := lancequery.Run(ctx, t.store, StorePolicy(), req)
		out = result
		return err
	})
	return out, err
}
