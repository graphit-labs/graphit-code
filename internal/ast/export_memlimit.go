package ast

import (
	"log/slog"
	"math"
	"os"
	"runtime/debug"
	"strconv"
	"strings"
	"sync"
)

// exportMemoryHeadroom is the share of the machine's memory the export is allowed to use
// as a soft limit. Go's default GC target is twice the live heap, which on a corpus large
// enough to matter means the peak is set by the GC rather than by the data — and the kernel
// resolves that by killing the process. A soft limit inverts it: the collector runs as
// often as it has to, and the export gets slower instead of dying.
const exportMemoryHeadroom = 0.75

// SAFETY: the limit is process-wide, and the daemon rebuilds more than one project. The
// counter is what keeps a rebuild that finishes early from lifting the limit off one that
// is still running.
var (
	exportLimitMu    sync.Mutex
	exportLimitHeld  int
	exportLimitPrior int64
)

// applyExportMemoryLimit installs the soft limit for the duration of a rebuild and returns
// the undo. An explicit GOMEMLIMIT is left alone: the operator asked for a number.
func applyExportMemoryLimit(logger *slog.Logger) func() {
	limit := exportMemoryLimitBytes()
	if limit <= 0 {
		return func() {}
	}

	exportLimitMu.Lock()
	defer exportLimitMu.Unlock()
	if exportLimitHeld == 0 {
		exportLimitPrior = debug.SetMemoryLimit(limit)
		if logger != nil {
			logger.Debug("icebug rebuild memory limit", "limit_bytes", limit)
		}
	}
	exportLimitHeld++

	var once sync.Once
	return func() {
		once.Do(func() {
			exportLimitMu.Lock()
			defer exportLimitMu.Unlock()
			exportLimitHeld--
			if exportLimitHeld == 0 {
				debug.SetMemoryLimit(exportLimitPrior)
			}
		})
	}
}

// exportMemoryLimitBytes is 0 when there is nothing to install: an operator already named a
// limit, or the machine's capacity cannot be read.
func exportMemoryLimitBytes() int64 {
	exportLimitMu.Lock()
	held := exportLimitHeld
	exportLimitMu.Unlock()
	if held == 0 {
		if current := debug.SetMemoryLimit(-1); current != math.MaxInt64 {
			return 0
		}
	}
	total := availableMemoryBytes()
	if total <= 0 {
		return 0
	}
	return int64(float64(total) * exportMemoryHeadroom)
}

// availableMemoryBytes is the smaller of the machine's memory and the cgroup's cap, or 0
// when neither can be read — a container is capped well below MemTotal and it is the cap
// the kernel enforces.
func availableMemoryBytes() int64 {
	total := procMemTotalBytes()
	if cg := cgroupMemoryLimitBytes(); cg > 0 && (total == 0 || cg < total) {
		total = cg
	}
	return total
}

func procMemTotalBytes() int64 {
	raw, err := os.ReadFile("/proc/meminfo")
	if err != nil {
		return 0
	}
	for _, line := range strings.Split(string(raw), "\n") {
		if !strings.HasPrefix(line, "MemTotal:") {
			continue
		}
		fields := strings.Fields(line)
		if len(fields) < 2 {
			return 0
		}
		kb, err := strconv.ParseInt(fields[1], 10, 64)
		if err != nil {
			return 0
		}
		return kb * 1024
	}
	return 0
}

func cgroupMemoryLimitBytes() int64 {
	for _, path := range []string{
		"/sys/fs/cgroup/memory.max",
		"/sys/fs/cgroup/memory/memory.limit_in_bytes",
	} {
		raw, err := os.ReadFile(path)
		if err != nil {
			continue
		}
		text := strings.TrimSpace(string(raw))
		if text == "max" {
			continue
		}
		n, err := strconv.ParseInt(text, 10, 64)
		if err != nil || n <= 0 || n == math.MaxInt64 {
			continue
		}
		return n
	}
	return 0
}
