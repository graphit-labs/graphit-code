package commands

import (
	"context"
	"fmt"
	"path/filepath"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/output"
)

const (
	// modelBarWidth leaves room for the label and both byte counts on an
	// 80-column terminal, which is the narrowest one worth designing for.
	modelBarWidth = 24

	// modelRefreshInterval throttles the terminal case. The line is rewritten
	// in place, so refreshing ten times a second costs nothing and reads as
	// motion rather than as a stall.
	modelRefreshInterval = 100 * time.Millisecond

	// modelRefreshPercent throttles the case with no cursor to move, where
	// every update is another line in a log. Tenths are enough to show that
	// something is happening without burying the rest of setup.
	modelRefreshPercent = 10

	// modelDownloadTimeout bounds the whole bundle. The download is around
	// 132 MB, which is minutes on a slow link — but a setup that hangs with no
	// end is worse than one that fails and says to try again, and since this
	// step is fatal a hang here is a hang of the whole installation.
	modelDownloadTimeout = 15 * time.Minute
)

// ensureEmbeddingModel puts the embedding model and its tokenizer in the shared
// cache under the brand directory, downloading them when they are not already
// there, and reports progress on p.
//
// It returns where the model is — or, when the error is non-nil, where it should
// have landed, so the caller can name the directory in a message about a download
// that produced no path — and whether anything had to be downloaded to get there.
//
// The error is fatal to setup. An installation with no model cannot embed, so it
// cannot answer a semantic query; finishing with a success message would hide
// that until some later search silently came back on keywords alone.
func ensureEmbeddingModel(ctx context.Context, p *output.Printer) (dir string, downloaded bool, err error) {
	cacheDir, cacheErr := ai.ModelCacheDir()
	if cacheErr != nil {
		return "", false, fmt.Errorf("resolving model cache: %w", cacheErr)
	}

	mgr, mgrErr := ai.NewModelManager()
	if mgrErr != nil {
		return cacheDir, false, fmt.Errorf("resolving model cache: %w", mgrErr)
	}

	reporter := &modelProgress{p: p, tty: output.IsTTY()}
	mgr.OnProgress = reporter.report

	ctx, cancel := context.WithTimeout(ctx, modelDownloadTimeout)
	defer cancel()

	modelPath, _, ensureErr := mgr.EnsureModel(ctx)
	if ensureErr != nil {
		return cacheDir, reporter.spoke, ensureErr
	}

	// Not necessarily the cache: a build that ships the weights beside the core
	// binary wins over it, and saying so is the point of returning a path.
	return filepath.Dir(modelPath), reporter.spoke, nil
}

// modelProgress turns the per-write callback into something a terminal — or a
// log file — can stand to read. Which of the two it is decides how often it may
// speak, so the throttle lives here rather than in the download.
type modelProgress struct {
	p *output.Printer

	// tty is captured once rather than asked per call, so that the throttle is
	// a decision about a value and can be exercised both ways in a test.
	tty bool

	// spoke records that at least one byte was reported, which is how the
	// caller tells a download apart from a cache hit without asking.
	spoke bool

	file      string
	announced bool
	lastAt    time.Time
	lastPct   int
}

func (r *modelProgress) report(file string, downloaded, total int64) {
	line, ok := r.next(file, downloaded, total, time.Now())
	if !ok {
		return
	}
	if r.tty {
		r.p.StepProgress("%s", line)
		return
	}
	r.p.Step("%s", line)
}

// next decides whether this update is worth saying out loud, and what it would
// say. A false second return means the update was throttled away.
//
// now is a parameter because the terminal throttle is a rate, and a test that
// has to sleep to observe a rate is a test that is slow and flaky at once.
func (r *modelProgress) next(file string, downloaded, total int64, now time.Time) (string, bool) {
	if file != r.file {
		r.file = file
		r.announced = false
		r.lastPct = 0
		r.lastAt = time.Time{}
	}
	r.spoke = true

	label := "Downloading " + file
	done := total > 0 && downloaded == total

	if !r.tty {
		if !r.announced {
			r.announced = true
			return output.DownloadLine(label, downloaded, total, 0), true
		}
		if total <= 0 {
			// No length was declared, so there is no percentage to step
			// through and the opening line is everything we can honestly say.
			return "", false
		}
		pct := int(downloaded * 100 / total)
		if pct < r.lastPct+modelRefreshPercent && !done {
			return "", false
		}
		r.lastPct = pct
		return output.DownloadLine(label, downloaded, total, 0), true
	}

	if downloaded > 0 && !done && now.Sub(r.lastAt) < modelRefreshInterval {
		return "", false
	}
	r.lastAt = now
	return output.DownloadLine(label, downloaded, total, modelBarWidth), true
}
