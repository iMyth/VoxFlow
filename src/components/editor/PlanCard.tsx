import { Check, X, Plus, Loader2, Pencil } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '../ui/button';
import { Card, CardContent } from '../ui/card';
import { Input } from '../ui/input';
import { Label } from '../ui/label';
import { Badge } from '../ui/badge';
import { Separator } from '../ui/separator';
import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
} from '../ui/select';
import type { AgentPlan } from '../../lib/ipc';
import type { Character } from '../../types';

interface PlanCardProps {
    plan: AgentPlan;
    existingCharacters: Character[];
    onDismiss: () => void;
    onConfirmGenerate: () => void;
    onManualMode: () => void;
    onCharacterMapping: (suggestedName: string, targetName: string) => void;
    onNewChar: (suggestedName: string) => Promise<void>;
    onCancelNewChar: (suggestedName: string) => void;
    creatingChars: Record<string, boolean>;
    newCharForms: Record<string, { name: string; voice: string; speed: number; pitch: number }>;
    onFormChange: (name: string, field: string, value: string | number) => void;
    characterMapping: Record<string, string>;
    extraInstructions: string;
    onExtraChange: (value: string) => void;
    isGenerating: boolean;
}

export default function PlanCard({
    plan,
    existingCharacters,
    onDismiss,
    onConfirmGenerate,
    onManualMode,
    onCharacterMapping,
    onNewChar,
    onCancelNewChar,
    creatingChars,
    newCharForms,
    onFormChange,
    characterMapping,
    extraInstructions,
    onExtraChange,
    isGenerating,
}: PlanCardProps) {
    const { t } = useTranslation();

    const totalLines = plan.chapters.reduce((sum, c) => sum + c.estimated_lines, 0);
    const allMapped = plan.suggested_characters.every(
        (c) => characterMapping[c.name] && !creatingChars[c.name],
    );

    return (
        <Card className="border-blue-200 dark:border-blue-800/60 bg-blue-50/80 dark:bg-blue-950/40">
            <CardContent className="space-y-4 py-4">
                {/* Header */}
                <div className="flex items-center justify-between">
                    <h3 className="text-sm font-semibold text-blue-700 dark:text-blue-300">
                        {t('editor.planTitle')}
                    </h3>
                    <div className="flex items-center gap-1">
                        <Button size="sm" variant="ghost" onClick={onManualMode} className="h-7 px-2 text-xs text-muted-foreground hover:text-foreground">
                            <Pencil className="h-3.5 w-3.5 mr-1" />
                            {t('editor.manualModeTitle')}
                        </Button>
                        <Button size="sm" variant="ghost" onClick={onDismiss} className="h-7 w-7 p-0">
                            <X className="h-3.5 w-3.5" />
                        </Button>
                    </div>
                </div>

                <Separator className="bg-blue-200/60 dark:bg-blue-800/40" />

                {/* Style */}
                <div>
                    <p className="text-xs font-medium text-blue-600 dark:text-blue-400 mb-1">{t('editor.planStyle')}</p>
                    <p className="text-sm text-foreground">{plan.overall_style}</p>
                </div>

                {/* Chapters */}
                <div>
                    <p className="text-xs font-medium text-blue-600 dark:text-blue-400 mb-2">
                        {t('editor.planChapters', { count: plan.chapters.length, totalLines })}
                    </p>
                    <div className="space-y-1">
                        {plan.chapters.map((ch, i) => (
                            <div key={i} className="flex items-center gap-2 text-xs bg-card/80 dark:bg-card/50 rounded-md border border-border/30 px-2.5 py-2">
                                <span className="font-medium text-muted-foreground w-6 shrink-0">{i + 1}.</span>
                                <span className="flex-1 font-medium text-foreground">{ch.title}</span>
                                <span className="text-muted-foreground shrink-0">{ch.estimated_lines} {t('editor.planLines')}</span>
                                {ch.characters.length > 0 && (
                                    <span className="text-muted-foreground shrink-0">· {ch.characters.join(', ')}</span>
                                )}
                            </div>
                        ))}
                    </div>
                </div>

                {/* Characters */}
                <div>
                    <p className="text-xs font-medium text-blue-600 dark:text-blue-400 mb-2">{t('editor.planCharacters')}</p>
                    <div className="space-y-2">
                        {plan.suggested_characters.map((ch, i) => (
                            <div key={i} className="space-y-2 bg-card/80 dark:bg-card/50 rounded-md border border-border/30 px-3 py-2.5">
                                <div className="flex items-center gap-2">
                                    <div className="flex-1 min-w-0">
                                        <p className="text-sm font-semibold text-foreground">{ch.name}</p>
                                        <p className="text-xs text-muted-foreground">{ch.role}</p>
                                    </div>
                                    {ch.matched_existing && (
                                        <Badge variant="outline" className="text-xs bg-green-50 dark:bg-green-900/30 text-green-700 dark:text-green-300 border-green-200 dark:border-green-800/60 shrink-0">
                                            {t('editor.planMatched')}
                                        </Badge>
                                    )}
                                    <Select
                                        value={characterMapping[ch.name] || undefined}
                                        onValueChange={(v) => onCharacterMapping(ch.name, v)}
                                    >
                                        <SelectTrigger className="min-w-[140px] h-8 text-xs" size="sm">
                                            <SelectValue placeholder={t('editor.planSelectChar')} />
                                        </SelectTrigger>
                                        <SelectContent>
                                            {existingCharacters.map((ec) => (
                                                <SelectItem key={ec.id} value={ec.name}>{ec.name}</SelectItem>
                                            ))}
                                            <SelectItem value="__new__">{t('editor.planNewChar')}</SelectItem>
                                        </SelectContent>
                                    </Select>
                                </div>

                                {/* New character form */}
                                {creatingChars[ch.name] && newCharForms[ch.name] && (
                                    <div className="ml-2 mt-2 p-3 rounded-md border border-dashed border-border/60 space-y-2.5 bg-card/40">
                                        <div className="flex items-center gap-2">
                                            <Label className="text-xs shrink-0 w-12 text-muted-foreground">{t('character.name')}</Label>
                                            <Input
                                                value={newCharForms[ch.name].name}
                                                onChange={(e) => onFormChange(ch.name, 'name', e.target.value)}
                                                placeholder={t('character.namePlaceholder')}
                                                className="h-7 text-xs"
                                            />
                                        </div>
                                        <div className="flex items-center gap-2">
                                            <Label className="text-xs shrink-0 w-12 text-muted-foreground">{t('character.voice')}</Label>
                                            <Input
                                                value={newCharForms[ch.name].voice}
                                                onChange={(e) => onFormChange(ch.name, 'voice', e.target.value)}
                                                className="h-7 text-xs"
                                            />
                                        </div>
                                        <div className="flex gap-4">
                                            <div className="flex items-center gap-2 flex-1">
                                                <Label className="text-xs shrink-0 w-12 text-muted-foreground">{t('character.speed')}</Label>
                                                <Input
                                                    type="number"
                                                    step="0.1"
                                                    min="0.1"
                                                    max="3.0"
                                                    className="w-20 h-7 text-xs"
                                                    value={newCharForms[ch.name].speed}
                                                    onChange={(e) => onFormChange(ch.name, 'speed', parseFloat(e.target.value) || 1.0)}
                                                />
                                            </div>
                                            <div className="flex items-center gap-2 flex-1">
                                                <Label className="text-xs shrink-0 w-12 text-muted-foreground">{t('character.pitch')}</Label>
                                                <Input
                                                    type="number"
                                                    step="0.1"
                                                    min="-1.0"
                                                    max="1.0"
                                                    className="w-20 h-7 text-xs"
                                                    value={newCharForms[ch.name].pitch}
                                                    onChange={(e) => onFormChange(ch.name, 'pitch', parseFloat(e.target.value) || 0.0)}
                                                />
                                            </div>
                                        </div>
                                        <div className="flex justify-end gap-2 pt-1">
                                            <Button
                                                size="sm"
                                                variant="ghost"
                                                className="h-6 text-xs"
                                                onClick={() => onCancelNewChar(ch.name)}
                                            >
                                                {t('character.cancel')}
                                            </Button>
                                            <Button
                                                size="sm"
                                                className="h-6 text-xs"
                                                onClick={() => onNewChar(ch.name)}
                                            >
                                                <Plus className="h-3 w-3 mr-1" />
                                                {t('character.create')}
                                            </Button>
                                        </div>
                                    </div>
                                )}
                            </div>
                        ))}
                    </div>
                </div>

                <Separator className="bg-blue-200/60 dark:bg-blue-800/40" />

                {/* Extra instructions */}
                <div>
                    <p className="text-xs font-medium text-blue-600 dark:text-blue-400 mb-1">{t('editor.planExtraInstructions')}</p>
                    <textarea
                        className="w-full rounded-lg border border-input bg-transparent px-3 py-2 text-sm text-foreground focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/50 outline-none resize-y min-h-[60px]"
                        placeholder={t('editor.planExtraPlaceholder')}
                        value={extraInstructions}
                        onChange={(e) => onExtraChange(e.target.value)}
                    />
                </div>

                {/* Confirm generate button */}
                <div className="flex justify-end pt-1">
                    <Button
                        size="sm"
                        onClick={onConfirmGenerate}
                        disabled={!allMapped || isGenerating}
                        className="h-8 text-xs bg-blue-600 hover:bg-blue-700 dark:bg-blue-600 dark:hover:bg-blue-500"
                    >
                        {isGenerating ? (
                            <Loader2 className="h-3 w-3 mr-1 animate-spin" />
                        ) : (
                            <Check className="h-3 w-3 mr-1" />
                        )}
                        {isGenerating ? t('editor.generating') : t('editor.confirmPlan')}
                    </Button>
                </div>
            </CardContent>
        </Card>
    );
}
