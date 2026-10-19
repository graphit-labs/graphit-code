---
type: architecture
title: Publicação autenticada da exportação textual Knowledge
description: Perfis de texto para PROJECT e Knowledge instalado em diretório externo ausente, com autoridade real e publicação conhecida.
references:
  - target: {type: task, id: tsk-cab0}
    relation: implements
  - target: {type: task, id: tsk-9d92}
    relation: implements
  - target: {type: task, id: tsk-b324}
    relation: implements
  - target: {type: task, id: tsk-bd73}
    relation: derived_from
  - target: {type: task, id: tsk-cd46}
    relation: uses
  - target: {type: task, id: tsk-1ed0}
    relation: supports
  - target: {type: task, id: tsk-2706}
    relation: supports
  - target: {type: task, id: tsk-72ce}
    relation: supports
  - target: {type: task, id: tsk-1f4a}
    relation: supports
  - target: {type: task, id: tsk-7dd5}
    relation: uses
---

# Publicação autenticada da exportação textual Knowledge

Integradores de `graphit_app::knowledge_export` exportam uma captura Source Knowledge
completa para um diretório externo escolhido pelo caller. A tool
`graphit_knowledge_export` seleciona `format: native|markdown|okf|obsidian`; `native` é o padrão.
A CLI oferece `knowledge export --format markdown|okf|obsidian --output PATH`.
A qualificação Main é registrada separadamente da biblioteca. A seleção padrão
é PROJECT; para o artefato instalado a mesma tool recebe
`source: { kind: "imported_knowledge", artifact: { id, version, context_alias? } }`,
sem `project_dir` ou `project_version`. Os seletores não concedem acesso.
A [documentação CLI](../specs/rust_foreground_knowledge.md) descreve a entrada Main.

## Destino e autoridade

O contrato **FreshDestination** exige caminho absoluto, parent existente e destino
ausente. O destino pode estar fora do PROJECT ou da instalação. O App valida o
parent real com `graphit_fs::read_directory::open_root_readonly`, prepara arquivos
privados nesse mesmo parent e publica com `graphit_fs::rename_exclusive`.
A renomeação exclusiva conserva qualquer arquivo, diretório ou symlink já existente;
`AlreadyExists` é uma causa IO retornada, sem substituição ou atualização automática.
O App não cria parents e não altera os `allowed_roots` de scopes Core. Antes do
primeiro staging, `validate_export_destination` verifica o destino contra o pin
Source real no PROJECT ou contra o payload instalado inteiro no importado.
Uma saída contida nesses dados é rejeitada sem criar arquivos dentro deles.

São exigidos `RoleIssuer`, caller autenticado, `ScopeInput` PROJECT e
`RequestContext` original. A admissão real PROJECT ReadKnowledge valida issuer,
caller, request, scope e lineage. Separadamente, o caller deve ter
`External(PublishedFilesystem)` com obrigação `Unconditional`.
Destino, fingerprint e metadata não concedem autoridade. O exportador não exige
MutateKnowledge e não fabrica `ProjectReadAdmission`.

## API e leitura

`export_text_authenticated` recebe issuer, caller, scope, Original,
`KnowledgeTextExportRequest { destination: &Path, format: KnowledgeTextExportFormat }` e
`&KnowledgeTextExportPolicy`. A policy fornece `index`, `files`, `text`,
`modes` e `payload`, consumindo os tipos reais dos produtores. Não há defaults
ocultos ou key de artefato gerenciado para esta operação.

O perfil owned vem de `graphit_knowledge::okf_export::KnowledgeTextExportFormat`.
A tool constrói Markdown, OKF ou Obsidian com a marca da instalação; a entrada
não aceita uma marca fornecida pelo usuário. O perfil é clonado apenas ao criar
o estágio lazy, sem cópia por documento ou lote. O SDK conserva o mesmo spool,
EOF, plano físico e publisher para os três formatos. OKF/Obsidian usam
frontmatter 0.2 real e a extensão nativa `graphit`, sem inventar datas.

O App abre uma única `capture_native_knowledge_search` Source-only, sem NN.
`next_documents` lê lotes owned FULL18 da mesma captura com Original e admissão.
Cada documento é consumido por
[`MarkdownExportStage::push`](rust-knowledge-markdown-export.md) e seu corpo é
liberado após gravar o spool. `Some([])` não significa EOF; apenas `None`
completa a leitura validada. Falhas, corrupção e leitura incompleta não publicam.

O estágio é criado no primeiro documento com `MarkdownExportStage::new_in` no
parent validado. Uma captura ausente retorna `NoCorpus`; uma geração publicada
sem documentos ativos retorna `PublishedEmpty(generation)`. Ambos conservam um
destino anterior e não criam staging. O SDK `finish` de estágio vazio continua
retornando `NoDocuments`.

## Knowledge importado

`export_imported_text_authenticated` recebe issuer, caller, Original, o mesmo
request/policy e `ImportedKnowledgeTextSource { installation, input, directory }`.
`input` contém o registry e a seleção exata de id, versão e alias opcional.
O App exige que o caller pertença ao Original recebido, cria
`ImportedKnowledgeReadAdmission` real e abre a seleção com `open_imported_knowledge`.
Não cria nem aceita PROJECT para esse fluxo. Ausência no registry retorna
`RegistryAbsent`; dataset ausente, binding inválido ou corrupção continuam erros.

`reader.capture_source(policy.index)` conserva o mesmo ModuleSnapshot por Arc,
sem clone físico de dataset, abertura de NN ou leitura de Pair. O cursor entrega
lotes owned até `None`; os guards revalidam instalação, admissão, registry e
capturas físicas. A geração vem de `source_generation()`, e não da versão do
artefato Hub. Após EOF o mesmo estágio e publisher concreto publicam o output
externo. `Imported(ImportedKnowledgeSourceError)` conserva causas de leitura e
revalidação da seleção real.

## Publicação e efeitos conhecidos

Depois de EOF, `finish` completa índice/navegação e captura um
`SourceSnapshot` file-backed da saída gerada, sem quota agregada de corpus. O plano inteiro
retém seu TempDir; não se reúne um vetor com corpos ou bytes do corpus.
O publisher concreto `publish_text_plan` revalida o guard real, executa
`plan.payload.revalidate_content(Original)`, revalida o guard e faz a renomeação
exclusiva. A captura Source e o descriptor do parent permanecem vivos até o fim.
Os pins reais de leitura/transporte Source ou NN pertencem aos seus produtores;
esta exportação não cria payload pin, journal ou recovery de ArtifactStore.

O sucesso da renomeação é registrado antes de qualquer trabalho final falível.
`Published` contém `output`, geração observada, fingerprint, comunidades owned e
`cleanup`. O guard é revalidado após publicar e depois de `plan.close()`, que relata a
limpeza real do spool. Um erro de cleanup conserva o resultado publicado conhecido.
A geração descreve a captura lida, sem promessa de latest global; `corpus_uri`
não integra os arquivos exportados.

`Publication { cause, output, cleanup }` preserva a causa nativa de uma tentativa
de renomeação que retornou erro. Um IO genérico não é convertido em alegação de
rollback ou ausência de efeitos. `AfterPublication` preserva output, geração,
fingerprint, causa do guard final e resultado de cleanup quando a renomeação já
terminou com sucesso. Os demais erros preservam as causas de Original, Auth,
Source, parser, captura física ou validação de input. Abandonar uma preparação
que ainda não chegou ao publisher usa a limpeza best effort do TempDir.

A operação fornece visibilidade atômica de uma árvore completa pelo rename
exclusivo do filesystem. Não promete journal, durabilidade após queda de energia,
substituição de diretório existente ou proteção contra mutação hostil concorrente
por outro processo com acesso ao filesystem. IO nativo síncrono não é preemptível.
Original e permissões são revalidados nos limites observáveis.

## Evidência e relações

`crates/graphit-app/tests/knowledge_export.rs` compõe Auth, publicação Source e
exportador reais. Cobre FULL18, metadata integral, body literal final, modos,
fingerprint, comunidades da geração lida, destino externo, ausência/vazio,
permissões, Original, cancelamento e revogação Runtime, limites por arquivo, destino ocupado e parent
inválido/ausente. Os testes de pacote Native no mesmo arquivo conservam os pins
reais e o par Source/NN; não são mecanismos de escrita do exportador Markdown.

A qualificação Linux deste consumer cobre nove funções Markdown por delta:
sete casos passaram na execução inicial e os dois casos corrigidos passaram em
focais independentes. O SDK passou seus oito testes de integração e o lint
pertinente; a compilação App inclui os adapters PROJECT/importado e Source.
Isso não é evidência de runtime importado ou da CLI/MCP Main, que pertencem
à qualificação integrada. O receipt local em
`.build/rust-knowledge-markdown-streaming/validation-current.json` separa
compilação/fila, runtime, causas e resoluções.

As evidências anteriores de publicação managed qualificam apenas aquele caminho
histórico. A exportação externa exige sua própria execução focal e o oráculo Main
com output fora do projeto. Veja também a
[publicação nativa Knowledge](rust-knowledge-native-publication.md) e o
[contrato filesystem](../guides/filesystem_contract.md).

## Perfis OKF e Obsidian no produto

O novo focal App compõe publicação Source e autorização reais para os dois
perfis: passou uma função em 3,40 s. O focal separado do executável passou uma
função em 17,04 s, com watcher, pacote Native, importação física e registro
LocalLink reais. Ele exporta PROJECT e o artefato instalado exato em ambos os
perfis, compara árvores/fingerprint/comunidades e verifica metadata 0.2, marca da
instalação e corpos com fence/HTML abertos como sufixo final.

A proveniência `source_path` é comparada literalmente com `focused_source`
do MCP na mesma geração; o digest de compilação também é comparado aos bytes
observados. O produtor FileScan publica uma chave gerenciada como
`knowledge-source/t/646f6373/622e6d64`, que não é o caminho `docs/b.md` do
checkout. O exportador não decodifica essa chave nem consulta o arquivo original.
As duas falhas do oráculo anterior e sua correção ficam no receipt, sem alteração
do produtor ou do contrato de Source.

A execução posterior dos mesmos dois casos, após a normalização de referências
locais no SDK, passou App em 3,59 s e Main em 19,71 s. OKF conserva o corpo
literal; Obsidian reescreve wikilinks locais observados para o arquivo exportado
real, incluindo `.md`, âncora e display implícito original ou label explícito.
As fixtures incluem referências percent-encoded a slugs existentes. Os destinos
vêm do índice público e são conferidos como arquivos reais; o oráculo não
reimplementa hash ou parser. Links sem alvo observado e os contidos nos blocos
abertos de código/HTML continuam literais. PROJECT e instalado mantêm igualdade
de arquivos, fingerprint, comunidades e proveniência. Isso qualifica os casos
exercitados, sem promessa de conformidade universal de um host Obsidian.

## Composição portátil de Knowledge

Os módulos ativos de publicação, reconciliação documental, materialização de
branch, leitura de Knowledge instalado e busca são selecionados para Linux,
macOS e Windows. Os mesmos produtores atendem o wrapper `foreground_knowledge`
do App, incluindo `Query::Search`, perguntas e causas de erro semântico. A seleção
por plataforma conserva Original, admissões, geração observada, limites e causas
nativas dos produtores; não cria um caminho alternativo de armazenamento.

A policy textual usa `PayloadMode::private_directory()` e `private_file()` do Core:
0700/0600 em Unix e atributo gravável em Windows. Esses valores descrevem os
atributos desejados do payload. A criação exclusiva e o acesso privado continuam
responsabilidade dos produtores FS reais, incluindo a DACL em Windows; construir
um modo não concede acesso nem comprova a privacidade de um arquivo.

Essa composição substitui a criação comum de `records`, `payload`, `pages` e
arquivos do estágio por `graphit_fs::create_private_directory` e
`create_private_file`. A privacidade é estabelecida antes dos bytes documentais;
o diretório externo de `TempDir` conserva somente os nomes desses filhos privados.
A implementação anterior herdava o acesso do parent e os modos de payload não
comprovavam uma DACL privada em Windows. O teste
`markdown_export::tests::staging_is_private_before_document_bytes` inspeciona os
diretórios privados antes do primeiro documento e o arquivo real do spool depois
da gravação. Sua execução, assim como a dos focais do workspace, é registrada no
receipt sem presumir qualificação nativa dos demais hosts.

O coordenador relatou PASS desse focal SDK no Linux: um teste em 0,013178 s.
A emissão compartilhada Now02 compilou App, Daemon e Knowledge com os assets
finais em 211,075630 s, contabilizados separadamente da execução. O filtro App
`knowledge_live::workspace::identity_tests` passou seus três testes no Linux em
0,017831 s, conforme execução Runtime relatada pelo coordenador. Esses quatro
testes qualificam somente os focais descritos: a integração do daemon, incluindo
Native Now, tem evidência separada. Windows e macOS não foram executados
nativamente nessa evidência.

O workspace mutável de Knowledge Live retém o `File` de nascimento de cada
diretório. Antes de usar um caminho, compara a identidade desse handle com a
observação física portátil do Core e verifica o acesso privado pelo FS. Symlinks,
substituições, permissões inadequadas e Original cancelado continuam rejeitados.
Ao registrar o workspace, `CapturedDocumentDirectory::verify_private_handle`
verifica o handle retido contra a captura real. O registro serializado continua
sendo dado de seleção, sem conceder autoridade para restaurar ou remover recursos.

A seleção de produtores por plataforma não comprova execução nativa nesses hosts.
