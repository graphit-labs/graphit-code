---
title: Reconciliação de documentos Knowledge em Rust
type: spec
references:
  - target: {type: task, id: tsk-e6b2}
    relation: implements
  - target: {type: task, id: tsk-69f2}
    relation: implements
  - target: {type: task, id: tsk-d523}
    relation: implements
  - target: {type: task, id: tsk-990e}
    relation: specified_by
  - target: {type: task, id: tsk-cd410}
    relation: uses
  - target: {type: task, id: tsk-c8e9}
    relation: uses
  - target: {type: task, id: tsk-f02f}
    relation: uses
  - target: {type: task, id: tsk-84d5}
    relation: uses
  - target: {type: task, id: tsk-185c}
    relation: refined_by
  - target: {type: task, id: tsk-cab0}
    relation: implements
---

# Reconciliação de documentos Knowledge em Rust

O Source publica cada arquivo preparado no índice nativo, sem aguardar o EOF do
corpus ou embeddings. O consumidor conserva o último recibo confirmado se um
arquivo posterior falhar. Authored, origens não afetadas e versões isoladas não
são substituídos por uma atualização de outro arquivo.

## APIs produtivas

```text
reconcile_native_knowledge_source_file(original, read, write, expected,
                                       source, batch, limits, body_equivalent)
remove_native_knowledge_source_origins(original, read, write, expected,
                                       source, origins, limits)
read_native_knowledge_source_origins(original, read, read_limits)
```

`body_equivalent` é uma função pura que recebe o Original e os corpos antigo e
novo efetivamente observados sob CAS; ela não concede autoridade. Consumidores
Data passam igualdade exata. Knowledge fornece sua projeção YAML de
[continuidade de guias](rust_knowledge_guide_navigation.md), mantendo os demais
guards de identidade, Active e campos editoriais antes de conservar certificados
vetoriais na mesma transação. A elegibilidade ainda exige o fingerprint da entrada
normalizada atual.

O primeiro produtor recebe uma batch FULL18 de um FILE real. O segundo recebe
somente origens explicitamente ausentes: delta não trata todos os arquivos não
recebidos como removidos. Full calcula as ausências depois de completar a
seleção. Ambos reutilizam o merge, os guards e os recibos do mesmo kernel Lance.

A leitura projeta membership, digest físico, receita, parâmetros e datas, sem
corpos. Os heads permitem pular arquivos inalterados com a mesma receita. Os
slugs geridos Removed já persistidos aparecem como `removed_slugs`; não há uma
fila adicional para retomar a limpeza dos vetores.

`Unchanged { generation }` não cria commit ou recovery key.
`Updated { generation, recovery_key }` identifica o efeito confirmado. Gerações
e keys são observações de armazenamento, nunca permissões. `NoCorpus` não cria
um índice; a [primeira publicação](rust_knowledge_source_first_publication.md)
usa outro efeito. Materialização de branch continua sendo um efeito explícito.

## Segurança, referências e concorrência

Read/Write derivam do mesmo Original e da mesma seleção PROJECT/runtime/version/
branch. O Source reader real e os owners da factory permanecem vivos até o fim.
Strings, hashes e batches comuns não restauram essa autoridade.

O corpo FULL18 é o `whole_source` original. O guard Core
`CapturedSourceReader::validate_file_bytes` confere bytes, comprimento, SHA e
bindings desse arquivo antes e depois das etapas de publicação. Ele não
recaptura nem rehasha todos os irmãos por FILE. Uma mudança independente em B
não impede o commit coerente de A. A seleção completa e suas ausências são
revalidadas pelo consumidor no EOF full; remoções usam guards de membership e
bindings. Rename pode conservar o slug somente quando a captura genuína cobre
o path da origem antiga gerida e comprova sua ausência, revalidada antes do
commit. Um FILE fora dos paths de um delta não é uma ausência, mesmo que não
apareça em `source.nodes()`. Colisões com uma origem ainda selecionada, fora do
escopo capturado ou foreign falham.

O scan e as omissões do merge são restritos a origens/slugs afetados. O corpo
antigo usado na comparação permanece local ao FILE; o kernel não retém uma
imagem textual do corpus. A preparação ocorre sem o writer PROJECT. Depois da
espera pelo writer, o HEAD deve continuar igual à geração usada na preparação.
Um `GenerationChanged` anterior ao commit permite reaplicar somente a seleção
local, após recaptura autorizada e comparação exata das 18 colunas documentais
entre a versão esperada e o HEAD atual. A consulta abrange origens ou slugs
afetados, inclusive ausências e rows Removed. URI, branch e inherited target
permanecem idênticos. Avanço só de vetores, índices ou outro FILE não invalida
essa seleção; alteração documental selecionada conserva o conflito real.
Cada reaplicação usa novamente o mesmo produtor e seu CAS, Source, Auth e EOF.
Não se repetem erros de commit, efeitos publicados, IO ou cancelamento; não há
fila, reconstrução do corpus ou rollback prometido por Drop. Commit iniciado
é aguardado. Uma operação local Unchanged recapturada informa a nova geração.
O lote adota esse recibo enquanto não tiver confirmado um efeito Source próprio.
Depois de First/Updated, qualquer noop conserva o recibo e a recovery key do
último commit Source, mesmo que NN tenha avançado independentemente. A geração
desse recibo identifica o efeito Source, sem prometer o HEAD global mais recente.

Preserve mantém a distinção authored None/Some e recalcula o witness quando o
header muda. Replace pode criar Some vazio. Remoção mantém os campos históricos,
marca Removed e limpa referências/targets ativos. Reativação não restaura
referências antigas. Foreign e arquivos intactos permanecem integralmente
preservados. Namespace, schema, Unicode, estado, EOF da seleção local e limites
concretos dos identificadores continuam validados pelo codec existente.

`GenerationChanged` conserva a observação real. `Commit` mantém a causa SDK e a
key. Uma falha após commit retorna `Published { generation, recovery_key, cause }`;
não apaga o efeito confirmado nem transforma um tail inválido em EOF saudável.

## Embeddings progressivos

A captura NN selecionada lê somente os documentos pedidos e suas rows vetoriais
necessárias. A identidade global usa schema e uma observação nativa compacta,
sem scan do corpus NN por batch. A publicação Attach confirma cada lote; se B
avançou o Source enquanto A era inferido, A só é aceito após igualdade de sua row
nos pins reais. Modificação de A continua sendo conflito.

A consulta semântica fica disponível com o primeiro subconjunto coerente. O
filtro nativo antecede o nearest e inclui apenas slugs com fingerprint e
identidade atuais. Vetores ausentes, alterados ou removidos não entram. Troca de
espaço incompatível faz o primeiro lote real sobrescrever o sidecar; os demais
usam Attach. Não há mistura de modelos nem reaproveitamento de vetores
incompatíveis.

`remove_knowledge_embedding_documents` elimina vetores de slugs comprovadamente
Removed/ausentes sem modelo, download ou inferência; recusa documentos Active.
Se a limpeza falhar após Source confirmado, o consumidor preserva esse recibo e
retoma a partir dos tombstones já existentes.

## Evidência funcional

Na emissão22 Linux, os dois focais Source passaram: publicação/substituição/
remoção explícita/CAS/rename/colisão (8,47 s) e primeiro FILE enquanto um irmão
muda, com rejeição do próprio FILE alterado (2,54 s). O focal NN de batches
passou em 4,48 s, incluindo avanço independente do Source, busca parcial, reset
de dimensões, exclusão sem modelo e rejeição de Source alterado. Os três cenários
NN afetados também passaram: partial attach (3,22 s), remoção (2,89 s) e Source
vazio (1,99 s). Compilação/espera não está incluída nesses tempos de execução.

Depois da remoção dos publishers sem consumidores, a emissão25 qualificou a
API FILE ativa com referências Preserve/Replace, witnesses, foreign, remoção e
reativação (1 PASS, 7,33 s). Os quatro testes de histórico portados para os
produtores ativos passaram em 4,85 s, mantendo cursores, named, no-op, referências
não contíguas, propriedade corrompida e efeitos confirmados diante de erro tardio.
Os seis resultados da emissão22 acima foram reutilizados.

Recibos: `knowledge-progressive-data-20261023/attempt22-*.log` no diretório de
validações da instalação, além de `attempt25-scoped-reference-states.log` e
`attempt25-first-publication-history.log`. Provas válidas dos kernels inalterados são reutilizadas;
esses resultados não qualificam plataformas não executadas, inferência de
modelos ou todo o daemon. Os consumidores produtivos são qualificados na
[composição Knowledge](rust_foreground_knowledge.md).
