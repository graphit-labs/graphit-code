package prep

import (
	"os"

	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/store"
)

// Reclaim is the livesearch.ReclaimFunc. It deletes anything on this machine keyed by
// a session ID.
//
// A current session creates none of these. The workspace is marked ephemeral, so the
// knowledge wiki is never compiled for it, the memory scope is redirected to the
// user's, and a write to its own code graph is refused. This function is what clears
// what earlier versions of that same code left behind, and it is the reason removing
// a session is now genuinely complete rather than complete as far as the session
// directory goes.
//
// Every step is best-effort and silent. There is normally nothing to delete, "nothing
// there" and "deleted" are the same outcome from the caller's point of view, and a
// failure to reclaim must not turn a successful removal into an error.
func Reclaim(sessionID string) {
	if sessionID == "" {
		return
	}

	_ = os.RemoveAll(store.ASTProjectDirByID(sessionID))
	_ = os.RemoveAll(store.KnowledgeProjectDirByID(sessionID))
	_ = os.RemoveAll(store.MemoryWikiDir(string(memory.MemoryScopeProject), sessionID))
	_ = os.RemoveAll(store.MemoryTableDir(string(memory.MemoryScopeProject), sessionID))

	// The memory scope is the one that reached outside the global directory: opening
	// it created an orphan branch and a worktree in the shared memory repository, so
	// deleting the wiki alone would leave the branch behind.
	if gs, err := memory.NewMemoryStore(); err == nil {
		_ = gs.PruneScope(string(memory.MemoryScopeProject), sessionID)
	}
}
