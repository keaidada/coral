// Small client-side helpers. Everything in this module is browser-only.
// Use inside 'use client' components.

import { format as sqlFormat } from 'sql-formatter';

export const API_BASE = process.env.NEXT_PUBLIC_CORAL_SERVICE_API_URL || '';

const HISTORY_KEY = 'coral.history.v1';
const MAX_HISTORY = 100;

export function readHistory() {
  if (typeof window === 'undefined') return [];
  try {
    const raw = localStorage.getItem(HISTORY_KEY);
    return raw ? JSON.parse(raw) : [];
  } catch {
    return [];
  }
}

export function writeHistory(entries) {
  if (typeof window === 'undefined') return;
  const trimmed = entries.slice(0, MAX_HISTORY);
  localStorage.setItem(HISTORY_KEY, JSON.stringify(trimmed));
  // Broadcast so other tabs / components refresh.
  window.dispatchEvent(new CustomEvent('coral:history-changed'));
}

export function addHistory(entry) {
  const next = [
    {
      id: crypto.randomUUID(),
      createdAt: Date.now(),
      favorite: false,
      ...entry,
    },
    ...readHistory(),
  ];
  writeHistory(next);
}

export function updateHistory(id, patch) {
  const next = readHistory().map((e) => (e.id === id ? { ...e, ...patch } : e));
  writeHistory(next);
}

export function removeHistory(id) {
  writeHistory(readHistory().filter((e) => e.id !== id));
}

export function clearHistory() {
  writeHistory([]);
}

export function formatDuration(ms) {
  if (ms < 1000) return `${ms} ms`;
  return `${(ms / 1000).toFixed(2)} s`;
}

export function formatRelative(ts) {
  const diff = Date.now() - ts;
  if (diff < 60_000) return 'just now';
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`;
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)}h ago`;
  const d = new Date(ts);
  return d.toLocaleDateString();
}

export function sqlFormatterDialect(lang) {
  switch (lang) {
    case 'hive':
      return 'hive';
    case 'spark':
      return 'spark';
    case 'trino':
      return 'trino';
    case 'gaussdb':
    case 'postgres':
    case 'postgresql':
      return 'postgresql';
    default:
      return 'sql';
  }
}

export function formatSql(sql, dialect = 'postgresql') {
  return sqlFormat(sql, {
    language: sqlFormatterDialect(dialect),
    keywordCase: 'upper',
    tabWidth: 2,
    linesBetweenQueries: 2,
  });
}

// Parse the Java-style plain-text translate response:
//   "Original query in <Source>:\n<q>\nTranslated to <Target>:\n<sql>\n"
// so we can render structured cards. Falls back to raw text.
export function parseTranslateBody(body) {
  if (!body) return { raw: body, original: '', translated: body };
  const translatedIdx = body.indexOf('Translated to');
  if (!body.startsWith('Original query in') || translatedIdx < 0) {
    return { raw: body, original: '', translated: body };
  }
  const header1End = body.indexOf('\n');
  const sourceLabel = body
    .slice('Original query in'.length, header1End)
    .replace(':', '')
    .trim();
  const original = body.slice(header1End + 1, translatedIdx).trimEnd();
  const afterTranslated = body.slice(translatedIdx);
  const header2End = afterTranslated.indexOf('\n');
  const targetLabel = afterTranslated
    .slice('Translated to'.length, header2End)
    .replace(':', '')
    .trim();
  const translated = afterTranslated.slice(header2End + 1).trimEnd();
  return { raw: body, sourceLabel, targetLabel, original, translated };
}
