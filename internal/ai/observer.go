package ai

import "context"

type eventObserverKey struct{}

// WithEventObserver observes the CLI behind the normal Conversation API without
// replacing its serialization, native session or working-directory semantics.
func WithEventObserver(ctx context.Context, emit EventFunc) context.Context {
	return context.WithValue(ctx, eventObserverKey{}, emit)
}
func eventObserver(ctx context.Context) EventFunc {
	emit, _ := ctx.Value(eventObserverKey{}).(EventFunc)
	return emit
}
