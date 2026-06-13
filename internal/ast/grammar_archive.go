package ast

import (
	"encoding/binary"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"runtime"
	"strings"

	"github.com/klauspost/compress/zstd"
)

// GrammarArchiveMagic is the 4-byte magic identifier for .grammar files.
const GrammarArchiveMagic = "GRMT"

// GrammarArchiveVersion is the current archive format version.
const GrammarArchiveVersion = 1

// grammarHeaderSize is the fixed header size in bytes.
const grammarHeaderSize = 16

// grammarEntrySize is the fixed platform entry size in bytes.
// 16 (OS) + 16 (Arch) + 64 (SymbolName) + 8 (Offset) + 8 (CompressedSize) + 8 (OriginalSize) = 120
const grammarEntrySize = 120

// ErrPlatformNotFound is returned when the current platform is not in the archive.
var ErrPlatformNotFound = errors.New("grammar archive: platform not found")

// GrammarArchive represents a parsed .grammar fat archive.
type GrammarArchive struct {
	Platforms []GrammarPlatform

	// entries holds the raw on-disk metadata for lazy extraction.
	entries []grammarEntry
}

// GrammarPlatform describes a single platform's grammar data.
type GrammarPlatform struct {
	OS         string
	Arch       string
	SymbolName string
	Data       []byte // uncompressed shared lib data (only populated during write or after extraction)
}

// grammarEntry is the on-disk platform entry metadata.
type grammarEntry struct {
	os             string
	arch           string
	symbolName     string
	offset         uint64
	compressedSize uint64
	originalSize   uint64
}

// grammarHeader is the on-disk file header.
type grammarHeader struct {
	magic        [4]byte
	version      uint32
	numPlatforms uint32
	flags        uint32
}

// ReadGrammarArchive reads a .grammar file and returns the archive metadata.
// It does NOT decompress platform data until ExtractForCurrentPlatform is called.
func ReadGrammarArchive(path string) (*GrammarArchive, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, fmt.Errorf("grammar archive: open %q: %w", path, err)
	}
	defer f.Close()

	// Read header.
	var hdr grammarHeader
	if err := binary.Read(f, binary.LittleEndian, &hdr.magic); err != nil {
		return nil, fmt.Errorf("grammar archive: read magic: %w", err)
	}
	if string(hdr.magic[:]) != GrammarArchiveMagic {
		return nil, fmt.Errorf("grammar archive: invalid magic %q (expected %q)", string(hdr.magic[:]), GrammarArchiveMagic)
	}

	if err := binary.Read(f, binary.LittleEndian, &hdr.version); err != nil {
		return nil, fmt.Errorf("grammar archive: read version: %w", err)
	}
	if hdr.version != GrammarArchiveVersion {
		return nil, fmt.Errorf("grammar archive: unsupported version %d (expected %d)", hdr.version, GrammarArchiveVersion)
	}

	if err := binary.Read(f, binary.LittleEndian, &hdr.numPlatforms); err != nil {
		return nil, fmt.Errorf("grammar archive: read numPlatforms: %w", err)
	}
	if err := binary.Read(f, binary.LittleEndian, &hdr.flags); err != nil {
		return nil, fmt.Errorf("grammar archive: read flags: %w", err)
	}

	// Read platform entries.
	archive := &GrammarArchive{
		entries:   make([]grammarEntry, hdr.numPlatforms),
		Platforms: make([]GrammarPlatform, hdr.numPlatforms),
	}

	for i := uint32(0); i < hdr.numPlatforms; i++ {
		var osBytes [16]byte
		var archBytes [16]byte
		var symbolBytes [64]byte
		var offset, compressedSize, originalSize uint64

		if err := binary.Read(f, binary.LittleEndian, &osBytes); err != nil {
			return nil, fmt.Errorf("grammar archive: read entry[%d] OS: %w", i, err)
		}
		if err := binary.Read(f, binary.LittleEndian, &archBytes); err != nil {
			return nil, fmt.Errorf("grammar archive: read entry[%d] Arch: %w", i, err)
		}
		if err := binary.Read(f, binary.LittleEndian, &symbolBytes); err != nil {
			return nil, fmt.Errorf("grammar archive: read entry[%d] Symbol: %w", i, err)
		}
		if err := binary.Read(f, binary.LittleEndian, &offset); err != nil {
			return nil, fmt.Errorf("grammar archive: read entry[%d] Offset: %w", i, err)
		}
		if err := binary.Read(f, binary.LittleEndian, &compressedSize); err != nil {
			return nil, fmt.Errorf("grammar archive: read entry[%d] CompressedSize: %w", i, err)
		}
		if err := binary.Read(f, binary.LittleEndian, &originalSize); err != nil {
			return nil, fmt.Errorf("grammar archive: read entry[%d] OriginalSize: %w", i, err)
		}

		entry := grammarEntry{
			os:             nullTermString(osBytes[:]),
			arch:           nullTermString(archBytes[:]),
			symbolName:     nullTermString(symbolBytes[:]),
			offset:         offset,
			compressedSize: compressedSize,
			originalSize:   originalSize,
		}
		archive.entries[i] = entry
		archive.Platforms[i] = GrammarPlatform{
			OS:         entry.os,
			Arch:       entry.arch,
			SymbolName: entry.symbolName,
		}
	}

	return archive, nil
}

// ExtractForCurrentPlatform extracts the shared lib for the current OS/arch
// to the cache directory. Returns the path to the extracted file.
// If already cached (by checking file size against originalSize), returns immediately.
func (a *GrammarArchive) ExtractForCurrentPlatform(archivePath, cacheDir string) (string, error) {
	goos := runtime.GOOS
	goarch := runtime.GOARCH

	// Find matching platform entry.
	idx := -1
	for i, e := range a.entries {
		if e.os == goos && e.arch == goarch {
			idx = i
			break
		}
	}
	if idx == -1 {
		return "", fmt.Errorf("%w: %s/%s not in archive (available: %s)",
			ErrPlatformNotFound, goos, goarch, a.availablePlatforms())
	}

	entry := a.entries[idx]
	archiveName := strings.TrimSuffix(filepath.Base(archivePath), ".grammar")
	cachePath := grammarCachePath(cacheDir, archiveName, goos, goarch)

	// Check cache: if file exists and size matches original, skip extraction.
	if info, err := os.Stat(cachePath); err == nil {
		if uint64(info.Size()) == entry.originalSize {
			return cachePath, nil
		}
	}

	// Extract: read compressed data from archive, decompress, write to cache.
	f, err := os.Open(archivePath)
	if err != nil {
		return "", fmt.Errorf("grammar archive: open for extract: %w", err)
	}
	defer f.Close()

	if _, err := f.Seek(int64(entry.offset), io.SeekStart); err != nil {
		return "", fmt.Errorf("grammar archive: seek to offset %d: %w", entry.offset, err)
	}

	compressed := make([]byte, entry.compressedSize)
	if _, err := io.ReadFull(f, compressed); err != nil {
		return "", fmt.Errorf("grammar archive: read compressed data: %w", err)
	}

	// Decompress with zstd.
	decoder, err := zstd.NewReader(nil)
	if err != nil {
		return "", fmt.Errorf("grammar archive: create zstd decoder: %w", err)
	}
	defer decoder.Close()

	decompressed, err := decoder.DecodeAll(compressed, nil)
	if err != nil {
		return "", fmt.Errorf("grammar archive: zstd decompress: %w", err)
	}

	if uint64(len(decompressed)) != entry.originalSize {
		return "", fmt.Errorf("grammar archive: size mismatch: decompressed %d, expected %d",
			len(decompressed), entry.originalSize)
	}

	// Write to cache dir.
	if err := os.MkdirAll(filepath.Dir(cachePath), 0o755); err != nil {
		return "", fmt.Errorf("grammar archive: create cache dir: %w", err)
	}

	if err := os.WriteFile(cachePath, decompressed, 0o755); err != nil {
		return "", fmt.Errorf("grammar archive: write cache file: %w", err)
	}

	return cachePath, nil
}

// WriteGrammarArchive writes a .grammar file from the given platforms.
// Each platform's Data field must be populated with the uncompressed shared library bytes.
func WriteGrammarArchive(path string, platforms []GrammarPlatform) error {
	if len(platforms) == 0 {
		return errors.New("grammar archive: no platforms to write")
	}

	f, err := os.Create(path)
	if err != nil {
		return fmt.Errorf("grammar archive: create %q: %w", path, err)
	}
	defer f.Close()

	// Create zstd encoder.
	encoder, err := zstd.NewWriter(nil, zstd.WithEncoderLevel(zstd.SpeedDefault))
	if err != nil {
		return fmt.Errorf("grammar archive: create zstd encoder: %w", err)
	}
	defer encoder.Close()

	// Compress all platform data first to know sizes.
	type compressedEntry struct {
		data         []byte
		originalSize uint64
	}
	compressed := make([]compressedEntry, len(platforms))
	for i, p := range platforms {
		if len(p.Data) == 0 {
			return fmt.Errorf("grammar archive: platform %s/%s has no data", p.OS, p.Arch)
		}
		compressed[i] = compressedEntry{
			data:         encoder.EncodeAll(p.Data, nil),
			originalSize: uint64(len(p.Data)),
		}
	}

	// Calculate data section offsets.
	// Data starts after header + all entries.
	dataStart := uint64(grammarHeaderSize) + uint64(len(platforms))*uint64(grammarEntrySize)
	offsets := make([]uint64, len(platforms))
	currentOffset := dataStart
	for i, ce := range compressed {
		offsets[i] = currentOffset
		currentOffset += uint64(len(ce.data))
	}

	// Write header.
	var magic [4]byte
	copy(magic[:], GrammarArchiveMagic)
	if err := binary.Write(f, binary.LittleEndian, magic); err != nil {
		return fmt.Errorf("grammar archive: write magic: %w", err)
	}
	if err := binary.Write(f, binary.LittleEndian, uint32(GrammarArchiveVersion)); err != nil {
		return fmt.Errorf("grammar archive: write version: %w", err)
	}
	if err := binary.Write(f, binary.LittleEndian, uint32(len(platforms))); err != nil {
		return fmt.Errorf("grammar archive: write numPlatforms: %w", err)
	}
	if err := binary.Write(f, binary.LittleEndian, uint32(0)); err != nil { // flags reserved
		return fmt.Errorf("grammar archive: write flags: %w", err)
	}

	// Write platform entries.
	for i, p := range platforms {
		var osBytes [16]byte
		var archBytes [16]byte
		var symbolBytes [64]byte

		copy(osBytes[:], p.OS)
		copy(archBytes[:], p.Arch)
		copy(symbolBytes[:], p.SymbolName)

		if err := binary.Write(f, binary.LittleEndian, osBytes); err != nil {
			return fmt.Errorf("grammar archive: write entry[%d] OS: %w", i, err)
		}
		if err := binary.Write(f, binary.LittleEndian, archBytes); err != nil {
			return fmt.Errorf("grammar archive: write entry[%d] Arch: %w", i, err)
		}
		if err := binary.Write(f, binary.LittleEndian, symbolBytes); err != nil {
			return fmt.Errorf("grammar archive: write entry[%d] Symbol: %w", i, err)
		}
		if err := binary.Write(f, binary.LittleEndian, offsets[i]); err != nil {
			return fmt.Errorf("grammar archive: write entry[%d] Offset: %w", i, err)
		}
		if err := binary.Write(f, binary.LittleEndian, uint64(len(compressed[i].data))); err != nil {
			return fmt.Errorf("grammar archive: write entry[%d] CompressedSize: %w", i, err)
		}
		if err := binary.Write(f, binary.LittleEndian, compressed[i].originalSize); err != nil {
			return fmt.Errorf("grammar archive: write entry[%d] OriginalSize: %w", i, err)
		}
	}

	// Write compressed data sections.
	for i, ce := range compressed {
		if _, err := f.Write(ce.data); err != nil {
			return fmt.Errorf("grammar archive: write data[%d]: %w", i, err)
		}
	}

	return nil
}

// grammarCachePath returns the cache file path for an extracted grammar.
func grammarCachePath(cacheDir, archiveName, goos, goarch string) string {
	ext := ".so"
	switch goos {
	case "darwin":
		ext = ".dylib"
	case "windows":
		ext = ".dll"
	}
	return filepath.Join(cacheDir, fmt.Sprintf("%s-%s-%s%s", archiveName, goos, goarch, ext))
}

// availablePlatforms returns a human-readable list of available platforms.
func (a *GrammarArchive) availablePlatforms() string {
	var parts []string
	for _, e := range a.entries {
		parts = append(parts, e.os+"/"+e.arch)
	}
	return strings.Join(parts, ", ")
}

// nullTermString converts a null-padded byte slice to a Go string.
func nullTermString(b []byte) string {
	for i, c := range b {
		if c == 0 {
			return string(b[:i])
		}
	}
	return string(b)
}
