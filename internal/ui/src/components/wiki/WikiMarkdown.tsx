import { useState, useMemo, isValidElement } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter'
import { oneLight, oneDark } from 'react-syntax-highlighter/dist/esm/styles/prism'
import { useTheme } from '@/hooks/useTheme'
import { ExternalLink, Copy, Check } from 'lucide-react'

import { wikiLinkFriendlyName } from '@/lib/utils'

function plantumlEncode(text: string): string {
  const hex = Array.from(new TextEncoder().encode(text))
    .map(b => b.toString(16).padStart(2, '0')).join('')
  return '~h' + hex
}

function preprocessWikiLinks(md: string): string {
  let processed = md

  processed = processed.replace(/`\[([^\]]+)\]\/\[\[([^\]]+)\]\]`/g, (_, module, target) => {
    const friendly = wikiLinkFriendlyName(target)
    return `[${friendly}](wiki://${encodeURIComponent(module)}/${encodeURIComponent(target)})`
  })

  processed = processed.replace(/\[([^\]]+)\]\/\[\[([^\]]+)\]\]/g, (_, module, target) => {
    const friendly = wikiLinkFriendlyName(target)
    return `[${friendly}](wiki://${encodeURIComponent(module)}/${encodeURIComponent(target)})`
  })

  processed = processed.replace(/`\[\[([^\]]+)\]\]`/g, (_, target) => {
    const friendly = wikiLinkFriendlyName(target)
    return `[${friendly}](wiki://${encodeURIComponent(target)})`
  })

  processed = processed.replace(/\[\[(.+?)\]\]/g, (_, target) => {
    const friendly = wikiLinkFriendlyName(target)
    return `[${friendly}](wiki://${encodeURIComponent(target)})`
  })

  processed = processed.replace(/`(\[[^\]]+\]\(wiki:\/\/[^)]+\))`/g, '$1')

  return processed
}


function wikiPageTarget(href: string): string | null {
  let raw = href
  if (raw.startsWith('wiki://')) raw = raw.slice(7)
  else if (/^[a-z][a-z0-9+.-]*:/i.test(raw) || raw.startsWith('#') || raw.startsWith('//')) return null

  let target: string
  try {
    target = decodeURIComponent(raw)
  } catch {
    target = raw
  }
  const hash = target.search(/[#?]/)
  if (hash >= 0) target = target.slice(0, hash)
  target = target.replace(/^\//, '').trim()
  if (!target) return null
  if (/[/\\]/.test(target)) return null

  const dot = target.lastIndexOf('.')
  if (dot > 0) {
    const ext = target.slice(dot).toLowerCase()
    if (ext === '.md') return target.slice(0, dot)

    if (/^\.[a-z0-9]{1,5}$/.test(ext) && ext !== '.md') return null
  }
  return target
}

function CodeCopyButton({ code }: { code: string }) {
  const [copied, setCopied] = useState(false)
  const handleCopy = () => {
    navigator.clipboard.writeText(code)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }
  return (
    <button
      onClick={handleCopy}
      className="p-1 rounded bg-muted/60 hover:bg-accent/40 text-muted-foreground hover:text-foreground transition-all flex items-center gap-1 text-[10px] font-semibold"
    >
      {copied ? (
        <>
          <Check className="w-3.5 h-3.5 text-emerald-500" />
          Copied!
        </>
      ) : (
        <>
          <Copy className="w-3.5 h-3.5" />
          Copy
        </>
      )}
    </button>
  )
}

export interface WikiMarkdownProps {
  content: string
  onLink?: (page: string) => void
}

export function WikiMarkdown({ content, onLink }: WikiMarkdownProps) {
  const processed = useMemo(() => preprocessWikiLinks(content), [content])
  const { theme } = useTheme()

  return (
    <div className="wiki-prose prose prose-sm dark:prose-invert max-w-none">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        urlTransform={(url) => url}
        components={{
          h1: ({ children }) => (
            <h1 className="text-2xl font-heading font-extrabold tracking-tight text-foreground mt-8 mb-4 pb-2 border-b border-border/40 leading-snug">
              {children}
            </h1>
          ),
          h2: ({ children }) => (
            <h2 className="text-xl font-heading font-bold tracking-tight text-foreground mt-7 mb-3.5 leading-snug">
              {children}
            </h2>
          ),
          h3: ({ children }) => (
            <h3 className="text-lg font-heading font-semibold tracking-tight text-foreground mt-6 mb-3 leading-snug">
              {children}
            </h3>
          ),
          h4: ({ children }) => (
            <h4 className="text-base font-heading font-medium tracking-tight text-foreground mt-5 mb-2 leading-snug">
              {children}
            </h4>
          ),
          p: ({ children }) => (
            <p className="text-[14px] text-foreground/80 leading-relaxed mb-4">
              {children}
            </p>
          ),
          a: ({ href, children }) => {
            if (href) {
              const page = wikiPageTarget(href)
              if (page !== null) {
                return (
                  <button
                    onClick={() => onLink && onLink(page)}
                    className={onLink ? "text-primary hover:text-primary/80 hover:underline underline-offset-2 font-semibold transition-colors" : "text-foreground font-semibold cursor-default"}
                    type="button"
                  >
                    {children}
                  </button>
                )
              }

              if (href.startsWith('#')) {
                return (
                  <a href={href} className="text-primary hover:underline underline-offset-2">
                    {children}
                  </a>
                )
              }
              const isExternal = /^[a-z][a-z0-9+.-]*:\/\//i.test(href) || href.startsWith('mailto:') || href.startsWith('tel:')
              if (!isExternal) {

                return (
                  <span
                    title={`Source path: ${href}`}
                    className="text-muted-foreground/90 font-mono text-[12px] underline decoration-dotted decoration-muted-foreground/40 underline-offset-2"
                  >
                    {children}
                  </span>
                )
              }
            }
            return (
              <a
                href={href}
                target="_blank"
                rel="noopener noreferrer"
                className="text-primary hover:underline inline-flex items-center gap-0.5"
              >
                {children}
                <ExternalLink className="w-3 h-3 inline shrink-0 opacity-60" />
              </a>
            )
          },
          code: ({ children }) => {
            return (
              <code className="bg-accent/40 text-primary border border-border/20 px-1.5 py-0.5 rounded-md text-[12px] font-mono">
                {children}
              </code>
            )
          },
          pre: ({ children }) => {
            const codeEl = (isValidElement(children)
              ? (children.props as { className?: string; children?: unknown })
              : {}) as { className?: string; children?: unknown }
            const className = codeEl.className || ''
            const match = /language-(\w+)/.exec(className)
            const lang = match?.[1] ?? ''
            const code = String(codeEl.children ?? '').replace(/\n$/, '')

            if (lang === 'plantuml' || lang === 'puml') {
              const encoded = plantumlEncode(code)
              const src = `https://www.plantuml.com/plantuml/svg/${encoded}`
              return (
                <div className="my-6 flex justify-center not-prose">
                  <img
                    src={src}
                    alt="PlantUML diagram"
                    className="max-w-full rounded-2xl border border-border/40 shadow-sm bg-white p-3 cursor-zoom-in hover:shadow-md transition-all hover:scale-[1.005]"
                    onClick={() => document.dispatchEvent(new CustomEvent('wiki-lightbox', { detail: src }))}
                  />
                </div>
              )
            }

            if (lang === 'mermaid') {
              const encoded = btoa(unescape(encodeURIComponent(code)))
              const src = `https://mermaid.ink/svg/${encoded}`
              return (
                <div className="my-6 flex justify-center not-prose">
                  <img
                    src={src}
                    alt="Mermaid diagram"
                    className="max-w-full rounded-2xl border border-border/40 shadow-sm bg-white p-3 cursor-zoom-in hover:shadow-md transition-all hover:scale-[1.005]"
                    onClick={() => document.dispatchEvent(new CustomEvent('wiki-lightbox', { detail: src }))}
                  />
                </div>
              )
            }

            return (
              <div className="my-6 rounded-2xl overflow-hidden border border-border/40 shadow-sm bg-card/40 backdrop-blur-sm group relative not-prose">
                <div className="bg-accent/35 px-4 py-2 text-[10px] text-muted-foreground font-mono border-b border-border/30 flex items-center justify-between">
                  <span className="uppercase tracking-widest font-bold text-muted-foreground/75">{lang || 'text'}</span>
                  <CodeCopyButton code={code} />
                </div>
                {lang ? (
                  <SyntaxHighlighter
                    language={lang}
                    style={theme === 'dark' ? oneDark : oneLight}
                    customStyle={{
                      margin: 0,
                      padding: '1.25rem',
                      background: 'transparent',
                      fontSize: '13px',
                      lineHeight: '1.6',
                      fontFamily: 'ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace'
                    }}
                  >
                    {code}
                  </SyntaxHighlighter>
                ) : (
                  <pre className="p-4 text-[13px] font-mono overflow-x-auto text-foreground/80 leading-relaxed whitespace-pre bg-transparent">
                    <code>{code}</code>
                  </pre>
                )}
              </div>
            )
          },
          table: ({ children }) => (
            <div className="my-6 overflow-x-auto rounded-2xl border border-border/40 shadow-sm bg-card/25 backdrop-blur-sm">
              <table className="w-full text-xs text-left border-collapse">{children}</table>
            </div>
          ),
          thead: ({ children }) => <thead className="border-b border-border/40 bg-accent/20">{children}</thead>,
          tbody: ({ children }) => <tbody className="divide-y divide-border/20">{children}</tbody>,
          tr: ({ children }) => <tr className="hover:bg-accent/15 transition-colors">{children}</tr>,
          th: ({ children }) => (
            <th className="px-4 py-3 text-xs font-bold text-muted-foreground uppercase tracking-wider">
              {children}
            </th>
          ),
          td: ({ children }) => <td className="px-4 py-2.5 text-foreground/85 font-medium">{children}</td>,
          blockquote: ({ children }) => (
            <blockquote className="border-l-4 border-primary pl-4 py-2.5 my-5 text-[14px] text-muted-foreground/90 italic bg-primary/5 rounded-r-xl border-dashed">
              {children}
            </blockquote>
          ),
          ul: ({ children }) => <ul className="pl-5 space-y-1 mb-4 text-[14px] text-foreground/80 list-disc">{children}</ul>,
          ol: ({ children }) => <ol className="pl-5 space-y-1 mb-4 text-[14px] text-foreground/80 list-decimal">{children}</ol>,
          li: ({ children }) => <li className="leading-relaxed pl-0.5">{children}</li>,
          hr: () => <hr className="border-border/40 my-6" />,
          img: ({ src, alt }) => (
            <div className="my-6 flex justify-center">
              <img
                src={src}
                alt={alt || ''}
                className="max-w-full rounded-2xl border border-border/40 shadow-sm cursor-zoom-in hover:shadow-md transition-all hover:scale-[1.005]"
                onClick={() => src && document.dispatchEvent(new CustomEvent('wiki-lightbox', { detail: src }))}
              />
            </div>
          ),
          strong: ({ children }) => <strong className="font-bold text-foreground">{children}</strong>,
          em: ({ children }) => <em className="italic text-foreground/90">{children}</em>,
          del: ({ children }) => <del className="line-through text-muted-foreground/60">{children}</del>,
        }}
      >
        {processed}
      </ReactMarkdown>
    </div>
  )
}
