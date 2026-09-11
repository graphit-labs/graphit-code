package task

import (
	"context"
	"testing"
)

func TestBatchRejectsInvalidEnvelopeBeforeOpeningStorage(t *testing.T) {
	svc := &Service{}
	if _, err := svc.Batch(context.Background(), BatchInput{Actor: "agent"}); err == nil {
		t.Fatal("empty batch was accepted")
	}
	tooMany := make([]BatchOperation, MaxBatchOperations+1)
	if _, err := svc.Batch(context.Background(), BatchInput{Actor: "agent", Operations: tooMany}); err == nil {
		t.Fatal("oversized batch was accepted")
	}
	if _, err := svc.Batch(context.Background(), BatchInput{Actor: "agent", Lease: "invalid", Operations: []BatchOperation{{Action: "create"}}}); err == nil {
		t.Fatal("invalid default lease was accepted")
	}
}
