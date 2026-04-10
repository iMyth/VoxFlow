import { useState } from 'react';
import { Settings, Plus } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import ProjectList from '../project/ProjectList';
import SettingsDialog from '../settings/SettingsDialog';
import { Button } from '../ui/button';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '../ui/tooltip';
import ToastContainer from './ToastContainer';
import { useUpdateStore } from '../../store/updateStore';

interface ProjectListPageProps {
    onSelectProject: (projectId: string) => void;
}

export default function ProjectListPage({ onSelectProject }: ProjectListPageProps) {
    const { t } = useTranslation();
    const [settingsOpen, setSettingsOpen] = useState(false);
    const [showInput, setShowInput] = useState(false);
    const { updateAvailable, latestVersion, downloading, installUpdate } = useUpdateStore();

    return (
        <TooltipProvider>
            <div className="min-h-screen">
                <ToastContainer />
                {updateAvailable && (
                    <div className="bg-gradient-to-r from-blue-600 to-indigo-600 text-white px-4 py-2 flex items-center justify-between text-sm shrink-0">
                        <span>
                            {t('update.available')} <strong>{t('update.version', { version: latestVersion })}</strong>
                        </span>
                        <button
                            onClick={() => installUpdate()}
                            disabled={downloading}
                            className="ml-3 px-3 py-1 bg-white text-blue-700 rounded-md text-xs font-medium hover:bg-blue-50 disabled:opacity-50 transition-colors"
                        >
                            {downloading ? t('update.downloading') : t('update.installNow')}
                        </button>
                    </div>
                )}
                <div className="fixed top-4 right-4 z-10 flex items-center gap-1">
                    <Tooltip>
                        <TooltipTrigger asChild>
                            <Button
                                variant="ghost"
                                size="icon"
                                onClick={() => setShowInput(true)}
                                aria-label={t('app.newProject')}
                            >
                                <Plus className="h-5 w-5" />
                            </Button>
                        </TooltipTrigger>
                        <TooltipContent>{t('app.newProject')}</TooltipContent>
                    </Tooltip>
                    <Tooltip>
                        <TooltipTrigger asChild>
                            <Button
                                variant="ghost"
                                size="icon"
                                onClick={() => setSettingsOpen(true)}
                                aria-label={t('app.settings')}
                            >
                                <Settings className="h-5 w-5" />
                            </Button>
                        </TooltipTrigger>
                        <TooltipContent>{t('app.settings')}</TooltipContent>
                    </Tooltip>
                </div>
                <ProjectList
                    onSelectProject={onSelectProject}
                    showInput={showInput}
                    onShowInput={setShowInput}
                />
                {settingsOpen && <SettingsDialog onClose={() => setSettingsOpen(false)} />}
            </div>
        </TooltipProvider>
    );
}
