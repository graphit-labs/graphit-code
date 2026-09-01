package memory

import (
	"fmt"
	"os"
	"path"
	"path/filepath"
	"sort"
	"strings"
)

// RepairReport counts what one repair pass changed. A zero report means the store was already
// consistent, which is the normal case after the first pass.
type RepairReport struct {
	Removed  []string
	Archived []string
	Promoted []string
	Linked   []string
}

// Changed reports whether the pass modified anything.
func (r RepairReport) Changed() bool {
	return len(r.Removed) > 0 || len(r.Archived) > 0 || len(r.Promoted) > 0 || len(r.Linked) > 0
}

func (r RepairReport) String() string {
	return fmt.Sprintf("removed %d, archived %d, promoted %d, linked %d",
		len(r.Removed), len(r.Archived), len(r.Promoted), len(r.Linked))
}

// RepairForkedMemories folds every twin memory back into the chain it was forked from.
//
// A twin is a file whose name is not `<its declared id>.md`. It exists because a write path once
// recovered the memory id from the FILE NAME: given `<ulid>_important_.md` — the fossil of a
// layout where the name carried the importance flag — MemoryIDFromFileName yielded
// `<ulid>_important_`, and the write landed on a new file under that corrupted id. The twin then
// evolved on its own, with its own revisions and its own history directory. Measured in this
// repository's project scope: 184 twins over 312 real memories, every one of them compiled into
// the wiki as a second page, so every search answered one memory twice.
//
// Resolution per twin, in this order, because it is ordered by how much can be lost:
//
//  1. no live memory for the chain — the twin IS the memory. It is promoted to `<id>.md` with the
//     id corrected, because deleting it would be the only copy going.
//  2. its body already exists, in the live memory or in any archived revision of the chain —
//     nothing is lost, so it is removed.
//  3. its body exists nowhere — it is a genuine rewrite that landed on the wrong twin. It is
//     archived into the chain as a superseded revision, then removed. Searchable, annotated with
//     the current revision, and not a duplicate.
//
// Every removal goes through ScopeStore.RemoveFile rather than os.Remove: a Pull MERGES and never
// deletes locally to match the remote, so a plain unlink is undone by the next sync while the
// object is still in the bucket. This is also why the repair is code and not a script — every
// clone of the bucket carries the same twins and heals itself on its next index.
//
// Idempotent: a store with no twins costs one directory listing and returns an empty report.
func (m *MemoryService) RepairForkedMemories() (RepairReport, error) {
	var report RepairReport

	if m.store == nil {
		return report, nil
	}

	scope, err := m.store.OpenScopeLocal(m.ScopePrefix())
	if err != nil {
		return report, fmt.Errorf("opening the memory scope: %w", err)
	}

	entries, err := scope.ListDir(".")
	if err != nil {
		if os.IsNotExist(err) {
			return report, nil
		}
		return report, fmt.Errorf("listing the memory scope: %w", err)
	}

	for _, e := range entries {
		name := e.Name()
		if e.IsDir() || filepath.Ext(name) != ".md" {
			continue
		}
		if name == "index.md" || name == "log.md" || strings.HasPrefix(name, "Memory_Wiki") {
			continue
		}

		data, readErr := scope.ReadFile(name)
		if readErr != nil {
			continue
		}
		chainID := chainIDOf(string(data), name)
		if chainID == "" || name == MemoryFileName(chainID) {
			continue
		}

		if err := m.foldTwin(scope, &report, name, chainID, string(data)); err != nil {
			m.log().Warn("repair: folding a forked memory failed", "file", name, "error", err)
		}
	}

	if err := m.foldForkedHistoryDirs(scope, &report); err != nil {
		m.log().Warn("repair: folding forked history directories failed", "error", err)
	}

	if err := m.backfillChainLinks(scope, &report); err != nil {
		m.log().Warn("repair: backfilling chain links failed", "error", err)
	}

	if report.Changed() {
		if err := scope.Publish(fmt.Sprintf("memory: repair forked ids (%s)", report)); err != nil {
			m.log().Warn("repair: publishing failed", "error", err)
		}
	}
	return report, nil
}

// foldTwin applies the three-way resolution to one twin file.
func (m *MemoryService) foldTwin(scope *ScopeStore, report *RepairReport, name, chainID, content string) error {
	live, liveErr := scope.ReadFile(MemoryFileName(chainID))

	if liveErr != nil {
		promoted := promotedMemoryContent(content, chainID)
		if err := scope.WriteFile(MemoryFileName(chainID), []byte(promoted)); err != nil {
			return fmt.Errorf("promoting %s to %s: %w", name, chainID, err)
		}
		if err := scope.RemoveFile(name); err != nil {
			return fmt.Errorf("removing %s after promotion: %w", name, err)
		}
		report.Promoted = append(report.Promoted, chainID)
		return nil
	}

	if sameMemoryBody(content, string(live)) || m.chainHoldsBody(scope, chainID, content) {
		if err := scope.RemoveFile(name); err != nil {
			return fmt.Errorf("removing the duplicate %s: %w", name, err)
		}
		report.Removed = append(report.Removed, name)
		return nil
	}

	archived, err := m.archiveRevision(scope, chainID, content, MemoryFileName(chainID))
	if err != nil {
		return err
	}
	if err := scope.RemoveFile(name); err != nil {
		return fmt.Errorf("removing %s after archiving it: %w", name, err)
	}
	report.Archived = append(report.Archived, archived)
	return nil
}

// foldForkedHistoryDirs merges `history/<corrupted-id>/` into `history/<chain-id>/`.
//
// A twin that reached more than one revision archived its own history under its corrupted id.
// Those revisions belong to the chain the twin was forked from, and reading them requires being
// under the chain's directory, because that is what HistoryPath addresses.
func (m *MemoryService) foldForkedHistoryDirs(scope *ScopeStore, report *RepairReport) error {
	chains, err := scope.ListDir(HistoryDirName)
	if err != nil {
		if os.IsNotExist(err) {
			return nil
		}
		return err
	}

	for _, chain := range chains {
		if !chain.IsDir() {
			continue
		}
		forkedID := chain.Name()
		chainID := chainIDOf("", forkedID)
		if chainID == "" || chainID == forkedID {
			continue
		}

		revisions, err := scope.ListDir(path.Join(HistoryDirName, forkedID))
		if err != nil {
			continue
		}
		for _, rev := range revisions {
			if rev.IsDir() || filepath.Ext(rev.Name()) != ".md" {
				continue
			}
			rel := path.Join(HistoryDirName, forkedID, rev.Name())

			data, readErr := scope.ReadFile(rel)
			if readErr != nil {
				continue
			}
			if !m.chainHoldsBody(scope, chainID, string(data)) {
				archived, err := m.archiveRevision(scope, chainID, string(data), MemoryFileName(chainID))
				if err != nil {
					m.log().Warn("repair: re-archiving a forked revision failed", "file", rel, "error", err)
					continue
				}
				report.Archived = append(report.Archived, archived)
			}
			if err := scope.RemoveFile(rel); err != nil {
				m.log().Warn("repair: removing a forked revision failed", "file", rel, "error", err)
				continue
			}
			report.Removed = append(report.Removed, rel)
		}

		// The directory itself is not an object, so it goes with a plain remove — and only when
		// it is empty, which fails harmlessly otherwise.
		_ = os.Remove(filepath.Join(scope.Dir(), HistoryDirName, forkedID))
	}
	return nil
}

// backfillChainLinks gives every archived revision the two fields that make the chain walkable.
//
// An archive written before `revision_id` and `next` existed declares neither, so it can say what
// it replaced but not what replaced it, and it cannot name its own address. The compiler no longer
// depends on either — a file under history/ is a superseded revision by location — but a reader
// walking forward does, and so does anything that wants to address one revision rather than a
// chain.
//
// Within a chain the successor is the next name in sorted order, which is correct for both naming
// schemes: the zero-padded counter and the ULID are each lexicographically ordered by age, and
// "0001" precedes every ULID, so a mixed directory sorts oldest-first. The newest archive's
// successor is the live memory.
func (m *MemoryService) backfillChainLinks(scope *ScopeStore, report *RepairReport) error {
	chains, err := scope.ListDir(HistoryDirName)
	if err != nil {
		if os.IsNotExist(err) {
			return nil
		}
		return err
	}

	for _, chain := range chains {
		if !chain.IsDir() || !IsMemoryID(chain.Name()) {
			continue
		}
		chainID := chain.Name()

		entries, err := scope.ListDir(HistoryDirFor(chainID))
		if err != nil {
			continue
		}
		var names []string
		for _, e := range entries {
			if !e.IsDir() && filepath.Ext(e.Name()) == ".md" {
				names = append(names, e.Name())
			}
		}
		if len(names) == 0 {
			continue
		}
		sort.Strings(names)

		for i, name := range names {
			rel := path.Join(HistoryDirFor(chainID), name)
			data, readErr := scope.ReadFile(rel)
			if readErr != nil {
				continue
			}

			fm, parsed := ParseMemoryFrontmatterOK(string(data))
			if !parsed {
				// SAFETY: this pass adds two navigation fields. Writing them back from a failed
				// parse would cost the revision its title, type, tags and timestamps — which is
				// exactly the trade that made 20 archives anonymous before this guard existed.
				m.log().Warn("repair: revision frontmatter unreadable; not backfilling", "file", rel)
				continue
			}
			wantRevisionID := strings.TrimSuffix(name, ".md")
			wantNext := MemoryFileName(chainID)
			if i+1 < len(names) {
				wantNext = path.Join(HistoryDirFor(chainID), names[i+1])
			}
			// A chain whose live memory is gone ends at its last revision: nothing replaced it,
			// and inventing a pointer to a file that does not exist would break a forward walk
			// rather than complete it.
			if i+1 == len(names) {
				if _, err := scope.ReadFile(MemoryFileName(chainID)); err != nil {
					wantNext = ""
				}
			}

			if fm.RevisionID == wantRevisionID && fm.Next == wantNext && fm.ID == chainID {
				continue
			}
			fm.RevisionID = wantRevisionID
			fm.Next = wantNext
			fm.ID = chainID
			if fm.Revision < 1 {
				fm.Revision = 1
			}

			if err := scope.WriteFile(rel, []byte(renderMemoryFile(fm, extractBodyAfterFrontmatter(string(data))))); err != nil {
				m.log().Warn("repair: backfilling a revision failed", "file", rel, "error", err)
				continue
			}
			report.Linked = append(report.Linked, rel)
		}
	}
	return nil
}

// chainHoldsBody reports whether any archived revision of a chain already carries this body.
func (m *MemoryService) chainHoldsBody(scope *ScopeStore, chainID, content string) bool {
	revisions, err := scope.ListDir(HistoryDirFor(chainID))
	if err != nil {
		return false
	}
	for _, rev := range revisions {
		if rev.IsDir() || filepath.Ext(rev.Name()) != ".md" {
			continue
		}
		data, err := scope.ReadFile(path.Join(HistoryDirFor(chainID), rev.Name()))
		if err != nil {
			continue
		}
		if sameMemoryBody(content, string(data)) {
			return true
		}
	}
	return false
}

// chainIDOf recovers the real memory id from a twin's content or name.
//
// A twin's own frontmatter id is corrupted too — it carries the same suffix the file name did,
// because that is where it came from — so neither source can be trusted whole. Both are truncated
// to the ULID prefix they start with, which is the chain the twin was forked from.
func chainIDOf(content, name string) string {
	candidates := []string{}
	if content != "" {
		candidates = append(candidates, ParseMemoryFrontmatter(content).ID)
	}
	candidates = append(candidates, strings.TrimSuffix(path.Base(filepath.ToSlash(name)), ".md"))

	for _, c := range candidates {
		if IsMemoryID(c) {
			return c
		}
		if len(c) > 26 && IsMemoryID(c[:26]) {
			return c[:26]
		}
	}
	return ""
}

// sameMemoryBody compares two memory files by body alone.
//
// Frontmatter is deliberately excluded: a twin and the memory it was forked from routinely differ
// in `updated_at`, `revision`, quoting style and the tags a buggy write path dropped, while
// carrying the same knowledge. Only the body decides whether removing one loses anything.
func sameMemoryBody(a, b string) bool {
	return strings.TrimSpace(extractBodyAfterFrontmatter(a)) ==
		strings.TrimSpace(extractBodyAfterFrontmatter(b))
}

// promotedMemoryContent turns an orphaned twin into the live memory of its chain.
func promotedMemoryContent(content, chainID string) string {
	fm, parsed := ParseMemoryFrontmatterOK(content)
	if !parsed {
		fm.Title = firstH1(content)
	}
	fm.ID = chainID
	fm.RevisionID = ""
	fm.Next = ""
	if fm.Revision < 1 {
		fm.Revision = 1
	}
	return renderMemoryFile(fm, extractBodyAfterFrontmatter(content))
}
