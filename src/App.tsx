import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import CharacterPanel from './components/character/CharacterPanel';
import ExportPanel from './components/editor/ExportPanel';
import ScriptEditor from './components/editor/ScriptEditor';
import AppHeader from './components/layout/AppHeader';
import ProjectListPage from './components/layout/ProjectListPage';
import ToastContainer from './components/layout/ToastContainer';
import UpdateBanner from './components/layout/UpdateBanner';
import SettingsDialog from './components/settings/SettingsDialog';
import { useCharacterStore } from './store/characterStore';
import { useProjectStore } from './store/projectStore';
import { useScriptStore } from './store/scriptStore';
import { useSettingsStore } from './store/settingsStore';
import { useUpdateStore } from './store/updateStore';
import './App.css';

type Tab = 'editor' | 'characters' | 'export';

function App() {
  const { t: _t } = useTranslation(); // Reserved for future i18n usage
  const { currentProject, loadProject } = useProjectStore();
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [activeTab, setActiveTab] = useState<Tab>('editor');
  const { isDirty } = useScriptStore();
  const { updateAvailable, latestVersion, downloading, checkForUpdates, installUpdate } = useUpdateStore();

  // Load settings on app startup to restore persisted preferences
  useEffect(() => {
    void useSettingsStore.getState().loadSettings();
  }, []);

  // Check for updates on app startup (delayed to not block initial render)
  useEffect(() => {
    const timer = setTimeout(() => {
      void checkForUpdates();
    }, 3000);
    return () => {
      clearTimeout(timer);
    };
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
      void useCharacterStore.getState().fetchCharacters();
      useScriptStore.setState({
        lines: currentProject.script_lines,
        sections: currentProject.sections,
        isDirty: false,
      });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId]);

  if (!currentProject) {
    return <ProjectListPage onSelectProject={(id: string) => void handleSelectProject(id)} />;
  }

  return (
    <div className="min-h-screen flex flex-col">
      <ToastContainer />

      {/* Update Notification Banner */}
      <UpdateBanner
        updateAvailable={updateAvailable}
        latestVersion={latestVersion}
        downloading={downloading}
        onInstall={() => void installUpdate()}
      />

      <AppHeader
        project={currentProject}
        activeTab={activeTab}
        onTabChange={setActiveTab}
        onBack={() => void handleBack()}
        onSettings={() => {
          setSettingsOpen(true);
        }}
      />

      <main className="flex-1 overflow-auto">
        {activeTab === 'editor' && <ScriptEditor />}
        {activeTab === 'characters' && <CharacterPanel />}
        {activeTab === 'export' && <ExportPanel />}
      </main>

      {settingsOpen && (
        <SettingsDialog
          onClose={() => {
            setSettingsOpen(false);
          }}
        />
      )}
    </div>
  );
}

export default App;
