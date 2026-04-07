import { Loader2, Square } from 'lucide-react';
import { Card, CardContent } from '../ui/card';
import { Button } from '../ui/button';

interface StreamingCardProps {
    color: 'blue' | 'purple';
    label: string;
    text: string;
    isAnalyzing?: boolean;
    onCancel?: () => void;
}

export default function StreamingCard({ color, label, text, isAnalyzing, onCancel }: StreamingCardProps) {
    const colors = {
        blue: {
            border: 'border-blue-200 dark:border-blue-800',
            bg: 'bg-blue-50 dark:bg-blue-900/20',
            text: 'text-blue-700 dark:text-blue-300',
            textLight: 'text-blue-600 dark:text-blue-400',
        },
        purple: {
            border: 'border-purple-200 dark:border-purple-800',
            bg: 'bg-purple-50 dark:bg-purple-900/20',
            text: 'text-purple-700 dark:text-purple-300',
            textLight: 'text-purple-600 dark:text-purple-400',
        },
    };

    const c = colors[color];

    // Analyzing: compact spinner when no text yet
    if (isAnalyzing && !text) {
        return (
            <Card className={`border-blue-200 dark:border-blue-800 bg-blue-50 dark:bg-blue-900/20`}>
                <CardContent className="flex items-center gap-3 py-4">
                    <Loader2 className="h-5 w-5 animate-spin text-blue-500" />
                    <p className="text-sm text-blue-700 dark:text-blue-300 flex-1">{label}</p>
                    {onCancel && (
                        <Button variant="destructive" size="sm" onClick={onCancel} className="h-7 text-xs">
                            <Square className="h-3 w-3 mr-1" />
                            取消
                        </Button>
                    )}
                </CardContent>
            </Card>
        );
    }

    return (
        <Card className={`${c.border} ${c.bg}`}>
            <CardContent>
                <div className="flex items-center justify-between mb-2">
                    {isAnalyzing && (
                        <div className="flex items-center gap-2">
                            <Loader2 className="h-4 w-4 animate-spin text-blue-500" />
                            <p className={`text-sm font-medium ${c.text}`}>{label}</p>
                        </div>
                    )}
                    {!isAnalyzing && (
                        <p className={`text-sm font-medium ${c.text}`}>{label}</p>
                    )}
                    {onCancel && (
                        <Button variant="destructive" size="sm" onClick={onCancel} className="h-7 text-xs shrink-0">
                            <Square className="h-3 w-3 mr-1" />
                            取消
                        </Button>
                    )}
                </div>
                <pre className={`text-sm whitespace-pre-wrap ${c.textLight}`} style={{ contain: 'layout style', minHeight: '4rem', maxHeight: '60vh', overflowY: 'auto' }}>
                    {text}
                    <span className="animate-pulse">▌</span>
                </pre>
            </CardContent>
        </Card>
    );
}
