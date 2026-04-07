import { useState } from 'react';
import { Sparkles, ChevronDown, ChevronUp, Loader2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '../ui/button';
import { Card, CardContent } from '../ui/card';

interface OutlinePanelProps {
    outline: string;
    onOutlineChange: (value: string) => void;
    isAnalyzing: boolean;
    onAnalyze: () => void;
    hasAgentPlan: boolean;
}

export default function OutlinePanel({
    outline,
    onOutlineChange,
    isAnalyzing,
    onAnalyze,
    hasAgentPlan,
}: OutlinePanelProps) {
    const { t } = useTranslation();
    const [collapsed, setCollapsed] = useState(false);

    return (
        <Card>
            <CardContent className="space-y-3">
                <div className="flex items-center justify-between">
                    <span className="text-sm font-medium">{t('editor.outlineLabel')}</span>
                    <Button
                        variant="ghost"
                        size="sm"
                        className="h-6 px-2 text-xs"
                        onClick={() => setCollapsed(!collapsed)}
                    >
                        {collapsed ? (
                            <ChevronDown className="h-3 w-3 mr-1" />
                        ) : (
                            <ChevronUp className="h-3 w-3 mr-1" />
                        )}
                        {collapsed ? t('editor.outlineExpand') : t('editor.outlineCollapse')}
                    </Button>
                </div>
                {!collapsed && (
                    <>
                        <textarea
                            className="w-full rounded-lg border border-input bg-transparent px-3 py-2 text-sm focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 outline-none resize-y min-h-[80px] dark:bg-input/30"
                            placeholder={t('editor.outlinePlaceholder')}
                            value={outline}
                            onChange={(e) => onOutlineChange(e.target.value)}
                        />
                        <div className="flex gap-2 flex-wrap items-center">
                            {!hasAgentPlan && (
                                <Button onClick={onAnalyze} disabled={isAnalyzing || !outline.trim()}>
                                    {isAnalyzing ? (
                                        <Loader2 className="h-4 w-4 animate-spin" />
                                    ) : (
                                        <Sparkles className="h-4 w-4" />
                                    )}
                                    {isAnalyzing ? t('editor.analyzing') : t('editor.analyze')}
                                </Button>
                            )}
                        </div>
                    </>
                )}
            </CardContent>
        </Card>
    );
}
