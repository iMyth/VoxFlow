import {
  FileAudio,
  FileVideo,
  FolderOpen,
  HardDrive,
  Music,
  Trash2,
  FileCode,
  Download,
  CheckSquare,
  Square,
  RefreshCw,
} from 'lucide-react';
import { useEffect, useMemo, useCallback } from 'react';
import { useTranslation } from 'react-i18next';

import { useProjectStore } from '../../store/projectStore';
import { useResourceStore } from '../../store/resourceStore';
import { Button } from '../ui/button';
import { Card, CardContent } from '../ui/card';

type ResourceFilter = 'all' | 'audio' | 'video' | 'composition' | 'bgm' | 'export';

function formatFileSize(bytes: number): string {
  if (bytes === 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  return `${(bytes / Math.pow(1024, i)).toFixed(i > 0 ? 1 : 0)} ${units[i]}`;
}

function formatDuration(ms: number): string {
  const seconds = Math.floor(ms / 1000);
  const minutes = Math.floor(seconds / 60);
  const secs = seconds % 60;
  if (minutes > 0) {
    return `${minutes}:${secs.toString().padStart(2, '0')}`;
  }
  return `0:${secs.toString().padStart(2, '0')}`;
}

function ResourceTypeIcon({ type }: { type: string }) {
  switch (type) {
    case 'audio':
      return <FileAudio className="h-4 w-4 text-blue-500" />;
    case 'video':
      return <FileVideo className="h-4 w-4 text-purple-500" />;
    case 'composition':
      return <FileCode className="h-4 w-4 text-green-500" />;
    case 'bgm':
      return <Music className="h-4 w-4 text-orange-500" />;
    case 'export':
      return <Download className="h-4 w-4 text-emerald-500" />;
    default:
      return <HardDrive className="h-4 w-4 text-muted-foreground" />;
  }
}

export default function ResourcePanel() {
  const { t } = useTranslation();
  const { currentProject } = useProjectStore();
  const {
    resources,
    summary,
    loading,
    filter,
    selectedIds,
    fetchResources,
    fetchSummary,
    setFilter,
    toggleSelection,
    selectAll,
    clearSelection,
    deleteSelected,
    deleteSingle,
    openFolder,
  } = useResourceStore();

  const projectId = currentProject?.project.id;

  useEffect(() => {
    if (projectId) {
      void fetchResources(projectId);
      void fetchSummary(projectId);
    }
  }, [projectId, fetchResources, fetchSummary]);

  const filteredResources = useMemo(() => {
    if (filter === 'all') return resources;
    return resources.filter((r) => r.resource_type === filter);
  }, [resources, filter]);

  const handleDeleteSelected = useCallback(() => {
    if (!projectId || selectedIds.size === 0) return;
    void deleteSelected(projectId);
  }, [projectId, selectedIds, deleteSelected]);

  const handleDeleteSingle = useCallback(
    (filePath: string) => {
      if (!projectId) return;
      void deleteSingle(projectId, filePath);
    },
    [projectId, deleteSingle]
  );

  const handleOpenFolder = useCallback(
    (subfolder?: string) => {
      if (!projectId) return;
      void openFolder(projectId, subfolder);
    },
    [projectId, openFolder]
  );

  const handleRefresh = useCallback(() => {
    if (!projectId) return;
    void fetchResources(projectId);
    void fetchSummary(projectId);
  }, [projectId, fetchResources, fetchSummary]);

  const filters: { key: ResourceFilter; label: string; count: number }[] = useMemo(() => {
    const counts = { all: resources.length, audio: 0, video: 0, composition: 0, bgm: 0, export: 0 };
    for (const r of resources) {
      if (r.resource_type in counts) {
        counts[r.resource_type as keyof typeof counts]++;
      }
    }
    return [
      { key: 'all', label: t('resources.filter.all'), count: counts.all },
      { key: 'audio', label: t('resources.filter.audio'), count: counts.audio },
      { key: 'video', label: t('resources.filter.video'), count: counts.video },
      { key: 'bgm', label: t('resources.filter.bgm'), count: counts.bgm },
      { key: 'export', label: t('resources.filter.export'), count: counts.export },
      { key: 'composition', label: t('resources.filter.composition'), count: counts.composition },
    ];
  }, [resources, t]);

  if (!projectId) return null;

  return (
    <div className="flex flex-col h-full p-6 gap-4">
      {/* Summary Cards */}
      {summary && (
        <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
          <Card>
            <CardContent className="p-3">
              <div className="flex items-center gap-2">
                <HardDrive className="h-4 w-4 text-muted-foreground" />
                <div>
                  <p className="text-xs text-muted-foreground">{t('resources.summary.total')}</p>
                  <p className="text-sm font-medium">{formatFileSize(summary.total_size_bytes)}</p>
                </div>
              </div>
            </CardContent>
          </Card>
          <Card>
            <CardContent className="p-3">
              <div className="flex items-center gap-2">
                <FileAudio className="h-4 w-4 text-blue-500" />
                <div>
                  <p className="text-xs text-muted-foreground">{t('resources.summary.audio')}</p>
                  <p className="text-sm font-medium">
                    {summary.audio_count} {t('resources.summary.files')} · {formatFileSize(summary.audio_size_bytes)}
                  </p>
                </div>
              </div>
            </CardContent>
          </Card>
          <Card>
            <CardContent className="p-3">
              <div className="flex items-center gap-2">
                <FileVideo className="h-4 w-4 text-purple-500" />
                <div>
                  <p className="text-xs text-muted-foreground">{t('resources.summary.video')}</p>
                  <p className="text-sm font-medium">
                    {summary.video_count} {t('resources.summary.files')} · {formatFileSize(summary.video_size_bytes)}
                  </p>
                </div>
              </div>
            </CardContent>
          </Card>
          <Card>
            <CardContent className="p-3">
              <div className="flex items-center gap-2">
                <FolderOpen className="h-4 w-4 text-muted-foreground" />
                <div>
                  <p className="text-xs text-muted-foreground">{t('resources.summary.other')}</p>
                  <p className="text-sm font-medium">
                    {summary.other_count} {t('resources.summary.files')} · {formatFileSize(summary.other_size_bytes)}
                  </p>
                </div>
              </div>
            </CardContent>
          </Card>
        </div>
      )}

      {/* Toolbar */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-1 flex-wrap">
          {filters.map((f) => (
            <Button
              key={f.key}
              variant={filter === f.key ? 'default' : 'outline'}
              size="sm"
              onClick={() => {
                setFilter(f.key);
              }}
              className="text-xs"
            >
              {f.label}
              {f.count > 0 && <span className="ml-1 opacity-60">({f.count})</span>}
            </Button>
          ))}
        </div>

        <div className="flex items-center gap-2">
          <Button variant="outline" size="sm" onClick={handleRefresh} title={t('resources.refresh')}>
            <RefreshCw className={`h-3.5 w-3.5 ${loading ? 'animate-spin' : ''}`} />
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              handleOpenFolder();
            }}
          >
            <FolderOpen className="h-3.5 w-3.5 mr-1" />
            {t('resources.openFolder')}
          </Button>
          {selectedIds.size > 0 && (
            <Button variant="destructive" size="sm" onClick={handleDeleteSelected}>
              <Trash2 className="h-3.5 w-3.5 mr-1" />
              {t('resources.deleteSelected', { count: selectedIds.size })}
            </Button>
          )}
        </div>
      </div>

      {/* Select All / Clear */}
      {filteredResources.length > 0 && (
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <button onClick={selectAll} className="hover:text-foreground underline">
            {t('resources.selectAll')}
          </button>
          {selectedIds.size > 0 && (
            <button onClick={clearSelection} className="hover:text-foreground underline">
              {t('resources.clearSelection')}
            </button>
          )}
          <span className="ml-auto">
            {filteredResources.length} {t('resources.summary.files')}
          </span>
        </div>
      )}

      {/* Resource List */}
      <div className="flex-1 overflow-auto">
        {loading && resources.length === 0 ? (
          <div className="flex items-center justify-center h-32 text-muted-foreground">
            <RefreshCw className="h-4 w-4 animate-spin mr-2" />
            {t('resources.loading')}
          </div>
        ) : filteredResources.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-32 text-muted-foreground gap-2">
            <HardDrive className="h-8 w-8 opacity-40" />
            <p className="text-sm">{t('resources.empty')}</p>
          </div>
        ) : (
          <div className="space-y-1">
            {filteredResources.map((resource) => (
              <div
                key={resource.file_path}
                className="flex items-center gap-3 px-3 py-2 rounded-md hover:bg-accent/50 group transition-colors"
              >
                {/* Selection checkbox */}
                <button
                  onClick={() => {
                    toggleSelection(resource.file_path);
                  }}
                  className="shrink-0 text-muted-foreground hover:text-foreground"
                >
                  {selectedIds.has(resource.file_path) ? (
                    <CheckSquare className="h-4 w-4 text-primary" />
                  ) : (
                    <Square className="h-4 w-4" />
                  )}
                </button>

                {/* Icon */}
                <ResourceTypeIcon type={resource.resource_type} />

                {/* Name and info */}
                <div className="flex-1 min-w-0">
                  <p className="text-sm truncate">{resource.name}</p>
                  <div className="flex items-center gap-2 text-xs text-muted-foreground">
                    {resource.section_title && (
                      <span className="bg-accent px-1.5 py-0.5 rounded text-[10px]">{resource.section_title}</span>
                    )}
                    <span>{formatFileSize(resource.file_size)}</span>
                    {resource.duration_ms != null && <span>· {formatDuration(resource.duration_ms)}</span>}
                    <span>· {resource.created_at}</span>
                  </div>
                </div>

                {/* Delete button */}
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-7 w-7 opacity-0 group-hover:opacity-100 transition-opacity"
                  onClick={() => {
                    handleDeleteSingle(resource.file_path);
                  }}
                  title={t('resources.delete')}
                >
                  <Trash2 className="h-3.5 w-3.5 text-destructive" />
                </Button>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
