import { useState, useEffect } from 'react';
import { Download, Music, AlertTriangle, CheckCircle, Loader2 } from 'lucide-react';
import { useProjectStore } from '../../store/projectStore';
import { useScriptStore } from '../../store/scriptStore';
import * as ipc from '../../lib/ipc';
import type { MixProgress } from '../../types';

export default function ExportPanel() {
    const currentProject = useProjectStore((s) => s.currentProject);
    const { lines } = useScriptStore();
    const [bgmPath, setBgmPath] = useState<string | null>(null);
    const [bgmVolume, setBgmVolume] = useState(0.3);
    const [outputPath, setOutputPath] = useState('');
    const [exporting, setExporting] = useState(false);
    const [progress, setProgress] = useState<MixProgress | null>(null);
    const [done, setDone] = useState(false);
    const [error, setError] = useState<string | null>(null);

    // Detect missing audio
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
        setExporting(true);
        setProgress(null); 
        setDone(false);
        setError(null);

        const unlisten = await ipc.onMixProgress((p) => setProgress(p));

        try {
            await ipc.exportAudioMix(
                currentProject.project.id,
                outputPath,
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
        // For now, we use a simple text input for the BGM path
    };

    return (
        <div className="mx-auto max-w-3xl px-6 py-8 space-y-6">
            <h2 className="text-xl font-bold">导出有声书</h2>

            {/* Missing audio warning */}
            {missingLines.length > 0 && (
                <div className="flex items-start gap-3 rounded-xl border border-amber-300 dark:border-amber-700 bg-amber-50 dark:bg-amber-900/20 p-4">
                    <AlertTriangle className="h-5 w-5 text-amber-500 shrink-0 mt-0.5" />
                    <div>
                        <p className="text-sm font-medium text-amber-700 dark:text-amber-300">
                            {missingLines.length} 行剧本缺少音频
                        </p>
                        <ul className="mt-1 text-xs text-amber-600 dark:text-amber-400 space-y-0.5">
                            {missingLines.slice(0, 5).map((l) => (
                                <li key={l.id}>第 {l.line_order + 1} 行: {l.text.slice(0, 40)}...</li>
                            ))}
                            {missingLines.length > 5 && <li>...还有 {missingLines.length - 5} 行</li>}
                        </ul>
                        <p className="mt-2 text-xs text-amber-600 dark:text-amber-400">
                            请先为所有剧本行生成语音后再导出
                        </p>
                    </div>
                </div>
            )}

            {/* BGM config */}
            <div className="rounded-xl border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 p-5 space-y-4">
                <h3 className="text-sm font-semibold flex items-center gap-2">
                    <Music className="h-4 w-4" /> 背景音乐 (BGM)
                </h3>
                <div className="flex gap-3 items-center">
                    <input
                        className="flex-1 rounded-lg border border-gray-300 dark:border-gray-600 px-3 py-2 text-sm dark:bg-gray-900"
                        placeholder="BGM 文件路径（可选）"
                        value={bgmPath ?? ''}
                        onChange={(e) => setBgmPath(e.target.value || null)}
                    />
                    <button
                        className="rounded-lg border border-gray-300 dark:border-gray-600 px-3 py-2 text-sm hover:bg-gray-100 dark:hover:bg-gray-700"
                        onClick={handleBgmImport}
                    >
                        浏览
                    </button>
                </div>
                {bgmPath && (
                    <div>
                        <label className="block text-sm mb-1">BGM 音量 ({Math.round(bgmVolume * 100)}%)</label>
                        <input
                            type="range"
                            min="0"
                            max="1"
                            step="0.05"
                            className="w-full"
                            value={bgmVolume}
                            onChange={(e) => setBgmVolume(parseFloat(e.target.value))}
                        />
                    </div>
                )}
            </div>

            {/* Output path */}
            <div className="rounded-xl border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 p-5 space-y-3">
                <label className="block text-sm font-medium">输出文件名</label>
                <input
                    className="w-full rounded-lg border border-gray-300 dark:border-gray-600 px-3 py-2 text-sm dark:bg-gray-900"
                    value={outputPath}
                    onChange={(e) => setOutputPath(e.target.value)}
                />
            </div>

            {/* Progress bar */}
            {exporting && progress && (
                <div className="rounded-xl border border-blue-200 dark:border-blue-800 bg-blue-50 dark:bg-blue-900/20 p-4 space-y-2">
                    <div className="flex items-center gap-2 text-sm text-blue-700 dark:text-blue-300">
                        <Loader2 className="h-4 w-4 animate-spin" />
                        {progress.stage}
                    </div>
                    <div className="w-full bg-blue-200 dark:bg-blue-800 rounded-full h-2">
                        <div
                            className="bg-blue-600 h-2 rounded-full transition-all"
                            style={{ width: `${progress.percent}%` }}
                        />
                    </div>
                    <p className="text-xs text-blue-500">{Math.round(progress.percent)}%</p>
                </div>
            )}

            {/* Success */}
            {done && (
                <div className="flex items-center gap-3 rounded-xl border border-green-300 dark:border-green-700 bg-green-50 dark:bg-green-900/20 p-4">
                    <CheckCircle className="h-5 w-5 text-green-500" />
                    <p className="text-sm text-green-700 dark:text-green-300">导出成功！</p>
                </div>
            )}

            {/* Error */}
            {error && (
                <div className="flex items-start gap-3 rounded-xl border border-red-300 dark:border-red-700 bg-red-50 dark:bg-red-900/20 p-4">
                    <AlertTriangle className="h-5 w-5 text-red-500 shrink-0" />
                    <p className="text-sm text-red-700 dark:text-red-300">{error}</p>
                </div>
            )}

            {/* Export button */}
            <button
                className="flex items-center gap-2 rounded-lg bg-blue-600 px-6 py-3 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
                onClick={handleExport}
                disabled={exporting || missingLines.length > 0 || !outputPath.trim()}
            >
                <Download className="h-4 w-4" />
                {exporting ? '导出中...' : '导出有声书'}
            </button>
        </div>
    );
}
