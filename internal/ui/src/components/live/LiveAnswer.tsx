import { WikiMarkdown } from '@/components/wiki/WikiMarkdown';

export function LiveAnswer({ content, onLink }: { content: string; onLink: (target: string) => void }) {
  return <WikiMarkdown content={content} onLink={onLink} compactWikiLinks />;
}
