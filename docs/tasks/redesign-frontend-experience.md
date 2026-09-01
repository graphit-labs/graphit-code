---
title: Redesign completo da experiência frontend
status: done
created: 2026-08-31
updated: 2026-08-31
tags: [frontend, ui, ux, react, accessibility]
---

# Redesign completo da experiência frontend

## Objective

Reconstruir integralmente a UI/UX do dashboard Graphit Code sem tomar a aparência atual como referência e sem remover, esconder ou degradar funcionalidades existentes. O pedido concede liberdade visual; a interpretação adotada é transformar o produto em um workspace técnico de alta densidade e alta legibilidade — um “observatório” para código, conhecimento, agentes e ecossistema — preservando contratos de API, rotas, ações, estados, feedbacks e fluxos.

A abordagem parte do inventário funcional documentado e do grafo AST, depois estabelece um design system coerente antes de redesenhar o shell e cada superfície. Isso foi escolhido em vez de alterações pontuais porque a solicitação é sistêmica: apenas trocar cores e espaçamentos perpetuaria a hierarquia e os padrões atuais; reescrever comportamento, por outro lado, criaria risco desnecessário de regressão funcional.

## Plan & Task Breakdown

- [x] **T1 — Inventário funcional e de impacto** — Spec: mapear rotas, páginas, componentes, stores, APIs, ações e cobertura de teste em `internal/ui/`; concluído quando cada superfície e interação existente estiver contabilizada; a invariante é não inferir funcionalidade pela aparência atual.
- [x] **T2 — Fundação visual e shell responsivo** — Spec: reconstruir tokens, tipografia, cores, motion, foco, superfícies, `AppShell`, `Sidebar` e `ProjectSwitcher`; concluído quando navegação desktop/mobile, tema e contexto de projeto funcionarem com hierarquia consistente; a invariante é manter todos os destinos e controles existentes.
- [x] **T3 — Superfícies de exploração** — Spec: redesenhar AST Explorer, canvas, árvore de nós, query bar, resultados, código, schema, Live Search, Wiki e contextos; concluído quando leitura, busca, seleção e exploração preservarem seus estados e ações; a invariante é priorizar densidade sem sacrificar legibilidade.
- [x] **T4 — Superfícies operacionais** — Spec: redesenhar Hub, uploads, projetos, modais, daemon, dream, ecossistema, backlog e estados compartilhados; concluído quando catálogos, formulários, confirmações, status e feedbacks mantiverem comportamento; a invariante é que ações destrutivas ou assíncronas permaneçam inequívocas.
- [x] **T5 — Verificação funcional e visual** — Spec: executar testes, typecheck/build e inspeção visual nas rotas representativas e larguras desktop/mobile; concluído sem falhas introduzidas e sem overflow/contraste/foco evidentes; a invariante é tratar o build como prova técnica e o navegador como prova de experiência.
- [x] **T6 — Documentação, memória e sincronização** — Spec: atualizar este log, a especificação de UI e a memória do projeto, depois sincronizar índices; concluído quando decisões, arquivos, testes e dívida estiverem registrados e os índices refletirem o estado final.
- [x] **T7 — Identidade do favicon** — Spec: substituir o favicon legado por um símbolo vetorial coerente com o Graphite Observatory e atualizar os metadados do documento; concluído quando navegador e atalhos deixam de referenciar a marca anterior.
- [x] **T8 — Nomes lógicos de relações no AST Explorer** — Spec: resolver nomes físicos das tabelas de relação pelo mapeamento canônico do manifest Icebug na fronteira da API, incluindo schema, filtros e links do canvas; concluído quando nenhuma superfície do Explorer expuser a limitação física e testes cobrirem agregação e tradução.
- [x] **T9 — Fidelidade do brand glyph** — Spec: substituir a interpretação livre do favicon por uma transcrição vetorial exata do `.brand-glyph` da sidebar; concluído quando formas, proporções, cores e posições coincidirem com o CSS e com a referência visual anexada, inclusive com as duas bolinhas inferiores visualmente separadas.

## Implementation Details

O frontend foi reconstruído sobre o conceito **Graphite Observatory**. `index.css` agora define tokens claros/escuros, Manrope + IBM Plex Mono, superfícies sólidas de papel técnico, grid coordenado, acento fosforescente, foco, seleção, motion reduzido, scrollbar e estados globais. `AppShell`, `Sidebar` e `ProjectSwitcher` compõem uma navegação carvão independente do tema, redimensionável no desktop e em drawer no mobile, preservando todos os destinos e filtros.

As páginas operacionais receberam espaçamento coerente, hierarquia editorial e marcadores de domínio. AST e Wiki usam `explorer-frame` e colapsam o índice por padrão em viewport móvel. Live Search passa de duas colunas para fluxo vertical abaixo de `lg`, com picker de altura limitada e console sempre acessível. Estados vazios, toasts e loader foram refeitos com semântica e ARIA preservadas ou ampliadas.

## Use Cases

### UC-01: Navegar pelo workspace Graphit
- **Actor**: pessoa desenvolvedora usando o dashboard.
- **Preconditions**: servidor de UI ativo e SPA carregada.
- **Main Flow**:
  1. A pessoa identifica a área atual pela navegação global.
  2. Seleciona um destino existente do produto.
  3. A rota correspondente é aberta sem perder o contexto global do projeto.
- **Alternative Flows**:
  - Em viewport estreita, a navegação usa o padrão responsivo definido pelo shell.
  - A pessoa alterna o projeto ativo no seletor global.
- **Error Scenarios**:
  - Falhas de carregamento continuam produzindo feedback visível e recuperável.
- **Postconditions**: a área escolhida está ativa e todas as ações originais continuam disponíveis.
- **Affected Files**: `internal/ui/src/App.tsx`, `internal/ui/src/components/layout/AppShell.tsx`, `internal/ui/src/components/layout/Sidebar.tsx`, `internal/ui/src/components/layout/ProjectSwitcher.tsx`.

### UC-02: Explorar código, conhecimento e resultados
- **Actor**: pessoa desenvolvedora investigando um projeto.
- **Preconditions**: projeto válido selecionado e fontes correspondentes disponíveis.
- **Main Flow**:
  1. A pessoa abre AST, Live Search ou Wiki.
  2. Executa uma busca, consulta ou navegação pela árvore.
  3. Inspeciona grafo, resultado tabular, código, schema ou conteúdo Markdown.
  4. Seleciona entidades e alterna painéis sem perder o contexto relevante.
- **Alternative Flows**:
  - O sistema apresenta estados vazios, carregamento e erros conforme a fonte.
- **Error Scenarios**:
  - Consultas inválidas e falhas de API mantêm mensagens claras sem corromper o estado anterior.
- **Postconditions**: a informação solicitada é exibida ou a falha é explicada com caminho de recuperação.
- **Affected Files**: `internal/ui/src/components/ast/`, `internal/ui/src/components/live/`, `internal/ui/src/components/wiki/`.

### UC-03: Operar serviços e artefatos do ecossistema
- **Actor**: pessoa desenvolvedora administrando Graphit.
- **Preconditions**: backend disponível e permissões/configuração adequadas.
- **Main Flow**:
  1. A pessoa abre Hub, Daemon, Dream, Ecossistema ou Contextos.
  2. Consulta estado e dados operacionais.
  3. Executa uma ação existente, preenche um formulário ou confirma uma operação.
  4. Recebe feedback de progresso, sucesso ou erro.
- **Alternative Flows**:
  - Listas sem dados exibem orientação em vez de áreas vazias.
- **Error Scenarios**:
  - A API pode falhar; a UI preserva dados úteis e apresenta erro acionável.
- **Postconditions**: o resultado da operação é refletido na superfície correspondente e por toast quando aplicável.
- **Affected Files**: `internal/ui/src/components/hub/`, `internal/ui/src/components/daemon/`, `internal/ui/src/components/dream/`, `internal/ui/src/components/system/`.

## Test Cases & Acceptance Criteria

### Feature: Navegação e shell responsivo
Ref: UC-01

#### Scenario: Navegação preserva destinos existentes
```gherkin
Given o dashboard carregado com um projeto selecionado
When a pessoa percorre cada destino disponível na navegação
Then cada rota existente continua acessível
  And o destino atual possui indicação visual e semântica inequívoca
```

#### Scenario: Shell se adapta a viewport móvel
```gherkin
Given uma viewport de 390 por 844 pixels
When o dashboard é carregado e a navegação é aberta
Then todos os destinos e o seletor de projeto permanecem acessíveis
  And o conteúdo não cria overflow horizontal involuntário
```

### Feature: Exploração de informações
Ref: UC-02

#### Scenario: Consulta AST preserva resultados e painéis
```gherkin
Given um projeto com índice AST e a página de exploração aberta
When a pessoa executa uma consulta válida e seleciona um resultado
Then o resultado continua disponível nos formatos suportados
  And os painéis de detalhe e código mantêm as interações existentes
```

#### Scenario: Falha de consulta é recuperável
```gherkin
Given a página de exploração aberta
When a pessoa executa uma consulta que recebe erro do backend
Then a UI apresenta a mensagem de erro
  And uma nova consulta pode ser executada sem recarregar a página
```

### Feature: Operações do ecossistema
Ref: UC-03

#### Scenario: Operação assíncrona comunica estado
```gherkin
Given uma superfície operacional com uma ação existente disponível
When a pessoa inicia a ação
Then a UI comunica o estado em andamento
  And apresenta sucesso ou erro ao concluir
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `docs/tasks/redesign-frontend-experience.md` | Created | Abrir o trabalho com objetivo, invariantes, plano, casos de uso e critérios verificáveis. |
| `docs/specs/ui_dashboard.md` | Modified | Registrar a linguagem visual e o contrato responsivo atuais. |
| `docs/decisions/graphite-observatory-ui.md` | Created | Formalizar a direção visual e suas consequências. |
| `internal/ui/src/index.css` | Rebuilt | Definir o design system Graphite Observatory e os comportamentos globais de acessibilidade/motion. |
| `internal/ui/tailwind.config.js` | Modified | Alinhar famílias tipográficas e motion ao novo sistema. |
| `internal/ui/src/components/layout/AppShell.tsx` | Modified | Reestruturar canvas, largura, grid e resizer do workspace. |
| `internal/ui/src/components/layout/Sidebar.tsx` | Modified | Criar nova identidade, hierarquia de navegação, estados ativos e drawer acessível. |
| `internal/ui/src/components/layout/ProjectSwitcher.tsx` | Modified | Adaptar seletores de projeto e IDE à navegação carvão. |
| `internal/ui/src/components/shared/EmptyState.tsx` | Modified | Padronizar estados vazios como sinal técnico e melhorar composição visual. |
| `internal/ui/src/components/shared/Toast.tsx` | Modified | Recriar feedback global e adicionar região ARIA live. |
| `internal/ui/src/components/shared/GlobalLoader.tsx` | Modified | Tornar processamento global legível e não intrusivo. |
| `internal/ui/src/components/ast/ExplorerPage.tsx` | Modified | Aplicar frame visual e colapso móvel do rail/query bar. |
| `internal/ui/src/components/wiki/WikiExplorerPage.tsx` | Modified | Aplicar frame visual e colapso móvel do índice. |
| `internal/ui/src/components/live/LiveSearchPage.tsx` | Modified | Reorganizar picker e console responsivamente e garantir acesso móvel. |
| `internal/ui/src/components/ast/ContextsPage.tsx` | Modified | Aplicar hierarquia e espaçamento de página. |
| `internal/ui/src/components/wiki/WikiContextsPage.tsx` | Modified | Aplicar hierarquia e espaçamento de página. |
| `internal/ui/src/components/hub/RegistryPage.tsx` | Modified | Aplicar hierarquia e espaçamento de página. |
| `internal/ui/src/components/hub/ProjectArtifactsPage.tsx` | Modified | Aplicar hierarquia e espaçamento de página. |
| `internal/ui/src/components/hub/UploadPage.tsx` | Modified | Aplicar hierarquia e espaçamento de página. |
| `internal/ui/src/components/daemon/DaemonDashboard.tsx` | Modified | Aplicar hierarquia e espaçamento de página. |
| `internal/ui/src/components/dream/DreamDashboard.tsx` | Modified | Aplicar hierarquia e espaçamento de página. |
| `internal/ui/src/components/system/EcosystemDashboard.tsx` | Modified | Aplicar hierarquia e espaçamento de página. |
| `internal/ui/public/favicon.svg` | Created | Substituir o favicon legado pelo glifo do Graphite Observatory. |
| `internal/ui/index.html` | Modified | Referenciar o novo favicon com cache busting e alinhar metadados da página. |
| `internal/ast/ladybug_relationship_names.go` | Created | Resolver e agregar tipos lógicos a partir do manifest específico de cada backend/projeto. |
| `internal/ast/ladybug_relationship_names_test.go` | Created | Impedir vazamento de nomes físicos e provar isolamento do mapeamento entre projetos. |
| `internal/ast/server.go` | Modified | Publicar apenas nomes lógicos em `/api/schema` e `/api/graph`. |
| `docs/specs/ui_dashboard.md` | Modified | Registrar o limite público entre nomes lógicos e tabelas físicas do Icebug. |

## Trade-offs & Decisions

- A nova identidade partirá do propósito do Graphit — exploração conectada de código e conhecimento — e não de convenções de painel administrativo genérico.
- Contratos de API, store e rotas serão preservados por padrão; mudanças ficarão concentradas em composição, estilos, acessibilidade e microinterações.
- A direção visual será definida após o inventário funcional, evitando que uma estética escolhida prematuramente esconda controles ou estados raros.
- A navegação usa carvão fixo nos dois temas para funcionar como referência espacial estável; somente o workspace alterna entre papel claro e grafite escuro.
- Live Search empilha seus painéis no mobile em vez de esconder o console ou criar scroll horizontal; isso aceita maior altura de documento em troca de acesso integral às ações.

## Technical Debt

- [ ] Adicionar testes de interação dedicados para `AppShell`, `Sidebar`, `ProjectSwitcher`, Live Search responsivo e estados móveis dos exploradores. O grafo não mostra testes alcançando esses componentes e o relatório atual cobre apenas 5,61% das statements totais, apesar de os 42 testes existentes passarem.
- [ ] Executar o smoke visual também contra `graphit ui` com backend real e dados representativos. O Vite isolado comprovou composição e responsividade, mas naturalmente respondeu vazio nas rotas `/api`; os erros JSON observados no console vieram dessa ausência de backend, não das mudanças de apresentação.

## System Knowledge

- A UI é uma SPA React/Vite/Tailwind embutida pelo servidor Go.
- O dashboard integra exploração AST 3D, wikis/memória, Hub, daemon, dream, ecossistema e busca ao vivo; a navegação é portanto uma ferramenta de orientação entre domínios, não apenas uma lista de páginas.
- JSX renderizado como chamada de componente não aparece como aresta `CALLS` no grafo atual; por isso os componentes de layout e página não apresentam callers/testes alcançáveis mesmo sendo rotas ativas. O inventário foi confirmado pela tabela de imports, `App.tsx`, build e smoke test no navegador.

## Progress Log

### 2026-08-31
- Memória pesquisada; não houve memória específica de UI relevante para orientar o redesign.
- Wiki consultada: `UI Dashboard Specification` (confiança 0,90) e `System Architecture Overview` (confiança 1,00).
- Schema AST consultado antes de qualquer query estrutural; inventário inicial encontrou 54 arquivos TSX/TypeScript/CSS no frontend.
- Inventário de 202 funções e 207 imports do frontend concluído. O índice não armazena o source desses arquivos, então a leitura direta foi usada somente após o fallback explícito da ferramenta.
- Design system, shell, navegação, superfícies, feedbacks e exploradores reconstruídos sem mudar contratos de API/store/rotas.
- `npm run build` passou após a fundação inicial.
- Inspeção em navegador passou em tema claro/escuro, desktop e viewport 390×844; todas as rotas conhecidas renderizaram sem overflow horizontal.
- A inspeção móvel encontrou o console de Live Search inacessível fora da viewport; a composição foi corrigida para empilhar picker e console. AST e Wiki passam a iniciar com rail colapsado no mobile.
- Suíte final concluída: `npm test` com 5 arquivos/42 testes aprovados; `npm run lint` sem avisos; `npm run build` concluído; `git diff --check` limpo.
- ADR `docs/decisions/graphite-observatory-ui.md` criado e `docs/specs/ui_dashboard.md` atualizado.
- Trabalho concluído sem alteração de rotas, contratos de API, store ou ações de domínio.

### 2026-08-31 — Correções pós-entrega
- O usuário apontou que `internal/ui/index.html` ainda referenciava o `logo.svg` legado como favicon e que `/api/schema`/`/api/graph` expunham nomes físicos das relações canônicas do Icebug.
- A memória, o task log anterior, o schema AST e o ecossistema foram reconsultados antes da correção. O manifest canônico é a fonte de verdade: `CanonicalRelGroup.Type` é o nome lógico público; `Members[].Table` e `ReverseMembers[].Table` são detalhes físicos do engine.
- Direção definida: corrigir no backend, agregando estatísticas e normalizando links pela mesma instância de `CanonicalManifest` já carregada pelo tradutor/planner; o frontend continuará consumindo apenas nomes públicos e não receberá um mapa duplicado.
- T7 implementada: criado `public/favicon.svg` com o glifo verde/preto do Observatory; `index.html` agora usa o novo asset com versão de cache, título, descrição e `theme-color` coerentes.
- T8 implementada no backend: o manifest canônico resolve cada tabela física para `CanonicalRelGroup.Type`; `/api/schema` agrega membros forward sob o tipo lógico e `/api/graph` normaliza os links antes da serialização. Mirrors reverse não inflam a contagem pública.
- Refinamento solicitado: o mapa não é global. `dbForContext` resolve `project_dir`/`context`, o cache é chaveado por `projectDir + storeDir`, cada backend aponta `IcebugDir` para `<store>/graph.icebug` e `loadCanonicalManifestLocked` lê o `icebug.json` dessa pasta. O teste de regressão usa dois manifests reais com a mesma tabela física e nomes lógicos diferentes para provar o isolamento por projeto.
- Validação concluída: testes focados cobrem leitura por projeto, tradução de links, agregação sem mirrors e a resposta lógica do schema; suíte completa `go test ./internal/ast -count=1` aprovada; frontend com 5 arquivos/42 testes, lint e build de produção aprovados; `git diff --check` limpo; favicon renderizado em 256×256 para inspeção visual.
- Nova correção visual solicitada: o favicon deve ser exatamente o `.brand-glyph`, não apenas usar suas cores. A referência anexada e o CSS foram inspecionados; a nova versão será uma transcrição direta do quadrado arredondado, anel e três pontos, sem elementos gráficos adicionais.
- T9 implementada: o SVG agora usa o mesmo canvas proporcional de 40×40, raio externo de `0.6rem`, inset de `0.45rem`, anel de 1 px e círculos derivados literalmente dos dois `box-shadow` e do `::after`. O URL avançou para `v=3` para invalidar o favicon interpretativo já armazenado pelo navegador.
- O SVG foi rasterizado em 256×256 e comparado com a referência anexada: anel, ponto superior direito e o par sobreposto inferior esquerdo preservam a composição do `.brand-glyph`, sem os nós e diagonais da versão rejeitada.
- A comparação foi corrigida pelo usuário: embora a composição estivesse certa, os círculos inferiores ficaram grandes e próximos demais. Medição da referência de 50 px, normalizada para 40×40, posiciona os centros aproximadamente em `(7.0,31.5)` e `(10.5,28.7)`, com raio visual próximo de `2.4`; T9 foi reaberta para preservar a separação entre as formas.
- Ajuste aplicado: os três pontos agora usam raio `2.4`; os centros inferiores passaram de `(7.2,32)`/`(9.92,30.08)` para `(7,31.5)`/`(10.5,28.7)`, recuperando duas silhuetas distintas. O ponto superior acompanha a proporção medida em `(33.2,10.8)` e o cache avançou para `v=4`.
- Nova rasterização em 256×256 confirma as duas bolinhas inferiores separadas, com tangência curta e proporção equivalente à referência; T9 concluída novamente com a geometria medida, não inferida.
- O usuário autorizou explicitamente o commit diretamente na `main`. O estado do worktree foi revisado: todos os arquivos modificados pertencem ao redesign Graphite Observatory, às correções do AST Explorer e ao favicon; nenhum arquivo alheio será incluído.
