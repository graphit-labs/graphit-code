---
title: setup_guide
type: guide
updated: 2026-05-26
tags: [knowledge, guide, setup]
---

# Guia de Configuração e Setup

Este documento orienta os desenvolvedores sobre a instalação, inicialização e configuração do ecossistema do **Graphit Code**, explicando o fluxo de kickstart e todas as opções de configuração possíveis.

---

## 🚀 Kickstart (Inicialização Rápida)

Para começar a utilizar o Graphit Code, siga os passos abaixo no seu terminal:

1. **Configuração Global Inicial:**
   Execute o setup interativo global para vincular repositórios centrais e configurar ferramentas padrão:
   ```bash
   graphit setup
   ```
   *O setup irá solicitar o repositório Git do Hub (onde são publicados pacotes, regras e skills), o repositório opcional de Memória remota, a IDE principal (ex: Cursor, Claude, Antigravity) e o executável CLI de fallback de IA.*

2. **Inicialização do Projeto:**
   Navegue até a raiz do seu repositório de código e inicialize o Graphit Code localmente:
   ```bash
   graphit init
   ```
   *Isso cria uma pasta local `.graphit/` contendo as pastas de cache, regras personalizadas do projeto e o arquivo de controle local `graphit.lock.json`.*

3. **Construção do Grafo AST e Indexação da Wiki:**
   Extraia os conhecimentos do seu projeto executando a indexação inicial do código e da pasta `docs/`:
   ```bash
   # Indexa o código fonte usando parsers Tree-sitter
   graphit ast index
   
   # Indexa a pasta docs/ gerando a wiki local
   graphit knowledge index
   ```

---

## ⚙️ Hierarquia e Resolução de Configurações

O Graphit Code resolve chaves de configuração sob demanda seguindo uma ordem de precedência estrita (o primeiro que corresponder é retornado):

1. **Configurações Inline (Flags do CLI):**
   Valores passados diretamente na linha de comando via flag `-c` ou `--config` (ex: `graphit ast query --config ide=cursor`).
2. **Variáveis de Ambiente:**
   Variáveis prefixadas com a marca configurada (ex: `GRAPHIT_IDE`, `GRAPHIT_CLI`, `GRAPHIT_HUB_REPO`).
3. **Configurações do Projeto:**
   Definidas no arquivo `graphit.lock.json` presente na raiz do diretório de trabalho do projeto.
4. **Configurações Globais do Usuário:**
   Definidas no arquivo `~/.graphit/config.json`.
5. **Padrões Compilados (Defaults):**
   Valores definidos de fábrica no código fonte do utilitário.

---

## 📂 Arquivos de Configuração

### 1. Configuração Global (`~/.graphit/config.json`)
Armazena preferências de uso que se aplicam a todos os projetos abertos por aquele usuário.

**Exemplo de conteúdo:**
```json
{
  "ide": "cursor",
  "cli": "claude",
  "hub": {
    "repo": "git@github.com:graphit-labs/graphit-code.git"
  },
  "memory": {
    "repo": "git@github.com:my-org/my-memories.git"
  }
}
```

### 2. Lockfile do Projeto (`<project-root>/graphit.lock.json`)
Armazena a identidade do projeto local, versões e configurações específicas que são commitadas no controle de versão.

**Campos principais:**
* `version`: Versão do esquema do lockfile do projeto.
* `project`: Contém `id` (UUID único gerado no init), `name`, e `description`.
* `config`: Mapa de chaves locais (ex: `knowledge.docs_dir`, `modules.dream`).
* `artifacts`: Registro das ferramentas, regras, skills e MCPs importados do Hub central.

---

## 🔌 Integração de IDE e Fallbacks de CLI de IA

O Graphit Code se integra nativamente a ferramentas de desenvolvimento. A chave `ide` define qual IDE/Agente de IA está orquestrando a sessão, mapeando automaticamente qual CLI de IA deve ser usado para completar prompts quando chaves de API não estão diretamente configuradas no backend.

> [!IMPORTANT]
> **Raciocínio Fora do Ambiente Agente da IDE:**
> Ao rodar em modo integrado, o Graphit Code utiliza as IDEs e seus agentes internos. Quando há necessidade de realizar qualquer tipo de processamento ou raciocínio fora deste ambiente (ex: por comandos manuais no terminal), o framework invoca o próprio CLI do respectivo agente (como `claude` ou `cursor-agent`). 
> 
> Como o sistema delega as chamadas de IA diretamente para estes executáveis locais de fallback (que já possuem sua própria autenticação/login ativa), **não há necessidade de configurar chaves de API adicionais (API Keys)** no Graphit Code para utilizar o framework.

### Mapeamento IDE → CLI de IA:
* **antigravity** → `agy` (Antigravity SDK / CLI)
* **gemini** ou **gemini-code** → `gemini`
* **claude** ou **claude-code** → `claude` (Claude CLI)
* **cursor** → `cursor-agent`
* **codex** → `codex`
* **opencode** → `opencode`
* **kiro** → `kiro-cli`

### Lista de Executáveis de Fallback de IA Procurados no PATH:
Caso uma IA específica não seja especificada ou o CLI correspondente não seja encontrado, o sistema testa automaticamente os seguintes binários candidatos disponíveis no sistema do usuário:
1. `opencode`
2. `agy`
3. `gemini`
4. `claude`
5. `codex`
6. `grok`
7. `kiro-cli`
8. `cursor-agent`
9. `agent`

---
*Próximo passo recomendado:* Leia sobre a [[security_privacy]] do sistema.
