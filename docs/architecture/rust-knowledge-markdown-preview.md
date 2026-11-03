---
title: Prévia Markdown Knowledge em Rust
references:
  - target: {type: task, id: tsk-87f7}
    relation: implements
  - target: {type: task, id: tsk-2706}
    relation: uses
  - target: {type: task, id: tsk-2ccd}
    relation: implements
  - target: {type: task, id: tsk-69f2}
    relation: derived_from
  - target: {type: task, id: tsk-b106}
    relation: derived_from
  - target: {type: task, id: tsk-31f2}
    relation: uses
  - target: {type: memory, id: 01M4GMYQ6H5SEFTA86F5B5D7DT}
    relation: relates_to
---

# Prévia Markdown Knowledge em Rust

Referência para integradores que já têm corpo e resumo UTF-8 de um resultado
documental. A API foi ativada no módulo compilado pelo principal e qualificada
em Linux: 12 testes passaram, sem falhas, ignorados ou filtrados, e Clippy
estrito passou. O guard de integração também foi qualificado individualmente
em Linux, com falha sentinela bloqueada pelo hook Git e restauração positiva.
Esta unidade `tsk-2ccd` não declara validação ponta a ponta, integração de busca ou
autoridade física entregues.

## Jornada e prioridade do resumo

O consumidor fornece `body`, `summary`, o `SnapshotContext` original e três
limites positivos. A função retorna texto, seleção `Summary` ou `Body`, intervalo
de bytes no input selecionado e indicação de corte. Um resumo com ao menos um
scalar que não seja `char::is_whitespace()` vence integralmente: whitespace
externo, indentação, CRLF e Markdown são preservados. Resumo vazio ou composto
somente de whitespace Unicode seleciona o corpo.

O contrato summary-first preserva Markdown. Os limites vêm explicitamente do consumidor e o texto selecionado mantém sua semântica de whitespace. O corpo integral de detalhe, cópia e exportação não é alterado pela prévia. A API Rust está em `crates/graphit-knowledge/src/preview.rs`.

## API e lifetime

Módulo ativo: `graphit_knowledge::preview` em
`crates/graphit-knowledge/src/preview.rs`.

```rust
pub fn preview_markdown<'a>(
    context: &SnapshotContext,
    body: &'a str,
    summary: &'a str,
    limits: MarkdownPreviewLimits,
) -> Result<MarkdownPreview<'a>, MarkdownPreviewError>;
```

`MarkdownPreviewLimits` expõe `max_input_bytes`, `max_output_bytes` e
`max_body_scalars`, todos `NonZeroUsize`, sem `Default`. O produto tem campos
privados e getters `text() -> &str`, `source() -> PreviewSource`,
`source_range() -> Range<usize>` e `truncated() -> bool`. `PreviewSource` tem
variantes `Summary` e `Body`. Não há construtor público, `Clone` ou serde para
o produto.

O texto sem corte usa `Cow::Borrowed`; o texto cortado usa `Cow::Owned` somente
para o prefixo e marker. O lifetime `'a` depende dos inputs, independentemente
do contexto. Assim, um resultado emprestado continua legível após o contexto
morrer enquanto seus inputs continuam vivos. O tipo não permite presumir que
o ramo owned possa sobreviver aos inputs por uma análise dinâmica do ramo.

## Limites e corte editorial

| Limite | Regra |
|---|---|
| `max_input_bytes` | Hard: soma checked de bytes brutos do corpo e resumo, inclusive corpo não selecionado. Verificado antes de qualquer scan ou alocação. |
| `max_output_bytes` | Hard: saída integral escolhida, incluindo os três bytes da ellipsis quando há corte. Excesso retorna erro completo. |
| `max_body_scalars` | Soft: quantidade de scalars Unicode do prefixo do corpo; não restringe o resumo. Sem default. |

Se o corpo cabe no cap de scalars, o resultado preserva todos seus bytes.
Corpo vazio produz `Body`, `0..0`, texto vazio e `truncated=false`; corpo somente
whitespace não é ausência e mantém seu texto sob cap suficiente.

Se precisa de corte, o scan guarda o último delimiter whitespace Unicode cujo
índice de scalar esteja em `[floor(cap/2), cap)` e cujo índice de byte seja maior
que zero. O corte ocorre antes desse delimiter. Sem candidato, ocorre depois
de exatamente `cap` scalars. Um único lookahead comprova que há mais texto;
o scalar de índice `cap` não pode ser escolhido como delimiter.

O intervalo retornado é exatamente `0..cut` no input original. A saída é esse
prefixo mais uma ellipsis `…` (U+2026, três bytes), excluída do intervalo.
O marker não consome o cap de scalars do corpo. Não há UAX29, segmentação de
graphemes, NFC, casefold, balancing de Markdown ou reescrita de palavras. Um
cluster pode ser separado entre scalars; o corte nunca divide bytes UTF-8.

| Input / cap de scalars | Intervalo / saída |
|---|---|
| `ação 🍀` / 6 | `0..11`, integral, sem corte |
| `ação 🍀` / 5 | `0..6`, `ação…` |
| 241 vezes `界` / 240 | `0..720`, 240 vezes `界` mais `…`; saída 723 bytes |
| `abé` + U+2003 + `cdef` / 6 | `0..4`, `abé…` |
| `e` + U+0301 + `XYZ` / 2 | `0..3`, `e` + U+0301 + `…`, sem NFC |
| `👨‍👩‍👧‍👦X` / 2 | `0..7`, `👨` + U+200D + `…`, saída 10 bytes |
| `a b c dXYZ` / 7 | `0..5`, `a b c…`; último delimiter elegível |
| `abcdef ghi` / 6 | `0..6`, `abcdef…`; lookahead whitespace excluído |

Uma saída que precisa de 723 bytes é aceita em 723 e rejeitada em 722, sem
cortar mais ou retirar o marker. Um resumo acima do hard output cap retorna
erro; não troca para corpo. Texto vazio cabe num cap positivo de um byte.

## Erros, contexto e recursos

`MarkdownPreviewError` expõe `Context(io::Error)`,
`LimitExceeded(&'static str)` e `Allocation(TryReserveError)`. `kind()` preserva
o kind de contexto, usa `FileTooLarge` para limite e `OutOfMemory` para alocação.
As labels de limite são `input bytes` e `output bytes`. `Error::source()` mantém
a causa de contexto/alocação; limite não tem causa. `Display` é genérico e não
inclui corpo ou resumo. `&str` já é UTF-8: esta API não implementa decoder de
bytes nem diagnóstico de codificação inválida.

`context.check()` vem primeiro, antes de quotas; cancelamento retorna
`Interrupted`, prazo passado `TimedOut`, e cancelamento vence se ambos existem,
conforme o produtor Core `tsk-31f2`. Há checks por scalar varrido, antes de
reserve/copy e antes do retorno. O consumidor fornece o contexto original;
Arc clones conservam sua identidade, prazo e sinal de cancelamento. A função
não lê ambiente, CWD ou filesystem. A ausência de chamadas I/O é verificável
no Source; não é medida por contadores artificiais. Getters do texto imutável
não recebem contexto e continuam legíveis enquanto os inputs vivem.

O scan usa memória O(1), sem `Vec<char>`, clone integral ou lowercase. Somente
o ramo cortado aloca, com `try_reserve_exact` do tamanho real após preflight de
saída. Caps `usize::MAX` não são pedidos de capacidade. Há aritmética checked
para somas e contadores. Checks são cooperativos: allocator e cópias nativas
não têm garantia de preempção, hard RSS ou ganho medido de desempenho.

## Fronteiras dos consumidores

Seleção e intervalo representam texto; não são `SourceID`, captura física,
geração Data, autorização ou recibo de origem. A função não consulta nem gera
documentos, não faz ranking/highlights, staleness, lint, lifecycle ou resolução
de referências. O contrato de recuperação MCP versus AI/Live continua no
registro Memory citado; uma prévia não executa nenhuma dessas jornadas.

A saída continua Markdown não confiável. O renderer `MarkdownContent` existente
da UI mantém sua responsabilidade por raw HTML desativado, sanitização de URLs
e links fora do botão de registro, conforme `tsk-b106`. Esta unidade não altera
UI, `project_document`, `settings`, `ignore_policy`, referências ou staleness.
Não escapa, renderiza HTML, completa fences ou resolve links. O caso de fence
com prefixo de quatro scalars produz literalmente três backticks mais `r…`;
lista `- a\n- b` com cap dois produz `-…`. O texto original permanece intacto.

## Source, oráculos e ativação

Stage histórico privado de ownership Knowledge, preservado após ativação:

- `.build/knowledge-markdown-preview-stage/src/preview.rs`
- `.build/knowledge-markdown-preview-stage/tests/preview.rs`
- `.build/knowledge-markdown-preview-stage/docs/rust-knowledge-markdown-preview.md`

Os testes públicos em `crates/graphit-knowledge/tests/preview.rs`
cobrem P1–P12: prioridade/preservação do resumo, whitespace Unicode/corpo,
vazio, scalars/bytes e integridade, CJK/marker, clusters/delimiters/lookahead,
sintaxe Markdown literal, input bruto/MAX caps, hard output sem fallback,
cancelamento/prazo/causas, integridade sob o request original e lifetime/error Display.
Os oráculos têm entradas e saídas finitas na especificação `tsk-2ccd`; a suíte
executada contém 12 funções de teste e protege P1–P12. Resultado Linux
qualificado abaixo; revisão e contagem de casos não substituem runtime.
Alocação falível é inspecionada no Source; não se força exaustão de RAM.

O principal ativou três arquivos byte-exatos ao Stage e o export
`pub mod preview`; acrescentou a entrada
`FastTest::Integration { package: "graphit-knowledge", target: "preview" }`
preservando as 31 entradas contemporâneas, totalizando 32. O receipt de
ativação é
`.build/knowledge-markdown-preview-stage/evidence/root-activation-1791613002815212932/receipt.json`,
SHA-256 `1f1dfd91c3ebd216afb51f088d441815622aa63c97112b1f413f6748b172266e`.
A integração compartilhada, navegação e futuras janelas pertencem ao principal.
Nenhum manifest ou lockfile foi alterado por esta unidade.

Na janela delegada, o worker executou somente os dois comandos focados:

```text
cargo test --locked -p graphit-knowledge --test preview
cargo clippy --locked -p graphit-knowledge --lib --test preview -- -D warnings
```

Ambos terminaram com EXIT0 e child aguardado. Testes: 12 PASS, zero falhas,
ignorados ou filtrados. Clippy: sem warnings. Os 12 hashes de fontes próprias,
export, runner, produtores, manifests, lockfiles e documento ativado foram
idênticos antes/depois de cada execução. O documento recebeu somente esta
atualização de status depois de ambos os processos terminarem; contratos,
fontes Rust e Stage histórico não mudaram.

O target privado do próprio worker foi reutilizado de sua qualificação de
ignores (`.build/knowledge-ignore-policy-stage/qualification-target`), com
jobs=2, incremental=0 e debug=0.
TMPDIR `.build/knowledge-markdown-preview-stage/qualification-tmp` estava em
disco ext4, sem piso de RAM. Não houve primeira falha, correção ou retry.
Receipts imutáveis preservam comando, ambiente, fences, duração e raw log:

- `.build/knowledge-markdown-preview-stage/evidence/qualification/tests-1791613195661718346/receipt.json`:
  1,652886 s; SHA-256 `87607d625ee7b233f519a85681fce27423e3317ffd682664369a0eb017c10a2d`;
  log SHA-256 `024f30e4b2a3afe8f9c6eaee0e5c16624ba1f826f57b48b60b447ef800d02865`.
- `.build/knowledge-markdown-preview-stage/evidence/qualification/clippy-1791613214553841694/receipt.json`:
  2,449311 s; SHA-256 `66e7fafc7b1300afe3716396379a10628c2d3280d9e9cd20ea7d881002aca0b2`;
  log SHA-256 `1fb541491ae106bad4240dc2e714b92c08b0797f1669c4ac34bde988f2a61599`.

`rustfmt --check --edition 2024` nos dois arquivos Rust próprios ativados
terminou com EXIT0. A revisão independente confere contratos, fixtures,
Source e documentação nos dois sentidos. Não houve execução em Windows/macOS;
runtime Linux ou cross-compilation não qualificam runtime nesses sistemas.

O principal qualificou individualmente o guard correspondente à entrada de
integração `graphit-knowledge/preview`. A execução original do grupo de oito
guards terminou em `FAULT` por outro alvo; esse resultado histórico permanece
registrado. A prova individual selecionada do preview observou 12 testes reais
PASS e exatamente uma falha `broken_fast_contract`, Cargo EXIT101 e
`git hook run pre-commit` EXIT1. A restauração executou as 35 fases positivas
contemporâneas com EXIT0. Os 922 inputs copiados permaneceram byte-exatos,
todos os filhos diretos iniciados foram aguardados, scratch residual estava
vazio e o TMP privado foi removido. A prova de isolamento da cópia compilada
também passou.

Receipt individual selecionado:
`.build/rust-core-birth-anchor-20261009/root-qualification/seven-individual-guard-proofs-1791622887400100385/receipt.json`,
SHA-256 `446e707b9905496e82e515f8fcd9faa93478f579d75f9b885e743a0301e662cc`.
O alvo preview é o ordinal original 5; sua seção tem SHA-256
`1cf91cbfc37ea90499436af3a837e6c6ba37a88d385f3d39897057450e4e6255`.
O raw log original tem SHA-256
`2eea2033f21b4fa8fa728788ae731f893cde59bad5c0da68107388b7b3b051fb`;
o trace Git tem SHA-256
`25e2e1798620d6cb4eef14317f240edbfddf5494265059ed2e0d5cff0296e60d`.
Source e testes atuais conservam os hashes qualificados
`3e5d90a4377450132516f9c3ec714dde38071c15f7d91e186a09c1cc9df92071`
e `da325d1c3098c09d3dece7e10e026703b3c35590263c40a582176f44044b81f2`.
O documento recebeu apenas esta atualização de status, na janela liberada pelo
principal; não houve nova compilação ou alteração da API.

A manutenção do guard usa a invocação portátil do projeto:

```text
cargo xtask verify-fast-guards graphit-knowledge/preview
```

A qualificação individual não transforma o grupo histórico `FAULT` em PASS,
nem prova runtime Windows/macOS, quiescência de SDKs internos, desempenho de
produto ou o gate amplo final da migração. A recuperação, captura e geração
documental seguem seus produtores e contratos próprios.

## Adaptação privada Core65

A folha tsk-87f7 adapta fixtures e exemplos ao contrato Core65 SourceReady.
SnapshotContext::new(deadline, Cancellation) é infalível; SourceReadLimits
recebe somente range_bytes, block_bytes e stream_bytes. O contexto original
conserva identidade, cancelamento e prazo.

A qualificação privada Core65 de tsk-87f7 passou no Linux: 151 testes
funcionais/unitários, sete doctests, Clippy estrito nos targets afetados e
formatação dos 18 arquivos Rust. O patch aguarda composição serial pelo principal
com o produtor Core. As qualificações Main anteriores permanecem históricas.
