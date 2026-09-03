package hub

import (
	"testing"
)

func TestParseVersionConstraint(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name      string
		raw       string
		wantMajor int
		wantMinor int
		wantPatch int
		wantErr   bool
	}{
		{name: "empty", raw: "", wantMajor: -1, wantMinor: -1, wantPatch: -1},
		{name: "latest", raw: "latest", wantMajor: -1, wantMinor: -1, wantPatch: -1},
		{name: "major only", raw: "1", wantMajor: 1, wantMinor: -1, wantPatch: -1},
		{name: "major.minor", raw: "1.2", wantMajor: 1, wantMinor: 2, wantPatch: -1},
		{name: "full", raw: "1.2.3", wantMajor: 1, wantMinor: 2, wantPatch: 3},
		{name: "v prefix", raw: "v1.2.3", wantMajor: 1, wantMinor: 2, wantPatch: 3},
		{name: "too many parts", raw: "1.2.3.4", wantErr: true},
		{name: "empty segment", raw: "1..3", wantErr: true},
		{name: "non-numeric", raw: "abc", wantErr: true},
		{name: "negative", raw: "-1", wantErr: true},
		{name: "zero", raw: "0", wantMajor: 0, wantMinor: -1, wantPatch: -1},
		{name: "zero.zero.zero", raw: "0.0.0", wantMajor: 0, wantMinor: 0, wantPatch: 0},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			c, err := ParseVersionConstraint(tt.raw)
			if tt.wantErr {
				if err == nil {
					t.Errorf("expected error, got nil")
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if c.Major != tt.wantMajor {
				t.Errorf("Major = %d, want %d", c.Major, tt.wantMajor)
			}
			if c.Minor != tt.wantMinor {
				t.Errorf("Minor = %d, want %d", c.Minor, tt.wantMinor)
			}
			if c.Patch != tt.wantPatch {
				t.Errorf("Patch = %d, want %d", c.Patch, tt.wantPatch)
			}
		})
	}
}

func TestVersionConstraintIsLatest(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name string
		raw  string
		want bool
	}{
		{name: "empty is latest", raw: "", want: true},
		{name: "latest is latest", raw: "latest", want: true},
		{name: "major is not latest", raw: "1", want: false},
		{name: "full is not latest", raw: "1.2.3", want: false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			c, err := ParseVersionConstraint(tt.raw)
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if got := c.IsLatest(); got != tt.want {
				t.Errorf("IsLatest() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestVersionConstraintIsExact(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name string
		raw  string
		want bool
	}{
		{name: "empty is not exact", raw: "", want: false},
		{name: "major only is not exact", raw: "1", want: false},
		{name: "major.minor is not exact", raw: "1.2", want: false},
		{name: "full is exact", raw: "1.2.3", want: true},
		{name: "zero full is exact", raw: "0.0.0", want: true},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			c, err := ParseVersionConstraint(tt.raw)
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if got := c.IsExact(); got != tt.want {
				t.Errorf("IsExact() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestVersionConstraintMatches(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name       string
		constraint string
		version    string
		want       bool
	}{
		{name: "latest matches any", constraint: "", version: "1.2.3", want: true},
		{name: "major match", constraint: "1", version: "1.2.3", want: true},
		{name: "major mismatch", constraint: "2", version: "1.2.3", want: false},
		{name: "major.minor match", constraint: "1.2", version: "1.2.3", want: true},
		{name: "major.minor mismatch", constraint: "1.3", version: "1.2.3", want: false},
		{name: "exact match", constraint: "1.2.3", version: "1.2.3", want: true},
		{name: "exact mismatch", constraint: "1.2.4", version: "1.2.3", want: false},
		{name: "v prefix version", constraint: "1.2.3", version: "v1.2.3", want: true},
		{name: "non-numeric major in version", constraint: "1", version: "abc", want: false},
		{name: "non-numeric minor in version", constraint: "1.2", version: "1.abc", want: false},
		{name: "non-numeric patch in version", constraint: "1.2.3", version: "1.2.abc", want: false},
		{name: "version lacks minor", constraint: "1.2", version: "1", want: false},
		{name: "version lacks patch", constraint: "1.2.3", version: "1.2", want: false},
		{name: "empty version parts", constraint: "1", version: "", want: false},
		{name: "pre-release match", constraint: "1.2.3", version: "1.2.3-beta", want: true},
		{name: "plus build match", constraint: "1.2.3", version: "1.2.3+build", want: true},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			c, err := ParseVersionConstraint(tt.constraint)
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if got := c.Matches(tt.version); got != tt.want {
				t.Errorf("Matches(%q) = %v, want %v", tt.version, got, tt.want)
			}
		})
	}
}

func TestResolveVersion(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name       string
		versions   []string
		constraint string
		want       string
		wantErr    bool
	}{
		{
			name:       "latest returns highest",
			versions:   []string{"1.0.0", "2.0.0", "1.5.0"},
			constraint: "",
			want:       "2.0.0",
		},
		{
			name:       "exact match",
			versions:   []string{"1.0.0", "2.0.0", "1.5.0"},
			constraint: "1.5.0",
			want:       "1.5.0",
		},
		{
			name:       "major constraint",
			versions:   []string{"1.0.0", "2.0.0", "1.5.0"},
			constraint: "1",
			want:       "1.5.0",
		},
		{
			name:       "major.minor constraint",
			versions:   []string{"1.0.0", "1.2.0", "1.2.5", "2.0.0"},
			constraint: "1.2",
			want:       "1.2.5",
		},
		{
			name:       "no matching version",
			versions:   []string{"1.0.0", "2.0.0"},
			constraint: "3.0.0",
			wantErr:    true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			c, err := ParseVersionConstraint(tt.constraint)
			if err != nil {
				t.Fatalf("unexpected parse error: %v", err)
			}
			got, err := ResolveVersion(tt.versions, c)
			if tt.wantErr {
				if err == nil {
					t.Errorf("expected error, got %q", got)
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if got != tt.want {
				t.Errorf("ResolveVersion() = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestSortVersionsDesc(t *testing.T) {
	t.Parallel()
	versions := []string{"1.0.0", "3.0.0", "2.0.0", "2.1.0", "1.5.0"}
	SortVersionsDesc(versions)
	expected := []string{"3.0.0", "2.1.0", "2.0.0", "1.5.0", "1.0.0"}
	for i, v := range versions {
		if v != expected[i] {
			t.Errorf("index %d: got %q, want %q", i, v, expected[i])
		}
	}
}

func TestCompareSemver(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name string
		a, b string
		want int
	}{
		{name: "equal", a: "1.2.3", b: "1.2.3", want: 0},
		{name: "a greater major", a: "2.0.0", b: "1.0.0", want: 1},
		{name: "b greater major", a: "1.0.0", b: "2.0.0", want: -1},
		{name: "a greater minor", a: "1.2.0", b: "1.1.0", want: 1},
		{name: "b greater minor", a: "1.1.0", b: "1.2.0", want: -1},
		{name: "a greater patch", a: "1.2.4", b: "1.2.3", want: 1},
		{name: "b greater patch", a: "1.2.3", b: "1.2.4", want: -1},
		{name: "v prefix", a: "v1.2.3", b: "v1.2.3", want: 0},
		{name: "prerelease stripped", a: "1.2.3-beta", b: "1.2.3", want: 0},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := compareSemver(tt.a, tt.b)
			if got != tt.want {
				t.Errorf("compareSemver(%q, %q) = %d, want %d", tt.a, tt.b, got, tt.want)
			}
		})
	}
}

func TestParseSemverParts(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name string
		v    string
		want [3]int
	}{
		{name: "full", v: "1.2.3", want: [3]int{1, 2, 3}},
		{name: "v prefix", v: "v1.2.3", want: [3]int{1, 2, 3}},
		{name: "two parts", v: "1.2", want: [3]int{1, 2, 0}},
		{name: "one part", v: "5", want: [3]int{5, 0, 0}},
		{name: "prerelease", v: "1.2.3-alpha", want: [3]int{1, 2, 3}},
		{name: "build meta", v: "1.2.3+build", want: [3]int{1, 2, 3}},
		{name: "non-numeric", v: "abc", want: [3]int{0, 0, 0}},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := parseSemverParts(tt.v)
			if got != tt.want {
				t.Errorf("parseSemverParts(%q) = %v, want %v", tt.v, got, tt.want)
			}
		})
	}
}
