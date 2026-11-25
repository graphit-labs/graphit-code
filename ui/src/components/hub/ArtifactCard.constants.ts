import {
  BookOpen,
  Bot,
  Code2,
  FileCode2,
  Scale,
  Server,
  Terminal,
  Wand2,
  Zap,
} from 'lucide-react'
import type { LucideIcon } from 'lucide-react'

export const TYPE_ICONS: Record<string, LucideIcon> = {
  knowledge: BookOpen,
  skill: Zap,
  agent: Bot,
  rule: Scale,
  ast: Code2,
  command: Terminal,
  mcp: Server,
  power: Wand2,
  language: FileCode2,
}

export const TYPE_STYLES: Record<string, { bg: string; text: string; border: string; glow: string; gradient: string }> = {
  knowledge: {
    bg: 'bg-blue-500/10',
    text: 'text-blue-500 dark:text-blue-400',
    border: 'border-blue-500/20',
    glow: 'bg-blue-500',
    gradient: 'from-blue-500 to-indigo-500',
  },
  skill: {
    bg: 'bg-amber-500/10',
    text: 'text-amber-500 dark:text-amber-400',
    border: 'border-amber-500/20',
    glow: 'bg-amber-500',
    gradient: 'from-amber-500 to-orange-500',
  },
  agent: {
    bg: 'bg-purple-500/10',
    text: 'text-purple-500 dark:text-purple-400',
    border: 'border-purple-500/20',
    glow: 'bg-purple-500',
    gradient: 'from-purple-500 to-indigo-500',
  },
  rule: {
    bg: 'bg-emerald-500/10',
    text: 'text-emerald-500 dark:text-emerald-400',
    border: 'border-emerald-500/20',
    glow: 'bg-emerald-500',
    gradient: 'from-emerald-500 to-teal-500',
  },
  ast: {
    bg: 'bg-pink-500/10',
    text: 'text-pink-500 dark:text-pink-400',
    border: 'border-pink-500/20',
    glow: 'bg-pink-500',
    gradient: 'from-pink-500 to-rose-500',
  },
  command: {
    bg: 'bg-cyan-500/10',
    text: 'text-cyan-500 dark:text-cyan-400',
    border: 'border-cyan-500/20',
    glow: 'bg-cyan-500',
    gradient: 'from-cyan-500 to-blue-500',
  },
  mcp: {
    bg: 'bg-red-500/10',
    text: 'text-red-500 dark:text-red-400',
    border: 'border-red-500/20',
    glow: 'bg-red-500',
    gradient: 'from-red-500 to-rose-500',
  },
  power: {
    bg: 'bg-violet-500/10',
    text: 'text-violet-500 dark:text-violet-400',
    border: 'border-violet-500/20',
    glow: 'bg-violet-500',
    gradient: 'from-violet-500 to-fuchsia-500',
  },
  language: {
    bg: 'bg-lime-500/10',
    text: 'text-lime-500 dark:text-lime-400',
    border: 'border-lime-500/20',
    glow: 'bg-lime-500',
    gradient: 'from-lime-500 to-green-500',
  },
}
