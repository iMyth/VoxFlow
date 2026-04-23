import { save } from '@tauri-apps/plugin-dialog';
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
    <div className="px-6 py-10">
      <div className="flex items-center justify-between mb-8">
        <h1 className="text-2xl font-bold">{t('project.title')}</h1>
      </div>

      {showInput && (
        <div className="mb-6 flex gap-3">
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
        <p className="text-center text-muted-foreground py-20">{t('project.empty')}</p>
      ) : (
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
      )}

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
