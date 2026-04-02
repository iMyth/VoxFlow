import { useEffect, useState } from 'react';
import { Plus } from 'lucide-react';
import { useProjectStore } from '../../store/projectStore';
import ProjectCard from './ProjectCard';

interface ProjectListProps {
    onSelectProject: (projectId: string) => void;
}

export default function ProjectList({ onSelectProject }: ProjectListProps) {
    const { projects, fetchProjects, createProject, deleteProject } = useProjectStore();
    const [newName, setNewName] = useState('');
    const [showInput, setShowInput] = useState(false);

    useEffect(() => {
        fetchProjects();
    }, [fetchProjects]);

    const handleCreate = async () => {
        const name = newName.trim();
        if (!name) return;
        await createProject(name);
        setNewName('');
        setShowInput(false);
    };

    const handleDelete = async (id: string) => {
        if (window.confirm('确定要删除此项目吗？所有关联数据将被清除。')) {
            await deleteProject(id);
        }
    };

    return (
        <div className="mx-auto max-w-4xl px-6 py-10">
            <div className="flex items-center justify-between mb-8">
                <h1 className="text-2xl font-bold">VoxFlow 项目</h1>
                <button
                    className="flex items-center gap-2 rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 transition"
                    onClick={() => setShowInput(true)}
                >
                    <Plus className="h-4 w-4" />
                    新建项目
                </button>
            </div>

            {showInput && (
                <div className="mb-6 flex gap-3">
                    <input
                        className="flex-1 rounded-lg border border-gray-300 px-4 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 dark:border-gray-600 dark:bg-gray-800"
                        placeholder="输入项目名称..."
                        value={newName}
                        onChange={(e) => setNewName(e.target.value)}
                        onKeyDown={(e) => e.key === 'Enter' && handleCreate()}
                        autoFocus
                    />
                    <button
                        className="rounded-lg bg-blue-600 px-4 py-2 text-sm text-white hover:bg-blue-700"
                        onClick={handleCreate}
                    >
                        创建
                    </button>
                    <button
                        className="rounded-lg border border-gray-300 px-4 py-2 text-sm hover:bg-gray-100 dark:border-gray-600 dark:hover:bg-gray-700"
                        onClick={() => { setShowInput(false); setNewName(''); }}
                    >
                        取消
                    </button>
                </div>
            )}

            {projects.length === 0 ? (
                <p className="text-center text-gray-500 py-20">暂无项目，点击"新建项目"开始创作</p>
            ) : (
                <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
                    {projects.map((p) => (
                        <ProjectCard
                            key={p.id}
                            project={p}
                            onClick={() => onSelectProject(p.id)}
                            onDelete={() => handleDelete(p.id)}
                        />
                    ))}
                </div>
            )}
        </div>
    );
}
