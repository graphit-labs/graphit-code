---
title: Continuidade opcional de guias Knowledge
description: Destinos previous e next no frontmatter, resolução autorizada e reutilização de vetores.
type: spec
references:
  - target: {type: task, id: 01M5C6B85MCWH5HX932YVEKN3Z}
    relation: implements
---

# Continuidade opcional de guias Knowledge

Autores podem indicar uma sequência real de leitura com `previous` e `next` no
mapping YAML raiz. Uma página independente não precisa desses campos. A seleção
não cria reciprocidade automática, uma ordem global ou permissão sobre destinos.

```yaml
---
title: Indexação do projeto
previous: getting_started.md
next: "[[advanced-indexing]]"
---
```

Os valores são destinos textuais escalares interpretados pelo parser YAML
existente; quoting, escapes e scalars multiline conservam seu texto decodificado.
Ausência, null e texto somente com whitespace representam ausência de navegação.
Sequências e mappings não são destinos escalares válidos. O corpo persistido já
contém o frontmatter integral: não há novas colunas, dataset de navegação,
importador ou reconstrução do corpus.

Slugs literais, wiki-links e links Markdown usam o resolvedor existente. Um
`.md` relativo é resolvido a partir da origem autoral e consultado pelo
`source_path` canônico, portanto o destino pode ter um `id` explícito diferente
do nome do arquivo. Query/fragment e percent decoding seguem as regras de
[mapeamento documental](../architecture/rust-knowledge-native-document-mapping.md).
URLs externas e caminhos que escapam da seleção não selecionam outro contexto.

## Página, Source e exportação

A página JSON de PROJECT e artefatos importados expõe `document.previous` e
`document.next`, cada um null ou um objeto deste formato:

```json
{"target":"[Continuar](next.md)","slug":"next-guide","title":"Próximo guia"}
```

`target` conserva a declaração autoral. Se a consulta autorizada não encontra o
destino, `slug` e `title` ficam null; o consumidor mostra o diagnóstico e não cria
um link navegável. Uma ausência pontual não bloqueia o corpo da página. Falhas de
autorização, cancelamento, integridade e I/O conservam seus erros e causas; não
são convertidas em ausência.

São feitas no máximo duas consultas selecionadas de head/witness, sem carregar
os corpos anterior/próximo nem enumerar a biblioteca. PROJECT preserva a mesma
seleção autorizada de projeto/versão; importados preservam o artefato/versão
admitido. Consultas independentes podem observar atualizações entre leituras;
não existe uma geração global comum obrigatória para a sequência.

No catálogo PROJECT ou importado, nos resultados de busca e na leitura por slug
importado, selecionar uma página por clique ou teclado abre um único leitor
inline e leva imediatamente seu topo ao início da área visível abaixo do header.
Esse reveal vale também para a seleção inicial, antes de concluir a leitura, e
conserva a posição do leitor durante loading, erro e retry. O leitor selecionado
recebe a altura mínima observada da área rolável local, para permitir que também
a última página curta, carregando ou com erro alcance esse topo; o corpo longo
cresce normalmente, sem novo scroll a cada render ou ação no documento. A rolagem
é confinada ao `WorkPage`: o destino considera seu início interno, a borda inferior
do header visível no mesmo workspace e o `scroll-margin-top` do artigo. Esse cálculo
preserva a rolagem externa da janela e mantém o título abaixo do header também
quando a área de leitura já está parcialmente encoberta em uma tela estreita. Uma chegada tardia do
catálogo reposiciona o leitor se as linhas anteriores deslocarem sua abertura.
URL direta, histórico e navegação anterior/próxima seguem o mesmo contrato;
fechar o leitor restaura o foco da entrada, sem selecionar outra página.

Na apresentação de `NativeKnowledgeDocument`, o H1 de metadados (ou slug)
permanece como título do leitor que recebe foco. O opt-in `displayedTitle` reutiliza o
pipeline Markdown existente e omite apenas o H1 que abre o corpo (após o
frontmatter, quando presente), se seu conteúdo é somente textual e seu texto é
exatamente igual ao título exibido. Títulos divergentes ou posteriores, ações, imagens e
identificadores autorais são preservados. Essa opção não modifica Source, corpo
raw, cópia ou exportação.

Source CLI/MCP/HTTP conserva `previous`/`next` como scalars no frontmatter virtual
e mantém o corpo literal. O header usado pela exportação Markdown/OKF/Obsidian
reutiliza essas declarações. A navegação renderizada da exportação usa seu roster
já admitido, cria links somente para destinos presentes e mostra destinos
indisponíveis sem href fictício. O pacote Native continua preservando os mesmos
registros, origens e corpos; importação usa os leitores e SDKs existentes.

## Texto semântico e publicação

A projeção de embeddings remove somente as entradas `previous`/`next`, usando
spans da arena YAML existente. Documentos sem essas entradas mantêm texto e
fingerprint byte a byte. Outros campos e comentários independentes são
preservados. Um envelope introduzido apenas para navegação não entra no texto
semântico; a receita/modelo global permanece a mesma.

Há uma exceção inevitável sem histórico de formatação: adicionar navegação a um
envelope YAML que já era vazio pode mudar o fingerprint uma vez. O mesmo source
resultante pode ter vindo de um documento sem envelope ou de um envelope vazio;
não há como reconstruir ambos os textos anteriores de forma stateless. Isso não
autoriza normalizar documentos sem navegação nem recalcular os demais casos.

Na substituição, Data recebe uma comparação pura dos corpos antigo/novo
observados sob o mesmo CAS. Active, slug, título, tipo, resumo e descrição também
precisam permanecer iguais. Nessa condição as colunas vetoriais e seus
certificados são preservados por Arc na própria transação Source, sem inferência
nem um segundo commit de reanexação. Texto efetivamente alterado invalida os
certificados. Identidade de modelo, dimensão, recipe, role e input fingerprint
continuam sujeitos aos filtros normais de elegibilidade.
Conservar as colunas não torna um certificado elegível por si só: se seu
`input_fingerprint` não corresponde à entrada normalizada atual, a página
permanece pendente para a indexação incremental normal. Não se aceita um
fingerprint alternativo nem um vetor incompatível.

## Manutenção e evidência

Os templates canônicos de skill/mandate Knowledge orientam preencher os campos
quando houver uma sequência útil. O par mantido
[Getting Started](../guides/getting_started.md) →
[Indexing](../guides/indexing.md) representa instalar/inicializar o checkout e,
em seguida, entender disponibilidade progressiva, atualização e versões.

As fontes correspondentes são `document.rs`, `document/navigation.rs`,
`navigation.rs`, `embedding_projection.rs`, `document_source.rs` e
`markdown_export.rs` em Knowledge; os head lookups e a substituição semântica em
Data; e `foreground_knowledge/navigation.rs`/`PageView` em App. Os testes focais
cobrem estilos YAML, wire HTTP, destinos relativos com ID autoral, erro causal,
Source/export, publicação full/incremental e preservação/invalidação de vetores.
A execução e seus recibos pertencem à Task; teste de projeção isolado não
qualifica preservação de vetor durante publicação real.
