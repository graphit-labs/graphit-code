import {
  BookOpen,
  Bot,
  Code2,
  FileCode2,
  Layers,
  Scale,
  Server,
  Terminal,
  Wand2,
} from 'lucide-react'

export const TYPE_FILTERS = [
  { label: 'Any Type', value: 'all', icon: <Layers className="w-3.5 h-3.5" /> },
  { label: 'Knowledge', value: 'knowledge', icon: <BookOpen className="w-3.5 h-3.5" /> },
  { label: 'Skill', value: 'skill', icon: <Wand2 className="w-3.5 h-3.5" /> },
  { label: 'Agent', value: 'agent', icon: <Bot className="w-3.5 h-3.5" /> },
  { label: 'Rule', value: 'rule', icon: <Scale className="w-3.5 h-3.5" /> },
  { label: 'AST', value: 'ast', icon: <Code2 className="w-3.5 h-3.5" /> },
  { label: 'Command', value: 'command', icon: <Terminal className="w-3.5 h-3.5" /> },
  { label: 'MCP Server', value: 'mcp', icon: <Server className="w-3.5 h-3.5" /> },
  { label: 'Power', value: 'power', icon: <Layers className="w-3.5 h-3.5" /> },
  { label: 'Language', value: 'language', icon: <FileCode2 className="w-3.5 h-3.5" /> },
]
