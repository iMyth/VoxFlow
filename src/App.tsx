import { useEffect, useState } from 'react';
import { Settings, Plus, X, AlertTriangle, CheckCircle, Info } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useProjectStore } from './store/projectStore';
import { useCharacterStore } from './store/characterStore';
import { useScriptStore } from './store/scriptStore';
import { useToastStore } from './store/toastStore';
import ProjectList from './components/project/ProjectList';
import CharacterPanel from './components/character/CharacterPanel';
import ScriptEditor from './components/editor/ScriptEditor';
import SettingsDialog from './components/settings/SettingsDialog';
import ExportPanel from './components/editor/ExportPanel';
import { Button } from './components/ui/button';
import { Tabs, TabsList, TabsTrigger } from './components/ui/tabs';
import {
    Tooltip,
    TooltipContent,
    TooltipProvider,
    TooltipTrigger,
} from './components/ui/tooltip';
import './App.css';

type Tab = 'editor' | 'characters' | 'export';

const ToastContainer = () => {
    const { toasts, removeToast } = useToastStore();

    if (toasts.length === 0) return null;

    const iconMap = {
        error: <AlertTriangle className="h-4 w-4 shrink-0 text-red-500" />,
        success: <CheckCircle className="h-4 w-4 shrink-0 text-green-500" />,
        info: <Info className="h-4 w-4 shrink-0 text-blue-500" />,
    };

    const bgMap = {
        error: 'bg-red-50 border-red-200 text-red-800 dark:bg-red-900/20 dark:border-red-800 dark:text-red-200',
        success: 'bg-green-50 border-green-200 text-green-800 dark:bg-green-900/20 dark:border-green-800 dark:text-green-200',
        info: 'bg-blue-50 border-blue-200 text-blue-800 dark:bg-blue-900/20 dark:border-blue-800 dark:text-blue-200',
    };

    return (
        <div className="fixed top-4 right-4 z-50 flex flex-col gap-2 max-w-sm" style={{ top: '1rem' }}>
            {toasts.map((toast) => (
                <div
                    key={toast.id}
                    className={`flex items-center gap-2 rounded-lg border px-3 py-2 shadow-md ${bgMap[toast.type]}`}
                >
                    {iconMap[toast.type]}
                    <span className="flex-1 text-sm">{toast.message}</span>
                    <button onClick={() => removeToast(toast.id)} className="shrink-0 opacity-60 hover:opacity-100">
                        <X className="h-3 w-3" />
                    </button>
                </div>
            ))}
        </div>
    );
};

function App() {
    const { t } = useTranslation();
    const { currentProject, loadProject } = useProjectStore();
    const [settingsOpen, setSettingsOpen] = useState(false);
    const [activeTab, setActiveTab] = useState<Tab>('editor');
    const [showNewProject, setShowNewProject] = useState(false);
    const { isDirty } = useScriptStore();

    const tabLabels: Record<Tab, string> = {
        editor: t('app.tab.editor'),
        characters: t('app.tab.characters'),
        export: t('app.tab.export'),
    };

    const handleSelectProject = async (projectId: string) => {
        await loadProject(projectId);
    };

    const handleBack = async () => {
        if (isDirty) {
            await useScriptStore.getState().saveScript();
        }
        useProjectStore.setState({ currentProject: null });
        useCharacterStore.setState({ characters: [] });
        useScriptStore.setState({ lines: [], isDirty: false, streamingText: '' });
        setActiveTab('editor');
    };

    const projectId = currentProject?.project.id;
    useEffect(() => {
        if (currentProject && projectId) {
            useCharacterStore.getState().fetchCharacters();
            useScriptStore.setState({
                lines: currentProject.script_lines,
                isDirty: false,
            });
        }
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [projectId]);

    if (!currentProject) {
        return (
            <TooltipProvider>
                <div className="min-h-screen">
                    <ToastContainer />
                    <div className="fixed top-4 right-4 z-10 flex items-center gap-1">
                        <Tooltip>
                            <TooltipTrigger asChild>
                                <Button
                                    variant="ghost"
                                    size="icon"
                                    onClick={() => setShowNewProject(true)}
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
                        onSelectProject={handleSelectProject}
                        showInput={showNewProject}
                        onShowInput={setShowNewProject}
                    />
                    {settingsOpen && <SettingsDialog onClose={() => setSettingsOpen(false)} />}
                </div>
            </TooltipProvider>
        );
    }

    return (
        <TooltipProvider>
            <div className="min-h-screen flex flex-col">
                <ToastContainer />
                <header className="flex items-center justify-between border-b border-gray-200 dark:border-gray-700 px-6 py-3">
                    <div className="flex items-center gap-4">
                        <Button variant="link" size="sm" onClick={handleBack}>
                            {t('app.backToProjects')}
                        </Button>
                        <h1 className="text-lg font-semibold">{currentProject.project.name}</h1>
                    </div>
                    <div className="flex items-center gap-2">
                        <Tabs value={activeTab} onValueChange={(v) => setActiveTab(v as Tab)}>
                            <TabsList>
                                {(Object.keys(tabLabels) as Tab[]).map((tab) => (
                                    <TabsTrigger key={tab} value={tab}>
                                        {tabLabels[tab]}
                                    </TabsTrigger>
                                ))}
                            </TabsList>
                        </Tabs>
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
                </header>

                <main className="flex-1 overflow-auto">
                    {activeTab === 'editor' && <ScriptEditor />}
                    {activeTab === 'characters' && <CharacterPanel />}
                    {activeTab === 'export' && <ExportPanel />}
                </main>

                {settingsOpen && <SettingsDialog onClose={() => setSettingsOpen(false)} />}
            </div>
        </TooltipProvider>
    );
}

export default App;
