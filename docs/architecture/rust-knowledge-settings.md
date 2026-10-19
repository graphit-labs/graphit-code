---
type: architecture
title: Configuração tipada Knowledge em Rust
description: Resolução pura de docs_dir, README e extensões com origem, limites e contexto Core.
references:
  - target: {type: task, id: tsk-87f7}
    relation: implements
  - target: {type: task, id: tsk-2706}
    relation: uses
  - target: {type: task, id: tsk-c775}
    relation: validated_by
  - target: {type: task, id: tsk-729f}
    relation: describes
  - target: {type: task, id: tsk-b210}
    relation: derived_from
  - target: {type: task, id: tsk-6997}
    relation: uses
  - target: {type: task, id: tsk-31f2}
    relation: uses
  - target: {type: task, id: tsk-542d}
    relation: uses
  - target: {type: knowledge, id: Configuration_Reference}
    relation: implements
---

# Configuração tipada Knowledge em Rust

O consumidor Knowledge resolve uma configuração capturada antes de solicitar
a seleção documental. `tsk-729f` possui a implementação candidata em
`crates/graphit-knowledge/src/settings.rs`, seus testes inline e esta página.

Estado: Source ativada no crate e target público `tests/settings.rs` publicado
conjuntamente no grant do principal. A qualificação focada Linux passou em
2026-10-09: 17 unitários, duas integrações públicas, um doctest pertinente e
Clippy estrito, sem falhas ou testes ignorados. O guard proporcional Settings
foi qualificado em `tsk-c775` r14, com falha runtime e bloqueio Git reais,
restauração positiva e fontes íntegras. A referência de configuração
ativa está em [Configuration Reference](../guides/configuration.md#knowledge-indexing);
as regras estritas abaixo são o contrato Rust aprovado, ainda sem cutover.

## Resolução e origem

```rust,ignore
KnowledgeSettings::resolve(
    context: &SnapshotContext,
    snapshot: &ConfigSnapshot,
    index_path: Option<&str>,
    limits: KnowledgeSettingsLimits,
) -> Result<KnowledgeSettings, KnowledgeSettingsError>
```

`ConfigSnapshot` é o produtor Core entregue por `tsk-6997`. Ele seleciona cada
chave por presença: Command → Environment → Project → Global → Distribution.
Knowledge usa esse produtor; não lê ambiente global, JSON ou arquivos por conta
própria. A API do snapshot não o vincula a um `SnapshotContext`: estes argumentos
não demonstram pareamento, autorização ou captura documental.

A view possui campos privados, sem construtor público, `Clone` ou wire DTO.
`docs_dir()` retorna `&str`, `include_readme()` retorna `bool` e `extensions()`
retorna `&[String]`. `origin(KnowledgeSetting)` informa `SettingOrigin`:

| Origem | Significado |
|---|---|
| `Default` | A chave está ausente; não existe `ConfigSource` para o valor padrão. |
| `Config(ConfigSource)` | Camada vencedora realmente presente no snapshot. |
| `IndexPath` | Argumento explícito escolhido apenas para `docs_dir`. |

Os campos são resolvidos na ordem docs_dir, include_readme, extensions. Um erro
aborta toda a view; não há resultado parcial. Um valor vencedor inválido nunca
consulta uma camada inferior. `index_path: Some(...)` substitui somente o campo
docs, sem consultar ou validar seu valor inferior descartado. `Some("")` é
inválido, e um booleano ou CSV inválido ainda impede a view inteira.

| Chave | Tipo de entrada | Valor somente quando ausente |
|---|---|---|
| `knowledge.docs_dir` | Texto UTF-8 | `docs` |
| `knowledge.include_readme` | Booleano Core | `true` |
| `knowledge.extensions` | Texto CSV | Os dezesseis sufixos abaixo |

Defaults de extensões, na ordem de saída:

```text
.md, .markdown, .mdx, .txt, .adoc, .rst, .puml, .plantuml,
.yaml, .yml, .json, .proto, .graphql, .gql, .wsdl, .xml
```

Todos os defaults passam pelos mesmos limites de critérios e projeção. Não há
fallback privado de doze extensões, nem default para CSV explicitamente vazio.
Uma chave pública canônica semanticamente inválida para outro consumidor, como
`modules.ast` com texto, não é resolvida por Knowledge. Chaves inválidas ou null
já são rejeitados pelo produtor Core e não formam esse snapshot.

## Caminho de configuração

Admitir os bytes raw antes de aplicar `str::trim` nas bordas. Vazio após trim é
inválido. `.` representa projeto inteiro; os demais valores são componentes
relativos separados por slash, sem componentes vazios, `.` ou `..`.

Rejeitar absoluto, slash duplicado ou final, backslash, colon, NUL e caracteres
Unicode de controle. Preservar Unicode, caixa e espaços dentro dos componentes:
` 文档/Guide space/é ` resolve para `文档/Guide space/é`, sem normalização NFC ou
case-fold. `docs/./guide`, `../docs`, `C:/docs` e `docs\\guide` são inválidos.

Essa é uma sintaxe portátil de dados. Não garante que `CON`, trailing dot/space
ou qualquer componente exista ou seja aceito pelo filesystem de cada sistema.
Knowledge não faz `canonicalize`, `Path::components` dependente do host,
consulta de diretório ou symlink grant. A existência, os nomes observados e a
seleção física continuam responsabilidade do produtor CoreDocumentSelection.

## CSV e booleanos

Antes de trim/validação/dedup, contar todos os tokens de `split(',')`, incluindo
vazios e duplicados. Fazer trim de cada token; omitir vazios mistos. Uma lista
inteiramente vazia é inválida. Remover um leading dot opcional e exigir sufixo
não vazio, sem dot interno, slash, backslash, colon, controle ou whitespace.
Unicode é permitido e preservado; somente ASCII recebe case-fold.

Adicionar o dot de saída e deduplicar texto normalizado, preservando a primeira
ordem. `.MD, yaml,MD,,.JSON` produz `[".md", ".yaml", ".json"]`, mas debita
cinco tokens. `.MÉ,mÉ,é,É,é` preserva distinções Unicode e o texto decomposto.
Arrays, numbers e booleans não substituem a entrada CSV. `tar.gz`, `..md` e `.`
não são sufixos válidos.

Knowledge chama `get::<bool>` Core para `include_readme`. Fora da camada
Environment, somente booleano JSON é válido. Environment usa a conversão Core:
texto JSON `true` ou `false`, incluindo whitespace, é aceito; `TRUE`, `1`, `yes`,
vazio ou texto JSON quoted `"true"` são inválidos. Uma sondagem `get::<&str>`
emprestada contabiliza o texto vencedor antes dessa conversão; ela não procura
outra camada nem faz conversão permissiva.

O Core captura variáveis solicitadas pela Brand real. Por exemplo,
`Brand::new("acme-code")` produz `ACME_CODE_KNOWLEDGE__DOCS_DIR`,
`ACME_CODE_KNOWLEDGE__INCLUDE_README` e `ACME_CODE_KNOWLEDGE__EXTENSIONS`.
As variáveis usam `__` entre partes da chave; não há alias para nomes Go.
Valores solicitados não UTF-8 falham em `capture_environment` antes de Knowledge.
Nomes nativos Unix bytes e Windows UTF-16, incluindo matching de extensão de
filename observado, pertencem ao produtor físico. Esta view não os converte
com `to_string_lossy` nem confere identidade ao converter strings.

## Limites, accounting e cancelamento

`KnowledgeSettingsLimits` recebe sete `NonZeroUsize`, sem `Default`. O chamador
escolhe caps explícitos. `LimitExceeded` usa exatamente os labels desta tabela:

| Campo / label | Domínio contabilizado |
|---|---|
| `max_input_bytes` | Soma checked das strings vencedoras escolhidas, incluindo docs, CSV e booleano se string, com whitespace raw. O override substitui docs inferior; ausência/defaults não debita entrada. |
| `max_docs_dir_bytes` | Bytes raw do docs escolhido antes de trim/cópia. |
| `max_path_components` | Todos os componentes slash antes de validá-los, inclusive vazios malformados; `.` conta um. |
| `max_extension_tokens` | Todos os tokens CSV raw antes de validar/dedup; defaults contam dezesseis. |
| `max_extensions` | Quantidade de sufixos únicos normalizados. |
| `max_extension_bytes` | Bytes de cada candidato normalizado, incluindo dot, antes de dedup. |
| `max_projection_bytes` | Soma dos textos finais owned: docs e cada extensão única, antes de crescimento/cópia. |

`SettingsStats` contém cinco campos públicos `usize`: `input_bytes`,
`path_components`, `extension_tokens`, `extensions`, `projection_bytes`.
`stats()` retorna uma cópia de dados, sem capacidade de acesso. Para snapshot
vazio, a saída tem input 0, um componente, dezesseis tokens/extensões e 88 bytes
de projeção. Para docs raw ` docs/sub `, booleano Environment ` false ` e CSV
` .MD,md,,.JSON `, os valores são input 32, componentes 2, tokens 4, extensões 2,
projeção 16; os caps individuais raw docs e maior sufixo são 10 e 5 bytes.

Preflight de componentes/tokens e comparação de dedup usam empréstimos. Cópias
próprias `String` e crescimento `Vec` usam reservas fallíveis após admissão,
com checked addition. A metadata Vec é limitada pela quantidade de extensões;
capacidades do allocator, overhead e RSS não são esses contadores. Caps do
ConfigSnapshot permanecem um domínio Core separado. Uma string booleana
inválida oversized pode produzir erro de cap antes do erro de tipo.

Durante a resolução, `context.check()` ocorre no início, antes/depois de getters
do Config e trim, entre componentes, tokens, caracteres de validação/normalização,
comparações/cópias e antes do resultado. O contexto original conserva prazo e
cancelamento. Chamadas síncronas Core/trim não são preemptadas; os limites de
entrada restringem seu tamanho. A resolução não realiza I/O. Os getters da view
imutável resultante não recebem contexto nem concedem autoridade adicional.

## Erros e recuperação

`KnowledgeSettingsError` contém `Context(io::Error)`,
`InvalidValue { field, origin, reason, source }`, `LimitExceeded(&'static str)`
ou `Allocation(TryReserveError)`. `SettingsProblem` distingue `ExpectedText`,
`ExpectedBoolean`, `Empty`, `InvalidPath` e `InvalidExtension`.

| `kind()` | Condição e ação |
|---|---|
| `InvalidData` | Valor inválido vencedor Project/Global; corrigir essa configuração. |
| `InvalidInput` | Valor inválido Command/Environment/Distribution ou index_path; corrigir a entrada vencedora. |
| `FileTooLarge` | Cap indicado pelo label foi excedido; ajustar critérios/caps explicitamente dentro da política do chamador. |
| `Interrupted` / `TimedOut` | Cancelamento ou prazo original; não reiniciar internamente a request. |
| `OutOfMemory` | Reserva própria fallível falhou; nenhum resultado parcial foi publicado. |

`Display` usa mensagens de campo/reason estáticas e não inclui caminho, CSV ou
texto de ambiente. `Error::source` preserva o erro sanitizado Core, contexto ou
allocation. Campos inválidos Default usam `InvalidInput` defensivamente; um
default oversized produz cap, sem trocar de lista.

## Fronteiras e qualificação

CoreDocumentSelection recebe critérios de configuração resolvidos e continua
responsável por acesso real, ignores/control I/O, nomes nativos, symlinks,
dedup de fontes, captura, binding/revalidação e budgets. A view não implementa
indexação, lifecycle, Records, Auth ou Data. O parser documental e esses
produtores têm entregas próprias; esta folha não afirma cutover do indexador.

Os testes inline cobrem K01–K10 da Task: ausência/defaults, cinco layers,
origins/types/invalid winner, override/whole view, paths/CSV/Unicode, sete caps
exact/one-under, raw boolean accounting, checked overflow, cancel/expired e
contexto original Core, Brand/env e a fronteira UTF-8 Unix. Os 17 testes passaram
no Linux, dos quais um é `cfg(unix)`; um doctest pertinente e os dois testes
de integração pública também passaram. Eles verificam uma view que sobrevive à vida
dos produtores Config usando o contexto original, erros públicos sanitizados e a
precedência de cancelamento original sobre valores inválidos. Inspeção de
reservas fallíveis não afirma que OutOfMemory ocorreu; testes de contexto antes
da resolução não afirmam cancelamento inflight por corrida artificial.

Comandos executados na janela exclusiva do principal, com jobs 2, incremental
desabilitado e dois test threads:

```text
cargo test --locked -p graphit-knowledge --lib settings::tests -- --test-threads=2
cargo test --locked -p graphit-knowledge --test settings -- --test-threads=2
cargo test --locked -p graphit-knowledge --doc settings::KnowledgeSettings::resolve -- --test-threads=2
cargo clippy --locked -p graphit-knowledge --all-targets -- -D warnings
```

O filtro de unitários foi separado do target de integração para executar ambos;
não houve replay de References/Staleness ou gate amplo. Logs, time/RSS, hashes
antes/depois e a evidência de qualificação (`../evidence/knowledge-729f/README.md`; historical local artifact unavailable)
registram as quatro invocações, sem alteração da fonte durante a execução.
Os processos foram aguardados e a CPU devolvida ao principal. O guard Settings
foi qualificado pelo principal em `tsk-c775` r14, label `graphit-knowledge/settings`:
`broken_fast_contract` falhou com panic/sentinel na cópia isolada do target
público, a fase Cargo e o Git precommit retornaram erro, e o mesmo gate positivo
passou após restauração. O recibo de aceite (`../evidence/knowledge-729f/guard-acceptance.json`; historical local artifact unavailable)
registra hashes e escopo. O guard staleness de `tsk-b96c` não foi reutilizado
para Settings; o parser ainda requer sua própria ativação e prova. Windows e
macOS não foram executados nesta qualificação; nenhum ganho de performance ou
cutover do indexador é afirmado por esses resultados.

## Adaptação privada Core65

A qualificação privada Core65 de tsk-87f7 passou no Linux: 151 testes
funcionais/unitários, sete doctests, Clippy estrito nos targets afetados e
formatação dos 18 arquivos Rust. O patch aguarda composição serial pelo principal
com o produtor Core. As qualificações Main anteriores permanecem históricas.
