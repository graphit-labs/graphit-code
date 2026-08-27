package ai

import (
	"context"
	"strings"
	"testing"

	"github.com/sugarme/tokenizer"
)

// sugarme/tokenizer v0.3.0 panics on some inputs instead of returning an error:
// NormalizedString.Slice derives the original-string range through ConvertOffset,
// which does not clamp to the string's length the way IntoFullRange does on the
// other branch, so RangeOriginal slices past the end
// ("slice bounds out of range [:551] with capacity 550"). v0.3.0 is the newest
// release, so there is nothing to upgrade to.
//
// Unprotected, that took down the whole process. The daemon's embedding module
// restarted into the same panic every two minutes for twelve days, and because
// the supervisor recorded only the panic VALUE and never the stack, sixty-six log
// lines named no file and no function.
//
// What the inputs have in common could not be characterized from outside the
// library — a dozen synthetic candidates (accents, CJK, emoji, special tokens,
// combining marks, control characters) all tokenized cleanly. So what is tested
// here is this package's containment of the panic, which is what this package
// owns, rather than a reproduction of the upstream bug.

type panickingEncoder struct {
	panicOn string
	calls   int
}

func (p *panickingEncoder) EncodeSingle(text string, _ ...bool) (*tokenizer.Encoding, error) {
	p.calls++
	if p.panicOn != "" && strings.Contains(text, p.panicOn) {
		// The shape of the real one: a runtime slice-bounds error, not a string.
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
