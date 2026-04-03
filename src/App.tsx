import { useEffect, useState } from 'react';
import { Settings, Plus } from 'lucide-react';
import { useProjectStore } from './store/projectStore';
import { useCharacterStore } from './store/characterStore';
import { useScriptStore } from './store/scriptStore';
import ProjectList from './components/project/ProjectList';
import CharacterPanel from './components/character/CharacterPanel';
import ScriptEditor from './components/editor/ScriptEditor';
import SettingsDialog from './components/settings/SettingsDialog';
import ExportPanel from './components/editor/ExportPanel';
import './App.css';

type Tab = 'editor' | 'characters' | 'export';

function App() {
    const { currentProject, loadProject } = useProjectStore();
    const [settingsOpen, setSettingsOpen] = useState(false);
    const [activeTab, setActiveTab] = useState<Tab>('editor');
    const [showNewProject, setShowNewProject] = useState(false);
    const { isDirty } = useScriptStore();

    const handleSelectProject = async (projectId: string) => {
        await loadProject(projectId);
    };

    const handleBack = () => {
        if (isDirty && !window.confirm('有未保存的更改，确定要离开吗？')) {
            return;
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
        // Only re-run when the project ID changes, not on every currentProject update
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [projectId]);

    if (!currentProject) {
        return (
            <div className="min-h-screen">
                <div className="fixed top-4 right-4 z-10 flex items-center gap-1">
                    <button
                        className="p-2 rounded-lg hover:bg-gray-200 dark:hover:bg-gray-700 transition"
                        onClick={() => setShowNewProject(true)}
                        aria-label="新建项目"
                        title="新建项目"
                    >
                        <Plus className="h-5 w-5" />
                    </button>
                    <button
                        className="p-2 rounded-lg hover:bg-gray-200 dark:hover:bg-gray-700 transition"
                        onClick={() => setSettingsOpen(true)}
                        aria-label="Settings"
                    >
                        <Settings className="h-5 w-5" />
                    </button>
                </div>
                <ProjectList
                    onSelectProject={handleSelectProject}
                    showInput={showNewProject}
                    onShowInput={setShowNewProject}
                />
                {settingsOpen && <SettingsDialog onClose={() => setSettingsOpen(false)} />}
            </div>
        );
    }

    return (
        <div className="min-h-screen flex flex-col">
            {/* Header */}
            <header className="flex items-center justify-between border-b border-gray-200 dark:border-gray-700 px-6 py-3">
                <div className="flex items-center gap-4">
                    <button
                        className="text-sm text-blue-600 hover:underline"
                        onClick={handleBack}
                    >
                        ← 返回项目列表
                    </button>
                    <h1 className="text-lg font-semibold">{currentProject.project.name}</h1>
                </div>
                <div className="flex items-center gap-2">
                    <nav className="flex gap-1 rounded-lg bg-gray-100 dark:bg-gray-800 p-1">
                        {(['editor', 'characters', 'export'] as Tab[]).map((tab) => (
                            <button
                                key={tab}
                                className={`px-3 py-1.5 text-sm rounded-md transition ${
                                    activeTab === tab
                                        ? 'bg-white dark:bg-gray-700 shadow-sm font-medium'
                                        : 'hover:bg-gray-200 dark:hover:bg-gray-600'
                                }`}
                                onClick={() => setActiveTab(tab)}
                            >
                                {tab === 'editor' ? '剧本编辑' : tab === 'characters' ? '角色管理' : '导出'}
                            </button>
                        ))}
                    </nav>
                    <button
                        className="p-2 rounded-lg hover:bg-gray-200 dark:hover:bg-gray-700 transition"
                        onClick={() => setSettingsOpen(true)}
                        aria-label="Settings"
                    >
                        <Settings className="h-5 w-5" />
                    </button>
                </div>
            </header>

            {/* Content */}
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
