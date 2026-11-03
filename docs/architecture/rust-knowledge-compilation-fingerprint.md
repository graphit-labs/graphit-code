---
type: architecture
title: Fingerprint de compilação documental em Rust
description: Receita JSON compacta limitada sobre metadados admitidos e corpo capturado, com SHA-256 incremental e requisição original.
tags:
  - wiki
  - architecture
  - important
sources:
  - resource: crates/graphit-knowledge/src/compilation_fingerprint.rs
  - resource: crates/graphit-knowledge/tests/compilation_fingerprint.rs
  - resource: crates/graphit-knowledge/src/publication_metadata.rs
references:
  - target: {type: task, id: tsk-87f7}
    relation: implements
  - target: {type: task, id: tsk-2706}
    relation: uses
  - target: {type: task, id: tsk-db76}
    relation: implements
  - target: {type: task, id: tsk-25a0}
    relation: implements
  - target: {type: task, id: tsk-41ba}
    relation: uses
  - target: {type: task, id: tsk-9df1}
    relation: uses
  - target: {type: task, id: tsk-a32d}
    relation: derived_from
---

Este módulo calcula SHA256 dos bytes da receita authored-document-v1 sobre
CompiledDocument admitido pela [factory](rust-knowledge-publication-metadata.md).
O fingerprint distingue a compilação completa do digest físico da fonte.

## Consumo e ownership atual

O consumidor fornece o mesmo &Arc<SnapshotContext> da captura, empresta o produto
CompiledDocument e escolhe CompilationFingerprintLimits com max_preimage_bytes
positivo. fingerprint_compiled(request, compiled, limits) retorna um newtype
privado de32bytes, as_bytes()/Eq e Debug opaco. Sem construtor público, Default,
Clone, Deserialize ou conversão do digest físico. Os bytes são dados e podem
sobreviver aos owners; não publicam nem concedem acesso à fonte.

Metadados/policy usam ownership Rust normal; Core conserva o corpo externo.
Fingerprint empresta os owners durante a operação, sem copiar texto/contexto/pin.
CompiledDocument::check_request delega ao reader original para identidade,
cancelamento e prazo. Não há NativeCaptureRequest/NativePermit/work/resource,
contabilidade universal ou revalidação física. Foreign falha antes encoding.

## Receita exata

A preimage contém estes bytes em ordem:

1. ASCII `graphit.knowledge.compilation-fingerprint` e um byte NUL: 42 bytes.
2. A tuple JSON compacta UTF-8 produzida por `serde_json@1.0.151`, sem LF final,
   BOM adicional ou pretty printing.

A estrutura fixa é:

```text
[
  1,
  "authored-document-v1",
  [
    "title-explicit-h1-fallback-trim.v1",
    "description-yaml-text-integral.v1",
    "kind-explicit-or-supplied.v1",
    "summary-editorial-prefix-utf8.v1",
    "serde-saphyr@1.3.0",
    "pulldown-cmark@0.13.4"
  ],
  [fallback_title_exact, kind_on_absent_exact, max_summary_bytes_u64],
  [whole_authored_source, title, kind, description_option, summary, truncated]
]
```

O bloco formatado explica a estrutura; seus espaços e quebras não fazem parte
da codificação compacta. P1 já conserva o limite de summary como `u64` admitido.
O encoder recebe somente essa tuple de tipos built-in, arrays fixos e strings
emprestadas; não recebe callback `Serialize`, formatter ou mapas do caller.

Fallback e kind policy entram integralmente mesmo quando o título H1 ou kind
autoral os tornam dispensáveis. O limite de summary entra mesmo se nenhum corte
aconteceu. Description `None` codifica `null`; `Some("")` codifica `""`; whitespace
integral continua integral. Kind custom e vazio permanecem strings. Summary usa
o resultado exato do parser, incluindo espaço final preservado por um corte.

Whole source inclui BOM, frontmatter, CRLF/LF e espaços capturados. Não há
normalização YAML/Markdown, canonicalização própria, JCS ou fallback
de framing. Escapes JSON seguem a biblioteca madura. `sha2@0.11.0` calcula
SHA-256 incremental sobre os bytes efetivamente emitidos, incluindo o prefixo.

Para a fixture `# H\n\nbody\n`, fallback `F`, kind policy `guide` e summary cap32,
os campos efetivos são `H`, `guide`, `None`, `body`, `false`. A JSON literal é:

```json
[1,"authored-document-v1",["title-explicit-h1-fallback-trim.v1","description-yaml-text-integral.v1","kind-explicit-or-supplied.v1","summary-editorial-prefix-utf8.v1","serde-saphyr@1.3.0","pulldown-cmark@0.13.4"],["F","guide",32],["# H\n\nbody\n","H","guide",null,"body",false]]
```

São 277 bytes JSON e 42 de prefixo, total319. O digest esperado é
`a744b7dd3e91055494faf5be007dc956b5729be8f8d04b2c3e827d345b7fea9c`.
Cap319 passou; cap318 devolveu erro sem fingerprint na qualificação privada Linux.
Esse oráculo foi definido independentemente no planejamento Python, sem execução
Rust naquela fase; o confronto Rust posterior passou junto aos nove oráculos.


## Limites e falhas

Cada write ativo verifica o contexto original, soma emitted + chunk.len com
checked_add, confere o limite e só então atualiza integralmente hash/contador.
Um chunk negado não altera nenhum dos dois. O limite inclui domínio/NUL e bytes
JSON/escapes realmente emitidos, não caracteres nem tamanho bruto de input.
Prefixo e JSON passam pelo mesmo writer. Antes do SDK e imediatamente após
finalize, o binding original é verificado. Empty write/flush também verificam
o contexto e não alteram hash/contador.

Write::write_all implementa uma tentativa all-or-error inclusive buffer vazio;
o default std repete Interrupted e pula writes vazios. Cancelamento conserva
Interrupted no transporte SDK. A primeira falha encerra a operação: posteriores
write/write_all/flush devolvem só seu transporte, sem substituir causa ou alterar
estado. io::Error original é movido para slot privado; o transporte contém seu
kind. Limit/Overflow usam InvalidData no transporte e conservam a variante tipada.
Após SDK, a primeira causa local vence proxy SDK e cancelamento posterior.
Sem causa local, Json conserva serde_json::Error; no sucesso, o finalcheck ainda
pode impedir o resultado inteiro. Não há digest parcial ou reconstrução por texto.

| Variante | Causa |
| --- | --- |
| OriginalRequest(io::Error) | identidade, cancelamento ou prazo original |
| Json(serde_json::Error) | erro SDK sem primeira falha local |
| Limit(&'static str) | preimage excedida antes de update |
| Overflow(&'static str) | soma impossível antes de mutação |

Display não inclui fonte/path. Error::source conserva causa owned, sem prometer
wrapper IO em serde_json::Error::source. A tuple fechada aceita só built-in
primitives, arrays e strings emprestadas; não serializer/formatter do caller.
O SDK pode alocar um container terminal O(1) de erro. Texto limitado e checks
cooperativos não são garantia de RSS/OOM nem preempção do SDK síncrono.

## Verificação atual

A corretiva tsk-db76 conserva nove oráculos literais independentes com capturas
reais, BOM/CRLF/Unicode/escapes, config, null/empty, kind custom/vazio, descrição
integral/summary truncada, determinismo, cap exato/one-under e prefixo41/42.
Cancel após compilação e foreign conservam causa e entradas. Deadline agora é
operação real sobre compilação válida: contexto original expira e fingerprint
retorna TimedOut. Não há prova baseada em construtor de wrapper removido.

Units do writer/completion real protegem SDK Interrupted sem retry, empty/flush,
cancel após update, first-limit poisoning/cancelposterior, chunk negado sem
mutação, finalcancel de JSON completa e overflow útil do contador de bytes.
Testes apenaswork/quotas removidos não são obrigações atuais. Comandos:

~~~text
cargo test --offline --locked -p graphit-knowledge --test compilation_fingerprint --test publication_metadata
cargo test --offline --locked -p graphit-knowledge --lib compilation_fingerprint::tests
cargo test --offline --locked -p graphit-knowledge --lib publication_metadata::tests
cargo clippy --offline --locked -p graphit-knowledge --lib --test compilation_fingerprint --test publication_metadata -- -D warnings
~~~

A API está integrada ao Knowledge principal. Em Linux, os testes públicos de
fingerprint e metadados (19), suas unidades (10), Clippy com `-D warnings` e
formatação passaram no código principal. A avaliação de staleness tem sua
[API própria](rust-knowledge-staleness-projection.md). Outras plataformas e a
persistência documental permanecem em suas entregas próprias.

## Freshness, outras receitas e publicação

`PhysicalSourceDigest` é SHA256 dos bytes brutos da fonte. CompilationFingerprint
é SHA256 da recipe inteira; comparar os dois não avalia staleness. Para avaliar
uma nova observação, o consumidor futuro terá de aplicar a mesma receita e
parâmetros completos sob seleção/contexto autorizado, com seus próprios vínculos
de geração e pin. Igualdade não constitui prova matemática de ausência de colisões.

Receitas de domínio/comunidade/AI exigem inputs, prompt, modelo, provider e
configuração próprios; não podem declarar equivalência com `authored-document-v1`.
Também não se inferem CURRENT, seleção completa, revalidação física, PageHashes
fresh, publicação Data ou autoridade a partir destes32bytes. Mudanças semânticas
da receita/SDK exigem contrato e oráculos versionados, sem importador ou fallback.

Indexação e metadata de freshness do Wiki continuam um lifecycle separado.
Documentação/source com hash correspondente e um guard aprovado não removem
`stale`, `stale_since` ou dispensam validação documental autorizada. Esta folha
não realiza reset/sync manual nem declara a validação ponta a ponta concluída.

## Adaptação privada Core65

A folha tsk-87f7 adapta fixtures e exemplos ao contrato Core65 SourceReady.
SnapshotContext::new(deadline, Cancellation) é infalível; SourceReadLimits
recebe somente range_bytes, block_bytes e stream_bytes. O contexto original
conserva identidade, cancelamento e prazo.

A qualificação privada Core65 de tsk-87f7 passou no Linux: 151 testes
funcionais/unitários, sete doctests, Clippy estrito nos targets afetados e
formatação dos 18 arquivos Rust. O patch aguarda composição serial pelo principal
com o produtor Core. As qualificações Main anteriores permanecem históricas.

Em Unix, os casos existentes removem o caminho físico depois da captura
e antes de compilar. Bytes retidos, borrowing e digests normativos permanecem
verificados; o teste prova independência de reabrir o path, sem instrumentar
todos os syscalls. Os contratos de recipe e limites intrínsecos são preservados.
