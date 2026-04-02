import { Trash2, FolderOpen } from 'lucide-react';
import type { Project } from '../../types';

interface ProjectCardProps {
    project: Project;
    onClick: () => void;
    onDelete: () => void;
}

export default function ProjectCard({ project, onClick, onDelete }: ProjectCardProps) {
    return (
        <div
            className="group relative flex flex-col gap-2 rounded-xl border border-gray-200 bg-white p-5 shadow-sm transition hover:shadow-md cursor-pointer dark:border-gray-700 dark:bg-gray-800"
            onClick={onClick}
            role="button"
            tabIndex={0}
            onKeyDown={(e) => e.key === 'Enter' && onClick()}
        >
            <div className="flex items-center gap-3">
                <FolderOpen className="h-5 w-5 text-blue-500" />
                <h3 className="text-lg font-semibold truncate">{project.name}</h3>
            </div>
            <p className="text-sm text-gray-500 dark:text-gray-400">
                {new Date(project.created_at).toLocaleDateString()}
            </p>
            <button
                className="absolute top-3 right-3 opacity-0 group-hover:opacity-100 transition p-1.5 rounded-lg hover:bg-red-50 dark:hover:bg-red-900/30 text-gray-400 hover:text-red-500"
                onClick={(e) => {
                    e.stopPropagation();
                    onDelete();
                }}
                aria-label={`Delete project ${project.name}`}
            >
                <Trash2 className="h-4 w-4" />
            </button>
        </div>
    );
}
