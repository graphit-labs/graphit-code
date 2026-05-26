package ast

import (
	"sync"
	"time"
)

type JobStatus string

const (
	JobPending   JobStatus = "pending"
	JobRunning   JobStatus = "running"
	JobDone      JobStatus = "done"
	JobFailed    JobStatus = "failed"
	JobCancelled JobStatus = "cancelled"
)

type Job struct {
	ID        string    `json:"id"`
	Path      string    `json:"path"`
	Status    JobStatus `json:"status"`
	Progress  int       `json:"progress"`
	Total     int       `json:"total"`
	Processed int       `json:"processed"`
	Errors    []string  `json:"errors,omitempty"`
	StartedAt time.Time `json:"started_at"`
	EndedAt   time.Time `json:"ended_at,omitempty"`
}

type JobManager struct {
	mu   sync.RWMutex
	jobs map[string]*Job
}

func NewJobManager() *JobManager {
	return &JobManager{jobs: make(map[string]*Job)}
}

func (m *JobManager) Create(id, path string, total int) *Job {
	job := &Job{
		ID:        id,
		Path:      path,
		Status:    JobPending,
		Total:     total,
		StartedAt: time.Now(),
	}
	m.mu.Lock()
	m.jobs[id] = job
	m.mu.Unlock()
	return job
}

func (m *JobManager) Get(id string) *Job {
	m.mu.RLock()
	defer m.mu.RUnlock()
	return m.jobs[id]
}

func (m *JobManager) List() []*Job {
	m.mu.RLock()
	defer m.mu.RUnlock()
	out := make([]*Job, 0, len(m.jobs))
	for _, j := range m.jobs {
		out = append(out, j)
	}

	sortJobsDesc(out)
	return out
}

func (m *JobManager) Update(id string, fn func(*Job)) {
	m.mu.Lock()
	defer m.mu.Unlock()
	if j, ok := m.jobs[id]; ok {
		fn(j)
	}
}

func (m *JobManager) Tick(id string, errs []string) {
	m.mu.Lock()
	defer m.mu.Unlock()
	j, ok := m.jobs[id]
	if !ok {
		return
	}
	j.Processed++
	j.Errors = append(j.Errors, errs...)
	if j.Total > 0 {
		j.Progress = int(float64(j.Processed) / float64(j.Total) * 100)
	}
	if j.Status == JobPending {
		j.Status = JobRunning
	}
}

func (m *JobManager) Done(id string, failed bool) {
	m.mu.Lock()
	defer m.mu.Unlock()
	j, ok := m.jobs[id]
	if !ok {
		return
	}
	j.EndedAt = time.Now()
	j.Progress = 100
	if failed {
		j.Status = JobFailed
	} else {
		j.Status = JobDone
	}
}

func (m *JobManager) Prune(maxAge time.Duration) {
	cutoff := time.Now().Add(-maxAge)
	m.mu.Lock()
	defer m.mu.Unlock()
	for id, j := range m.jobs {
		done := j.Status == JobDone || j.Status == JobFailed || j.Status == JobCancelled
		if done && j.EndedAt.Before(cutoff) {
			delete(m.jobs, id)
		}
	}
}

func sortJobsDesc(jobs []*Job) {
	for i := 1; i < len(jobs); i++ {
		for j := i; j > 0 && jobs[j].StartedAt.After(jobs[j-1].StartedAt); j-- {
			jobs[j], jobs[j-1] = jobs[j-1], jobs[j]
		}
	}
}
