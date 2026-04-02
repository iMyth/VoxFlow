import { useState } from 'react';
import { Sparkles, Save, Plus } from 'lucide-react';
import { useScriptStore } from '../../store/scriptStore';
import ScriptLineComponent from './ScriptLine';

export default function ScriptEditor() {
    const { lines, isGenerating, isDirty, streamingText, generateScript, saveScript, addLine } =
        useScriptStore();
    const [outline, setOutline] = useState('');

    const handleGenerate = () => {
        if (!outline.trim() || isGenerating) return;
        generateScript(outline.trim());
    };

    return (
        <div className="mx-auto max-w-4xl px-6 py-6 space-y-4">
            {/* Outline input + generate */}
            <div className="rounded-xl border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 p-4 space-y-3">
                <label className="block text-sm font-medium">大纲输入</label>
                <textarea
                    className="w-full rounded-lg border border-gray-300 dark:border-gray-600 px-3 py-2 text-sm dark:bg-gray-900 focus:outline-none focus:ring-2 focus:ring-blue-500 resize-y min-h-[80px]"
                    placeholder="输入有声书大纲，AI 将为你生成剧本..."
                    value={outline}
                    onChange={(e) => setOutline(e.target.value)}
                    disabled={isGenerating}
                />
                <div className="flex gap-2">
                    <button
                        className="flex items-center gap-2 rounded-lg bg-purple-600 px-4 py-2 text-sm text-white hover:bg-purple-700 disabled:opacity-50"
                        onClick={handleGenerate}
                        disabled={isGenerating || !outline.trim()}
                    >
                        <Sparkles className="h-4 w-4" />
                        {isGenerating ? '生成中...' : '生成剧本'}
                    </button>
                    {isDirty && (
                        <button
                            className="flex items-center gap-2 rounded-lg bg-green-600 px-4 py-2 text-sm text-white hover:bg-green-700"
                            onClick={() => saveScript()}
                        >
                            <Save className="h-4 w-4" /> 保存剧本
                        </button>
                    )}
                </div>
            </div>

            {/* Streaming text (typewriter effect) */}
            {isGenerating && streamingText && (
                <div className="rounded-xl border border-purple-200 dark:border-purple-800 bg-purple-50 dark:bg-purple-900/20 p-4">
                    <p className="text-sm text-purple-700 dark:text-purple-300 font-medium mb-2">AI 正在生成...</p>
                    <pre className="text-sm whitespace-pre-wrap">{streamingText}<span className="animate-pulse">▌</span></pre>
                </div>
            )}

            {/* Script lines */}
            {lines.length > 0 && (
                <div className="space-y-2">
                    {lines.map((line, index) => (
                        <ScriptLineComponent key={line.id} line={line} index={index} />
                    ))}
                    <button
                        className="flex items-center gap-2 w-full justify-center rounded-lg border-2 border-dashed border-gray-300 dark:border-gray-600 py-3 text-sm text-gray-500 hover:border-blue-400 hover:text-blue-500 transition"
                        onClick={() => addLine(lines.length - 1)}
                    >
                        <Plus className="h-4 w-4" /> 添加新行
                    </button>
                </div>
            )}

            {lines.length === 0 && !isGenerating && (
                <p className="text-center text-gray-500 py-16">输入大纲生成剧本，或手动添加剧本行</p>
            )}

            {lines.length === 0 && !isGenerating && (
                <div className="flex justify-center">
                    <button
                        className="flex items-center gap-2 rounded-lg border-2 border-dashed border-gray-300 dark:border-gray-600 px-6 py-3 text-sm text-gray-500 hover:border-blue-400 hover:text-blue-500 transition"
                        onClick={() => addLine(-1)}
                    >
                        <Plus className="h-4 w-4" /> 添加第一行
                    </button>
                </div>
            )}
        </div>
    );
}
