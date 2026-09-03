package ai

import (
	"context"
	"strings"
	"testing"

	"github.com/sugarme/tokenizer"
)

type panickingEncoder struct {
	panicOn string
	calls   int
}

func (p *panickingEncoder) EncodeSingle(text string, _ ...bool) (*tokenizer.Encoding, error) {
	p.calls++
	if p.panicOn != "" && strings.Contains(text, p.panicOn) {
		var b []byte
		_ = b[:1]
	}
	return tokenizer.NewEncoding(
		[]int{101, 2000, 102},
		[]int{0, 0, 0},
		[]string{"[CLS]", "x", "[SEP]"},
		[][]int{{0, 0}, {0, 1}, {1, 1}},
		[]int{1, 0, 1},
		[]int{1, 1, 1},
		[]tokenizer.Encoding{},
	), nil
}

func TestEncodeSingleTurnsTokenizerPanicIntoError(t *testing.T) {
	c := &localEmbeddingClient{tk: &panickingEncoder{panicOn: "boom"}}

	ids, mask, err := c.encodeSingle("this one goes boom")
	if err == nil {
		t.Fatal("a tokenizer panic must surface as an error, not take the process down")
	}
	if !strings.Contains(err.Error(), "tokenizer panic") {
		t.Errorf("error should name the cause, got: %v", err)
	}
	if ids != nil || mask != nil {
		t.Errorf("no ids or mask may be returned for a failed encode, got %v / %v", ids, mask)
	}

	if _, _, err := c.encodeSingle("this one is fine"); err != nil {
		t.Errorf("an input that does not panic must still encode: %v", err)
	}
}

// The batch must survive one bad text. Every text panics here, which is the case
// that also proves EmbedBatch never reaches the ONNX session — this client has a
// nil session, so a call into the model would crash the test.
func TestEmbedBatchWithNoEncodableTextReturnsNilVectorsNotAPanic(t *testing.T) {
	enc := &panickingEncoder{panicOn: "boom"}
	c := &localEmbeddingClient{tk: enc}

	texts := []string{"boom one", "boom two", "boom three"}
	vecs, err := c.EmbedBatch(context.Background(), texts)
	if err != nil {
		t.Fatalf("a batch of unencodable texts is not an error for the caller: %v", err)
	}
	if len(vecs) != len(texts) {
		t.Fatalf("result must stay aligned with the input: got %d vectors for %d texts", len(vecs), len(texts))
	}
	for i, v := range vecs {
		if len(v) != 0 {
			t.Errorf("vector %d should be empty — callers read that as 'not embedded', got %d dims", i, len(v))
		}
	}
	if enc.calls != len(texts) {
		t.Errorf("every text should have been attempted, got %d attempts for %d texts", enc.calls, len(texts))
	}
}
