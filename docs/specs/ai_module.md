---
title: ai_module
type: specification
updated: 2026-05-26
tags: [knowledge, specification, ai]
---

# Especificação do Módulo de IA

O módulo de IA (`internal/ai`) é o componente central responsável por encapsular a comunicação com Large Language Models (LLMs). Sua principal característica é a **desacoplação de provedores de API proprietários diretos**, delegando a conclusão de prompts para utilitários de CLI locais e agentes instalados no sistema do usuário.

> [!IMPORTANT]
> **Delegação e Ausência de Chaves de API:**
> O Graphit Code funciona integrando-se nativamente com as IDEs e seus respectivos agentes locais. Quando o framework precisa realizar qualquer tipo de processamento ou raciocínio por IA fora do ambiente do agente integrado na IDE (por exemplo, disparando comandos interativos diretamente pelo terminal), ele invoca a ferramenta de linha de comando (CLI) correspondente àquela IDE (como `agy`, `claude` ou `cursor-agent`), que por sua vez atua como o próprio agente local.
> 
> Graças a esse modelo de delegação para executáveis locais já autenticados no sistema, **o desenvolvedor não precisa fornecer nenhuma chave de API adicional (API Key)** no arquivo de configuração do Graphit Code, aproveitando diretamente o contexto de login existente no ambiente do usuário.

---

## 🛠️ Interface do Cliente e Inicialização

O módulo expõe uma interface abstrata simples em Go:

```go
type Client interface {
    Complete(ctx context.Context, systemPrompt, userPrompt string) (string, error)
}
```

A inicialização do cliente é orientada pela configuração ativa (`NewClientFromConfig`):
1. Ele lê a chave global `ai.cli`. Se configurada, tenta invocar este executável diretamente.
2. Caso o usuário não tenha definido o CLI de IA manualmente, o sistema detecta qual IDE está orquestrando a execução atual (ex: Cursor, Claude, Antigravity) e infere qual CLI correspondente deve ser priorizado.
3. Se o executável da IDE não for encontrado, ele executa uma rotina de varredura buscando binários comuns de IA instalados no `PATH` do sistema do usuário (como `opencode`, `gemini`, `claude`, `codex`, `grok`, etc.).

---

## 🔌 Mecanismo de Execução de CLI e Fallbacks

A execução física do prompt ocorre disparando o processo externo correspondente via sistema operacional (`exec.CommandContext`):

### 1. Detecção de Comando por Provedor
O formatador adapta a chamada dependendo da sintaxe exigida por cada ferramenta de IA:
* **claude, gemini, agy, grok, cursor-agent, agent:** Utilizam a flag `-p` para receber o prompt.
* **codex, opencode:** Utilizam a flag `run`.
* **kiro-cli:** Utiliza a sintaxe `chat --no-interactive`.

### 2. Gerenciamento de Prompts Longos (Limite Stdin)
Para prompts pequenos ou médios, o texto é passado diretamente como argumento do processo. No entanto, sistemas operacionais possuem limites rígidos de tamanho máximo para argumentos de linha de comando (`ARG_MAX`).
Para evitar quebras de execução em prompts contendo extensos contextos de código, o módulo implementa o chaveamento automático:
* **Limite Seguro (`argMaxSafe`):** 128 Kilobytes (131.072 bytes).
* Se o tamanho combinado do prompt do sistema e prompt do usuário exceder 128 KB, o módulo altera automaticamente as flags de chamada para instruir o CLI de IA a ler o prompt a partir da entrada padrão (`Stdin` / `-` ou `run -`) e escreve o fluxo de texto diretamente no buffer do processo filho.

```go
// Exemplo conceitual do chaveamento interno
useStdin := len(prompt) > argMaxSafe
if useStdin {
    args = []string{"-p", "-"}
    cmd := exec.CommandContext(ctx, c.executablePath, args...)
    cmd.Stdin = strings.NewReader(prompt)
}
```

---

## 🧠 Geração de Embeddings (`EmbeddingClient`)

O módulo de IA também gerencia clientes de embeddings locais e proxy para alimentar as buscas semânticas da AST.
* **Provedor Local:** Capaz de computar embeddings localmente quando usando modelos compatíveis na máquina (privacidade máxima).
* **Provedor Lazy / Proxy:** Permite carregar embeddings sob demanda através de APIs leves e reutilizar caches locais para evitar chamadas duplicadas para o mesmo trecho de código fonte.

---
*Próximo passo recomendado:* Leia sobre o [[ast_module]] e como ele utiliza IA e embeddings.
