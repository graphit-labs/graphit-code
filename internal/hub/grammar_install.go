package hub

import (
	"encoding/binary"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"runtime"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/klauspost/compress/zstd"
)

// installGrammarArchive extracts a .grammar fat archive for the current platform
// and places the resulting binary in the appropriate grammars directory under the project.
//
// Tree-sitter grammars (tree-sitter-*.grammar) → <projectDir>/<dotDir>/grammars/treesitter/<name>.so
// ANTLR grammars (antlr-*.grammar) → <projectDir>/<dotDir>/grammars/antlr/<sidecar-name>
func installGrammarArchive(archivePath, projectDir, dotDir string) error {
	archive, err := ast.ReadGrammarArchive(archivePath)
	if err != nil {
		return fmt.Errorf("reading grammar archive: %w", err)
	}

	baseName := filepath.Base(archivePath)
	nameStem := strings.TrimSuffix(baseName, ".grammar")

	// Determine grammar type and target directory.
	var grammarDir string
	var targetName string
	var needsExec bool

	if strings.HasPrefix(nameStem, "tree-sitter-") {
		// Tree-sitter: extract shared library.
		if dotDir == "" {
			grammarDir = filepath.Join(projectDir, "grammars", "treesitter")
		} else {
			grammarDir = filepath.Join(projectDir, dotDir, "grammars", "treesitter")
		}
		ext := sharedLibExt()
		targetName = nameStem + ext
	} else if strings.HasPrefix(nameStem, "antlr-") {
		// ANTLR: extract sidecar binary.
		if dotDir == "" {
			grammarDir = filepath.Join(projectDir, "grammars", "antlr")
		} else {
			grammarDir = filepath.Join(projectDir, dotDir, "grammars", "antlr")
		}
		// e.g. antlr-plsql → antlr-sidecar-plsql
		langName := strings.TrimPrefix(nameStem, "antlr-")
		targetName = "antlr-sidecar-" + langName
		if runtime.GOOS == "windows" {
			targetName += ".exe"
		}
		needsExec = true
	} else {
		return fmt.Errorf("unknown grammar archive type: %s", baseName)
	}

	if err := os.MkdirAll(grammarDir, 0o755); err != nil {
		return fmt.Errorf("creating grammar dir: %w", err)
	}

	// Find the platform entry for current OS/arch and extract from the archive.
	platformData, err := extractPlatformData(archive, archivePath)
	if err != nil {
		return fmt.Errorf("extracting platform data from %s: %w", baseName, err)
	}

	targetPath := filepath.Join(grammarDir, targetName)
	perm := os.FileMode(0o644)
	if needsExec {
		perm = 0o755
	}

	if err := os.WriteFile(targetPath, platformData, perm); err != nil {
		return fmt.Errorf("writing grammar binary: %w", err)
	}

	// The parser caches its query directories and only re-reads them when their
	// contents change, on a timer. An install knows it changed something, so it
	// says so instead of letting a long-lived daemon wait out the interval.
	ast.InvalidateQueryCaches()

	return nil
}

// extractPlatformData reads the compressed data for the current OS/arch from the
// grammar archive and returns the decompressed bytes.
func extractPlatformData(archive *ast.GrammarArchive, archivePath string) ([]byte, error) {
	goos := runtime.GOOS
	goarch := runtime.GOARCH

	// Find matching platform.
	idx := -1
	for i, p := range archive.Platforms {
		if p.OS == goos && p.Arch == goarch {
			idx = i
			break
		}
	}
	if idx == -1 {
		return nil, fmt.Errorf("no binary for platform %s/%s in archive", goos, goarch)
	}

	// Read the archive to get entry metadata. The entry offsets are at fixed
	// positions in the file: header (16 bytes) + entry_index * 120 bytes.
	// Each entry has: OS[16] + Arch[16] + Symbol[64] + Offset[8] + CompressedSize[8] + OriginalSize[8].
	f, err := os.Open(archivePath)
	if err != nil {
		return nil, fmt.Errorf("open archive: %w", err)
	}
	defer f.Close()

	// Seek to the matching entry to re-read offset/sizes.
	entryOffset := int64(16 + idx*120) // grammarHeaderSize + idx * grammarEntrySize
	if _, err := f.Seek(entryOffset, io.SeekStart); err != nil {
		return nil, fmt.Errorf("seek to entry: %w", err)
	}

	// Skip OS (16) + Arch (16) + Symbol (64) = 96 bytes to reach offset field.
	var skip [96]byte
	if _, err := io.ReadFull(f, skip[:]); err != nil {
		return nil, fmt.Errorf("skip entry fields: %w", err)
	}

	var dataOffset, compressedSize, originalSize uint64
	if err := binary.Read(f, binary.LittleEndian, &dataOffset); err != nil {
		return nil, fmt.Errorf("read offset: %w", err)
	}
	if err := binary.Read(f, binary.LittleEndian, &compressedSize); err != nil {
		return nil, fmt.Errorf("read compressed size: %w", err)
	}
	if err := binary.Read(f, binary.LittleEndian, &originalSize); err != nil {
		return nil, fmt.Errorf("read original size: %w", err)
	}

	// Seek to data and read compressed bytes.
	if _, err := f.Seek(int64(dataOffset), io.SeekStart); err != nil {
		return nil, fmt.Errorf("seek to data: %w", err)
	}

	compressed := make([]byte, compressedSize)
	if _, err := io.ReadFull(f, compressed); err != nil {
		return nil, fmt.Errorf("read compressed data: %w", err)
	}

	// Decompress with zstd.
	decoder, err := zstd.NewReader(nil)
	if err != nil {
		return nil, fmt.Errorf("create zstd decoder: %w", err)
	}
	defer decoder.Close()

	decompressed, err := decoder.DecodeAll(compressed, nil)
	if err != nil {
		return nil, fmt.Errorf("zstd decompress: %w", err)
	}

	if uint64(len(decompressed)) != originalSize {
		return nil, fmt.Errorf("size mismatch: decompressed %d, expected %d", len(decompressed), originalSize)
	}

	return decompressed, nil
}

// uninstallGrammarFiles removes grammar binaries associated with a language artifact.
// It examines the files in the clone dir to determine which grammars were installed.
func uninstallGrammarFiles(cloneDir, projectDir, dotDir string) {
	entries, _ := os.ReadDir(cloneDir)
	for _, e := range entries {
		if e.IsDir() || !strings.HasSuffix(e.Name(), ".grammar") {
			continue
		}
		nameStem := strings.TrimSuffix(e.Name(), ".grammar")

		if strings.HasPrefix(nameStem, "tree-sitter-") {
			ext := sharedLibExt()
			var target string
			if dotDir == "" {
				target = filepath.Join(projectDir, "grammars", "treesitter", nameStem+ext)
			} else {
				target = filepath.Join(projectDir, dotDir, "grammars", "treesitter", nameStem+ext)
			}
			_ = os.Remove(target)
		} else if strings.HasPrefix(nameStem, "antlr-") {
			langName := strings.TrimPrefix(nameStem, "antlr-")
			targetName := "antlr-sidecar-" + langName
			if runtime.GOOS == "windows" {
				targetName += ".exe"
			}
			var target string
			if dotDir == "" {
				target = filepath.Join(projectDir, "grammars", "antlr", targetName)
			} else {
				target = filepath.Join(projectDir, dotDir, "grammars", "antlr", targetName)
			}
			_ = os.Remove(target)
		}
	}

	ast.InvalidateQueryCaches()
}

// sharedLibExt returns the platform-appropriate shared library extension.
func sharedLibExt() string {
	switch runtime.GOOS {
	case "darwin":
		return ".dylib"
	case "windows":
		return ".dll"
	default:
		return ".so"
	}
}
