import { Trash2, FolderOpen } from 'lucide-react';

import { Button } from '../ui/button';
import { Card, CardHeader, CardTitle, CardDescription, CardAction } from '../ui/card';

import type { Project } from '../../types';

interface ProjectCardProps {
  project: Project;
  onClick: () => void;
  onDelete: () => void;
}

export default function ProjectCard({ project, onClick, onDelete }: ProjectCardProps) {
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
