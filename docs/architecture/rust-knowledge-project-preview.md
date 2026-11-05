---
type: architecture
title: Preview de arquivo PROJECT capturado em Rust
description: Composição de captura conhecida, parser e preview sob a requisição original.
references:
  - target: {type: task, id: tsk-87f7}
    relation: implements
  - target: {type: task, id: tsk-2706}
    relation: uses
  - target: {type: task, id: tsk-5530}
    relation: implements
  - target: {type: task, id: tsk-cdb6}
    relation: uses
  - target: {type: task, id: tsk-2ccd}
    relation: uses
  - target: {type: task, id: tsk-b210}
    relation: derived_from
---

# Preview de arquivo PROJECT capturado em Rust

Referência para integradores que já possuem a captura Core de um arquivo
PROJECT conhecido. Esta composição usa as APIs públicas existentes de
CapturedSourceReader, parse_document e preview_markdown, com o contexto da
mesma requisição original. Não acrescenta uma API de produção.

Estado desta folha: a Source dos três testes PP1–PP3 está ativada no workspace
compartilhado. O target inteiro passou com sete casos na qualificação conjunta
Linux, sem falhas, ignorados ou filtros; Clippy estrito e formatação também
passaram. A qualificação privada anterior permanece como evidência histórica.
O guard Git correspondente passou com os sete casos atuais e somente a
sentinela FAILED, Cargo101/Git1 e restauração positiva do catálogo Main40.
Parser e preview foram qualificados separadamente em Linux. Windows/macOS não
foram executados: os quatro casos Linux/três Windows anteriores não provam a
nova suíte de sete/seis casos em outras plataformas.

## Jornada do consumidor

1. Obter CapturedSourceReader pela captura Core autorizada do FILE conhecido.
   Conservar o mesmo Arc<SnapshotContext> original e os owners da captura.
   O path vazio usado neste harness identifica a raiz interna do FILE já
   capturado, não a raiz PROJECT ou uma nova seleção.
2. Validar reader.check_request(&request). Usar reader.retained para
   os bytes já retidos; ausência de retenção não autoriza uma leitura de fallback.
3. Chamar parse_document com &request, os bytes retidos, fallback de
   título e DocumentParseLimits explícitos. Um erro encerra essa composição.
4. Selecionar o body pelo range parsed.envelope.body na fonte parsed.source.
   Passar esse body e parsed.summary diretamente a preview_markdown, com
   &request e MarkdownPreviewLimits positivos.
5. Usar text, source, source_range e truncated do preview. Conservar a captura
   e o ParsedDocument enquanto os empréstimos dependem deles.

Fonte integral, envelope, título e resumo continuam legíveis e inalterados
depois do preview. A prévia não atualiza a fonte, publica corpus, cria modelo
Data, renova acesso ou valida frescor físico. Texto/range de preview são dados,
não comprovantes de captura ou autorização.

## Lifetimes e seleção

parsed.source empresta os bytes retidos do reader; parsed.summary é String
próprio do ParsedDocument. Um resumo com texto não whitespace vence integralmente,
mesmo que o cap editorial do body seja um scalar. O preview sem corte empresta
a String real do resumo ou o body real.

Quando o body precisa de corte, o preview possui o prefixo UTF-8 e o marker
U+2026. A API mantém o lifetime dos inputs também nesse ramo; não converter
essa observação dinâmica em um produto que possa sobreviver aos inputs.
source_range é relativo ao input selecionado e exclui o marker. O range no
body não é automaticamente um offset na fonte integral: somar o início do
envelope somente quando o consumidor já possui esse contexto e o range válido.

## Fixtures e limites fechados

Todos os limites do parser e preview vêm do consumidor, sem Default. A soma
hard de input do preview inclui body mais summary, mesmo quando o body não
é escolhido; a saída inclui os três bytes do marker. Cap editorial conta
scalars Unicode, não bytes ou graphemes.

| Caso | Inputs reais | Limites preview | Resultado esperado |
|---|---|---|---|
| PP1 | Arquivo sem frontmatter de 24 B: # H, duas quebras CRLF, prosa Markdown **ação** 🍀 e CRLF final. title H; summary ação 🍀, 11 B; body integral 0..24. | input 35 B; output 11 B; body 1 scalar | Summary integral, range 0..11, sem corte; pointer igual à String do parser. |
| PP2 | Fixture `unicode-crlf.rs` de 35 B com BOM e CRLF explícitos no literal Rust. title Café; summary vazio; body # Olá + CRLF, source 27..35, 8 B. | input 8 B; output 9 B; body 5 scalars | Body # Olá… de 9 B; prefixo 0..6 no body / 27..33 na fonte, marker 3 B; texto owned e truncated. |
| PP3 | Captura e parse válidos da fixture de 35 B; cancelar a mesma requisição somente depois do parse. | input 8 B; output 9 B; body 5 scalars | Context/Interrupted com causa original; nenhum preview nem I/O adicional. |

Em PP1, input 34 B ou output 10 B retornam LimitExceeded de input bytes ou
output bytes. Em PP2, input 7 B ou output 8 B produzem as mesmas falhas integrais.
kind é FileTooLarge e source é None nessas falhas. Não há fallback para body,
remoção de marker, corte adicional para caber ou resultado parcial.

O harness usa ScopeLimits(4,4096,65536,256) e o Arc do contexto original.
A captura admite N bytes, um arquivo e retenção N; SourceReadLimits(N,4096,N)
limita range, block e stream, onde N é 24 ou 35 conforme a fixture.
O sibling não selecionado de 4096 B excede os limites de conteúdo do FILE
se for incluído por engano. Limites de parser e retenção são exercitados
com igualdade e excesso, sem quotas globais ou admissão artificial de handles.

DocumentParseLimits do harness: document N; input 1024; frontmatter 1024;
YAML events 100; Markdown events 100; YAML depth 10; scalar bytes 1024;
links 10; projection bytes 10000; summary bytes 1024. O prazo original é
finito, 30 segundos. Esses valores são fixtures, não defaults de produto,
hard RSS ou promessa de preempção de SDK.

## Cancelamento, integridade e descarte

A captura real lê o FILE; parser e preview recebem seus bytes retidos.
Os testes conferem bytes, pointers, spans e texto após sucesso e erros de
limite/cancelamento. Source dessas funções puras não contém chamadas I/O;
essa ausência não é medida por contadores artificiais. O teste PP3 confirma
Interrupted tanto em kind quanto no io::Error da variante Context;
Error::source conserva esse erro original.

Nenhuma etapa substitui a requisição original ou renova seu prazo.
Bytes retidos e projeções permanecem intactos depois da
falha. No harness, scope/request/paths vivem antes do owner de cleanup; reader,
parser e preview são descartados antes da remoção do diretório temporário.
O cancelamento após parse protege uma fronteira cooperativa real e não
comprova interromper uma chamada individual síncrona do SDK.

## Implementação e qualificação

Extensão ativada em crates/graphit-knowledge/tests/project_document.rs,
com este documento em
docs/architecture/rust-knowledge-project-preview.md. Os quatro testes antigos
Linux/três Windows e a fixture de 35 B permanecem; só a fixture privada de
teste foi parametrizada para o literal de 24 B. Não há helper de produção,
novo target, decoder concorrente ou alteração de Core/Auth/FS/Data.

A qualificação privada executou o target inteiro, sem filtro, na janela
combinada com o principal. A qualificação conjunta LIVE manteve a mesma Source;
o guard correspondente depois exercitou os sete casos atuais. Os alvos são:

    cargo test --locked -p graphit-knowledge --test project_document
    cargo clippy --locked -p graphit-knowledge --test project_document -- -D warnings

Conferir sete casos Linux/seis Windows esperados contra nomes e contagens
reais, zero falhas/ignorados/filtrados, Source antes/depois, logs, recursos e
os filhos diretos do executor aguardados. A execução privada observou sete casos passados,
zero falhas/ignorados/filtrados, Clippy EXIT0 e rustfmt EXIT0. O workspace
real foi copiado com 613 arquivos idênticos antes/depois; o único delta foi
o teste deste target. Receipts e logs imutáveis ficam em
.build/knowledge-project-preview-stage/evidence/qualification. Os subprocessos
foram aguardados; jobs2, incremental0, threads2, debug0 e TMP em ext4 foram
registrados. Windows/macOS permanecem sem execução. rustfmt isolado verifica
somente sintaxe/formato.
O principal qualificou o guard graphit-knowledge/project_document contra
esta nova Source, com sentinela, Cargo101/Git1, restauração e integridade.
O guard histórico de quatro casos não qualifica os novos sete.

A qualificação Linux e o guard Git correspondente passaram; a prova do alvo
atual e a reconciliação dos recibos estão registradas abaixo.
A execução no Linux qualifica o runtime dessa plataforma. Ver contratos dos
[parser](rust-knowledge-document-parser.md),
[preview](rust-knowledge-markdown-preview.md) e
[Knowledge/Wiki](rust-knowledge-wiki.md).

Root ativou esta Source e sua wiring de produção no pacote acoplado de 55
arquivos. O recibo de ativação é
.build/rust-core-birth-anchor-20261009/root-qualification/coupled-activation-stage/activation-receipt.json
(SHA256 4cc61b5cf86c38a092df5a712ee86569a97d2d0b52b7654f877dca953ef74f2e).
A evidência privada acima permanece histórica. A qualificação conjunta LIVE
passou; o guard Git correspondente também passou na execução posterior abaixo.
Await dos filhos diretos
não certifica quiescência de toda a família de processos.

## Qualificação conjunta LIVE

O recibo consolidado do principal é
.build/rust-core-birth-anchor-20261009/root-qualification/coupled-live-consolidated.json
(SHA256 4eb82f1ab8af0d457ac9ac429823bba7cd21f4e19939d3aac59a7500719c83fd).
A partição Knowledge do conjunto afetado foi, em ordem, 38 unitários, sete
testes project_document e 14 testes publication_metadata, sem falhas, ignorados,
medidos ou filtros; o conjunto Linux afetado somou 149 testes:

    cargo test --offline --locked -p graphit-knowledge --lib --test project_document --test publication_metadata

Log real: coupled-live-qualification-1791633146287479781/3.log sob o mesmo
diretório root-qualification, SHA256 f4c0d8040d36318545e9abd28b1d51d7fdf999bd21e6154f164268d36b6498c0.
Clippy estrito --all-targets passou para Core, Auth, Hub, Knowledge e xtask.
O runner bca596cc… conserva a primeira falha, limitada à ordenação do wiring
no scoped fmt. O reparo mudou somente a ordem de declarações/reexports,
preservando o multiset exato das linhas, e a cauda scoped rustfmt passou sem
repetir a compilação. lib.rs Knowledge atual é
9c570f0e64e83c640a63081a94c3cf044e5c951c6bfea922b5f467af47434c30;
Main40/Lockee8c e a Source deste target permaneceram iguais. O await comprova
os filhos diretos do executor, não quiescência de toda a família de processos.
Windows/macOS, desempenho e validação ponta a ponta não são provados. Na execução
conjunta acima, o guard Git ainda era UNRUN; a execução posterior abaixo
qualifica o alvo atual preservando esses recibos históricos.


## Guard Git dos sete casos atuais qualificado

O principal executou o grupo de cinco guards do catálogo Main40. O recibo real é
.build/rust-auth-initial-login/recovery-worker-20261010/five-current-guards-1791635166341582506/receipt.json
(SHA256 211de23e81c63ae93a08522139a336d5885f9578c97e9870c68b4ad33a9c3db3).
Para graphit-knowledge/project_document, sete casos passaram e somente
broken_fast_contract falhou após a injeção: zero ignorados, medidos ou filtros,
Cargo101 e git hook run pre-commit com Git1. A seção do log tem SHA256
05ccb949eaa458dfcacc41c85ae7d4da978b5662d69b472f1fad055264df104a;
o trace SID 20261010T123319.209142Z-H690e6844-P0018bc61 liga o hook à falha.

A linha de Cargo do guard foi:

    cargo test --locked -p graphit-knowledge --lib --test project_document

O baseline atual e o catálogo completo de 40 fases restaurado passaram.
Main40 9968c434… e driver 8242a067… permaneceram iguais; todos os 948 arquivos
e 150 diretórios copiados conferem antes/depois, incluindo 45 queries YAML.
Não houve drift. O executor aguardou seu único filho direto; a inspeção final
do TMP encontrou zero entradas e o TMP privado foi removido. O log completo
tem SHA256 83c30b07cc6345e659d936fd3f6551cdaa2d09a886e9934aca885db45d1d48aa;
o trace real tem SHA256 9038bbd303233870d799cece23db0e6ddf444e7a2b68a9c5e66f9a9e96385a31.

Esta prova qualifica o guard dos novos sete casos Linux, além da qualificação
privada e da conjunta descritas acima. Preserva o limite da prova: nenhum
runtime Windows/macOS, desempenho, publicação Data, quiescência da família
inteira ou validação ponta a ponta foi certificado. O guard também não limpa metadata
de freshness do Wiki; hash da fonte não substitui avaliação autorizada da
geração documental.

## Adaptação privada Core65

A folha tsk-87f7 adapta fixtures e exemplos ao contrato Core65 SourceReady.
SnapshotContext::new(deadline, Cancellation) é infalível; SourceReadLimits
recebe somente range_bytes, block_bytes e stream_bytes. O contexto original
conserva identidade, cancelamento e prazo.

A qualificação privada Core65 de tsk-87f7 passou no Linux: 151 testes
funcionais/unitários, sete doctests, Clippy estrito nos targets afetados e
formatação dos 18 arquivos Rust. O patch aguarda composição serial pelo principal
com o produtor Core. As qualificações Main anteriores permanecem históricas.
