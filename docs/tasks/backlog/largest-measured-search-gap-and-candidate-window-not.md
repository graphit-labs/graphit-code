# The biggest search hole measured is the candidate window, not the sorting: 1 out of 16 answers never reaches the reranker

# Candidate window is bottleneck, not sorting

Discovered on 2026-08-23 when measuring cross-encoder with real inference
(`docs/tasks/hub-on-s3-icebug-and-lancedb.md`, section "MEASURED with real inference";
`internal/ai/rerank_eval_test.go`, tag `rerankeval`). **Not a bug** — this is where the next gain of
search quality is, measured, and it's cheaper than the path we were taking.

## O fato medido

About 16 natural language questions and 24 real entities from this repository:

| | |
|---|---|
| queries that reranker * *does not move** | **14 of 16** |
| answers **out** of 10 applicants window | **1 of 16** |
| best reranker gain (`bge-reranker-base`) | +0.032 MRR, +0.023 nDCG@10 |
| cost of this gain | 1.04 GiB of model + 720 ms per query |

Query `"what keeps a broken remote from filling the disk with retries"` is answered
`evictOldestStaged`, which the lexical baseline places at **rank 24 of 24**. With a window of 10, it
** it is never presented to thereranker* * — no reranking reaches it, no matter how good.

That is: we spend 1 GiB and 720 ms to move one query from place, while another is lost by
first stage recall.

## Why it matters more than it seems

The reranker only reorders what the first stage has returned. Every quality effort invested in the
second stage has as its ceiling the recall of the first. **Widening the window is the lever with the largest
measured return per unit of cost** — and it is configuration, not model.

`internal/lancestore/rerank.go` already has `DefaultCandidateLimit = 50` and `candidates()` does the
reaming with trimming back to `Limit`. So the mechanism exists; what does not exist is **measurement
which window is worth it**, nor widening the search path when the reranker is turned off
(which is the default).

What to do

1. **Measure recall@N of the first stage alone**, for N in {10, 20, 50, 100}, over the
   actual evaluation set. This answers "how many right answers does the engine even return",
   which today is unknown. The instrumentation is already there: the test reports
   `first-stage miss: X/16`. 2. **Check the cost of enlargement in LanceDB.** Ask for 100 instead of 10 in a BM25 + index
   vector with RRF of the motor may be almost free (the motor already sweeps) or it may not be —
   measure, not assume. This is for the `Limit` of the `Query` in `internal/lancestore`. 3. **If enlargement is cheap, it is quality improvement, and reranker remains opt-in.**
   A high @50 recall with engine RRF can deliver more than 1 GiB of cross-encoder.

## Trap recorded, that this measurement stepped on

The first result came out `improved 1, worsened 2` and it seemed that the reranker made things worse. The
cause was the **account**: the baseline was measured over the entire corpus and the reranked over the
10, then the answer in rank 24 gave 1/24 credit to the baseline and 0 to the reranked — punishing the
reranker for a document he'd never seen.

**Both sides of a reranking benchmark have to be measured in the SAME window**, and the cost of
window reported separately. It's fixed in the test, which is why the line
`first-stage miss` exists.

## Limits of the current evaluation set, for those who are going to measure

16 questions, 24 documents, one right answer per question, and the baseline is **TF-IDF in Go**, no
the real hybrid of LanceDB. It serves to compare rerankers with each other (that's what it was made for),
but to decide the window **the baseline needs to be the real engine **, with the BM25 index and the
vector built. This requires the `lancedb` tag and the compiled native (`make fetch-lancedb`).

## How to know it worked

- There is a recall number @{10,20,50,100} of the first actual stage of LanceDB on the
  evaluation set, and it's in the task log.
- The window chosen is justified by this number and the measured cost of latency, not by
  inherited default.
- `first-stage miss` falls from 1/16 to 0/16, or it is recorded why it does not fall.
