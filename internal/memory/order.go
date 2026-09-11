package memory

import (
	"sort"
	"time"
)

// PriorityLabel names the canonical ordering group for a memory. Mandatory always wins over
// important when both flags are set.
func PriorityLabel(mandatory, important bool) string {
	switch {
	case mandatory:
		return "mandatory"
	case important:
		return "important"
	default:
		return "normal"
	}
}

func priorityRank(mandatory, important bool) int {
	switch {
	case mandatory:
		return 0
	case important:
		return 1
	default:
		return 2
	}
}

func memoryComesBefore(
	leftMandatory, leftImportant bool,
	leftUpdated, leftCreated, leftID string,
	rightMandatory, rightImportant bool,
	rightUpdated, rightCreated, rightID string,
) bool {
	leftRank := priorityRank(leftMandatory, leftImportant)
	rightRank := priorityRank(rightMandatory, rightImportant)
	if leftRank != rightRank {
		return leftRank < rightRank
	}
	leftDate := effectiveMemoryDate(leftUpdated, leftCreated)
	rightDate := effectiveMemoryDate(rightUpdated, rightCreated)
	if !leftDate.Equal(rightDate) {
		return leftDate.After(rightDate)
	}
	// RFC3339 strings are a useful deterministic fallback for malformed or absent legacy dates.
	leftRaw := leftUpdated
	if leftRaw == "" {
		leftRaw = leftCreated
	}
	rightRaw := rightUpdated
	if rightRaw == "" {
		rightRaw = rightCreated
	}
	if leftRaw != rightRaw {
		return leftRaw > rightRaw
	}
	return leftID < rightID
}

func effectiveMemoryDate(updated, created string) time.Time {
	for _, value := range []string{updated, created} {
		if parsed, err := time.Parse(time.RFC3339Nano, value); err == nil {
			return parsed
		}
	}
	return time.Time{}
}

// SortMemoryEntries applies the public Memory ordering contract in place: mandatory, important,
// normal; newest updated_at (or created_at when absent) first inside each group.
func SortMemoryEntries(entries []MemoryEntry) {
	sort.SliceStable(entries, func(i, j int) bool {
		return memoryComesBefore(
			entries[i].Mandatory, entries[i].Important, entries[i].UpdatedAt, entries[i].CreatedAt, entries[i].ID,
			entries[j].Mandatory, entries[j].Important, entries[j].UpdatedAt, entries[j].CreatedAt, entries[j].ID,
		)
	})
}

// SortChainResults applies the same public ordering contract to search results. Match score remains
// metadata and never outranks a memory's category or recency.
func SortChainResults(results []ChainResult) {
	sort.SliceStable(results, func(i, j int) bool {
		return memoryComesBefore(
			results[i].Mandatory, results[i].Important, results[i].UpdatedAt, results[i].CreatedAt, results[i].Path,
			results[j].Mandatory, results[j].Important, results[j].UpdatedAt, results[j].CreatedAt, results[j].Path,
		)
	})
}

func sortMemoryRecords(records []MemoryRecord) {
	sort.SliceStable(records, func(i, j int) bool {
		if records[i].Superseded != records[j].Superseded {
			return !records[i].Superseded
		}
		return memoryComesBefore(
			records[i].Mandatory, records[i].Important, records[i].UpdatedAt, records[i].CreatedAt, records[i].Key(),
			records[j].Mandatory, records[j].Important, records[j].UpdatedAt, records[j].CreatedAt, records[j].Key(),
		)
	})
}
