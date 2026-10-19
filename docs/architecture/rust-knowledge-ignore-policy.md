---
title: Política padrão Knowledge de ignores em Rust
references:
  - target: {type: task, id: tsk-1138}
    relation: implements
  - target: {type: task, id: tsk-87f7}
    relation: implements
  - target: {type: task, id: tsk-2706}
    relation: uses
  - target: {type: task, id: tsk-df9e}
    relation: implements
  - target: {type: task, id: tsk-b210}
    relation: derived_from
  - target: {type: task, id: tsk-6997}
    relation: uses
  - target: {type: task, id: tsk-8d62}
    relation: uses
  - target: {type: task, id: tsk-ab16}
    relation: uses
  - target: {type: task, id: tsk-f814}
    relation: relates_to
  - target: {type: task, id: tsk-9840}
    relation: supports
  - target: {type: knowledge, id: Ignore_Files}
    relation: implements
  - target: {type: knowledge, id: Configuração_tipada_Knowledge_em_Rust}
    relation: uses
---

# Política padrão Knowledge de ignores em Rust

Referência para consumidores da seleção documental. A folha `tsk-1138` remove
as quotas e stats que cobravam os defaults compilados; a API atual recebe
somente contexto original e ConfigSnapshot. A correção foi qualificada em
Linux com seis testes focais e Clippy estrito, sem repetir o gate amplo. As qualificações anteriores de `tsk-df9e` e Core65 são históricas.
Windows e macOS não foram executados por esta correção.

`KnowledgeIgnorePolicy` produz os critérios protegidos do módulo Knowledge.
Configuração, captura física, matching e autorização têm outros produtores.
A política não lê arquivos, ambiente, CWD ou keys de configuração, não carrega
controles e não concede acesso.

## Jornada e origem dos dados

O caller obtém a configuração efetiva com Core, conserva o contexto original
da operação e resolve a policy antes de pedir seleção ao Core. O getter real
`ConfigSnapshot::brand() -> &Brand` fornece o namespace imutável; não há key
runtime `brand` que esta policy leia.

Core valida o namespace: key com 1–64 bytes ASCII, bordas alfanuméricas,
hífens internos opcionais, lowercase ASCII após trim e rejeição dos nomes de
controle Git reservados. `Brand::directory()` retorna dot + key. A policy
acrescenta somente uma barra final para obter uma regra de diretório.

Com Brand `graphit`, a regra é `.graphit/`. Com `acme-code`, é
`.acme-code/`; a policy não conserva uma segunda regra `.graphit/`.
As outras 79 regras permanecem idênticas. Brand inválida é rejeitada pelo
produtor Core; não há parser ou fallback de Brand no consumidor.

A seleção documental e o watcher precisam respeitar a mesma lista de exclusões configurada. Os critérios externos de inclusão/exclusão permanecem o contrato; constructors e mecanismos internos não definem uma API pública da policy.

## Defaultset KI-D1 integral

KI-D1 identifica o contrato desta folha no packet, Source e fixture literal.
Não é formato wire, importador, DTO de autoridade ou promessa de
retrocompatibilidade. Há 80 padrões em ordem, incluindo uma regra dinâmica
no índice 70, contando a partir de zero.

| Categoria | Quantidade | Regras exatas |
|---|---:|---|
| compiled | 14 | `*.exe`, `*.dll`, `*.so`, `*.dylib`, `*.o`, `*.a`, `*.lib`, `*.class`, `*.jar`, `*.war`, `*.pyc`, `*.pyo`, `*.whl`, `*.egg` |
| media | 16 | `*.jpg`, `*.jpeg`, `*.png`, `*.gif`, `*.bmp`, `*.ico`, `*.svg`, `*.webp`, `*.mp3`, `*.mp4`, `*.avi`, `*.mov`, `*.mkv`, `*.flv`, `*.wav`, `*.flac` |
| archives | 7 | `*.zip`, `*.tar`, `*.gz`, `*.bz2`, `*.7z`, `*.rar`, `*.xz` |
| office | 7 | `*.pdf`, `*.doc`, `*.docx`, `*.xls`, `*.xlsx`, `*.ppt`, `*.pptx` |
| minified | 3 | `*.min.js`, `*.min.css`, `*.map` |
| locks | 5 | `*.lock`, `package-lock.json`, `yarn.lock`, `Cargo.lock`, `go.sum` |
| dependencies_build | 10 | `node_modules/`, `.git/`, `__pycache__/`, `.venv/`, `venv/`, `vendor/`, `dist/`, `build/`, `.cache/`, `coverage/` |
| editor | 6 | `.idea/`, `.vscode/`, `.vs/`, `*.swp`, `*.swo`, `*~` |
| os | 2 | `.DS_Store`, `Thumbs.db` |
| brand | 1 | `<Brand.directory()>/` |
| agents | 9 | `.agents/`, `.claude/`, `.cursor/`, `.kiro/`, `.codex/`, `.gemini/`, `.opencode/`, `.qwen/`, `.kimi-code/` |

Preservar ordem, case e barras, incluindo `Cargo.lock`, `.DS_Store` e
`Thumbs.db`. Não aplicar casefold ou normalização Unicode às regras.
A normalização da Brand pertence ao produtor upstream.

Não há exclusão genérica de dotdirectories nem regra adicional para
`graphit.lock.json`. A lista de extensões documentais de KnowledgeSettings
é outro contrato e não substitui estes defaults.

O custom control é `.wikiignore`; o controle Git é `.gitignore`.
Core mantém a precedência de defaults protegidos sobre custom deepest→root e
Git deepest→root. Uma negação custom não reabre um diretório excluído pelo
defaultset. A policy só fornece dados: quem carrega, aplica e revalida
controles é o produtor de seleção Core.

## API e limites de entrada

[Implementação](../../crates/graphit-knowledge/src/ignore_policy.rs),
[fixtures públicas](../../crates/graphit-knowledge/tests/ignore_policy.rs) e
[defaultset literal](../../crates/graphit-knowledge/tests/fixtures/ignore_policy/defaults.txt)
compõem a policy ativada em `graphit_knowledge::ignore_policy`.

```rust,ignore
use graphit_core::{config::ConfigSnapshot, file_io::SnapshotContext};
use graphit_knowledge::ignore_policy::{KnowledgeIgnorePolicy, KnowledgeIgnorePolicyError};

fn criteria(
    original: &SnapshotContext,
    captured_config: &ConfigSnapshot,
) -> Result<KnowledgeIgnorePolicy, KnowledgeIgnorePolicyError> {
    KnowledgeIgnorePolicy::resolve(original, captured_config)
}
```

Os 79 textos estáticos e a regra Brand sempre formam a lista completa de
80 padrões. Não há quotas de quantidade, bytes de padrão ou bytes totais da
policy, nem stats ou adapter de quotas. O limite real da Brand é validado
pelo produtor Core: uma key de até 64 bytes produz um padrão de até 66 bytes.
Os limites de leitura e validação da configuração e dos arquivos de controle
continuam nos seus produtores Core, incluindo a seleção física autorizada.

As reservas fallíveis usam os tamanhos reais da lista e strings, sem cobrar
constantes compiladas contra orçamento do consumidor. Isso não estabelece
um teto de RSS. O produto conserva campos privados e dados completos,
sem constructor público, Clone ou serialização/deserialização.

| Getter | Resultado |
|---|---|
| `patterns() -> &[String]` | 80 textos ordenados, emprestados e imutáveis |
| `brand_directory() -> &str` | Diretório interno sem barra final, derivado do padrão 70 |
| `custom_filename() -> &'static str` | `.wikiignore` |
| `git_filename() -> &'static str` | `.gitignore` |

## Erros e contexto

`KnowledgeIgnorePolicyError` preserva causa e categoria:

| Variante | `kind()` | Resultado |
|---|---|---|
| `Context(io::Error)` | Categoria original | Cancelamento Interrupted; expiração TimedOut |
| `Allocation(TryReserveError)` | OutOfMemory | Falha da reserva real, sem resultado parcial |

Display é genérico e não incorpora a Brand ou critérios. `Error::source`
preserva a causa de Context e Allocation.

Checks usam a instância de SnapshotContext passada, antes de examinar
inputs, antes de reservas/cópias, em cada padrão e antes do retorno.
O resolver conserva o contexto original fornecido. Cancelamento
vence expiração conforme Core, e contexto inválido é observado antes de
produzir os dados. São checks cooperativos, não promessa de preempção
durante alocação.

O caller usa o contexto original e seus sinais de cancelamento/prazo.
Policy já retornada é dado imutável e pode sobreviver ao ConfigSnapshot;
seus getters não recebem contexto nem autorizam acesso físico. Uma nova
resolução após cancelamento retorna a causa original Interrupted.

## Fronteira com seleção física

ConfigSnapshot conserva sua Brand e rejeita layers de outra Brand, mas não
comprova vínculo com SnapshotContext, PROJECT, native scope ou Auth. O caller
e o produtor Core devem fechar esse pareamento por seus contratos reais.
Paths, lista de padrões, hashes ou DTOs não recompõem autoridade.

A policy não chama `IgnoreScope::load/child`, não faz matching de nomes
físicos e não lê documentos. Core é responsável por metadata nativa,
pruning antes de entrar, controles observados sob o request original,
receipts de presença e ausência, custos, revalidação e payload somente
dos documentos selecionados. O contrato de seleção é consumido do produtor
`tsk-f814`; Knowledge não implementa um walker paralelo.

O teste de comportamento usa IgnoreScope existente para aplicar os
defaults. Seu loader global realiza I/O de preparação de fixture, fora do
resolver puro; isso não qualifica captura/accounting do futuro produtor.

## Fixtures e verificação atual

Seis testes públicos protegem os propósitos que permanecem na API:

- Os 80 textos literais, ordem, case e nomes dos controles.
- A troca somente do item Brand, lifetime dos dados e cancelamento posterior
  usando o mesmo contexto original.
- Brand máxima válida com a lista completa e todos os outros 79 itens intactos.
- Cancelamento, expiração e precedência, sem sleeps ou renovação do prazo.
- Defaults protegidos no matcher Core real, negações ineficazes e paths permitidos.
- Categoria, causa e mensagem pública de Context e Allocation. A causa de
  Allocation usa uma reserva realmente recusada por capacity overflow;
  não simula OOM físico ou cancelamento durante alocação.

O teste e os ramos que só verificavam quotas de defaults foram retirados junto
com esse contrato artificial. `defaults.txt` continua byte-exata. A correção
não modifica os limites de entrada da Brand, config ou controles Core.

```text
cargo test --offline --locked -p graphit-knowledge --test ignore_policy -j4
cargo clippy --offline --locked -p graphit-knowledge --lib --test ignore_policy --no-deps -j4 -- -D warnings
```

O antigo TestKnowledgePathsAndIgnore mistura HOME/storage com uma
asserção de node_modules. O novo teste mantém esse propósito de ignore
e amplia comportamento contratual, sem portar o setup de armazenamento
ou mutar ambiente. Testes de scope root/docs pertencem ao produtor Core
e não são duplicados nesta policy.

## Histórico da ativação e qualificação de df9e

Na janela original, após a ativação dos arquivos e export pelo principal,
o worker executou as duas invocações focadas:

```text
cargo test --locked -p graphit-knowledge --test ignore_policy
cargo clippy --locked -p graphit-knowledge --lib --test ignore_policy -- -D warnings
```

O principal acrescentou a entrada Integration no FAST_TESTS:
package `graphit-knowledge`, target `ignore_policy`, preservando o prefixo
das 29 entradas existentes. Na ativação, a lista passou a 30 entradas.
O principal qualificou a falha sentinela bloqueada pelo Git no grupo de sete
guards, já com 31 entradas contemporâneas. A invocação focada é:

```text
cargo xtask verify-fast-guards graphit-knowledge/ignore_policy
```

O teste terminou com sete PASS, zero falhas e zero testes filtrados; Clippy
terminou sem warnings. Ambos tiveram EXIT0, child aguardado e os dez hashes
de Source, produtores, manifest e lockfile idênticos antes e depois.
O target de compilação foi privado, com jobs=2, incremental=0 e debug=0;
TMPDIR estava em disco ext4, sem piso de RAM.

Os receipts abaixo identificam a qualificação histórica; sua disponibilidade
local não é pressuposto da correção atual. Não foram reconstruídos após a
remoção de artefatos `.build`. Task preserva a evidência registrada:

- `.build/knowledge-ignore-policy-stage/evidence/qualification/tests-1791609849338416192/receipt.json`:
  17,155288 s; SHA-256 `46e5d7535d32d0f739f03e821bd3b5871fc0ffb391ac8f8ada389a6f2b95c947`.
- `.build/knowledge-ignore-policy-stage/evidence/qualification/clippy-1791610009510655194/receipt.json`:
  15,252941 s; SHA-256 `7d805a273bdb1d1885ba3f177f3ca9b04ac7fcb64c5fdda72c7dae006739be5e`.

O guard executado incluiu `graphit-knowledge/ignore_policy` na invocação
`cargo xtask verify-fast-guards` com outros seis labels reais. O sentinel
`broken_fast_contract` falhou efetivamente neste target; Cargo retornou 101
e o comando Git pre-commit retornou 1. O runner confirmou o bloqueio deste
label e o gate positivo depois da restauração dos sete probes. O processo
terminou com EXIT0 em 613,724127 s, 308 fences de Source idênticas antes/depois,
nenhum scratch restante e todos os children aguardados. Evidência Root:

- `.build/rust-core-birth-anchor-20261009/root-qualification/seven-new-fast-guards-1791611469894397053/receipt.json`:
  SHA-256 `d82ec2294d036a7410aff0f914fc7f5de3fd2aba18bbb6ad8f86cc489279e263`.
- `command.log` no mesmo diretório:
  SHA-256 `9b1eccc00c1c64757196f49d3fdf39395702279af3dc3e4414175ada269798b6`.

As invocações do projeto podem ser usadas nas três plataformas; a qualificação
registrada comprova somente Linux. Não usar filtro Library que execute zero
testes do target público. O gate amplo final da migração não foi qualificado
por esta unidade. A captura física e o walker futuros continuam sob seus
contratos e produtores próprios.

## Histórico da adaptação privada Core65

A folha tsk-87f7 adaptou fixtures e exemplos ao contrato Core65 SourceReady.
SnapshotContext::new(deadline, Cancellation) é infalível; SourceReadLimits
recebe somente range_bytes, block_bytes e stream_bytes. O contexto original
conserva identidade, cancelamento e prazo.

A qualificação privada Core65 de tsk-87f7 passou no Linux: 151 testes
funcionais/unitários, sete doctests, Clippy estrito nos targets afetados e
formatação dos 18 arquivos Rust. Naquela qualificação, o patch aguardava composição serial pelo principal
com o produtor Core; este registro não descreve o estado atual dessa integração. As qualificações Main anteriores permanecem históricas.
