#!/usr/bin/env node
/* eslint-disable no-undef */
/**
 * 加载 .env 环境变量后执行后续命令
 * 用法: node scripts/load-env.mjs <command> [args...]
 */
import { readFileSync, existsSync } from 'fs';
import { join, dirname } from 'path';
import { execSync } from 'child_process';
import { fileURLToPath } from 'url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const envPath = join(ROOT, '.env');

if (existsSync(envPath)) {
  for (const line of readFileSync(envPath, 'utf-8').split('\n')) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith('#')) continue;
    const eq = trimmed.indexOf('=');
    if (eq === -1) continue;
    const key = trimmed.slice(0, eq).trim();
    const val = trimmed.slice(eq + 1).trim();
    // 只加载 TAURI_ 前缀的变量，避免覆盖 Vite 的 .env.production
    if (key.startsWith('TAURI_') && !process.env[key]) process.env[key] = val;
  }
}

const cmd = process.argv.slice(2).join(' ');
if (!cmd) {
  console.error('用法: node scripts/load-env.mjs <command>');
  process.exit(1);
}

try {
  execSync(cmd, { cwd: ROOT, stdio: 'inherit', env: process.env });
} catch {
  process.exit(1);
}
