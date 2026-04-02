import { useState } from 'react';
import { Plus, Pencil, Trash2, Save, X } from 'lucide-react';
import { useCharacterStore } from '../../store/characterStore';
import type { CharacterInput, Character } from '../../types';

const defaultInput: CharacterInput = {
    name: '',
    tts_engine: 'edge-tts',
    voice_name: 'zh-CN-XiaoxiaoNeural',
    speed: 1.0,
    pitch: 1.0,
};

export default function CharacterPanel() {
    const { characters, createCharacter, updateCharacter, deleteCharacter } = useCharacterStore();
    const [editing, setEditing] = useState<string | null>(null);
    const [form, setForm] = useState<CharacterInput>(defaultInput);
    const [isCreating, setIsCreating] = useState(false);

    const startCreate = () => {
        setIsCreating(true);
        setEditing(null);
        setForm(defaultInput);
    };

    const startEdit = (c: Character) => {
        setEditing(c.id);
        setIsCreating(false);
        setForm({
            name: c.name,
            tts_engine: c.tts_engine,
            voice_name: c.voice_name,
            speed: c.speed,
            pitch: c.pitch,
        });
    };

    const cancel = () => {
        setEditing(null);
        setIsCreating(false);
        setForm(defaultInput);
    };

    const handleSave = async () => {
        if (!form.name.trim()) return;
        if (isCreating) {
            await createCharacter(form);
        } else if (editing) {
            await updateCharacter(editing, form);
        }
        cancel();
    };

    const handleDelete = async (id: string) => {
        if (window.confirm('删除角色后，关联的剧本行将变为未分配状态。确定删除？')) {
            await deleteCharacter(id);
        }
    };

    const renderForm = () => (
        <div className="rounded-xl border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 p-5 space-y-4">
            <div>
                <label className="block text-sm font-medium mb-1">角色名称</label>
                <input
                    className="w-full rounded-lg border border-gray-300 dark:border-gray-600 px-3 py-2 text-sm dark:bg-gray-900 focus:outline-none focus:ring-2 focus:ring-blue-500"
                    value={form.name}
                    onChange={(e) => setForm({ ...form, name: e.target.value })}
                    placeholder="输入角色名称"
                    autoFocus
                />
            </div>
            <div className="grid grid-cols-2 gap-4">
                <div>
                    <label className="block text-sm font-medium mb-1">TTS 引擎</label>
                    <select
                        className="w-full rounded-lg border border-gray-300 dark:border-gray-600 px-3 py-2 text-sm dark:bg-gray-900"
                        value={form.tts_engine}
                        onChange={(e) => setForm({ ...form, tts_engine: e.target.value as 'edge-tts' | 'azure-tts' | 'dashscope-tts' })}
                    >
                        <option value="edge-tts">Edge TTS</option>
                        <option value="azure-tts">Azure TTS</option>
                        <option value="dashscope-tts">阿里百炼 (CosyVoice)</option>
                    </select>
                </div>
                <div>
                    <label className="block text-sm font-medium mb-1">音色</label>
                    <input
                        className="w-full rounded-lg border border-gray-300 dark:border-gray-600 px-3 py-2 text-sm dark:bg-gray-900"
                        value={form.voice_name}
                        onChange={(e) => setForm({ ...form, voice_name: e.target.value })}
                        placeholder="zh-CN-XiaoxiaoNeural"
                    />
                </div>
            </div>
            <div className="grid grid-cols-2 gap-4">
                <div>
                    <label className="block text-sm font-medium mb-1">语速 ({form.speed.toFixed(1)}x)</label>
                    <input
                        type="range"
                        min="0.5"
                        max="2.0"
                        step="0.1"
                        className="w-full"
                        value={form.speed}
                        onChange={(e) => setForm({ ...form, speed: parseFloat(e.target.value) })}
                    />
                </div>
                <div>
                    <label className="block text-sm font-medium mb-1">音调 ({form.pitch.toFixed(1)}x)</label>
                    <input
                        type="range"
                        min="0.5"
                        max="2.0"
                        step="0.1"
                        className="w-full"
                        value={form.pitch}
                        onChange={(e) => setForm({ ...form, pitch: parseFloat(e.target.value) })}
                    />
                </div>
            </div>
            <div className="flex gap-2 justify-end">
                <button
                    className="flex items-center gap-1 rounded-lg border border-gray-300 dark:border-gray-600 px-3 py-1.5 text-sm hover:bg-gray-100 dark:hover:bg-gray-700"
                    onClick={cancel}
                >
                    <X className="h-4 w-4" /> 取消
                </button>
                <button
                    className="flex items-center gap-1 rounded-lg bg-blue-600 px-3 py-1.5 text-sm text-white hover:bg-blue-700"
                    onClick={handleSave}
                >
                    <Save className="h-4 w-4" /> 保存
                </button>
            </div>
        </div>
    );

    return (
        <div className="mx-auto max-w-3xl px-6 py-8">
            <div className="flex items-center justify-between mb-6">
                <h2 className="text-xl font-bold">角色管理</h2>
                <button
                    className="flex items-center gap-2 rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700"
                    onClick={startCreate}
                >
                    <Plus className="h-4 w-4" /> 新建角色
                </button>
            </div>

            {isCreating && renderForm()}

            <div className="space-y-3 mt-4">
                {characters.map((c) =>
                    editing === c.id ? (
                        <div key={c.id}>{renderForm()}</div>
                    ) : (
                        <div
                            key={c.id}
                            className="flex items-center justify-between rounded-xl border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 p-4"
                        >
                            <div>
                                <p className="font-medium">{c.name}</p>
                                <p className="text-sm text-gray-500 dark:text-gray-400">
                                    {c.tts_engine} · {c.voice_name} · 语速 {c.speed}x · 音调 {c.pitch}x
                                </p>
                            </div>
                            <div className="flex gap-2">
                                <button
                                    className="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-500"
                                    onClick={() => startEdit(c)}
                                    aria-label={`Edit ${c.name}`}
                                >
                                    <Pencil className="h-4 w-4" />
                                </button>
                                <button
                                    className="p-1.5 rounded-lg hover:bg-red-50 dark:hover:bg-red-900/30 text-gray-500 hover:text-red-500"
                                    onClick={() => handleDelete(c.id)}
                                    aria-label={`Delete ${c.name}`}
                                >
                                    <Trash2 className="h-4 w-4" />
                                </button>
                            </div>
                        </div>
                    ),
                )}
                {characters.length === 0 && !isCreating && (
                    <p className="text-center text-gray-500 py-12">暂无角色，点击"新建角色"添加</p>
                )}
            </div>
        </div>
    );
}
