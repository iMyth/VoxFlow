import { Trash2, FolderOpen, FileText, Mic, Users } from 'lucide-react';

import { Button } from '../ui/button';
import { Card, CardHeader, CardTitle, CardDescription, CardAction } from '../ui/card';

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
}

export default function ProjectCard({ project, stats, onClick, onDelete }: ProjectCardProps) {
  return (
    <Card
      className="group cursor-pointer transition hover:shadow-md"
      onClick={onClick}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === 'Enter') onClick();
      }}
    >
      <CardHeader>
        <CardTitle className="flex items-center gap-3">
          <FolderOpen className="h-5 w-5 text-blue-500 shrink-0" />
          <span className="truncate">{project.name}</span>
        </CardTitle>
        <CardDescription>{new Date(project.created_at).toLocaleDateString()}</CardDescription>
        {stats && (stats.line_count > 0 || stats.audio_count > 0 || stats.character_count > 0) && (
          <div className="flex items-center gap-3 text-xs text-muted-foreground mt-1">
            {stats.line_count > 0 && (
              <span className="flex items-center gap-1">
                <FileText className="h-3 w-3" />
                {stats.line_count}
              </span>
            )}
            {stats.audio_count > 0 && (
              <span className="flex items-center gap-1">
                <Mic className="h-3 w-3" />
                {stats.audio_count}
              </span>
            )}
            {stats.character_count > 0 && (
              <span className="flex items-center gap-1">
                <Users className="h-3 w-3" />
                {stats.character_count}
              </span>
            )}
          </div>
        )}
        <CardAction>
          <Button
            variant="ghost"
            size="icon-sm"
            className="opacity-0 group-hover:opacity-100 transition hover:text-destructive"
            onClick={(e) => {
              e.stopPropagation();
              onDelete();
            }}
            aria-label={`Delete project ${project.name}`}
          >
            <Trash2 className="h-4 w-4" />
          </Button>
        </CardAction>
      </CardHeader>
    </Card>
  );
}
