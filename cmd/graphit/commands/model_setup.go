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
	modelBarWidth = 24

	modelRefreshInterval = 100 * time.Millisecond

	modelRefreshPercent = 10

	modelDownloadTimeout = 15 * time.Minute
)

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

	return filepath.Dir(modelPath), reporter.spoke, nil
}

type modelProgress struct {
	p *output.Printer

	tty bool

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
