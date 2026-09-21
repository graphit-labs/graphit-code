import { MarkdownContent } from "@/components/wiki/WikiMarkdown";

export function stripRepeatedTitle(content: string, title?: string): string {
  if (!title) return content;
  const match = content.match(/^\s*#\s+(.+?)(?:\s+#+)?\s*\r?\n/);
  return match && match[1].trim() === title.trim() ? content.slice(match[0].length) : content;
}

export function MemoryMarkdown({ content, title }: { content: string; title?: string }) {
  return <MarkdownContent content={stripRepeatedTitle(content, title)} />;
}
