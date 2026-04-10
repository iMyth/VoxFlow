import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useProjectStore } from './store/projectStore';
import { useCharacterStore } from './store/characterStore';
import { useScriptStore } from './store/scriptStore';
import { useSettingsStore } from './store/settingsStore';
import { useUpdateStore } from './store/updateStore';
import CharacterPanel from './components/character/CharacterPanel';
import ScriptEditor from './components/editor/ScriptEditor';
import ExportPanel from './components/editor/ExportPanel';
import SettingsDialog from './components/settings/SettingsDialog';
import AppHeader from './components/layout/AppHeader';
import ProjectListPage from './components/layout/ProjectListPage';
import ToastContainer from './components/layout/ToastContainer';
import './App.css';

type Tab = 'editor' | 'characters' | 'export';

function App() {
    const { t } = useTranslation();
    const { currentProject, loadProject } = useProjectStore();
    const [settingsOpen, setSettingsOpen] = useState(false);
    const [activeTab, setActiveTab] = useState<Tab>('editor');
    const { isDirty } = useScriptStore();
    const { updateAvailable, latestVersion, downloading, checkForUpdates, installUpdate } = useUpdateStore();

    // Load settings on app startup to restore persisted preferences
    useEffect(() => {
        useSettingsStore.getState().loadSettings();
    }, []);

    // Check for updates on app startup (delayed to not block initial render)
    useEffect(() => {
        const timer = setTimeout(() => checkForUpdates(), 3000);
        return () => clearTimeout(timer);
    }, []); // eslint-disable-line react-hooks/exhaustive-deps

    // Sync enableThinking from settings to scriptStore when settings load
    const settingsEnableThinking = useSettingsStore((s) => s.enableThinking);
    useEffect(() => {
        useScriptStore.getState().setEnableThinking(settingsEnableThinking);
    }, [settingsEnableThinking]);

    const handleSelectProject = async (projectId: string) => {
        await loadProject(projectId);
    };

    const handleBack = async () => {
        if (isDirty) {
            await useScriptStore.getState().saveScript();
        }
        useProjectStore.setState({ currentProject: null });
        useCharacterStore.setState({ characters: [] });
        useScriptStore.setState({
            lines: [],
            sections: [],
            isDirty: false,
            streamingText: '',
            thinkingText: '',
            agentPlan: null,
            workflow: null,
            isGenerating: false,
            isAnalyzing: false,
        });
        setActiveTab('editor');
    };

    // Load characters and script lines when project changes
    const projectId = currentProject?.project.id;
    useEffect(() => {
        if (currentProject && projectId) {
            useCharacterStore.getState().fetchCharacters();
            useScriptStore.setState({
                lines: currentProject.script_lines,
                sections: currentProject.sections ?? [],
                isDirty: false,
            });
        }
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [projectId]);

    if (!currentProject) {
        return <ProjectListPage onSelectProject={handleSelectProject} />;
    }

    return (
        <div className="min-h-screen flex flex-col">
            <ToastContainer />

            {/* Update Notification Banner */}
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

            <AppHeader
                project={currentProject}
                activeTab={activeTab}
                onTabChange={setActiveTab}
                onBack={handleBack}
                onSettings={() => setSettingsOpen(true)}
            />

            <main className="flex-1 overflow-auto">
                {activeTab === 'editor' && <ScriptEditor />}
                {activeTab === 'characters' && <CharacterPanel />}
                {activeTab === 'export' && <ExportPanel />}
            </main>

            {settingsOpen && <SettingsDialog onClose={() => setSettingsOpen(false)} />}
        </div>
    );
}

export default App;
