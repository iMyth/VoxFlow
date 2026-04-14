import { readFileSync, writeFileSync } from 'fs';
import { execSync } from 'child_process';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';

/**
 * Release script: auto-bump version and push a git tag to trigger CI.
 *
 * Usage:
 *   pnpm release          # bump patch (0.1.2 -> 0.1.3)
 *   pnpm release:minor    # bump minor (0.1.2 -> 0.2.0)
 *   pnpm release:major    # bump major (0.1.2 -> 1.0.0)
 *   pnpm release --dry-run # preview changes without committing
 */

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const ROOT = resolve(__dirname, '..');
const CONFIG_PATH = resolve(ROOT, 'src-tauri/tauri.conf.json');
const PACKAGE_PATH = resolve(ROOT, 'package.json');

const isDryRun = process.argv.includes('--dry-run');

function run(cmd, options) {
    console.log(`$ ${cmd}`);
    if (isDryRun) return;
    try {
        return execSync(cmd, { stdio: 'inherit', ...options });
    } catch (err) {
        console.error(`\nError executing: ${cmd}`);
        if (err.stdout) console.error(err.stdout.toString());
        if (err.stderr) console.error(err.stderr.toString());
        process.exit(1);
    }
}

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

// ── Safety checks ─────────────────────────────────────────

// Check that we are on main branch
const currentBranch = execSync('git rev-parse --abbrev-ref HEAD').toString().trim();
if (currentBranch !== 'main') {
    console.error(`Error: release must be run on the 'main' branch (currently on '${currentBranch}').`);
    process.exit(1);
}

// Check that the working tree is clean
const status = execSync('git status --porcelain').toString().trim();
if (status) {
    console.error('Error: working tree is not clean. Commit or stash changes before releasing.');
    console.error('Uncommitted files:');
    console.error(status);
    process.exit(1);
}

// ── Version bump ──────────────────────────────────────────

const bumpType = process.argv[2] === '--dry-run'
    ? (process.argv[3] || 'patch')
    : (process.argv[2] || 'patch');

const config = JSON.parse(readFileSync(CONFIG_PATH, 'utf-8'));
const oldVersion = config.version;
const newVersion = bumpVersion(oldVersion, bumpType);

config.version = newVersion;
writeFileSync(CONFIG_PATH, JSON.stringify(config, null, 2) + '\n');

const pkg = JSON.parse(readFileSync(PACKAGE_PATH, 'utf-8'));
pkg.version = newVersion;
writeFileSync(PACKAGE_PATH, JSON.stringify(pkg, null, 2) + '\n');

console.log(`Version bumped: ${oldVersion} -> ${newVersion} (${bumpType})`);

if (isDryRun) {
    console.log('\n[dry-run] Stopping here. No changes committed or pushed.');
    console.log('Run without --dry-run to actually release.');

    // Restore files to original state
    config.version = oldVersion;
    writeFileSync(CONFIG_PATH, JSON.stringify(config, null, 2) + '\n');
    pkg.version = oldVersion;
    writeFileSync(PACKAGE_PATH, JSON.stringify(pkg, null, 2) + '\n');
    console.log('Files restored to original versions.');
    process.exit(0);
}

// ── Commit, tag, push ────────────────────────────────────

run(`git add ${CONFIG_PATH} ${PACKAGE_PATH}`);
run(`git commit -m "chore: bump version to v${newVersion}"`);
run(`git tag -a v${newVersion} -m "Release v${newVersion}"`);
run('git push');
run(`git push origin v${newVersion}`);

console.log(`\nPushed tag v${newVersion}. GitHub release workflow is now running.`);
console.log(`  https://github.com/iMyth/VoxFlow/actions`);
