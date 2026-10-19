---
title: Leitura selecionada Knowledge de um snapshot SDK
doc_type: spec
references:
  - target: {type: task, id: tsk-70cf}
    relation: implements
  - target: {type: task, id: tsk-b86e}
    relation: derived_from
  - target: {type: task, id: tsk-c8e9}
    relation: relates_to
  - target: {type: task, id: tsk-8c07}
    relation: uses
  - target: {type: task, id: tsk-185c}
    relation: refined_by
  - target: {type: task, id: tsk-cab0}
    relation: refined_by
---

# Leitura selecionada Knowledge de um snapshot SDK

`graphit_data::knowledge_snapshot::read_knowledge_snapshot(original, snapshot, slug, limits)`
recebe o `ModuleSnapshot` já aberto pelo caller e o `Arc<SnapshotContext>`
original. O pin SDK mantém a versão observada; o caller conserva essa versão
para proveniência. A função não abre paths, não escolhe uma versão mais recente,
não escreve ou materializa branches e não concede autoridade. ReadKnowledge,
Hub/Broker e rechecks de captura/delivery pertencem ao caller Native real.

O schema Arrow completo precisa corresponder a `physical_schema()` nativo fechado de 18 campos, mantendo os 16 campos do DTO público.
O scanner normal filtra a identidade `slug` no pin fornecido. O decoder estrito
valida os registros selecionados até EOF, incluindo estado, witness, metadata,
campos obrigatórios, receitas e namespace. Rows Removed selecionadas são
validadas antes de excluir. O get não carrega nem valida corpos de outras
páginas; essa regra supersede a antiga falha por corrupção/duplicidade alheia.
Duplicidade do slug selecionado e erros posteriores no seu stream continuam
impedindo a entrega de uma página inicial. A leitura integral
`read_native_knowledge_index` conserva seu propósito de enumeração/exportação.

`KnowledgeSnapshotPage` distingue:

- `PublishedEmpty`: snapshot válido publicado sem linhas, observado até EOF.
- `PageAbsent`: snapshot válido não vazio, sem o slug exato solicitado após EOF.
- `Found { document, focused_source }`: documento integral e proveniência extraídos do mesmo registro selecionado; namespace/witness ausentes continuam ausentes.

`NoCorpus` pertence à abertura/captura do caller, não a este leitor de um pin
existente. Erros de schema, dados, SDK, cancelamento e deadline nunca viram ausência.

`KnowledgeIndexReadLimits.batch_rows` configura os batches SDK e conserva
o check físico de suas linhas no collector. Content contém somente
`max_identifier_bytes`, aplicado ao slug solicitado e às identidades
validadas. Não há quota de páginas, referências cruas ou texto por documento
ou corpus. A identidade selecionada continua sujeita a duplicidade até EOF.
O leitor não usa `native`: abertura do módulo pertence ao caller e não
captura outro diretório. O SDK pode materializar seu batch internamente;
a API não promete RSS ou preempção interna. O Original é checado antes/depois
das chamadas async, nas fronteiras das linhas e antes da entrega.

A integração Native instalada continua sendo uma entrega própria. Os testes
`knowledge_module_snapshot` usam Lance local real, duas versões, EOF completo,
página ausente/vazio, dados inválidos tardios, duplicatas, limites por documento,
integridade selecionada e independência de registros alheios, cancelamento/deadline e causas SDK. Na qualificação histórica do reader anterior, os cinco casos
SDK passaram; a regressão `knowledge_index::tests::exact_caps` passou e o Clippy
estrito da biblioteca e desse target terminou sem warnings. Esses resultados
qualificam a API Data; ingresso App e armazenamento remoto continuam sujeitos
aos testes reais do caller.

A implementação está em `crates/graphit-data/src/knowledge_snapshot.rs`, com
extração mínima de `decode_batch_matching` em `knowledge_index.rs`. O reader
local integral `read_native_knowledge_index` chama esse decoder sem seleção
para seus consumidores integrais. O reader local selecionado compartilha
o collector e acrescenta os checks de Auth/guard de sua admissão.
A cobertura externa está em `crates/graphit-data/tests/knowledge_module_snapshot.rs`.
Os comandos/resultados da folha estão em
`.build/rust-knowledge-snapshot-70cf/qualification.json`.

A qualificação do schema/estado/projeção de c8e9 pertence ao seu `receipt` próprio; os resultados anteriores não a substituem.

## Guard externo e catálogo de um pin

`read_knowledge_snapshot_guarded(original, snapshot, slug, limits, before)`
reutiliza o collector selecionado e recebe um callback externo tipado
`FnMut() -> Result<(), KnowledgeIndexError>`. Original e callback são checados
antes/depois de awaits SDK, por linha, em EOF e antes da entrega. O entrypoint de snapshot sem guard externo ainda checa o Original; os callers
com autorização/capturas externas fornecem seu callback real.

`read_knowledge_heads_snapshot_guarded(original, snapshot, limits, before)`
retorna `Vec<KnowledgeHead>` ordenado por slug UTF-8. Ele projeta somente os oito
campos necessários à metadata e ao witness; não carrega nem valida bodies.
Reutiliza o decoder estrito de heads com limite individual de slug e
batch SDK, sem contadores agregados. O mesmo validator de referências
recebe limite individual ausente, sem default máximo. Valida todas as linhas e rastreia também slugs
Removed para rejeitar duplicatas antes da exclusão. Um pin publicado fisicamente
vazio produz catálogo vazio, sem inventar NoCorpus.

Esses readers recebem um snapshot já aberto e não impõem que o namespace
persistido corresponda ao producer do artifact. O caller de
[Knowledge importada](rust_imported_knowledge.md) mantém grant Web, Registry e
capturas, guarda a causa externa concreta e revalida antes da resposta. A API
Data não fabrica autoridade nem reabre pathname.

Na folha 8c07, os oito testes SDK de ModuleSnapshot e dez de IndexPage passaram.
Os casos novos comprovam heads sem carregar bodies, preservação do pin após
commit, estado inválido tardio no contrato histórico anterior e erro externo
causal para página e heads. Uma falha externa e cancelamento real do Original
no callback terminal descartam também a saída após scan válido. Resultados e falhas anteriores estão em
`.build/rust-imported-knowledge/validation.json`.

O get preserva schema físico FULL18, EOF selecionado, witnesses, namespace/estado, causas reais e limites individuais de identificadores. Um probe de metadata de no máximo uma identidade distingue stream selecionado vazio de corpus fisicamente vazio, sem outro body ou novo pin. Heads continuam uma enumeração integral de metadata.
