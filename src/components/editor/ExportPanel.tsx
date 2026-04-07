import { useState, useEffect } from 'react';
import { Download, Music, AlertTriangle, CheckCircle, Loader2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { save } from '@tauri-apps/plugin-dialog';
import { useProjectStore } from '../../store/projectStore';
import { useScriptStore } from '../../store/scriptStore';
import * as ipc from '../../lib/ipc';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { Label } from '../ui/label';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { Alert, AlertTitle, AlertDescription } from '../ui/alert';
import { Progress } from '../ui/progress';
import { Slider } from '../ui/slider';
import type { MixProgress } from '../../types';

export default function ExportPanel() {
    const { t } = useTranslation();
    const currentProject = useProjectStore((s) => s.currentProject);
    const { lines } = useScriptStore();
    const [bgmPath, setBgmPath] = useState<string | null>(null);
    const [bgmVolume, setBgmVolume] = useState(0.3);
    const [outputPath, setOutputPath] = useState('');
    const [exporting, setExporting] = useState(false);
    const [progress, setProgress] = useState<MixProgress | null>(null);
    const [done, setDone] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const audioFragments = currentProject?.audio_fragments ?? [];
    const coveredLineIds = new Set(audioFragments.map((a) => a.line_id));
    const missingLines = lines.filter((l) => l.text.trim() && !coveredLineIds.has(l.id));

    useEffect(() => {
        if (currentProject) {
            setOutputPath(`${currentProject.project.name}.mp3`);
        }
    }, [currentProject]);

    const handleExport = async () => {
        if (!currentProject || missingLines.length > 0) return;

        // Open save dialog to let user choose full output path
        const selectedPath = await save({
            title: '导出有声书',
            defaultPath: outputPath,
            filters: [{ name: 'MP3 Audio', extensions: ['mp3'] }],
        });
        if (!selectedPath) return;

        setExporting(true);
        setProgress(null);
        setDone(false);
        setError(null);

        const unlisten = await ipc.onMixProgress((p) => setProgress(p));

        try {
            await ipc.exportAudioMix(
                currentProject.project.id,
                selectedPath,
                bgmPath,
                bgmVolume,
            );
            setDone(true);
        } catch (e) {
            setError(String(e));
        } finally {
            unlisten();
            setExporting(false);
        }
    };

    const handleBgmImport = () => {
        // In a real implementation, this would use Tauri's file dialog
    };

    return (
        <div className="mx-auto max-w-3xl px-6 py-8 space-y-6">
            <h2 className="text-xl font-bold">{t('export.title')}</h2>

            {missingLines.length > 0 && (
                <Alert variant="destructive">
                    <AlertTriangle className="h-4 w-4" />
                    <AlertTitle>{t('export.missingAudio', { count: missingLines.length })}</AlertTitle>
                    <AlertDescription>
                        <ul className="mt-1 space-y-0.5">
                            {missingLines.slice(0, 5).map((l) => (
                                <li key={l.id}>{t('export.missingLine', { line: l.line_order + 1, text: l.text.slice(0, 40) })}</li>
                            ))}
                            {missingLines.length > 5 && <li>{t('export.missingMore', { count: missingLines.length - 5 })}</li>}
                        </ul>
                        <p className="mt-2">{t('export.missingHint')}</p>
                    </AlertDescription>
                </Alert>
            )}

            <Card>
                <CardHeader>
                    <CardTitle className="flex items-center gap-2">
                        <Music className="h-4 w-4" /> {t('export.bgm')}
                    </CardTitle>
                </CardHeader>
                <CardContent className="space-y-4">
                    <div className="flex gap-3 items-center">
                        <Input
                            className="flex-1"
                            placeholder={t('export.bgmPlaceholder')}
                            value={bgmPath ?? ''}
                            onChange={(e) => setBgmPath(e.target.value || null)}
                        />
                        <Button variant="outline" onClick={handleBgmImport}>{t('export.browse')}</Button>
                    </div>
                    {bgmPath && (
                        <div className="space-y-2">
                            <Label>{t('export.bgmVolume', { percent: Math.round(bgmVolume * 100) })}</Label>
                            <Slider
                                min={0} max={1} step={0.05}
                                value={[bgmVolume]}
                                onValueChange={([v]) => setBgmVolume(v)}
                            />
                        </div>
                    )}
                </CardContent>
            </Card>

            <Card>
                <CardContent className="space-y-2">
                    <Label>{t('export.outputLabel')}</Label>
                    <Input
                        value={outputPath}
                        onChange={(e) => setOutputPath(e.target.value)}
                    />
                </CardContent>
            </Card>

            {exporting && progress && (
                <Alert>
                    <Loader2 className="h-4 w-4 animate-spin" />
                    <AlertTitle>{progress.stage}</AlertTitle>
                    <AlertDescription className="space-y-2">
                        <Progress value={progress.percent} className="h-2" />
                        <p className="text-xs">{Math.round(progress.percent)}%</p>
                    </AlertDescription>
                </Alert>
            )}

            {done && (
                <Alert>
                    <CheckCircle className="h-4 w-4 text-green-500" />
                    <AlertTitle>{t('export.exportSuccess')}</AlertTitle>
                </Alert>
            )}

            {error && (
                <Alert variant="destructive">
                    <AlertTriangle className="h-4 w-4" />
                    <AlertTitle>{t('export.exportFailed')}</AlertTitle>
                    <AlertDescription>{error}</AlertDescription>
                </Alert>
            )}

            <Button
                size="lg"
                onClick={handleExport}
                disabled={exporting || missingLines.length > 0 || !outputPath.trim()}
            >
                <Download className="h-4 w-4" />
                {exporting ? t('export.exporting') : t('export.exportButton')}
            </Button>
        </div>
    );
}
