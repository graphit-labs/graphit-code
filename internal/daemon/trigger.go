package daemon

// Trigger wakes a scheduled module ahead of its timer. Notifications coalesce: any
// number of them before the module wakes produce one extra run.
type Trigger struct{ ch chan struct{} }

func NewTrigger() *Trigger { return &Trigger{ch: make(chan struct{}, 1)} }

// Notify asks for one run as soon as the module is free. A nil Trigger ignores it.
func (t *Trigger) Notify() {
	if t == nil {
		return
	}
	select {
	case t.ch <- struct{}{}:
	default:
	}
}

// c is nil for a nil Trigger, which blocks forever in a select — no trigger, timer only.
func (t *Trigger) c() <-chan struct{} {
	if t == nil {
		return nil
	}
	return t.ch
}

// ProjectTriggers connects a project's indexing to its embedding: the embedders run
// when an index they depend on has just changed, instead of polling for work.
type ProjectTriggers struct {
	ASTIndexed       *Trigger
	KnowledgeIndexed *Trigger
}

func NewProjectTriggers() ProjectTriggers {
	return ProjectTriggers{ASTIndexed: NewTrigger(), KnowledgeIndexed: NewTrigger()}
}
