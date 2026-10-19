---
title: Primeira publicação da seleção Knowledge em stream Rust
type: spec
references:
  - target: {type: task, id: tsk-e6b2}
    relation: implements
  - target: {type: task, id: tsk-69f2}
    relation: implements
  - target: {type: task, id: tsk-6ded}
    relation: implements
  - target: {type: task, id: tsk-393d}
    relation: uses
  - target: {type: task, id: tsk-994a}
    relation: uses
  - target: {type: task, id: tsk-943a}
    relation: preserves
  - target: {type: task, id: tsk-185c}
    relation: uses
  - target: {type: task, id: tsk-c9e2}
    relation: uses
  - target: {type: task, id: tsk-f5a7}
    relation: refines
---

# Publicação progressiva Knowledge por arquivo

`reconcile_knowledge_source_changes` captura a seleção autorizada e publica cada
arquivo completo imediatamente. Source, referências e FTS ficam consultáveis
antes do EOF da seleção e independentemente do embedding.

A descoberta Core enumera nomes e controles de seleção sem abrir ou calcular o
hash dos documentos. Cada worker captura somente o FILE corrente, lê e valida
seus bytes, interpreta frontmatter, compila metadata e prepara o batch FULL18
com os codecs existentes. O worker é aguardado; seus owners terminam antes de processar
o próximo arquivo. Identidades e datas são metadata compartilhada, sem corpos
ou árvores de todo o corpus em memória.

A primeira publicação usa `publish_native_knowledge_source_file`; as seguintes
usam `reconcile_native_knowledge_source_file`, com geração esperada, writer e CAS
reais. Esses produtores validam os bytes do arquivo contra a captura Source;
não rehasham o conteúdo do corpus a cada commit. A seleção vazia publica o batch
vazio canônico pela primeira publicação existente.

Um delta descobre apenas a interseção dos paths do evento com a política de
seleção e captura seu conteúdo por FILE. Eventos de diretório usam o walker Core existente nesse
subtree; payloads dos irmãos intactos não são abertos nem hashados. A seleção
retém essa cobertura e a reutiliza nas revalidações de nomes e controles. Reescaneamento completo
compara SHA e receita salva para pular parsing de arquivos intactos. Datas permanecem
por slug, inclusive no rename. Referências authored ausentes preservam a decisão
atual; lista explícita vazia substitui por vazio. Unicode, BOM, frontmatter,
finais de linha e identidades literais permanecem no source integral.

Após EOF saudável e revalidação dos nomes e controles da seleção, sem um novo
hash do corpus, origens gerenciadas ausentes tornam-se
Removed somente dentro da cobertura genuína capturada. Ausência de uma origem
fora do delta não é prova de remoção. Linhas estrangeiras são preservadas; um
slug ativo em outra origem selecionada ou não coberta continua conflito. A remoção dos vetores de documentos Removed usa
admissões e CAS reais e não depende de modelo ou endpoint disponível.

Uma falha tardia preserva os arquivos já publicados. O erro `Published` contém a
última publicação real, causa original e caminhos relativos ainda não concluídos.
Não representa EOF saudável nem rollback dos commits anteriores. A prioridade
de um novo evento interrompe o trabalho de background somente entre FILEs
completos; não cancela um commit em andamento. O próximo FILE ainda não foi
capturado, e sua captura posterior observa os bytes atuais. Erro na captura
ou na finalização da seleção exige retomar a seleção no próximo evento genuíno;
a comparação de metadata evita reparse dos intactos.

O background recebe somente slugs confirmados, com Original e cancelamento
próprios. Cada arquivo corresponde hoje a um documento/vetor. O lote confirmado
é publicado antes da próxima inferência; não há espera pelos embeddings do
corpus inteiro. O leitor semântico aceita somente fingerprints e identidade de
modelo compatíveis, liberando cada lote atual e excluindo dados antigos.

Limites de arquivo, parser, campos e requisições permanecem concretos. Não há
quota global de corpus, reserva de recursos, importador ou compatibilidade com
o produtor antigo que exigia EOF integral antes de qualquer publicação.

O focal `progressive_files_remain_searchable_after_cancelled_tail_and_resume_only_delta`
valida Source/FTS antes do EOF, recuperação apenas do delta e rename preservando
um arquivo intacto. O consumidor Foreground verifica FTS enquanto a resposta real
do servidor de embeddings está bloqueada. Qualificações históricas dos streams
integrais não comprovam esses comportamentos novos.

Veja a [reconciliação](rust_knowledge_desired_reconciliation.md), a
[primeira publicação](rust_knowledge_source_first_publication.md) e o
[consumidor Foreground](../architecture/rust-knowledge-foreground.md).
