package hub

import (
	"fmt"
	"regexp"
	"sort"
	"strconv"
	"strings"
)

type VersionConstraint struct {
	Raw   string
	Name  string
	Major int
	Minor int
	Patch int
}

var sortableSemverRe = regexp.MustCompile(`^v?[0-9]+(?:\.[0-9]+){0,2}(?:[-+].*)?$`)

func ParseVersionConstraint(raw string) (*VersionConstraint, error) {
	c := &VersionConstraint{Raw: raw, Major: -1, Minor: -1, Patch: -1}
	if raw == "" || raw == "latest" {
		return c, nil
	}
	if !numericConstraintCandidate(raw) {
		if err := validateNamedVersion(raw); err != nil {
			return nil, err
		}
		c.Name = raw
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
	return c.Name == "" && c.Major < 0
}

func (c *VersionConstraint) IsExact() bool {
	return c.Name != "" || c.Major >= 0 && c.Minor >= 0 && c.Patch >= 0
}

func (c *VersionConstraint) Matches(version string) bool {
	if c.IsLatest() {
		return true
	}
	if c.Name != "" {
		return version == c.Name
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
		return compareVersions(versions[i], versions[j]) > 0
	})
}

func compareVersions(a, b string) int {
	aSemver := sortableSemverRe.MatchString(a)
	bSemver := sortableSemverRe.MatchString(b)
	if aSemver && bSemver {
		return compareSemver(a, b)
	}
	if aSemver {
		return 1
	}
	if bSemver {
		return -1
	}
	return strings.Compare(a, b)
}

func ValidatePublishedVersion(version string) error {
	if version == "" || strings.EqualFold(version, "latest") {
		return fmt.Errorf("version must name a numeric release or named channel, not %q", version)
	}
	_, err := ParseVersionConstraint(version)
	return err
}

func numericConstraintCandidate(raw string) bool {
	s := strings.TrimPrefix(raw, "v")
	if s == "" {
		return false
	}
	if s[0] == '-' {
		return len(s) > 1 && s[1] >= '0' && s[1] <= '9'
	}
	for _, r := range s {
		if r != '.' && (r < '0' || r > '9') {
			return false
		}
	}
	return true
}

func validateNamedVersion(version string) error {
	if strings.EqualFold(version, "latest") || version == "@" || strings.HasPrefix(version, "-") ||
		strings.HasPrefix(version, "/") || strings.HasSuffix(version, "/") ||
		strings.Contains(version, "//") || strings.Contains(version, "..") ||
		strings.Contains(version, "@{") {
		return fmt.Errorf("invalid named version %q", version)
	}
	for _, r := range version {
		if r <= ' ' || r == 0x7f || strings.ContainsRune(`~^:?*[\@`, r) {
			return fmt.Errorf("invalid named version %q", version)
		}
	}
	for _, segment := range strings.Split(version, "/") {
		lower := strings.ToLower(segment)
		if strings.HasPrefix(segment, ".") || strings.HasSuffix(segment, ".") || strings.HasSuffix(lower, ".lock") {
			return fmt.Errorf("invalid named version %q", version)
		}
	}
	return nil
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
