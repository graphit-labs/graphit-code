---
type: architecture
title: Locator local de Knowledge Native importada
description: Seleção de instalação e geração SDK para leitura readonly de Knowledge Native registrada globalmente.
references:
  - target: {type: task, id: tsk-2f62}
    relation: specified_by
  - target: {type: task, id: tsk-a456}
    relation: uses
  - target: {type: task, id: tsk-5612}
    relation: uses
  - target: {type: task, id: tsk-c8e9}
    relation: uses
  - target: {type: task, id: tsk-58db}
    relation: uses
  - target: {type: task, id: tsk-8c07}
    relation: relates_to
---

# Locator local de Knowledge Native importada

Este contrato orienta o integrador do reader Knowledge. O instalador Native
local está entregue; a leitura importada com grant Web permanece em sua folha
própria. O contrato não depende da publicação remota.

## Instalação e corpus

O [instalador Native](../development/native-packages.md) publica o payload na
raiz `SourceRef::hub_cache(...).directory()`. O Registry registra exatamente
Knowledge, ID completo e `ConcreteVersion`, com target `GlobalNative`,
backend `Native` e payload `Original { selection_binding: None }`, sem
alias ou entry name. `package.json` é envelope de transporte ZIP e não permanece
no payload instalado.

O componente Knowledge é `SourceRef.directory()/index.lance`. A admissão do
installer captura o diretório readonly e abre `LanceModuleStore::open(uri,
knowledge_index::physical_schema())`. O schema atual tem 18 campos físicos;
os DTOs documentais conservam 16, conforme o
[contrato Data](rust-knowledge-reference-storage.md).

| Identidade | Significado |
| --- | --- |
| Artifact ID + ConcreteVersion | Seleção literal do registro instalado. |
| SourceRef.directory | Raiz física do payload publicado; exige captura e revalidação. |
| index.lance | Filho Knowledge obrigatório na instalação válida. |
| ModuleSnapshot.version | Geração default efetivamente observada na abertura SDK. |
| Alias de contexto do reader | Identidade de seleção da fonte; separado do namespace e targets persistidos. |

O installer não fornece branch nem `with_version` à abertura. ConcreteVersion
não vira branch ou número Lance. A fixture real
`crates/graphit-app/tests/native_install.rs` instala a versão literal
`exact-version`, reabre `destination/index.lance` e observa uma geração SDK
positiva. Essa evidência pertence ao antecedente entregue; a investigação do
locator não executou novamente o installer.

## Consistência e autoridade do reader

O reader planejado valida o Registry global da mesma instalação autorizada,
seu parent root, tuple exato e `original_source.version`. Captura readonly a
raiz e o filho `index.lance`, abre o default corrente pelo SDK e retém seu
ModuleSnapshot privado durante toda a operação. Grant, request original,
Registry e capturas são rechecados nos limites de I/O, linhas, EOF e entrega.

O parent lógico do Registry conserva sua grafia; o vínculo físico usa o owner
canônico da instalação Auth capturada, além de ID e brand. A leitura capturada
valida o documento contra o Registry lógico original, sem alterar os roots
persistidos de GlobalCache. Para HubCache desse mesmo root lógico, o reader
compõe somente o sufixo validado sob a captura canônica antes de abrir o payload;
o SourceRef selecionado permanece literal e os children continuam sem links.

O Registry atual não armazena um selector SDK imutável. Portanto a leitura
promete a consistência da pin observada, sem afirmar que a versão do artifact
fixa uma geração Lance específica. A pin também não promete imunidade a GC ou
confinamento contínuo do pathname. Um requisito futuro de freezing entre essas
identidades precisa de produtor específico e qualificação própria.

A admissão Web de leitura importada está entregue por Auth. Ela concede leitura
na instalação e preserva a prova e Original; ID, versão, path, alias ou hash não
são grants. O reader não fabrica PROJECT/USER/NativeCaller. Namespace da origem
e targets declarados permanecem os dados persistidos, inclusive o witness;
alias importado não os renomeia.

## Folha consumidora e falhas

`tsk-8c07` implementará o reader e o seam Data de guard externo durante EOF.
Seu resultado distinguirá registro ausente e publicação com geração observada,
inclusive publicação vazia. Para uma instalação Knowledge Native válida,
`index.lance` é obrigatório: sua ausência física conserva erro de captura/SDK,
sem outcome inventado de registro sem corpus. Falhas de Auth, Registry, captura, schema, SDK, witness,
limites, cancelamento e deadline conservarão suas causas. Raiz perdida ou
payload corrupto não são registro ausente. Leitura não cria, importa, repara ou
recupera schema antigo.

A matriz da folha consumidora inclui grant Web e Registry reais, instalação
local válida, versão artifact distinta da geração SDK, abertura readonly,
wrong/foreign installation, tuple alterado, alias, symlink, schema errado,
projeção incompleta, witness adulterado, vazio, Removed, corrupção tardia,
limites e cancelamento. Esses são checks pendentes do produto; a conclusão do
locator não os declara executados.

Fontes do contrato: `crates/graphit-app/src/native_install.rs` (admissão Knowledge,
abertura e publicação/Registry), `crates/graphit-hub/src/source.rs`, o codec em
`crates/graphit-hub/src/native_package/mod.rs` e a fixture do installer citada.
