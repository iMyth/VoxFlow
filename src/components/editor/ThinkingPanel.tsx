import { useState, useEffect, useRef } from 'react';
import { ChevronDown, ChevronUp, Brain } from 'lucide-react';
import { useTranslation } from 'react-i18next';

interface ThinkingPanelProps {
    thinkingText: string;
    isThinking: boolean;
}

export default function ThinkingPanel({ thinkingText, isThinking }: ThinkingPanelProps) {
    const { t } = useTranslation();
    const [collapsed, setCollapsed] = useState(false);
    const scrollRef = useRef<HTMLDivElement>(null);

    // Auto-scroll when new thinking content arrives
    useEffect(() => {
        if (scrollRef.current && !collapsed) {
            scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
        }
    }, [thinkingText, collapsed]);

    if (!isThinking && !thinkingText) return null;

    return (
        <div className="rounded-lg border border-border/50 bg-muted/30 overflow-hidden">
            <button
                className="w-full flex items-center justify-between px-3 py-2 text-xs font-medium text-muted-foreground hover:text-foreground hover:bg-muted/50 transition-colors"
                onClick={() => setCollapsed(!collapsed)}
            >
                <div className="flex items-center gap-1.5">
                    <Brain className="h-3.5 w-3.5 shrink-0" />
                    <span className="leading-none">{t('editor.thinking')}</span>
                    {isThinking && (
                        <span className="flex items-center gap-1">
                            <span className="w-1 h-1 rounded-full bg-muted-foreground animate-pulse" />
                            <span className="w-1 h-1 rounded-full bg-muted-foreground animate-pulse" style={{ animationDelay: '0.2s' }} />
                            <span className="w-1 h-1 rounded-full bg-muted-foreground animate-pulse" style={{ animationDelay: '0.4s' }} />
                        </span>
                    )}
                </div>
                {collapsed ? (
                    <ChevronDown className="h-3 w-3" />
                ) : (
                    <ChevronUp className="h-3 w-3" />
                )}
            </button>
            {!collapsed && thinkingText && (
                <div
                    ref={scrollRef}
                    className="px-3 pb-2 text-xs text-muted-foreground/80 leading-relaxed whitespace-pre-wrap max-h-48 overflow-y-auto"
                >
                    {thinkingText}
                </div>
            )}
        </div>
    );
}
