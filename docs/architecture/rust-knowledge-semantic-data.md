---
references:
  - target: {type: task, id: tsk-cab0}
    relation: supports
  - target: {type: task, id: tsk-69f2}
    relation: refined_by
  - target: {type: task, id: tsk-ac1e}
    relation: implements
  - target: {type: task, id: tsk-598c}
    relation: uses
  - target: {type: task, id: tsk-9d92}
    relation: consumer_boundary
  - target: {type: task, id: tsk-0d3e}
    relation: implements
  - target: {type: task, id: tsk-5612}
    relation: supports
  - target: {type: task, id: tsk-1597}
    relation: implements
  - target: {type: task, id: tsk-446e}
    relation: specified_by
  - target: {type: task, id: tsk-72ce}
    relation: uses
  - target: {type: task, id: tsk-8a17}
    relation: uses
  - target: {type: task, id: tsk-4fde}
    relation: uses
  - target: {type: task, id: tsk-879d}
    relation: supports
  - target: {type: task, id: tsk-185c}
    relation: refined_by
  - target: {type: task, id: tsk-86ec}
    relation: refined_by
---

# Dados semânticos Knowledge em Rust

Este contrato descreve a publicação, consulta e transporte de Knowledge pelo App,
Knowledge e Data. A fonte autoral continua no checkout. Core seleciona o corpus
local por PROJECT ID estável e módulo. Todas as versões usam o mesmo dataset
`global/projects/<PROJECT_ID>/knowledge/index.lance`; a versão seleciona a branch
nativa Lance, sem diretórios ou mapas alternativos por versão.

`local_corpus::resolve` valida o caminho canônico sem criar estado. Knowledge
conserva a URI e o snapshot/branch durante a operação: leitura, guards, CAS,
primeira publicação, NN, histórico e retirement usam esse mesmo dataset. Branches
compartilham fragmentos até as alterações, preservando vetores compatíveis.

Se outro writer publicar primeiro enquanto um FILE é preparado, uma colisão
`InitialTargetChanged` confirmada como `BeforeCommit` reabre a branch autorizada
e aplica o FILE pelo caminho incremental e seu CAS. Um FILE concorrente com
fingerprint de compilação diferente continua sendo conflito; uma captura vazia
obsoleta não substitui um corpus publicado. Origens herdadas ausentes na seleção
são removidas pela captura Source e pelo CAS da branch, preservando o default.
As datas do head reaberto são preservadas no FILE corrente e nos seguintes.
Falhas com commit incerto ou já observado conservam o fluxo de recuperação
existente, sem repetir a criação.

## Um corpus, disponibilidade progressiva

O documento é publicado antes de resolver um modelo. Seu schema documental tem
18 campos: os 16 campos públicos, `document_state` e `reference_projection`.
Source, FTS, referências, lint, datas e exportação textual continuam disponíveis
sem vetores. Não há um segundo dataset de embeddings nem uma segunda fila de
publicação documental.

O primeiro lote vetorial válido informa a dimensão real. Sob a admissão write,
writer do PROJECT e CAS da branch selecionada, Data usa
`BranchSnapshot::add_nullable_columns` para acrescentar cinco campos nullable:

| Campo | Tipo |
| --- | --- |
| `input_fingerprint` | FixedSizeBinary(32) |
| `vector_id` | FixedSizeBinary(32) |
| `projection_recipe` | Utf8 |
| `role` | Utf8, valor Document quando há vetor |
| `vector` | FixedSizeList(Float32, dimensão real) |

Lance sintetiza NULL nos registros existentes sem reconstruir o corpus textual.
Uma largura inventada ou um List<Float32> variável não representa esse contrato.
Sem vetor, a linha permanece pendente. Com vetor, identidade e fingerprint devem
estar presentes, a largura deve ser exata e todos os componentes devem ser
presentes e finitos. Metadata remanescente após retirar uma coluna vetorial não
certifica um vetor nem prontidão.

O schema admite somente a forma documental, a extensão de identidade durante
uma troca de dimensão e a extensão completa com vetor. Colunas ou metadata de
schema desconhecidos são erros. A implementação fica em
`crates/graphit-data/src/knowledge_index/semantic.rs`.

## Identidades e reaproveitamento

O digest físico identifica os bytes do arquivo autoral. O fingerprint de
compilação identifica a compilação e seus parâmetros. O input_fingerprint
identifica o texto realmente entregue a Models; nenhum deles concede acesso.

A ABI `knowledge-document-json-v1` contém JSON compacto, na ordem
recipe/title/kind/description/summary, seguido por um LF e pelo body literal
integral. Ausência e string vazia são distintas. O hash cobre recipe, papel
Document, vector_id portátil, dimensão e esse texto. ConfigId operacional,
processo, checkout, caminho e slug não entram nesse hash.

O reaproveitamento exige o mesmo slug e fingerprint, recipe, papel, vector_id e
dimensão certificados. Mesmo nome de modelo ou mesma dimensão não bastam.
Writers documentais preservam o vetor somente quando title, kind, description,
summary e body permanecem iguais. Mudanças de datas, referências, digest físico
ou path não alteram por si o texto do modelo. Mudanças desse texto e remoções
limpam os campos semânticos na própria transação documental; não há uma limpeza
posterior em outro dataset. A identidade documental continua obedecendo às
regras reais de slug e de origem.

Links Markdown relativos terminados em `.md` são resolvidos no produtor contra
o diretório autoral da fonte. A origem canônica usa `KnowledgeSourceIdentity`;
um destino capturado com `id` autoral conserva esse ID. `.` e `..` são
normalizados sem escapar do PROJECT, e escapes percentuais são decodificados
antes da identidade. Query e fragmento não integram o ID do documento.
`[[wikiID]]` continua sendo um ID literal; URLs, caminhos absolutos e assets não
viram páginas locais. Âncoras isoladas continuam apontando para a própria página.

O fluxo Source mantém um lookup de identidades somente durante a passagem.
Reutiliza metadata publicada quando o digest físico coincide; para destinos
novos ou modificados, inspeciona um prefixo limitado até o fim do frontmatter,
sob a captura Core selecionada, os ignores e os limites existentes. A leitura
não concede acesso pelo link.
Destinos ausentes conservam uma referência canônica pendente. Um digest da fonte
inalterado não certifica a resolução: a publicação compara a imagem documental
e sua projeção. Corrigir apenas referências preserva o texto, o fingerprint
semântico e os vetores compatíveis. Projeções ausentes continuam ausentes;
resolver links não fabrica autoria nem declara cobertura completa.

## Captura, lotes e CAS

`capture_knowledge_embedding_corpus` e
`capture_knowledge_embedding_selection` retêm o pin documental real. A visão
semântica, quando existe, aponta para o mesmo URI, branch e versão. O cursor
privado entrega documentos em lotes e conserva EOF, poison após falha/drop,
limites e identidade do Original. Some(empty) não significa EOF.

`publish_knowledge_embeddings_stream` recebe lotes Arrow ordinários com slug,
fingerprint, identidade e vetor. Esse formato de entrada não é outro corpus e
não concede autoridade. Data associa cada vetor à linha documental do pin real,
valida chaves/duplicatas, identidade, largura, finitude e EOF. Os corpos ficam no
lote corrente; não há uma imagem de todos os bodies/vetores em memória.

Cada lote é publicado assim que foi validado. O produtor não mantém o writer
enquanto espera Models ou o próximo lote. Sob o writer existente, Data abre o
snapshot corrente da branch e verifica somente os documentos do lote. Se o
texto entregue ao modelo mudou ou o documento foi removido, o vetor calculado
é descartado somente para esse slug. A comparação usa title, body, kind, summary
e description; alterações em datas, referências ou outros documentos não
invalidam inferência compatível. O merge usa os campos documentais correntes,
sem restaurar metadata antiga trazida pela captura de inferência.

Não há revalidação do head global, comparação integral do corpus ou exigência
de geração comum entre Source e inferência. Snapshots nativos servem à operação
e os merges preservam o CAS do SDK. A identidade vetorial continua sendo
validada no alvo: a mudança concorrente para um espaço incompatível é erro,
mesmo quando uma linha selecionada ainda está sem vetor. Uma branch retirada
não é recriada por um lote que já estava em execução.

O cursor documental avança junto com a inferência; seu EOF é exigido ao concluir
a publicação, não antes do primeiro lote. EOF ainda valida o término do stream
e, no modo ReplaceComplete, a seleção completa solicitada. Lotes já confirmados
permanecem consultáveis se um lote
posterior falhar, for cancelado ou não completar a seleção. A falha conserva o
último efeito confirmado e sua causa; não afirma rollback dos lotes anteriores.

A primeira extensão nullable é um efeito confirmado independente do merge.
`CorpusChanged` conserva a última geração realmente confirmada pela operação,
incluindo Clone de branch, schema, merge ou índice, quando uma etapa posterior
falha. Observar uma branch criada por outro writer não prova um efeito próprio.
Trocar o espaço vetorial retira a coluna incompatível e acrescenta a largura
real; cada commit é observado e não desfaz texto, FTS ou branches irmãs.
Cancelamento posterior não pode transformar um efeito confirmado em ausência.

O merge vetorial usa o CAS e recovery existentes. Seu commit avança a versão
nativa do mesmo dataset, sem exigir alinhamento com componentes derivados
independentes. Um commit iniciado é aguardado mesmo quando chega cancelamento. `CommittedButRecheckFailed` e
`IndexFailed` conservam publicação, recovery e causa concreta.
Se a inferência falha depois de um desses efeitos, App retorna
`KnowledgeSemanticError::Publication`, com o resultado estruturado de Data e a
causa original da inferência. O chamador não precisa inferir ausência de efeitos
a partir de uma falha de Models. Erros de branch/SDK também permanecem nesse
resultado: uma falha de inferência não deve ocultar uma materialização parcial
ou um efeito cuja confirmação ainda seja incerta.

## Consulta e índice vetorial

Leitores documentais projetam os campos documentais e não dependem de Models.
Uma consulta semântica exige EOF dos CurrentInputs derivados dos documentos
atuais, identidade portátil compatível e fingerprint atual. Source/FTS podem
conter mais documentos do que NN. Sem vetor compatível, o resultado é Unavailable;
um conjunto parcial continua consultável somente nos slugs elegíveis.

Após o primeiro merge vetorial, Data cria um índice nativo Lance IVF_FLAT com
uma partição e métrica cosine. Uma partição preserva a busca no conjunto completo
sem quantização. O produtor `prepare_vector_index` não refaz um índice existente
com nome/campo/métrica compatíveis; fragmentos acrescentados conservam a
cobertura nativa de Lance. Query solicita o índice SDK e a métrica pedida, sem
implementar um mecanismo de busca paralelo. Erro de treinamento/índice conserva
o merge já confirmado e sua causa. Uma publicação posterior com todos os vetores
reaproveitados ainda pode preparar o índice ausente, sem reinferir os documentos.
Se o índice herdado já existe e os vetores são iguais, Unchanged não materializa
uma branch nem grava uma versão.

Na manutenção de postings FTS, o SDK pode finalizar o segmento mantendo o UUID
de staging; a limpeza preserva esse UUID retornado e os arquivos já publicados
que continuam referenciados por snapshots.

Os guards Auth, mesmo Original, limites, manifest físico e namespaces/witnesses
valem antes/depois de awaits, em cada linha, EOF e entrega. Valores decodificados,
fingerprints e cursores são observações, nunca grants. Os readers Source e NN
continuam distintos por propósito; seu armazenamento e pin são únicos.

## Source por arquivo e lifecycle

`reconcile_knowledge_source_changes` prepara e publica um FILE por vez. O canal
de slugs avisa somente depois de um commit documental real; o consumidor NN
consulta esse corpus persistido. Arquivos cujo SHA/recipe não mudaram dispensam
parse, e embeddings compatíveis dispensam inferência. Ausências são removidas
somente após completar a seleção autoral. Falhas conservam a última publicação e
os paths não confirmados; uma falha NN não recoloca trabalho documental na fila.

Retirement para a seleção para e aguarda seus produtores antes de um único
`retire_native_knowledge_index`. Não há segunda retirada semântica. A operação
preserva irmãos/default e os fatos compartilhados de versões do Core. Diretório
ou arquivo físico estranho falha antes do efeito, e erros posteriores conservam
os resultados realmente confirmados.

## Imported, pacote e S3

O pacote Knowledge contém somente `index.lance/`. NativeRoot declara um
componente Wiki com versão, manifest_path e digest reais. O próprio corpus
contém a identidade vetorial. Export valida o pin, drena Source até EOF e clona
uma vez; não há Pair, arquivo pareado ou staging semântico duplicado.

O reader Imported conserva Registry, admissão, Original, diretório e o pin real.
Uma visão NN deve pertencer ao mesmo URI e versão do snapshot documental.
Consultas e exportação física conservam o snapshot nativo admitido, sem exigir
igualdade com o head atual. Auth, registry e integridade dos arquivos físicos
continuam sendo validados; uma alteração concreta no conteúdo capturado durante
o transporte é erro. O transporte
S3 usa a mesma versão/manifesto verificados e sessão object-store admitida,
sem montar uma cópia local para pesquisar. Hub valida o envelope/componentes;
Knowledge/Data validam schema, conteúdo, EOF e elegibilidade.

A leitura Source-only de um corpus já admitido projeta os campos documentais;
um valor vetorial inválido não impede por si a leitura Markdown. A instalação
de um pacote nativo valida o conteúdo recebido inteiro antes de registrá-lo:
`received_native` também confere a extensão NN presente, sua recipe e os
fingerprints documentais. Um pacote com NN corrompido é rejeitado nessa fronteira;
a leitura Source-only não certifica a integridade do pacote para instalação.

## Conversão operacional e verificação

A relocação dos stores existentes pertence à integração. Deve preservar cada
PROJECT ID, branch, fonte documental e vetor compatível, fazendo a associação
por slug e identidade/fingerprint; equivalência de caminho não prova conteúdo.
Um ancestral default em v1 pode conter linhas e transações SYNC funcionais.
Número de versão, schema igual ou ausência de linhas não provam equivalência
histórica: referências nativas e propriedades funcionais precisam ser preservadas.
O cursor de WikiLog vincula também um digest da URI do corpus selecionado,
sem expor esse locator; uma seleção de outra linhagem não reutiliza o cursor,
mesmo quando números de versão ou UUIDs coincidem.
As linhas mantêm suas identidades originais durante a conversão. Mudanças de
origem são reconciliadas depois pelo lifecycle Source normal, sem adotar o slug
antigo como alias da nova origem. Diferenças reais em links ou body exigem novo
fingerprint. Não há importador permanente, fallback ou leitor de formato antigo
no produto.

WikiLog também é estado funcional: os eventos vêm de
`graphit.knowledge.sync.v1` nas transações attached da branch, com versão,
timestamp e UUID reais. Copiar somente as linhas HEAD para um novo Create perde
esse histórico. Clones de snapshot também criam uma transação nova; não provam
transplante das versões anteriores. A integração deve conservar os manifests e
transações relevantes e conferir os eventos antes/depois, sem reapresentá-los
como commits novos. Uma evolução nullable/merge sobre o mesmo dataset preserva
essa história; branches independentes exigem uma receita verificada antes da
conversão. Cleanup não deve remover manifests usados por WikiLog ou por pins
ainda necessários.

As regressões afetadas cobrem FTS antes de Models, evolução nullable SDK real,
NN indexado, preservação/invalidação em edição, NULL/NaN/limites/cancelamento,
CAS/EOF, incremental por FILE, Imported/S3, exportação única e retirement.
Fixtures sem inferência qualificam essas fronteiras; a integração instalada e
qualificação de outros sistemas operacionais continuam separadas.

Fontes principais: `knowledge_index`, `knowledge_embeddings`,
`knowledge_document_lifecycle/reconciliation`, `knowledge_document_update`,
`graphit-knowledge/src/embedding_projection.rs`, `knowledge_semantic`,
`knowledge_package_export`, `received_native` e readers Imported/remotos do App.
