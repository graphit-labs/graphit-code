---
title: Publicação Knowledge no ForeGround Rust
type: architecture
references:
  - target: {type: task, id: tsk-cab0, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: uses
  - target: {type: task, id: tsk-69f2}
    relation: implements
  - target: {type: task, id: tsk-6ded}
    relation: uses
  - target: {type: task, id: tsk-fa7b}
    relation: uses
  - target: {type: task, id: tsk-9d92}
    relation: consumer_boundary
  - target: {type: task, id: tsk-8b30}
    relation: uses
  - target: {type: task, id: tsk-2297}
    relation: uses
---

# Knowledge no Foreground Rust

`ForegroundKnowledgeSources` compõe Source e inferência concorrentemente.
A seleção estrutural é revisada no primeiro evento, rescan ou alteração de
controles; deltas processam os caminhos documentais afetados. Eventos de código
fora da seleção não repetem parsing documental.

Source usa admissões Read/Write Knowledge e Original de lifetime próprios.
Captura metadata Core e publica cada arquivo por
`reconcile_knowledge_source_changes`. Cada commit libera leitura, referências e
FTS. Datas são preservadas por slug. Branch herdada é materializada com o produtor
existente e geração capturada, sem trocar para main ou tratar herança como ausência.

Um canal transporta somente slugs já confirmados. O background usa outra captura
autorizada, com seu próprio Original e token de cancelamento. Só consulta modelo
após receber trabalho. Embedding desabilitado e recursos indisponíveis são estados
explícitos; nenhuma chamada remota ou carga local é exigida para Source/FTS.
A fila desses slugs é `unbounded`; nomes aguardando processamento e o conjunto
de slugs concluídos podem crescer durante o job. Essa fila é distinta do canal
de capacidade um que transporta batches NN dentro da operação semântica.

O consumidor `index_knowledge_semantics_selection` captura somente os documentos
solicitados, reutiliza vetores de conteúdo/modelo/receita compatíveis e confirma
cada lote antes de continuar. Hoje um arquivo gera um documento e um vetor.
Troca incompatível substitui o espaço na primeira publicação nova real; Attach
preserva lotes compatíveis. A consulta aceita resultados parciais atuais e não
mistura fingerprints ou identidades antigas.

Novo batch do watcher pode cancelar apenas o background anterior; o parser e
seus commits usam lifetime independente. Ambos os futures são aguardados.
Source consulta a fronteira de yield antes de configuração/admissão física,
preparação e descoberta, e entre heads do replay de metadata. O Original terminal
é verificado primeiro. Um FILE ou worker já admitido continua sendo aguardado;
yield não descarta sua publicação durável nem cancela o lifetime terminal.
Falha semântica conserva o recibo documental e é reportada separadamente.
Falha de Source prevalece e guarda os arquivos restantes; o próximo evento
real une esses caminhos ao delta. Se há recuperação de inferência ou troca de
modelo junto com outro delta, o produtor enfileira também os slugs já salvos
após o delta, por metadata e sem reparse. O consumidor deduplica esses nomes no
job: arquivos novos e intactos pendentes entram no mesmo espaço atual, sem
repetir a inferência dos já processados. Erros de Source permanecem prioritários;
a passagem de metadata conserva as admissões e não mascara Auth ou cancelamento. Falha de captura/finalização retoma a seleção,
pulando parsing dos intactos pela metadata. Não há polling nem autoretry global.

A remoção dos vetores de Source Removed é um produtor autorizado sem modelo.
Renames preservam slug/datas; colisões com outra origem selecionada são erros.
Cancelamento, prazo, autorização e efeitos conhecidos permanecem causais.

O teste do consumidor direto segura a primeira resposta do provedor, consulta
FTS pelo MCP real antes de liberá-la e depois verifica inferência, no-op e erro
semântico com Source preservado. Servidores de contrato são locais e não exigem
conta, plano ou executável de fornecedor. Qualificação desse teste não comprova
plataformas ou aceleração não executadas.

## Engine e retenção de memória

O corpus documental, as referências, a busca lexical e os vetores usam um único
`global/projects/<PROJECT_ID>/knowledge/index.lance`. Cada versão do projeto usa
uma branch nativa desse dataset, compartilhando fragmentos até as alterações.
Cada operação conserva sua branch/snapshot conforme o [contrato de dados](rust-knowledge-semantic-data.md).
O texto é publicado antes de
Models e a coluna vetorial nullable recebe a dimensão real por evolução SDK. A cópia importada abre um snapshot Lance local; o mount Native remoto
abre a versão Lance publicada com sua sessão de storage autorizada. Esses
caminhos Knowledge não abrem um Database Ladybug nem uma projeção Icebug.
`ast.query.buffer_pool_bytes` pertence às consultas de grafo AST: seu default
delegado ao SDK não configura cache, reserva ou teto de RSS do Knowledge.

Em `source_stream::prepare_file`, bytes, parser, compilação, mapping e projeção
vivem no worker do FILE corrente. Só o `RecordBatch` owned retorna ao publisher;
a publicação desse FILE é aguardada antes de preparar o próximo. Paths, heads,
datas e mapas de identidade permanecem pela reconciliação e crescem com a seleção,
sem conservar os corpos de todos os arquivos.

O background semântico recebe um slug por iteração e conserva as admissões e os
pin documental/vetorial único até encerrar essa operação selecionada. Esses owners saem de
escopo antes do próximo `recv`; os vetores compatíveis continuam persistidos,
disponíveis para reutilização. Isso descreve lifetimes do consumidor, não a
liberação imediata de RSS: streams, sessões e caches internos do SDK e o alocador
têm seus próprios lifetimes. A inspeção desse fluxo não atribui uma causa ao OOM.

Veja [Source por arquivo](../specs/rust_knowledge_source_stream.md),
[reconciliação](../specs/rust_knowledge_desired_reconciliation.md) e
[serviços semânticos](rust-knowledge-semantic-data.md).
