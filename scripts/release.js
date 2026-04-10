import { readFileSync, writeFileSync } from 'fs';
import { execSync } from 'child_process';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';

/**
 * Release script: auto-bump version and push a git tag to trigger CI.
 *
 * Usage:
 *   pnpm release          # bump patch (0.1.2 → 0.1.3)
 *   pnpm release:minor    # bump minor (0.1.2 → 0.2.0)
 *   pnpm release:major    # bump major (0.1.2 → 1.0.0)
 */

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const ROOT = resolve(__dirname, '..');
const CONFIG_PATH = resolve(ROOT, 'src-tauri/tauri.conf.json');

function bumpVersion(version, type) {
    const [major, minor, patch] = version.split('.').map(Number);
    switch (type) {
        case 'major':
            return `${major + 1}.0.0`;
        case 'minor':
            return `${major}.${minor + 1}.0`;
        case 'patch':
        default:
            return `${major}.${minor}.${patch + 1}`;
    }
}

const bumpType = process.argv[2] || 'patch';

const config = JSON.parse(readFileSync(CONFIG_PATH, 'utf-8'));
const oldVersion = config.version;
const newVersion = bumpVersion(oldVersion, bumpType);

config.version = newVersion;
writeFileSync(CONFIG_PATH, JSON.stringify(config, null, 2) + '\n');

console.log(`Version bumped: ${oldVersion} → ${newVersion} (${bumpType})`);

execSync(`git add ${CONFIG_PATH}`, { stdio: 'inherit' });
execSync(`git commit -m "chore: bump version to v${newVersion}"`, { stdio: 'inherit' });

execSync(`git tag -a v${newVersion} -m "Release v${newVersion}"`, { stdio: 'inherit' });
execSync('git push', { stdio: 'inherit' });
execSync(`git push origin v${newVersion}`, { stdio: 'inherit' });

console.log(`\nPushed tag v${newVersion}. GitHub release workflow is now running.`);
console.log(`  https://github.com/mythchow/VoxFlow/actions`);
