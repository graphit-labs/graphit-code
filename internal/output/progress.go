package output

import (
	"fmt"
	"strings"
)

const (
	barFilled = "█"
	barEmpty  = "░"
)

// HumanBytes renders a byte count in the largest unit that keeps it short,
// which is the constraint a progress line is under: it shares one terminal row
// with a label and a bar, and it is rewritten several times a second.
func HumanBytes(n int64) string {
	switch {
	case n >= 1<<20:
		return fmt.Sprintf("%.1f MB", float64(n)/float64(1<<20))
	case n >= 1<<10:
		return fmt.Sprintf("%.1f KB", float64(n)/float64(1<<10))
	default:
		return fmt.Sprintf("%d B", n)
	}
}

// ProgressBar renders fraction as a bar exactly width cells wide, brackets
// excluded. Fractions outside [0,1] are clamped rather than rejected: the
// caller is usually dividing two numbers a server reported, and a download
// that overshoots a stale Content-Length should still draw something.
func ProgressBar(fraction float64, width int) string {
	if width <= 0 {
		return ""
	}
	if fraction < 0 {
		fraction = 0
	}
	if fraction > 1 {
		fraction = 1
	}

	filled := int(fraction*float64(width) + 0.5)
	return "[" + strings.Repeat(barFilled, filled) + strings.Repeat(barEmpty, width-filled) + "]"
}

// DownloadLine composes the one line a download reports itself on.
//
// A total of zero means the server declared no Content-Length, which is a real
// case and not an error — there is then no percentage and no bar to draw, so
// the line degrades to the byte count alone rather than showing a bar that
// would have to lie about how far along it is. A barWidth of zero omits the
// bar while keeping the percentage, which is what a log file wants: the bar is
// only legible when it is rewritten in place.
func DownloadLine(label string, downloaded, total int64, barWidth int) string {
	if total <= 0 {
		return fmt.Sprintf("%s  %s", label, HumanBytes(downloaded))
	}

	fraction := float64(downloaded) / float64(total)

	parts := []string{label}
	if bar := ProgressBar(fraction, barWidth); bar != "" {
		parts = append(parts, bar)
	}
	parts = append(parts, fmt.Sprintf("%3.0f%%", fraction*100))
	parts = append(parts, fmt.Sprintf("%s / %s", HumanBytes(downloaded), HumanBytes(total)))

	return strings.Join(parts, "  ")
}
