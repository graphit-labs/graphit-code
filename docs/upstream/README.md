# Relatórios upstream pendentes

Quatro defeitos de dependências externas, encontrados e reduzidos a repro mínimo durante o
trabalho no indexador AST. **Nenhum foi enviado** — abrir issue em repositório de terceiro é
ação externa e depende de decisão do Engenheiro.

Cada arquivo aqui é o corpo do report, pronto para colar, em inglês (público do projeto
upstream).

| arquivo | projeto | severidade | efeito no Graphit |
|---|---|---|---|
| `antlr4-go-stdout.md` | antlr/antlr4 (Go runtime) v4.13.1 | alta | corrompia o protocolo stdout do MCP |
| `grammars-v4-plsql-sll-blowup.md` | antlr/grammars-v4 | alta | OOM em arquivo de 1,7 KB |
| `liblbug-fts-insert.md` | LadybugDB/liblbug 0.18.2 | alta | incremental O(corpus) em vez de O(1) |
| `liblbug-close-and-unwind.md` | LadybugDB/liblbug 0.18.2 | média | `Close()` até 5s; crash em `UNWIND ... CREATE` |

O de maior valor para nós é `liblbug-fts-insert.md`: enquanto ele não for corrigido, cada
atualização incremental do índice de busca recria sete índices FTS sobre o corpus inteiro.
`TestLadybugFTSPerRowInsertIsReliable` está invertido de propósito — passa enquanto o bug
existe e falha quando for consertado, avisando que a mitigação pode sair.
