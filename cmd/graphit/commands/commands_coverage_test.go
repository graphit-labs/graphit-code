package commands

import (
	"testing"
	"time"
)

func TestFormatDuration(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		d    time.Duration
		want string
	}{
		{name: "zero", d: 0, want: "0s"},
		{name: "negative clamps to zero", d: -5 * time.Second, want: "0s"},
		{name: "seconds only", d: 45 * time.Second, want: "45s"},
		{name: "one minute exact", d: time.Minute, want: "1m"},
		{name: "minutes and seconds", d: 3*time.Minute + 15*time.Second, want: "3m15s"},
		{name: "one hour exact", d: time.Hour, want: "1h"},
		{name: "hours and minutes", d: 2*time.Hour + 30*time.Minute, want: "2h30m"},
		{name: "one day exact", d: 24 * time.Hour, want: "1d"},
		{name: "days and hours", d: 3*24*time.Hour + 5*time.Hour, want: "3d5h"},
		{name: "days without hours", d: 2 * 24 * time.Hour, want: "2d"},
		{name: "hours without minutes", d: 5 * time.Hour, want: "5h"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := formatDuration(tt.d)
			if got != tt.want {
				t.Errorf("formatDuration(%v) = %q; want %q", tt.d, got, tt.want)
			}
		})
	}
}

func TestHumanSize(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name  string
		bytes int64
		want  string
	}{
		{name: "zero bytes", bytes: 0, want: "0 B"},
		{name: "small bytes", bytes: 512, want: "512 B"},
		{name: "one KB boundary", bytes: 1024, want: "1.0 KB"},
		{name: "fractional KB", bytes: 1536, want: "1.5 KB"},
		{name: "large KB", bytes: 500 * 1024, want: "500.0 KB"},
		{name: "one MB boundary", bytes: 1024 * 1024, want: "1.0 MB"},
		{name: "fractional MB", bytes: 3*1024*1024 + 512*1024, want: "3.5 MB"},
		{name: "just under KB", bytes: 1023, want: "1023 B"},
		{name: "just under MB", bytes: 1024*1024 - 1, want: "1024.0 KB"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := humanSize(tt.bytes)
			if got != tt.want {
				t.Errorf("humanSize(%d) = %q; want %q", tt.bytes, got, tt.want)
			}
		})
	}
}
