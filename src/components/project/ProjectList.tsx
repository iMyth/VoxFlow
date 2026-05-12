import { save } from '@tauri-apps/plugin-dialog';
import { Mic, Plus, Sparkles } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import ProjectCard from './ProjectCard';
import * as ipc from '../../lib/ipc';
import { useProjectStore } from '../../store/projectStore';
import { Button } from '../ui/button';
import ConfirmDialog from '../ui/confirm-dialog';
import { Input } from '../ui/input';

interface ProjectListProps {
  onSelectProject: (projectId: string) => void;
  showInput: boolean;
  onShowInput: (show: boolean) => void;
}

export default function ProjectList({ onSelectProject, showInput, onShowInput }: ProjectListProps) {
  const { t } = useTranslation();
  const { projects, fetchProjects, createProject, deleteProject, fetchProjectStats, projectStats } = useProjectStore();
  const [newName, setNewName] = useState('');
  const [deleteId, setDeleteId] = useState<string | null>(null);

  useEffect(() => {
    void fetchProjects();
    void fetchProjectStats();
  }, [fetchProjects, fetchProjectStats]);

  const handleCreate = async () => {
    const name = newName.trim();
    if (!name) return;
    await createProject(name);
    setNewName('');
    onShowInput(false);
  };

  const handleDelete = async () => {
    if (!deleteId) return;
    await deleteProject(deleteId);
    setDeleteId(null);
  };

  const handleExport = async (projectId: string, projectName: string) => {
    const selectedPath = await save({
      title: t('project.exportScriptTitle'),
      defaultPath: `${projectName}.txt`,
      filters: [{ name: 'Text File', extensions: ['txt'] }],
    });
    if (!selectedPath) return;

    try {
      await ipc.exportScriptText(projectId, selectedPath);
    } catch (e) {
      if (String(e).includes('No script lines found')) {
        alert(t('project.noScriptToExport'));
      } else {
        alert(`${t('project.exportScriptFailed')}: ${e}`);
      }
    }
  };

  return (
    <div className="min-h-screen flex flex-col">
      {/* Hero Section */}
      <header className="relative overflow-hidden border-b border-border/40 bg-gradient-to-b from-muted/50 to-background">
        <div className="absolute inset-0 bg-[radial-gradient(ellipse_at_top,_var(--tw-gradient-stops))] from-blue-100/20 via-transparent to-transparent dark:from-blue-900/10" />
        <div className="relative mx-auto max-w-5xl px-6 pt-16 pb-10">
          <div className="flex items-center gap-3 mb-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-gradient-to-br from-blue-500 to-indigo-600 shadow-lg shadow-blue-500/20">
              <Mic className="h-5 w-5 text-white" />
            </div>
            <h1 className="text-2xl font-bold tracking-tight">VoxFlow</h1>
          </div>
          <p className="text-muted-foreground text-sm max-w-md">{t('project.heroDescription')}</p>
        </div>
      </header>

      {/* Main Content */}
      <main className="flex-1 mx-auto w-full max-w-5xl px-6 py-8">
        {/* Create Input */}
        {showInput && (
          <div className="mb-8 flex gap-3 animate-in fade-in slide-in-from-top-2 duration-200">
            <Input
              className="flex-1"
              placeholder={t('project.inputPlaceholder')}
              value={newName}
              onChange={(e) => {
                setNewName(e.target.value);
              }}
              onKeyDown={(e) => {
                if (e.key === 'Enter') void handleCreate();
              }}
              autoFocus
            />
            <Button onClick={() => void handleCreate()}>{t('project.create')}</Button>
            <Button
              variant="outline"
              onClick={() => {
                onShowInput(false);
                setNewName('');
              }}
            >
              {t('project.cancel')}
            </Button>
          </div>
        )}

        {projects.length === 0 ? (
          /* Empty State */
          <div className="flex flex-col items-center justify-center py-24 text-center">
            <div className="mb-6 flex h-20 w-20 items-center justify-center rounded-full bg-muted">
              <Sparkles className="h-8 w-8 text-muted-foreground/60" />
            </div>
            <h2 className="text-lg font-medium mb-2">{t('project.emptyTitle')}</h2>
            <p className="text-sm text-muted-foreground mb-6 max-w-sm">{t('project.emptyDescription')}</p>
            <Button
              onClick={() => {
                onShowInput(true);
              }}
              className="gap-2"
            >
              <Plus className="h-4 w-4" />
              {t('app.newProject')}
            </Button>
          </div>
        ) : (
          /* Project Grid */
          <div>
            <div className="flex items-center justify-between mb-5">
              <h2 className="text-sm font-medium text-muted-foreground">
                {t('project.projectCount', { count: projects.length })}
              </h2>
            </div>
            <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
              {projects.map((p) => (
                <ProjectCard
                  key={p.id}
                  project={p}
                  stats={projectStats[p.id]}
                  onClick={() => {
                    onSelectProject(p.id);
                  }}
                  onDelete={() => {
                    setDeleteId(p.id);
                  }}
                  onExport={() => {
                    void handleExport(p.id, p.name);
                  }}
                />
              ))}
            </div>
          </div>
        )}
      </main>

      <ConfirmDialog
        open={!!deleteId}
        onOpenChange={(open) => {
          if (!open) setDeleteId(null);
        }}
        title={t('project.deleteConfirmTitle')}
        description={t('project.confirmDelete')}
        confirmText={t('project.delete')}
        cancelText={t('project.cancel')}
        onConfirm={() => void handleDelete()}
      />
    </div>
  );
}
