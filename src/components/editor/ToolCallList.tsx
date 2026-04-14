import { ChevronDown, ChevronRight, FileText, Users, ScrollText, Database, Brain, Loader2, Check } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Card, CardContent } from '../ui/card';

import type { ToolCallEntry } from '../../store/scriptStore';

interface ToolCallListProps {
  entries: ToolCallEntry[];
}

export default function ToolCallList({ entries }: ToolCallListProps) {
  const { t } = useTranslation();

  return (
    <Card className="border-0 shadow-none bg-transparent">
      <CardContent className="p-0 space-y-2">
        {/* Header */}
        <div className="flex items-center gap-2 px-3 py-1.5">
          <div className="flex items-center gap-1.5">
            <div className="h-1.5 w-1.5 rounded-full bg-blue-500 animate-pulse" />
            <span className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
              {t('tools.label')}
            </span>
          </div>
          <span className="text-xs text-muted-foreground/60 tabular-nums">{entries.length}</span>
        </div>

        {/* Timeline entries */}
        <div className="relative ml-4 pl-3 border-l border-border/50">
          {entries.map((entry, i) => (
            <ToolCallItem key={entry.id} entry={entry} index={i} isLast={i === entries.length - 1} />
          ))}
        </div>
      </CardContent>
    </Card>
  );
}

function ToolCallItem({ entry, index, isLast: _isLast }: { entry: ToolCallEntry; index: number; isLast: boolean }) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);
  const toolKey = `tools.${entry.tool}` as const;
  const label = t(toolKey, entry.tool);
  const config = TOOL_CONFIGS[entry.tool] ?? TOOL_CONFIGS['default'];
  const isDone = entry.status === 'done';
  const hasArgs = Object.keys(entry.args).length > 0;

  return (
    <div className="relative group">
      {/* Timeline dot */}
      <div
        className={`absolute -left-[19px] top-3 h-2.5 w-2.5 rounded-full border-2 border-background transition-colors duration-200 ${
          isDone ? 'bg-green-500 border-green-500/30' : 'bg-amber-500 border-amber-500/30 animate-pulse'
        }`}
      />

      <div
        className={`flex flex-col gap-1.5 rounded-lg border transition-all duration-200 hover:shadow-sm ${config.border} ${config.bg} cursor-pointer mb-2`}
        onClick={() => {
          setExpanded(!expanded);
        }}
      >
        {/* Main row */}
        <div className="flex items-center gap-2 px-3 py-2">
          {/* Tool icon */}
          <div className={`shrink-0 p-1 rounded-md ${config.iconBg}`}>
            <config.icon className="h-3.5 w-3.5" />
          </div>

          {/* Label + status */}
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-1.5">
              <span className="text-xs font-medium text-foreground">{label}</span>
              <span className="text-[10px] text-muted-foreground/50 tabular-nums">#{index + 1}</span>
            </div>

            {/* Arguments preview */}
            {hasArgs && <p className="text-xs text-muted-foreground/70 truncate">{formatArgs(entry.args)}</p>}
          </div>

          {/* Status + expand indicator */}
          <div className="flex items-center gap-1.5 shrink-0">
            {isDone ? (
              <Check className="h-3.5 w-3.5 text-green-500" />
            ) : (
              <Loader2 className="h-3.5 w-3.5 text-amber-500 animate-spin" />
            )}
            {(hasArgs || entry.result) &&
              (expanded ? (
                <ChevronDown className="h-3 w-3 text-muted-foreground/50" />
              ) : (
                <ChevronRight className="h-3 w-3 text-muted-foreground/50" />
              ))}
          </div>
        </div>

        {/* Expanded details */}
        {expanded && (hasArgs || entry.result) && (
          <div className="border-t border-border/50">
            {hasArgs && (
              <div className="px-3 py-2">
                <div className="text-[10px] font-medium text-muted-foreground uppercase tracking-wider mb-1">
                  Arguments
                </div>
                <pre className="text-xs text-muted-foreground whitespace-pre-wrap bg-background/50 rounded-md p-2 overflow-x-auto">
                  {JSON.stringify(entry.args, null, 2)}
                </pre>
              </div>
            )}
            {entry.result && (
              <div className="px-3 py-2">
                <div className="text-[10px] font-medium text-muted-foreground uppercase tracking-wider mb-1">
                  Result
                </div>
                <pre className="text-xs text-muted-foreground whitespace-pre-wrap bg-background/50 rounded-md p-2 overflow-x-auto max-h-32 overflow-y-auto">
                  {entry.result}
                </pre>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

const TOOL_CONFIGS: Record<
  string,
  { border: string; bg: string; iconBg: string; icon: React.ComponentType<{ className?: string }> }
> = {
  outline_analysis: {
    border: 'border-blue-200/60 dark:border-blue-800/60',
    bg: 'bg-blue-50/50 dark:bg-blue-950/20',
    iconBg: 'bg-blue-100 dark:bg-blue-900/40 text-blue-600 dark:text-blue-400',
    icon: FileText,
  },
  character_extraction: {
    border: 'border-green-200/60 dark:border-green-800/60',
    bg: 'bg-green-50/50 dark:bg-green-950/20',
    iconBg: 'bg-green-100 dark:bg-green-900/40 text-green-600 dark:text-green-400',
    icon: Users,
  },
  script_generation: {
    border: 'border-purple-200/60 dark:border-purple-800/60',
    bg: 'bg-purple-50/50 dark:bg-purple-950/20',
    iconBg: 'bg-purple-100 dark:bg-purple-900/40 text-purple-600 dark:text-purple-400',
    icon: ScrollText,
  },
  story_recall: {
    border: 'border-amber-200/60 dark:border-amber-800/60',
    bg: 'bg-amber-50/50 dark:bg-amber-950/20',
    iconBg: 'bg-amber-100 dark:bg-amber-900/40 text-amber-600 dark:text-amber-400',
    icon: Database,
  },
  story_memory: {
    border: 'border-cyan-200/60 dark:border-cyan-800/60',
    bg: 'bg-cyan-50/50 dark:bg-cyan-950/20',
    iconBg: 'bg-cyan-100 dark:bg-cyan-900/40 text-cyan-600 dark:text-cyan-400',
    icon: Brain,
  },
  default: {
    border: 'border-gray-200/60 dark:border-gray-800/60',
    bg: 'bg-gray-50/50 dark:bg-gray-900/20',
    iconBg: 'bg-gray-100 dark:bg-gray-800/40 text-gray-600 dark:text-gray-400',
    icon: Brain,
  },
};

function formatArgs(args: Record<string, unknown>): string {
  const entries = Object.entries(args);
  if (entries.length === 0) return '';
  const [key, val] = entries[0];
  const text = String(val);
  return `${key}: ${text.length > 60 ? text.slice(0, 60) + '...' : text}`;
}
