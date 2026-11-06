---
title: Identidade das fontes Knowledge em Rust
type: spec
references:
  - target: {type: task, id: tsk-cd410}
    relation: implements
  - target: {type: task, id: tsk-990e}
    relation: specified_by
  - target: {type: task, id: tsk-3f7e}
    relation: consumer_boundary
  - target: {type: task, id: tsk-c8e9}
    relation: preserves
---

# Identidade das fontes Knowledge em Rust

Contrato técnico para consumidores Rust de Knowledge. Esta folha entrega o codec de identidade e origem; a coleta autorizada do corpus desejado e sua reconciliação são entregas separadas. Os testes de identidade Unicode foram executados em Linux; Windows e macOS não foram executados. A fixture de rejeição de UTF-16 inválido permanece condicional ao Windows.

## Entrada e resultado

`KnowledgeSourceIdentity::from_relative_path(relative, authored_id, max_field_bytes)` recebe um `&Path` relativo, um `Option<&str>` literal e um `NonZeroUsize`. Retorna getters `slug()` e `source_path()`. Não lê o filesystem, não captura arquivos e não concede autorização. O caminho precisa conter componentes normais; raiz, prefixo de drive, pai, caminho vazio e ponto isolado produzem `InvalidRelativePath`. A decomposição segue `std::path::Path`: segmentos internos normalizados pelo parser de componentes não são uma grafia alternativa persistida.

Quando há `authored_id`, o slug preserva espaços, caixa e Unicode exatamente. ID vazio, prefixo `..`, barra, contrabarra ou controle produzem `InvalidAuthoredId`. O coletor é responsável por validar o tipo do campo YAML e rejeitar colisões; este codec recebe valores já tipados, sem resolver colisões com sufixos.

Sem ID, o slug deriva reversivelmente dos componentes do caminho. O título e o corpo não participam dessa identidade. Renomear `docs/a.md` para `docs/b.md` mantém o slug com ID explícito e muda a origem; sem ID, ambos mudam. A raiz absoluta do checkout nunca faz parte da representação.

```rust
use graphit_data::knowledge_document_lifecycle::KnowledgeSourceIdentity;
use std::{num::NonZeroUsize, path::Path};
let identity = KnowledgeSourceIdentity::from_relative_path(
    Path::new("docs/Guide.md"), None, NonZeroUsize::new(4096).unwrap(),
).unwrap();
assert_eq!(identity.source_path(), "knowledge-source/t/646f6373/47756964652e6d64");
assert_eq!(identity.slug(), "source-t-646f6373-47756964652e6d64");
```

## Representação canônica

A origem é `knowledge-source/t/{componente-hex}/...`; o slug default é
`source-t-{componente-hex}-...`. Cada componente usa bytes UTF-8 em hexadecimal
minúsculo, não vazio, com comprimento par. Não há percent encoding, normalização
Unicode, hash ou conversão com perda. O produtor rejeita um path não Unicode com
`NonUnicodePath`, mesmo quando foi fornecido um ID authored explícito.

`docs/ação.md` produz `knowledge-source/t/646f6373/61c3a7c3a36f2e6d64`. Formas
compostas e decompostas continuam distintas. Essas origens e slugs Unicode são
preservados. Componentes rejeitam NUL, barra, ponto e ponto duplo. A contrabarra
Unicode pode ser literal num nome Unix; o decoder devolve dados, não autoridade
para abrir o mesmo nome em outra plataforma. As antigas tags `u/w` são inválidas.

## Namespace, limites e erros

`decode_managed_knowledge_source_path(value, max_field_bytes)` retorna `Result<Option<KnowledgeSourcePathComponents>, KnowledgeSourceIdentityError>`. `Portable` contém os componentes Unicode como strings. Nenhum resultado restaura um path capturado, handle, root ou grant.

O valor exato `knowledge-source` e o prefixo `knowledge-source/` pertencem ao namespace reservado. Formatos incompletos, tags desconhecidas, hex maiúsculo ou inválido, componentes vazios, UTF-8 inválido em `t`, e tags `u/w` retornam `InvalidManagedSourcePath`. `knowledge-source-old/...`, `knowledge-sources/...` e outras origens retornam `None`. Assim, o consumidor pode preservar rows de outros produtores sem adotar formatos anteriores.

O limite mede bytes completos dos campos: prefixos, tags, separadores e UTF-8 do slug authored. Igualdade passa; exceder um byte retorna `LimitExceeded("source_path")` ou `LimitExceeded("slug")`. O decoder limita a entrada antes de classificar o namespace, inclusive entradas externas. Os comprimentos usam aritmética verificada e reservas fallíveis; `Overflow` e `Allocation` preservam a condição, com `TryReserveError` como causa de alocação. Estes limites não prometem um teto de RSS.

## Integração e verificação

A classe reservada é membership de dados. O consumidor ainda precisa de captura completa, grants Read/Write, mesmo contexto original, geração esperada, validação integral e CAS. O codec não entrega coleta, publicação, remoção, cancelamento de I/O ou integração Root. Para cancelamento e lifecycle, acompanhe o coletor e a reconciliação; nenhum desses comportamentos é provado pelo codec puro.

[Implementação](../../crates/graphit-data/src/knowledge_document_lifecycle.rs) e [fixtures de identidade](../../crates/graphit-data/tests/knowledge_source_identity.rs) cobrem exemplos independentes de hex, rename, namespace malformado, rejeição de paths não Unicode e igualdade/+1 dos limites. A fixture de rejeição Windows tem `cfg(windows)`; testes Linux não qualificam o filesystem Windows.

Comandos focais:

```text
cargo xtask cargo test --offline --locked -p graphit-data --test integration -- knowledge_source_identity::
cargo xtask cargo clippy --offline --locked -p graphit-data --lib --test integration --no-deps -- -D warnings
```

Os contratos de corpus e referências continuam em [Knowledge semântico Data](../architecture/rust-knowledge-semantic-data.md) e [consulta de referências](rust_knowledge_reference_query.md).
