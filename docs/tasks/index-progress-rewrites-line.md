# Tarefa: o progresso do `ast index` reescreve uma linha em vez de empilhar uma por refresh

Status: completed on August 7, 2026. Direct sequel to the progress monitor – he resolved it.
Silence for 16 minutes created a second problem in its place.

## O que o Engenheiro viu

```
  › Parsing: 22226/36824 file(s)
  › Parsing: 22586/36824 file(s)
  › Parsing: 22880/36824 file(s)
  … (mais 25 delas)
```

An indexing of 36,824 files took 677 seconds and spit out ~28 lines of `Parsing:`. Each one is the same.
Phrase with another number, and together they push everything that came before out of the screen— including
The `Grammar overrides:` line, which indicates whether the index is using the correct grammar.

## Por que estava assim

The function named `p.Step` was called by `indexProgressReporter`. And `Step` is a.
`Fprintln`: uma linha nova, sempre. O throttle de 10s existia justamente para que fossem poucas —
He was calibrated for a log, not for a screen.

The `internal/output` already had the right mechanism and wasn't being used here: `Task` spins a wheel.
Spinner has always been with `\r\033[K`. But `Task` is not suitable for this case, and the reason matters:
Outside of a terminal, it does not print anything. Replacing `Step` with `Task` would return the

Translation:
Outside of a terminal, nothing is printed. Replace `Step` with `Task` to get the result.
Silence for 16 minutes, for everyone who redirects the output—exactly as in this case with...
daemon e do CI.

## O que passou a existir

**`Printer.StepProgress`** — mesma linha que `Step`, escrita com `\r\033[K` e sem `\n`. Chamadas
Consecutive ones overwrite each other. Without TTY, falls to `Step`, because there's no cursor in an inline file.
Move: Each refresh is literally just one line, and the history is the only thing that the log has.

The cursor stops at the end of that line, so any following impression must be erased.
Before — then the summary falls on top of half a counter. Instead of spreading this everywhere.
Sites call, all methods of `Printer` have started writing through one point, `p.println`.
Here's the translation:

(And if there is one, it removes the transitional line; the spinner of __INLINE_2)
foi ligado no mesmo estado (`p.overwrite`), o que de quebra corrige um caso antigo: um `Step`
disparado com spinner rodando escrevia por cima dele.

The **`progressInterval(tty bool)`** separates the two cadences: 200ms with a cursor, and the usual 10 seconds.
The throttle continues being by the clock and keeps printing all phase changes at once —
Nothing changed; it's just the interval that remains the same.

**Truncagem pela largura do terminal.** Linha maior que a tela quebra em duas, e `\r` volta ao
The beginning of the last physical line only: those above remain on the screen as garbage. It is cut in runes, not in...
bytes.

## Arquivos

File | Change | Reason
|---|---|---|
The text is already in English, so it remains unchanged:

Portuguese:
| `internal/output/printer.go` | Modified |
| `StepProgress`, `EndProgress`, `overwrite`, `println`/`printf`, `truncate`, `termWidth`; `IsTTY` has moved from the test helper code to production code |

English:
Modified | Removed from here (now production)
| `cmd/graphit/commands/runners.go` | Modificado | relator usa `StepProgress`; `progressInterval`; `EndProgress` depois do pipeline |
Created | 6 Transient Line Tests |
| `cmd/graphit/commands/index_progress_test.go` | Modificado | teste do intervalo por TTY |

Verification

In addition to the buffer tests, I ran the actual path through pty (`script -qec`). The four updates came out.
as one physical line, each preceded by `\r\033[K`, and the `✓` final erases the preceding line
de imprimir. `go test -race ./internal/output ./cmd/graphit/commands` passa.

## O que ficou de fora

The elements "`Task.Done`" and "`Fail`" do not emit more "`\r\033[K`" conditionally — they only remove if present.
The transitional line is factually there. If the spinner never even started ticking (less than 80 milliseconds), now
There is no escape sequence at all. This is the correct behavior, and none of the tests relied on it.
Contrary, but an observable difference in those who perform diff of output.
The other consumers of progress (_`ast embed`, _`wiki embed`) already used _`Task.Update` and did not.
They were touched. They have the opposite problem, known and recognized: withoutTTY they don't report anything.

## Conhecimento do sistema

- `Printer` agora tem estado (`progress`, sob mutex). `WithWriter` continua devolvendo um
Newly created, so the copy starts clean — that's correct since the transitional line belongs
On the stream, not on the prefix.
- Ordem de lock entre `Task` e `Printer`: sempre `t.mu` → `p.mu`. O spinner e o `Done` seguem a
  mesma ordem; nada trava `p.mu` antes de `t.mu`.
It falls to 80 when `stdout` is not a terminal, which happens under `go test`.
That's why the test of truncation can assert 80 columns without simulating tty.
