package hub

import (
	"fmt"
	"sort"
	"strconv"
	"strings"
)

type VersionConstraint struct {
	Raw   string
	Major int
	Minor int
	Patch int
}

func ParseVersionConstraint(raw string) (*VersionConstraint, error) {
	c := &VersionConstraint{Raw: raw, Major: -1, Minor: -1, Patch: -1}
	if raw == "" || raw == "latest" {
		return c, nil
	}

	s := strings.TrimPrefix(raw, "v")
	parts := strings.Split(s, ".")

	if len(parts) > 3 {
		return nil, fmt.Errorf("invalid version constraint %q: too many parts", raw)
	}

	for i, p := range parts {
		if p == "" {
			return nil, fmt.Errorf("invalid version constraint %q: empty segment", raw)
		}
		n, err := strconv.Atoi(p)
		if err != nil {
			return nil, fmt.Errorf("invalid version constraint %q: %w", raw, err)
		}
		if n < 0 {
			return nil, fmt.Errorf("invalid version constraint %q: negative number", raw)
		}
		switch i {
		case 0:
			c.Major = n
		case 1:
			c.Minor = n
		case 2:
			c.Patch = n
		}
	}

	return c, nil
}

func (c *VersionConstraint) IsLatest() bool {
	return c.Major < 0
}

func (c *VersionConstraint) IsExact() bool {
	return c.Major >= 0 && c.Minor >= 0 && c.Patch >= 0
}

func (c *VersionConstraint) Matches(version string) bool {
	if c.IsLatest() {
		return true
	}

	v := strings.TrimPrefix(version, "v")
	parts := strings.Split(v, ".")
	if len(parts) < 1 {
		return false
	}

	major, err := strconv.Atoi(parts[0])
	if err != nil {
		return false
	}
	if major != c.Major {
		return false
	}

	if c.Minor < 0 {
		return true
	}

	if len(parts) < 2 {
		return false
	}
	minor, err := strconv.Atoi(parts[1])
	if err != nil {
		return false
	}
	if minor != c.Minor {
		return false
	}

	if c.Patch < 0 {
		return true
	}

	if len(parts) < 3 {
		return false
	}

	patchStr := parts[2]
	if idx := strings.IndexAny(patchStr, "-+"); idx >= 0 {
		patchStr = patchStr[:idx]
	}
	patch, err := strconv.Atoi(patchStr)
	if err != nil {
		return false
	}
	return patch == c.Patch
}

func ResolveVersion(versions []string, constraint *VersionConstraint) (string, error) {
	if constraint.IsLatest() && len(versions) > 0 {

		sorted := make([]string, len(versions))
		copy(sorted, versions)
		SortVersionsDesc(sorted)
		return sorted[0], nil
	}

	var matching []string
	for _, v := range versions {
		if constraint.Matches(v) {
			matching = append(matching, v)
		}
	}

	if len(matching) == 0 {
		return "", fmt.Errorf("no version matching constraint %q", constraint.Raw)
	}

	SortVersionsDesc(matching)
	return matching[0], nil
}

func SortVersionsDesc(versions []string) {
	sort.Slice(versions, func(i, j int) bool {
		return compareSemver(versions[i], versions[j]) > 0
	})
}

func compareSemver(a, b string) int {
	pa := parseSemverParts(a)
	pb := parseSemverParts(b)

	for i := 0; i < 3; i++ {
		if pa[i] > pb[i] {
			return 1
		}
		if pa[i] < pb[i] {
			return -1
		}
	}
	return 0
}

func parseSemverParts(v string) [3]int {
	v = strings.TrimPrefix(v, "v")
	parts := strings.SplitN(v, ".", 3)
	var result [3]int
	for i, p := range parts {
		if i >= 3 {
			break
		}

		if idx := strings.IndexAny(p, "-+"); idx >= 0 {
			p = p[:idx]
		}
		n, _ := strconv.Atoi(p)
		result[i] = n
	}
	return result
}
