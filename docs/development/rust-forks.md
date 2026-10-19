---
references:
  - target: {type: task, id: tsk-bd55}
    relation: implements
  - target: {type: task, id: tsk-effc}
    relation: implements
  - target: {type: task, id: tsk-625d}
    relation: relates_to
  - target: {type: task, id: tsk-cab0}
    relation: implements
---

# Dependências nativas e origem dos consumidores

Lance14.0.2 e Ladybug/lbug0.22.1 são dependências oficiais do Cargo, sem patches
de fontes do Graphit. Seus manifests e lockfile selecionam versões exatas. Veja
[a integração Lance](lance-upstream.md) e [o build Ladybug](../specs/rust_ladybug_sdk.md).

Graphit Libs mantém a receita independente de conversão BGE GGUF F16. O motor
Ladybug é preparado no build do Code a partir de `lbug-src`, incluído no crate
oficial que o Cargo resolveu; os headers gerados ficam fora do cache Cargo.
Não há SDK separado, fontes Cargo geradas, aplicação de patches, bundle Git
ou necessidade de publicar outra revisão do binding.

O empacotamento ELF isola símbolos de bibliotecas embutidas e do ANTLR para
permitir a ligação com os parsers oficiais do Graphit. Essa transformação do
archive não altera as fontes do motor nem cria outra FFI. `cxx` continua sendo a
fronteira C++/Rust existente.

O Graphit mantém a produção e composição autorizada dos shards Parquet/CSR,
identidades públicas, versões, Source e consumidores. O Ladybug oficial mantém
seu gerenciamento normal de memória e seus leitores. Não existe um backend
0.21.2 de fallback nem patches próprios de WAL/checkpoint/eviction no caminho
atual. O Copilot continua usando sua integração CLI, sem SDK modificado.

Use `cargo xtask cargo build/test/check ...` ou os alvos mantidos do Makefile.
O xtask prepara uma compilação Release do motor e passa a mesma biblioteca,
headers e compilador ao binding oficial e ao bridge `cxx`. CMake reutiliza seus
artefatos quando as entradas não mudam; `--offline` dispensa rede após preparar
as dependências e ferramentas. Veja o guia acima para target, toolchain e CRT.
Windows/macOS e CI remoto dependem de execução correspondente; não herdam um
PASS de fontes, scripts ou execução Linux.
