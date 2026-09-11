import ReactMarkdown, { defaultUrlTransform } from 'react-markdown'
import remarkGfm from 'remark-gfm'

export function MemoryMarkdown({ content }: { content: string }) {
  return (
    <div className="memory-prose prose prose-sm dark:prose-invert max-w-none">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        urlTransform={defaultUrlTransform}
        components={{
          h1: ({ children }) => <h1 className="mt-2 mb-4 border-b border-border/40 pb-2 text-2xl font-extrabold tracking-tight text-foreground">{children}</h1>,
          h2: ({ children }) => <h2 className="mt-7 mb-3 text-xl font-bold tracking-tight text-foreground">{children}</h2>,
          h3: ({ children }) => <h3 className="mt-6 mb-3 text-lg font-semibold text-foreground">{children}</h3>,
          p: ({ children }) => <p className="mb-4 text-sm leading-relaxed text-foreground/80">{children}</p>,
          a: ({ href, children }) => (
            <a href={href} target="_blank" rel="noopener noreferrer" className="font-semibold text-primary hover:underline">
              {children}
            </a>
          ),
          code: ({ children }) => <code className="rounded-md border border-border/20 bg-accent/40 px-1.5 py-0.5 font-mono text-xs text-primary">{children}</code>,
          pre: ({ children }) => <pre className="my-5 overflow-x-auto rounded-xl border border-border/40 bg-accent/25 p-4 text-xs leading-relaxed text-foreground/85">{children}</pre>,
          blockquote: ({ children }) => <blockquote className="my-5 rounded-r-xl border-l-4 border-primary bg-primary/5 py-2 pl-4 italic text-muted-foreground">{children}</blockquote>,
          ul: ({ children }) => <ul className="mb-4 list-disc space-y-1 pl-5 text-sm text-foreground/80">{children}</ul>,
          ol: ({ children }) => <ol className="mb-4 list-decimal space-y-1 pl-5 text-sm text-foreground/80">{children}</ol>,
          table: ({ children }) => <div className="my-5 overflow-x-auto rounded-xl border border-border/40"><table className="w-full border-collapse text-left text-xs">{children}</table></div>,
          th: ({ children }) => <th className="border-b border-border/40 bg-accent/20 px-3 py-2 font-bold">{children}</th>,
          td: ({ children }) => <td className="border-b border-border/20 px-3 py-2">{children}</td>,
          img: ({ src, alt }) => <img src={src} alt={alt ?? ''} className="my-5 max-w-full rounded-xl border border-border/40 shadow-sm" />,
        }}
      >
        {content}
      </ReactMarkdown>
    </div>
  )
}
