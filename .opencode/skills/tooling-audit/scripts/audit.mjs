#!/usr/bin/env node
// tooling-audit — механические проверки конфигов .opencode/ (агенты и навыки).
//
// Запуск: node .opencode/skills/tooling-audit/scripts/audit.mjs [каталог .opencode]
//
// Без сети и внешних зависимостей — только встроенные модули Node. Скрипт ничего
// не меняет и всегда выходит с кодом 0: находки не должны ронять пайплайн, это
// advisory-отчёт, а не блокирующий гейт.

import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// --- Схема (держать в синхроне при изменении схемы агентов/навыков) -----------

const AGENT_ALLOWED_KEYS = new Set([
  'name', 'description', 'mode', 'model', 'temperature', 'permission',
]);
const SKILL_ALLOWED_KEYS = new Set(['name', 'description', 'license']);
const DEPRECATED_KEYS = new Set(['tools', 'permissions']);
const PERMISSION_KEYS = new Set([
  'read', 'edit', 'glob', 'grep', 'list', 'bash', 'task', 'webfetch', 'websearch',
  'lsp', 'skill', 'question', 'external_directory', 'todowrite', 'doom_loop',
]);

// Глобальные (встроенные) навыки — известны заранее, чтобы не считать их висячими.
const GLOBAL_SKILLS = new Set([
  'agents-md', 'analyst', 'api-design-principles', 'api-documentation-generator',
  'backend-architect', 'brainstorming', 'business-requirement-artifacts', 'cavecrew',
  'caveman', 'caveman-commit', 'caveman-review', 'code-testing-agent', 'codebase-design',
  'composition-patterns', 'context7-cli', 'design-doc-mermaid', 'deslop', 'docx',
  'dotnet-webapi', 'executing-plans', 'faceless-explainer', 'find-skills',
  'frontend-api-integration-patterns', 'frontend-design', 'frontend-dev', 'genai-developer',
  'general-video', 'generate-testability-wrappers', 'gitlab-to-gitverse',
  'gitverse-cicd-workflows', 'goldie', 'grilling', 'hindsight-docs', 'hyperframes',
  'jenkins-to-gitverse', 'migrate-mstest-v1v2-to-v3', 'migrate-xunit-to-xunit-v3',
  'obsidian-wiki-maintainer', 'opencode', 'opencode-desktop-updater', 'opensource-guide-coach',
  'pdf', 'pm', 'pptx', 'product-requirements', 'prompt-images', 'qa', 'react-best-practices',
  'report', 'review', 'senior-discovery-product', 'senior-go-developer',
  'senior-system-analyst', 'setup-pre-commit', 'skill-creator', 'split-epic-by-certainty',
  'stop-slop', 'subagent-driven-development', 'systematic-debugging',
  'test-driven-development', 'typescript-advanced-types', 'verification-before-completion',
  'web-design-guidelines', 'webapp-testing', 'writing-plans', 'xlsx',
]);

// @-упоминания, которые не являются агентами (npm-скоупы `@scope/x` отсекаются
// lookahead'ом в регулярке и сюда не попадают).
const MENTION_DENYLIST = new Set([
  'manual', 'ts-ignore', 'ts-expect-error', 'ts-check', 'ts-nocheck',
  'media', 'import', 'keyframes', 'font-face', 'supports', 'charset', 'page',
  'namespace', 'layer', 'container', 'property', 'apply', 'use',
]);

// Backtick-токены вида kebab-case, которые не являются навыками (эвристический
// стоп-лист: атрибуты, имена крейтов, инструменты, общие термины).
const TOKEN_DENYLIST = new Set([
  'data-md', 'md-block', 'md-core', 'plugin-proto', 'plugin-host', 'src-tauri',
  'tauri-driver', 'host-call',
  'size-gate', 'opt-level', 'text-align', 'front-end', 'back-end', 'read-only',
  'read-write', 'no-op', 'kebab-case', 'snake-case', 'pull-request', 'one-shot',
  'end-to-end', 'type-level', 'third-party', 'self-test', 'word-count',
  'format-selection', 'export-html', 'allow-list', 'wall-clock', 'time-out',
]);

// Проектно-специфичные токены, которых не должно быть в телах агентов
// (инвариант: агент определяет структуру проекта сам).
const HARDCODE_TOKENS = [
  { token: 'crates/', label: 'путь крейтов' },
  { token: 'plugins/', label: 'путь плагинов' },
  { token: 'docs/adr', label: 'путь ADR' },
  { token: 'DESIGN_DOC', label: 'имя документа' },
  { token: 'src-tauri', label: 'путь Tauri-шелла' },
  { token: 'md-core', label: 'имя крейта' },
  { token: 'plugin-proto', label: 'имя крейта' },
  { token: 'plugin-host', label: 'имя крейта' },
  { token: 'darmark', label: 'имя проекта' },
];

const STATUS = { OK: 'OK', FAIL: 'FAIL', WARN: '\u26a0\ufe0f' };
const LEVEL_ORDER = { Critical: 0, Important: 1, Minor: 2 };

// --- Разбор frontmatter без внешнего YAML -------------------------------------

function splitFrontmatter(raw) {
  const text = raw.replace(/^\uFEFF/, '');
  const lines = text.split(/\r?\n/);
  if (lines[0].trim() !== '---') {
    return { ok: false, reason: 'нет открывающего разделителя ---', body: text };
  }
  let end = -1;
  for (let i = 1; i < lines.length; i++) {
    if (lines[i].trim() === '---') { end = i; break; }
  }
  if (end === -1) {
    return { ok: false, reason: 'нет закрывающего разделителя ---', body: text };
  }
  return {
    ok: true,
    fmLines: lines.slice(1, end),
    body: lines.slice(end + 1).join('\n'),
  };
}

function parseFrontmatter(fmLines) {
  const keys = [];
  const values = {};
  const children = {};
  let current = null;
  let foldedKey = null;
  for (const line of fmLines) {
    if (line.trim() === '') continue;
    const top = line.match(/^([A-Za-z_][A-Za-z0-9_-]*):(.*)$/);
    if (top) {
      current = top[1];
      keys.push(current);
      const val = top[2].trim();
      values[current] = val;
      foldedKey = (val === '>' || val === '|') ? current : null;
      if (current === 'permission') children.permission = [];
      continue;
    }
    const child = line.match(/^\s+([A-Za-z_][A-Za-z0-9_-]*):/);
    if (child && current === 'permission') {
      children.permission.push(child[1]);
      continue;
    }
    // Folded/literal block scalar: строки с отступом после > или |
    if (foldedKey && /^\s+/.test(line)) {
      values[foldedKey] = (values[foldedKey] ? values[foldedKey] + '\n' : '') + line.trim();
    }
  }
  return { keys, values, children };
}

// --- Хеширование и дубли абзацев ----------------------------------------------

function sha1(s) {
  return crypto.createHash('sha1').update(s).digest('hex');
}

function normalizeParagraph(p) {
  return p
    .toLowerCase()
    .replace(/[`*_>#|~[\]()]/g, ' ')
    .replace(/\s+/g, ' ')
    .replace(/[^\p{L}\p{N} ]/gu, '')
    .trim();
}

function paragraphs(body) {
  const noCode = body.replace(/```[\s\S]*?```/g, '');
  return noCode
    .split(/\n\s*\n/)
    .map((p) => p.trim())
    .filter((p) => p.length > 0);
}

// --- Сбор файлов ---------------------------------------------------------------

function readAgents(opencodeDir) {
  const dir = path.join(opencodeDir, 'agents');
  if (!fs.existsSync(dir)) return [];
  return fs.readdirSync(dir, { withFileTypes: true })
    .filter((e) => e.isFile() && e.name.endsWith('.md'))
    .map((e) => ({
      kind: 'agent',
      fileName: path.basename(e.name, '.md'),
      label: `.opencode/agents/${e.name}`,
      fullPath: path.join(dir, e.name),
    }))
    .sort((a, b) => a.fileName.localeCompare(b.fileName));
}

function readSkills(opencodeDir) {
  const dir = path.join(opencodeDir, 'skills');
  if (!fs.existsSync(dir)) return [];
  return fs.readdirSync(dir, { withFileTypes: true })
    .filter((e) => e.isDirectory())
    .map((e) => ({ dirName: e.name, fullPath: path.join(dir, e.name, 'SKILL.md') }))
    .filter((e) => fs.existsSync(e.fullPath))
    .map((e) => ({
      kind: 'skill',
      fileName: e.dirName,
      label: `.opencode/skills/${e.dirName}/SKILL.md`,
      fullPath: e.fullPath,
    }))
    .sort((a, b) => a.fileName.localeCompare(b.fileName));
}

// --- Аудит ---------------------------------------------------------------------

function audit(opencodeDir) {
  const agents = readAgents(opencodeDir);
  const skills = readSkills(opencodeDir);
  const entries = [...agents, ...skills];

  const agentNames = new Set(agents.map((a) => a.fileName));
  const projectSkillNames = new Set(skills.map((s) => s.fileName));

  const findings = [];
  const checksByFile = new Map();
  const add = (level, file, check, message) => findings.push({ level, file, check, message });
  const setCheck = (label, check, status) => {
    if (!checksByFile.has(label)) checksByFile.set(label, {});
    checksByFile.get(label)[check] = status;
  };

  const parsed = [];
  for (const entry of entries) {
    const raw = fs.readFileSync(entry.fullPath, 'utf8');
    const split = splitFrontmatter(raw);
    const isAgent = entry.kind === 'agent';

    if (!split.ok) {
      setCheck(entry.label, 'frontmatter', STATUS.FAIL);
      add('Critical', entry.label, 'frontmatter', `невалидный frontmatter: ${split.reason}`);
      parsed.push({ entry, split, fm: null, body: split.body, isAgent });
      continue;
    }
    setCheck(entry.label, 'frontmatter', STATUS.OK);
    const fm = parseFrontmatter(split.fmLines);
    parsed.push({ entry, split, fm, body: split.body, isAgent });

    // Ключи: устаревшие и неизвестные.
    const allowed = isAgent ? AGENT_ALLOWED_KEYS : SKILL_ALLOWED_KEYS;
    let keysStatus = STATUS.OK;
    const seenKeys = new Set();
    for (const key of fm.keys) {
      if (seenKeys.has(key)) {
        keysStatus = STATUS.FAIL;
        add('Important', entry.label, 'keys', `ключ \`${key}\` продублирован во frontmatter`);
      }
      seenKeys.add(key);
      if (DEPRECATED_KEYS.has(key)) {
        keysStatus = STATUS.FAIL;
        add('Critical', entry.label, 'keys',
          `устаревший ключ \`${key}\` — используй \`permission\` (в единственном числе)`);
      } else if (!allowed.has(key)) {
        keysStatus = STATUS.FAIL;
        add('Important', entry.label, 'keys',
          `недопустимый ключ frontmatter \`${key}\``);
      }
    }
    if (!fm.values.name) {
      keysStatus = STATUS.FAIL;
      add('Critical', entry.label, 'keys', 'отсутствует обязательный ключ `name`');
    }
    if (!fm.values.description) {
      keysStatus = STATUS.FAIL;
      add('Important', entry.label, 'keys', 'отсутствует ключ `description`');
    }
    setCheck(entry.label, 'keys', keysStatus);

    // name ↔ файл/каталог.
    const declared = fm.values.name;
    if (declared && declared !== entry.fileName) {
      setCheck(entry.label, 'name', STATUS.FAIL);
      add('Important', entry.label, 'name',
        `\`name: ${declared}\` не совпадает с ${isAgent ? 'именем файла' : 'именем каталога'} \`${entry.fileName}\``);
    } else {
      setCheck(entry.label, 'name', STATUS.OK);
    }

    if (isAgent) {
      // permission.
      const permKeys = fm.children.permission || [];
      if (!fm.keys.includes('permission')) {
        setCheck(entry.label, 'permission', STATUS.WARN);
        add('Minor', entry.label, 'permission', 'нет блока `permission` (ожидается по схеме агента)');
      } else {
        let permStatus = STATUS.OK;
        for (const pk of permKeys) {
          if (!PERMISSION_KEYS.has(pk)) {
            permStatus = STATUS.FAIL;
            add('Critical', entry.label, 'permission',
              `недопустимый ключ permission \`${pk}\` — нет в актуальном наборе`);
          }
        }
        setCheck(entry.label, 'permission', permStatus);
      }

      // model.
      const model = fm.values.model || '';
      if (!model) {
        setCheck(entry.label, 'model', STATUS.FAIL);
        add('Important', entry.label, 'model', 'отсутствует поле `model`');
      } else if (!/^[a-z0-9][a-z0-9._-]*\/[a-z0-9][a-z0-9._-]*$/i.test(model)) {
        setCheck(entry.label, 'model', STATUS.FAIL);
        add('Important', entry.label, 'model',
          `подозрительный формат \`model: ${model}\` (ожидается \`провайдер/модель\`)`);
      } else {
        setCheck(entry.label, 'model', STATUS.OK);
      }
    }
  }

  // Висячие ссылки (по телам и описаниям).
  for (const { entry, fm, body } of parsed) {
    const text = `${fm ? fm.values.description || '' : ''}\n${body}`;
    let refStatus = STATUS.OK;
    const seen = new Set();
    for (const m of text.matchAll(/@([a-z][a-z0-9]*(?:-[a-z0-9]+)*)(?![\w/-])/g)) {
      const name = m[1];
      if (MENTION_DENYLIST.has(name) || seen.has('@' + name)) continue;
      seen.add('@' + name);
      if (agentNames.has(name)) continue;
      if (projectSkillNames.has(name) || GLOBAL_SKILLS.has(name)) {
        refStatus = STATUS.WARN;
        add('Minor', entry.label, 'refs',
          `\`@${name}\` — это навык, а не агент (упомянут как агент)`);
      } else {
        refStatus = STATUS.FAIL;
        add('Important', entry.label, 'refs',
          `висячая ссылка на агента \`@${name}\` — такого агента нет`);
      }
    }
    for (const m of text.matchAll(/`([a-z0-9]+(?:-[a-z0-9]+)+)`/g)) {
      const name = m[1];
      if (TOKEN_DENYLIST.has(name) || seen.has(name)) continue;
      seen.add(name);
      if (projectSkillNames.has(name) || GLOBAL_SKILLS.has(name)) continue;
      if (agentNames.has(name)) continue;
      refStatus = STATUS.WARN;
      add('Minor', entry.label, 'refs',
        `возможная висячая ссылка на навык \`${name}\``);
    }
    setCheck(entry.label, 'refs', refStatus);
  }

  // Хардкод путей/имён проекта — только агенты (описание + тело).
  for (const { entry, fm, body, isAgent } of parsed) {
    if (!isAgent) continue;
    const text = `${fm ? fm.values.description || '' : ''}\n${body}`;
    let hcStatus = STATUS.OK;
    const reported = new Set();
    for (const { token, label } of HARDCODE_TOKENS) {
      const re = new RegExp(token.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'i');
      if (re.test(text) && !reported.has(token)) {
        reported.add(token);
        hcStatus = STATUS.WARN;
        add('Minor', entry.label, 'hardcode',
          `хардкод в теле/описании агента: \`${token}\` (${label})`);
      }
    }
    setCheck(entry.label, 'hardcode', hcStatus);
  }

  // Дубли абзацев.
  const paraMap = new Map(); // hash -> { preview, files: Map<label, count> }
  for (const { entry, body } of parsed) {
    for (const p of paragraphs(body)) {
      const norm = normalizeParagraph(p);
      if (norm.length < 80) continue;
      const h = sha1(norm);
      if (!paraMap.has(h)) paraMap.set(h, { preview: norm.slice(0, 90), files: new Map() });
      const rec = paraMap.get(h);
      rec.files.set(entry.label, (rec.files.get(entry.label) || 0) + 1);
    }
  }
  const dupStatus = new Map();
  const bump = (label, status) => {
    const cur = dupStatus.get(label);
    if (cur !== STATUS.FAIL) dupStatus.set(label, status);
  };
  for (const [, rec] of paraMap) {
    const files = [...rec.files.entries()];
    for (const [label, count] of files) {
      if (count >= 2) {
        bump(label, STATUS.FAIL);
        add('Important', label, 'duplicates',
          `абзац продублирован внутри файла: «${rec.preview}…»`);
      }
    }
    if (files.length >= 2) {
      for (const [label] of files) bump(label, STATUS.WARN);
      add('Minor', files[0][0], 'duplicates',
        `абзац совпадает между файлами (${files.map(([l]) => l).join(', ')}): «${rec.preview}…»`);
    }
  }
  for (const { entry } of parsed) {
    setCheck(entry.label, 'duplicates', dupStatus.get(entry.label) || STATUS.OK);
  }

  return { agents, skills, entries, checksByFile, findings };
}

// --- Вывод ---------------------------------------------------------------------

function render(opencodeDir, result) {
  const { agents, skills, entries, checksByFile, findings } = result;
  const out = [];
  out.push('tooling-audit — механический аудит .opencode/');
  out.push(`Каталог: ${opencodeDir}`);
  out.push(`Агентов: ${agents.length}, навыков: ${skills.length}`);
  out.push('');
  out.push('ТАБЛИЦА ПРОВЕРОК (файл → проверка → статус)');

  const checkOrder = ['frontmatter', 'keys', 'permission', 'model', 'name', 'refs', 'duplicates', 'hardcode'];
  const rows = [];
  for (const { label } of entries) {
    const checks = checksByFile.get(label) || {};
    for (const check of checkOrder) {
      if (checks[check] === undefined) continue;
      rows.push([label, check, checks[check]]);
    }
  }
  const wFile = Math.max(4, ...rows.map((r) => r[0].length));
  const wCheck = Math.max(5, ...rows.map((r) => r[1].length));
  for (const [file, check, status] of rows) {
    out.push(`  ${file.padEnd(wFile)}  ${check.padEnd(wCheck)}  ${status}`);
  }

  out.push('');
  out.push('НАХОДКИ');
  if (findings.length === 0) {
    out.push('  — механических находок нет');
  } else {
    const sorted = [...findings].sort(
      (a, b) => LEVEL_ORDER[a.level] - LEVEL_ORDER[b.level] || a.file.localeCompare(b.file),
    );
    let lastLevel = null;
    for (const f of sorted) {
      if (f.level !== lastLevel) {
        out.push('');
        out.push(`[${f.level.toUpperCase()}]`);
        lastLevel = f.level;
      }
      out.push(`  ${f.file} — ${f.message}`);
    }
  }

  const fails = rows.filter((r) => r[2] === STATUS.FAIL).length;
  const warns = rows.filter((r) => r[2] === STATUS.WARN).length;
  const oks = rows.filter((r) => r[2] === STATUS.OK).length;
  out.push('');
  out.push(`ИТОГО: файлов ${entries.length}, проверок ${rows.length} — OK ${oks}, FAIL ${fails}, ${STATUS.WARN} ${warns}`);
  out.push('Примечание: доступность `model` офлайн не проверяется — подтверждай вручную (⚠️).');
  return out.join('\n');
}

function main() {
  const opencodeDir = process.argv[2]
    ? path.resolve(process.argv[2])
    : path.resolve(__dirname, '..', '..', '..');

  if (!fs.existsSync(opencodeDir)) {
    console.error(`Каталог не найден: ${opencodeDir}`);
    process.exit(1);
  }

  const result = audit(opencodeDir);
  console.log(render(opencodeDir, result));
}

main();
