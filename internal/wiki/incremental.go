package wiki

import "sort"

// CrossRefsUnchanged reports whether none of the changed or deleted source
// keys altered their outgoing cross-references.
//
// When this returns true, the caller can safely skip:
//   - BuildCrossRefGraph  (reads all wiki pages from disk)
//   - InjectBacklinks     (rewrites wiki pages with backlink sections)
//   - Community detection (depends on the cross-ref graph)
func CrossRefsUnchanged(deletedWithRefs, changedKeys []string, oldOutRefs, newOutRefs map[string][]string) bool {
	if len(deletedWithRefs) > 0 {
		return false
	}

	for _, key := range changedKeys {
		if !sortedStringSliceEqual(sortedCopy(oldOutRefs[key]), sortedCopy(newOutRefs[key])) {
			return false
		}
	}
	return true
}

// sortedCopy returns a sorted, deduplicated copy of ss.
func sortedCopy(ss []string) []string {
	if len(ss) == 0 {
		return nil
	}
	out := make([]string, len(ss))
	copy(out, ss)
	sort.Strings(out)

	j := 0
	for i, s := range out {
		if i == 0 || s != out[i-1] {
			out[j] = s
			j++
		}
	}
	return out[:j]
}

func sortedStringSliceEqual(a, b []string) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}
