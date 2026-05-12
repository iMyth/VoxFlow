import { Trash2, FileText, Mic, Users, Download, ArrowRight } from 'lucide-react';
import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';

import { Button } from '../ui/button';

import type { Project } from '../../types';

interface ProjectStats {
  id: string;
  name: string;
  created_at: string;
  line_count: number;
  audio_count: number;
  character_count: number;
}

interface ProjectCardProps {
  project: Project;
  stats?: ProjectStats;
  onClick: () => void;
  onDelete: () => void;
  onExport: () => void;
}

function useProjectAccent(name: string) {
  return useMemo(() => {
    const accents = [
      { border: 'border-l-blue-500', text: 'text-blue-600 dark:text-blue-400', bg: 'bg-blue-50 dark:bg-blue-950/40' },
      {
        border: 'border-l-violet-500',
        text: 'text-violet-600 dark:text-violet-400',
        bg: 'bg-violet-50 dark:bg-violet-950/40',
      },
      { border: 'border-l-rose-500', text: 'text-rose-600 dark:text-rose-400', bg: 'bg-rose-50 dark:bg-rose-950/40' },
      {
        border: 'border-l-amber-500',
        text: 'text-amber-600 dark:text-amber-400',
        bg: 'bg-amber-50 dark:bg-amber-950/40',
      },
      {
        border: 'border-l-emerald-500',
        text: 'text-emerald-600 dark:text-emerald-400',
        bg: 'bg-emerald-50 dark:bg-emerald-950/40',
      },
      { border: 'border-l-cyan-500', text: 'text-cyan-600 dark:text-cyan-400', bg: 'bg-cyan-50 dark:bg-cyan-950/40' },
    ];
    let hash = 0;
    for (let i = 0; i < name.length; i++) {
      hash = name.charCodeAt(i) + ((hash << 5) - hash);
    }
    return accents[Math.abs(hash) % accents.length];
  }, [name]);
}

export default function ProjectCard({ project, stats, onClick, onDelete, onExport }: ProjectCardProps) {
  const { t } = useTranslation();
  const hasStats = stats && (stats.line_count > 0 || stats.audio_count > 0 || stats.character_count > 0);
  const accent = useProjectAccent(project.name);
  const initial = project.name.charAt(0).toUpperCase();

  return (
    <div
      className={`group relative flex items-start gap-4 rounded-xl border border-border/60 border-l-[3px] ${accent.border} bg-card p-4 cursor-pointer transition-all duration-200 hover:shadow-lg hover:shadow-black/5 hover:-translate-y-0.5 hover:border-border dark:hover:shadow-black/20`}
      onClick={onClick}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === 'Enter') onClick();
      }}
    >
      {/* Avatar */}
      <div
        className={`flex h-9 w-9 shrink-0 items-center justify-center rounded-lg ${accent.bg} text-sm font-bold ${accent.text}`}
      >
        {initial}
      </div>

      {/* Content */}
      <div className="flex-1 min-w-0">
        <div className="flex items-start justify-between gap-2">
          <div className="min-w-0 flex-1">
            <h3 className="font-semibold text-sm leading-tight truncate" title={project.name}>
              {project.name}
            </h3>
            <p className="text-xs text-muted-foreground mt-0.5">
              {new Date(project.created_at).toLocaleDateString(undefined, {
                year: 'numeric',
                month: 'short',
                day: 'numeric',
              })}
            </p>
          </div>

          {/* Actions */}
          <div className="flex gap-0.5 shrink-0 opacity-0 group-hover:opacity-100 transition-opacity">
            <Button
              variant="ghost"
              size="icon-sm"
              className="h-7 w-7"
              onClick={(e) => {
                e.stopPropagation();
                onExport();
              }}
              aria-label={`Export script for ${project.name}`}
            >
              <Download className="h-3.5 w-3.5" />
            </Button>
            <Button
              variant="ghost"
              size="icon-sm"
              className="h-7 w-7 hover:text-destructive"
              onClick={(e) => {
                e.stopPropagation();
                onDelete();
              }}
              aria-label={`Delete project ${project.name}`}
            >
              <Trash2 className="h-3.5 w-3.5" />
            </Button>
          </div>
        </div>

        {/* Stats + hover hint */}
        <div className="flex items-center justify-between mt-2.5">
          {hasStats ? (
            <div className="flex items-center gap-3 text-xs text-muted-foreground">
              {stats.line_count > 0 && (
                <span className="inline-flex items-center gap-1">
                  <FileText className="h-3 w-3" />
                  {stats.line_count} {t('project.statsLines')}
                </span>
              )}
              {stats.audio_count > 0 && (
                <span className="inline-flex items-center gap-1">
                  <Mic className="h-3 w-3" />
                  {stats.audio_count} {t('project.statsAudio')}
                </span>
              )}
              {stats.character_count > 0 && (
                <span className="inline-flex items-center gap-1">
                  <Users className="h-3 w-3" />
                  {stats.character_count} {t('project.statsCharacters')}
                </span>
              )}
            </div>
          ) : (
            <div />
          )}
          <span className="inline-flex items-center gap-0.5 text-xs text-muted-foreground opacity-0 group-hover:opacity-100 transition-opacity">
            {t('project.openProject')}
            <ArrowRight className="h-3 w-3" />
          </span>
        </div>
      </div>
    </div>
  );
}
