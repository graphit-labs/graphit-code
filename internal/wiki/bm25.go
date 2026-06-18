package wiki

import (
	"math"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"unicode"
)

type BM25Config struct {
	K1 float64
	B  float64
}

func DefaultBM25Config() BM25Config {
	return BM25Config{K1: 1.2, B: 0.75}
}

type BM25Index struct {
	cfg BM25Config

	docTermFreqs map[string]map[string]int

	docLen map[string]int

	docTitles map[string]string

	termDocCount map[string]int

	totalDocs int

	avgDocLen float64

	stopwords map[string]bool
}

type BM25Result struct {
	Path    string
	Title   string
	Score   float64
	Snippet string
}

func NewBM25Index(wikiDir string, cfg BM25Config) (*BM25Index, error) {
	idx := &BM25Index{
		cfg:          cfg,
		docTermFreqs: make(map[string]map[string]int),
		docLen:       make(map[string]int),
		docTitles:    make(map[string]string),
		termDocCount: make(map[string]int),
		stopwords:    defaultStopwords(),
	}

	err := filepath.WalkDir(wikiDir, func(path string, d os.DirEntry, err error) error {
		if err != nil || d.IsDir() {
			return nil
		}
		if filepath.Ext(path) != ".md" {
			return nil
		}
		data, readErr := os.ReadFile(path)
		if readErr != nil {
			return nil
		}
		rel, _ := filepath.Rel(wikiDir, path)
		idx.indexDocument(rel, string(data))
		return nil
	})
	if err != nil {
		return nil, err
	}

	idx.totalDocs = len(idx.docTermFreqs)
	if idx.totalDocs > 0 {
		total := 0
		for _, l := range idx.docLen {
			total += l
		}
		idx.avgDocLen = float64(total) / float64(idx.totalDocs)
	}

	return idx, nil
}

func (idx *BM25Index) indexDocument(docID, content string) {

	idx.docTitles[docID] = extractBM25Title(content)

	content = StripFrontmatter(content)

	tokens := idx.tokenize(content)
	idx.docLen[docID] = len(tokens)

	termFreqs := make(map[string]int)
	for _, token := range tokens {
		termFreqs[token]++
	}
	idx.docTermFreqs[docID] = termFreqs

	for term := range termFreqs {
		idx.termDocCount[term]++
	}
}

func (idx *BM25Index) Search(query string, topN int) []BM25Result {
	queryTerms := idx.tokenize(query)
	if len(queryTerms) == 0 {
		return nil
	}

	// Spelling correction / query expansion using trigrams
	expandedTerms := make([]string, len(queryTerms))
	for i, term := range queryTerms {
		if idx.termDocCount[term] > 0 {
			expandedTerms[i] = term
			continue
		}

		bestVocab := ""
		bestScore := 0.0
		for vocab := range idx.termDocCount {
			score := TrigramSimilarity(term, vocab)
			if score > bestScore {
				bestScore = score
				bestVocab = vocab
			}
		}

		if bestScore >= 0.60 {
			expandedTerms[i] = bestVocab
		} else {
			expandedTerms[i] = term
		}
	}

	type scored struct {
		docID string
		score float64
	}

	var results []scored
	for docID, termFreqs := range idx.docTermFreqs {
		score := 0.0
		for _, term := range expandedTerms {
			tf := float64(termFreqs[term])
			if tf == 0 {
				continue
			}
			df := float64(idx.termDocCount[term])
			idf := math.Log(1 + (float64(idx.totalDocs)-df+0.5)/(df+0.5))

			dl := float64(idx.docLen[docID])
			numerator := tf * (idx.cfg.K1 + 1)
			denominator := tf + idx.cfg.K1*(1-idx.cfg.B+idx.cfg.B*dl/idx.avgDocLen)
			score += idf * numerator / denominator
		}
		if score > 0 {
			results = append(results, scored{docID, score})
		}
	}

	sort.Slice(results, func(i, j int) bool {
		return results[i].score > results[j].score
	})

	if topN > 0 && len(results) > topN {
		results = results[:topN]
	}

	var out []BM25Result
	for _, r := range results {
		out = append(out, BM25Result{
			Path:  r.docID,
			Title: idx.docTitles[r.docID],
			Score: math.Round(r.score*1000) / 1000,
		})
	}
	return out
}

func (idx *BM25Index) tokenize(text string) []string {
	text = strings.ToLower(text)
	splitter := func(c rune) bool {
		return !unicode.IsLetter(c) && !unicode.IsDigit(c) && c != '_' && c != '-'
	}
	rawTokens := strings.FieldsFunc(text, splitter)

	var tokens []string
	for _, t := range rawTokens {
		t = strings.TrimFunc(t, func(r rune) bool { return r == '_' || r == '-' })
		if len(t) < 2 {
			continue
		}
		if idx.stopwords[t] {
			continue
		}
		tokens = append(tokens, t)
	}
	return tokens
}

var reBM25H1 = regexp.MustCompile(`(?m)^#\s+(.+)$`)

func extractBM25Title(content string) string {
	if m := reBM25H1.FindStringSubmatch(content); m != nil {
		return strings.TrimSpace(m[1])
	}
	return ""
}

func defaultStopwords() map[string]bool {
	words := []string{
		"the", "be", "to", "of", "and", "in", "that", "have",
		"it", "for", "not", "on", "with", "he", "as", "you",
		"do", "at", "this", "but", "his", "by", "from", "they",
		"we", "say", "her", "she", "or", "an", "will", "my",
		"one", "all", "would", "there", "their", "what", "so",
		"up", "out", "if", "about", "who", "get", "which", "go",
		"me", "when", "make", "can", "like", "time", "no", "just",
		"him", "know", "take", "come", "could", "than", "look",
		"only", "its", "over", "also", "back", "use", "how",
		"our", "even", "new", "want", "because", "any", "these",
		"us", "is", "are", "was", "were", "been", "being", "am",
		"has", "had", "does", "did", "a",
	}
	m := make(map[string]bool, len(words))
	for _, w := range words {
		m[w] = true
	}
	return m
}
