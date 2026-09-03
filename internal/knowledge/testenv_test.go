package knowledge

import (
	"os"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/testsupport/testenv"
)

func TestMain(m *testing.M) {
	os.Exit(testenv.Run(m))
}
