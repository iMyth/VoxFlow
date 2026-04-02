import { useEffect, useState } from 'react';
import { X, Save, KeyRound } from 'lucide-react';
import { useSettingsStore } from '../../store/settingsStore';

interface SettingsDialogProps {
    onClose: () => void;
}

export default function SettingsDialog({ onClose }: SettingsDialogProps) {
    const settings = useSettingsStore();
    const [llmKey, setLlmKey] = useState('');
    const [ttsKey, setTtsKey] = useState('');
    const [localEndpoint, setLocalEndpoint] = useState(settings.llmEndpoint);
    const [localModel, setLocalModel] = useState(settings.llmModel);
    const [localEngine, setLocalEngine] = useState(settings.defaultTtsEngine);
    const [localVoice, setLocalVoice] = useState(settings.defaultVoiceName);
    const [localSpeed, setLocalSpeed] = useState(settings.defaultSpeed);
    const [localPitch, setLocalPitch] = useState(settings.defaultPitch);

    useEffect(() => {
        settings.loadSettings();
        settings.loadApiKey('llm').then((k) => k && setLlmKey(k));
        settings.loadApiKey('tts').then((k) => k && setTtsKey(k));
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);

    useEffect(() => {
        setLocalEndpoint(settings.llmEndpoint);
        setLocalModel(settings.llmModel);
        setLocalEngine(settings.defaultTtsEngine);
        setLocalVoice(settings.defaultVoiceName);
        setLocalSpeed(settings.defaultSpeed);
        setLocalPitch(settings.defaultPitch);
    }, [settings.llmEndpoint, settings.llmModel, settings.defaultTtsEngine, settings.defaultVoiceName, settings.defaultSpeed, settings.defaultPitch]);

    const handleSave = async () => {
        settings.set({
            llmEndpoint: localEndpoint,
            llmModel: localModel,
            defaultTtsEngine: localEngine,
            defaultVoiceName: localVoice,
            defaultSpeed: localSpeed,
            defaultPitch: localPitch,
        });
        await settings.saveSettings();
        if (llmKey) await settings.saveApiKey('llm', llmKey);
        if (ttsKey) await settings.saveApiKey('tts', ttsKey);
        onClose();
    };

    return (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40" onClick={onClose}>
            <div
                className="w-full max-w-lg rounded-2xl bg-white dark:bg-gray-800 shadow-xl p-6 space-y-5 max-h-[90vh] overflow-y-auto"
                onClick={(e) => e.stopPropagation()}
            >
                <div className="flex items-center justify-between">
                    <h2 className="text-lg font-bold">设置</h2>
                    <button className="p-1 rounded-lg hover:bg-gray-200 dark:hover:bg-gray-700" onClick={onClose}>
                        <X className="h-5 w-5" />
                    </button>
                </div>

                {/* API Keys */}
                <section className="space-y-3">
                    <h3 className="text-sm font-semibold flex items-center gap-2">
                        <KeyRound className="h-4 w-4" /> API 密钥
                    </h3>
                    <div>
                        <label className="block text-sm mb-1">LLM API Key</label>
                        <input
                            type="password"
                            className="w-full rounded-lg border border-gray-300 dark:border-gray-600 px-3 py-2 text-sm dark:bg-gray-900 focus:outline-none focus:ring-2 focus:ring-blue-500"
                            value={llmKey}
                            onChange={(e) => setLlmKey(e.target.value)}
                            placeholder="sk-..."
                        />
                    </div>
                    <div>
                        <label className="block text-sm mb-1">TTS API Key (Azure)</label>
                        <input
                            type="password"
                            className="w-full rounded-lg border border-gray-300 dark:border-gray-600 px-3 py-2 text-sm dark:bg-gray-900 focus:outline-none focus:ring-2 focus:ring-blue-500"
                            value={ttsKey}
                            onChange={(e) => setTtsKey(e.target.value)}
                            placeholder="输入 Azure TTS 密钥"
                        />
                    </div>
                    {!llmKey && !ttsKey && (
                        <p className="text-xs text-amber-600 dark:text-amber-400">
                            请配置 API 密钥以使用 LLM 剧本生成和 TTS 语音合成功能
                        </p>
                    )}
                </section>

                {/* LLM Config */}
                <section className="space-y-3">
                    <h3 className="text-sm font-semibold">LLM 配置</h3>
                    <div>
                        <label className="block text-sm mb-1">API 端点</label>
                        <input
                            className="w-full rounded-lg border border-gray-300 dark:border-gray-600 px-3 py-2 text-sm dark:bg-gray-900 focus:outline-none focus:ring-2 focus:ring-blue-500"
                            value={localEndpoint}
                            onChange={(e) => setLocalEndpoint(e.target.value)}
                            placeholder="https://api.openai.com/v1"
                        />
                    </div>
                    <div>
                        <label className="block text-sm mb-1">模型名称</label>
                        <input
                            className="w-full rounded-lg border border-gray-300 dark:border-gray-600 px-3 py-2 text-sm dark:bg-gray-900 focus:outline-none focus:ring-2 focus:ring-blue-500"
                            value={localModel}
                            onChange={(e) => setLocalModel(e.target.value)}
                            placeholder="gpt-4"
                        />
                    </div>
                </section>

                {/* Default TTS Config */}
                <section className="space-y-3">
                    <h3 className="text-sm font-semibold">默认 TTS 配置</h3>
                    <div className="grid grid-cols-2 gap-4">
                        <div>
                            <label className="block text-sm mb-1">TTS 引擎</label>
                            <select
                                className="w-full rounded-lg border border-gray-300 dark:border-gray-600 px-3 py-2 text-sm dark:bg-gray-900"
                                value={localEngine}
                                onChange={(e) => setLocalEngine(e.target.value as 'edge-tts' | 'azure-tts' | 'dashscope-tts')}
                            >
                                <option value="edge-tts">Edge TTS</option>
                                <option value="azure-tts">Azure TTS</option>
                                <option value="dashscope-tts">阿里百炼 (CosyVoice)</option>
                            </select>
                        </div>
                        <div>
                            <label className="block text-sm mb-1">默认音色</label>
                            <input
                                className="w-full rounded-lg border border-gray-300 dark:border-gray-600 px-3 py-2 text-sm dark:bg-gray-900"
                                value={localVoice}
                                onChange={(e) => setLocalVoice(e.target.value)}
                            />
                        </div>
                    </div>
                    <div className="grid grid-cols-2 gap-4">
                        <div>
                            <label className="block text-sm mb-1">默认语速 ({localSpeed.toFixed(1)}x)</label>
                            <input
                                type="range" min="0.5" max="2.0" step="0.1"
                                className="w-full" value={localSpeed}
                                onChange={(e) => setLocalSpeed(parseFloat(e.target.value))}
                            />
                        </div>
                        <div>
                            <label className="block text-sm mb-1">默认音调 ({localPitch.toFixed(1)}x)</label>
                            <input
                                type="range" min="0.5" max="2.0" step="0.1"
                                className="w-full" value={localPitch}
                                onChange={(e) => setLocalPitch(parseFloat(e.target.value))}
                            />
                        </div>
                    </div>
                </section>

                <div className="flex justify-end gap-2 pt-2">
                    <button
                        className="rounded-lg border border-gray-300 dark:border-gray-600 px-4 py-2 text-sm hover:bg-gray-100 dark:hover:bg-gray-700"
                        onClick={onClose}
                    >
                        取消
                    </button>
                    <button
                        className="flex items-center gap-1 rounded-lg bg-blue-600 px-4 py-2 text-sm text-white hover:bg-blue-700"
                        onClick={handleSave}
                    >
                        <Save className="h-4 w-4" /> 保存设置
                    </button>
                </div>
            </div>
        </div>
    );
}
