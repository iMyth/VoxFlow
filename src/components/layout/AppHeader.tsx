import { useTranslation } from 'react-i18next';
import { Settings, ChevronLeft } from 'lucide-react';
import type { ProjectDetail } from '../../types';
import { Button } from '../ui/button';
import { Tabs, TabsList, TabsTrigger } from '../ui/tabs';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '../ui/tooltip';
import ThemeSelector from './ThemeSelector';

type Tab = 'editor' | 'characters' | 'export';

interface AppHeaderProps {
    project: ProjectDetail;
    activeTab: Tab;
    onTabChange: (tab: Tab) => void;
    onBack: () => void;
    onSettings: () => void;
}

export default function AppHeader({
    project,
    activeTab,
    onTabChange,
    onBack,
    onSettings,
}: AppHeaderProps) {
    const { t } = useTranslation();

    const tabLabels: Record<Tab, string> = {
        editor: t('app.tab.editor'),
        characters: t('app.tab.characters'),
        export: t('app.tab.export'),
    };

    return (
        <header className="flex items-center justify-between border-b border-border px-6 py-3">
            <div className="flex items-center gap-1">
                <Button variant="ghost" size="icon" onClick={onBack} className="h-9 w-9 shrink-0" title={t('app.backToProjects')}>
                    <ChevronLeft className="h-5 w-5" />
                </Button>
                <h1 className="text-sm font-semibold truncate max-w-[200px]">{project.project.name}</h1>
            </div>
            <div className="flex items-center gap-2">
                <Tabs value={activeTab} onValueChange={(v) => onTabChange(v as Tab)}>
                    <TabsList>
                        {(Object.keys(tabLabels) as Tab[]).map((tab) => (
                            <TabsTrigger key={tab} value={tab}>
                                {tabLabels[tab]}
                            </TabsTrigger>
                        ))}
                    </TabsList>
                </Tabs>

                <ThemeSelector />

                <TooltipProvider>
                    <Tooltip>
                        <TooltipTrigger asChild>
                            <Button
                                variant="ghost"
                                size="icon"
                                onClick={onSettings}
                                aria-label={t('app.settings')}
                            >
                                <Settings className="h-5 w-5" />
                            </Button>
                        </TooltipTrigger>
                        <TooltipContent>{t('app.settings')}</TooltipContent>
                    </Tooltip>
                </TooltipProvider>
            </div>
        </header>
    );
}
