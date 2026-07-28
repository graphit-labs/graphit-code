# Comentários viram entidades em todas as linguagens tree-sitter, com aresta para o que documentam

**Data:** 2026-07-28
**Escopo:** `internal/ast/treesitter_adapter.go`, `internal/ast/helper.go`,
`internal/ast/comment_entity_test.go`, `internal/ast/docstring_pipeline_test.go`
**Origem:** pedido do Engenheiro — comentários indexados como entidade com `type` Comment e
`name` sendo o texto, apontando para a entidade associada ou, em último caso, para o arquivo

---

## O que mudou

PL/SQL já transformava `COMMENT ON` em entidades `Comment` cujo nome é o texto, para que
"o que a documentação diz" fosse pesquisável. Em todas as outras linguagens um comentário só
existia como campo `Docstring` pendurado numa declaração — o que significa que **comentário
que não documenta nada não era indexado**: cabeçalho de licença, nota dentro de corpo de
função, bloco explicativo entre funções, código comentado.

Agora cada comentário é uma entidade `Comment` com `Name` igual ao próprio texto, e carrega
uma aresta `REFERENCES`:

- para a declaração que ele precede, quando há uma;
- para o arquivo, caso contrário — nada fica inalcançável por falta de dono.

## Como, sem reintroduzir a travessia

Comentários não são alcançáveis pelos arquivos de query por linguagem: aqueles descrevem
declarações, e nenhuma linguagem declara um padrão para os próprios comentários. Varrer a
árvore atrás deles reintroduziria exatamente a travessia de arquivo inteiro que acabou de ser
removida.

Em vez disso, `commentQueryFor` **sintetiza uma query** a partir dos tipos de nó de comentário
que a gramática realmente tem, e ela roda no mesmo passe, pelo mesmo motor, do lado C. A query
é cacheada por gramática.

Tipos ausentes da gramática são descartados em vez de repassados: **um único tipo de nó
desconhecido faz a query inteira falhar na compilação**, e o conjunto de tipos de comentário é
a união de todas as linguagens, então a maior parte dele não existe em nenhuma delas
individualmente. `IdForNodeKind` decide o que sobra.

## `cleanDocstring` só removia prefixo

Ele nunca removeu sufixo, então uma docstring Python de uma linha saía como
`Alpha docstring."""` e um comentário de bloco de uma linha guardava o `*/`. Isso era um dos
dois defeitos fixados nos testes com o defeito nomeado, e passa a importar mais do que antes:
**o nome da entidade Comment é o próprio texto**, então o lixo fica visível para quem busca.

Corrigido: sufixos `"""`, `'''`, `*/`, `-->` são removidos, e `--` e `<!--` entram na lista de
prefixos.

`TestDocstringsSurviveTheRealQueryPipeline` ficou vermelho com isso — era o teste que fixava o
defeito. Expectativa atualizada para o valor correto e o comentário de defeito removido. Foi
exatamente o cenário previsto quando o defeito foi fixado: quem conserta vê vermelho e precisa
saber que é conserto, não regressão.

## Deduplicação

Comentários idênticos no mesmo arquivo geram uma entidade só. Isso é o comportamento já
existente para rótulos não específicos, e aqui é desejável: separadores como `// -----`
gerariam centenas de entidades idênticas.

## O lado ANTLR

Os drivers montam o stream com `antlr.NewCommonTokenStream(lexer, antlr.TokenDefaultChannel)`
e nessas gramáticas comentários vão para o canal `HIDDEN`, então **não estão na árvore** que
`Parse` devolve. Não estão perdidos: o `CommonTokenStream` bufferiza todo token que o lexer
produziu e apenas filtra por canal no acesso, então depois do parse dá para lê-los de volta.

- `antlrcommon.CollectComments` chama `Fill()` primeiro — o parser pode ter parado antes do EOF,
  por erro de parse ou porque a regra de entrada terminou, e os tokens finais nunca teriam sido
  puxados do lexer — e devolve os tokens de comentário como `TreeNode`.
- O resultado é anexado à raiz em **`TreeNode.Comments`, não em `Children`**. `Children` é o que
  os padrões de extração percorrem, e injetar nós ali mudaria o que todo padrão existente casa.
  Um campo separado atravessa o JSON do sidecar por conta própria.
- "Não está no canal padrão" seria largo demais para significar "é comentário": o canal oculto
  carrega espaço em branco e, em algumas dessas gramáticas, diretivas. A decisão é pelo nome do
  token na gramática — todas as cinco nomeiam os seus com `COMMENT` dentro.
- Os cinco drivers nativos receberam uma linha cada.

### Posse por proximidade, não por estrutura

O lado tree-sitter decide estruturalmente: o comentário é dono da declaração que é seu irmão
seguinte. Aqui não há equivalente, porque os comentários nunca estiveram na árvore para ter
irmãos. `extractCommentsAntlr` usa proximidade: o comentário pertence à primeira entidade que
começa até `commentAttachGap` (2) linhas abaixo dele, e ao arquivo quando nada está tão perto.
Roda por último, porque precisa das linhas de todas as entidades já conhecidas.

**Ressalva do sidecar:** gramáticas instaladas como binário separado só produzem comentários
depois de reconstruídas com esta mudança. Como o campo é JSON, binários antigos simplesmente
omitem e o resultado é `nil` — degrada em silêncio, sem erro.

## Testes

`TestCommentsAreEntitiesInEveryLanguage` cobre Go e Python com os três casos que importam:
comentário de cabeçalho apontando para o arquivo, comentário de declaração apontando para a
declaração, e nota dentro de corpo de função apontando para o arquivo.
`TestCommentNamesCarryNoMarkers` garante que nenhum marcador sobrevive no nome.
`TestCommentsAreEntitiesInAntlrLanguages` percorre a rota inteira do ANTLR, do canal do lexer
até entidade indexada com aresta, e verifica que o cabeçalho vai para o arquivo enquanto o
bloco colado na função vai para a função.

Suíte completa com `-race` limpa.
