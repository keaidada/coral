'use client';

import { useEffect, useMemo, useState } from 'react';
import { useSearchParams, useRouter } from 'next/navigation';
import {
  Star,
  StarOff,
  Search,
  Trash2,
  Play,
  Copy,
  Check,
  Clock,
  ChevronDown,
  ChevronRight,
} from 'lucide-react';
import { Card, PageHeader, Button, Pill } from '@/app/components/ui';
import {
  formatDuration,
  formatRelative,
  parseTranslateBody,
  readHistory,
  removeHistory,
  updateHistory,
  clearHistory,
} from '@/app/lib/client';
import { useT } from '@/app/lib/i18n';

export default function HistoryPage() {
  const t = useT();
  const [entries, setEntries] = useState([]);
  const [query, setQuery] = useState('');
  const [showFavsOnly, setShowFavsOnly] = useState(false);
  const [expandedId, setExpandedId] = useState(null);
  const params = useSearchParams();
  const router = useRouter();

  useEffect(() => {
    setEntries(readHistory());
    const refresh = () => setEntries(readHistory());
    window.addEventListener('coral:history-changed', refresh);
    return () => window.removeEventListener('coral:history-changed', refresh);
  }, []);

  useEffect(() => {
    const openId = params.get('open');
    if (openId) setExpandedId(openId);
  }, [params]);

  const filtered = useMemo(() => {
    return entries
      .filter((e) => !showFavsOnly || e.favorite)
      .filter((e) => {
        if (!query.trim()) return true;
        const q = query.toLowerCase();
        return (
          e.query.toLowerCase().includes(q) ||
          e.response?.toLowerCase().includes(q) ||
          `${e.source} ${e.target}`.toLowerCase().includes(q)
        );
      });
  }, [entries, query, showFavsOnly]);

  return (
    <div>
      <PageHeader
        title={t('history.title')}
        subtitle={t('history.subtitle', { n: entries.length })}
        actions={
          <Button
            variant='secondary'
            size='sm'
            onClick={() => {
              if (confirm(t('history.clearConfirm'))) clearHistory();
            }}
            disabled={entries.length === 0}
          >
            <Trash2 size={14} />
            {t('history.clear')}
          </Button>
        }
      />

      <Card className='mb-4'>
        <div className='px-4 py-2.5 flex items-center gap-3 border-b border-ink-200 dark:border-ink-800'>
          <Search size={14} className='text-ink-400' />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t('history.filter')}
            className='flex-1 bg-transparent text-sm outline-none text-ink-800 dark:text-ink-100'
          />
          <label className='flex items-center gap-2 text-xs text-ink-600 dark:text-ink-400 select-none cursor-pointer'>
            <input
              type='checkbox'
              checked={showFavsOnly}
              onChange={(e) => setShowFavsOnly(e.target.checked)}
              className='rounded border-ink-300 text-coral-500 focus:ring-coral-500'
            />
            {t('history.favsOnly')}
          </label>
        </div>
        {filtered.length === 0 ? (
          <div className='py-10 text-center text-sm text-ink-500'>
            {entries.length === 0 ? t('history.empty') : t('history.noMatch')}
          </div>
        ) : (
          <ul className='divide-y divide-ink-200 dark:divide-ink-800'>
            {filtered.map((e) => (
              <HistoryRow
                key={e.id}
                entry={e}
                expanded={expandedId === e.id}
                onToggle={() =>
                  setExpandedId(expandedId === e.id ? null : e.id)
                }
                onRerun={() => router.push('/')}
              />
            ))}
          </ul>
        )}
      </Card>
    </div>
  );
}

function HistoryRow({ entry, expanded, onToggle, onRerun }) {
  const t = useT();
  const [copied, setCopied] = useState(false);
  const parsed = entry.parsed || parseTranslateBody(entry.response || '');

  async function copy() {
    await navigator.clipboard.writeText(
      parsed.translated || entry.response || '',
    );
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  }

  return (
    <li>
      <div
        className='flex items-center gap-3 px-5 py-3 hover:bg-ink-50/50 dark:hover:bg-ink-950/40 transition cursor-pointer'
        onClick={onToggle}
      >
        <button
          onClick={(e) => {
            e.stopPropagation();
            updateHistory(entry.id, { favorite: !entry.favorite });
          }}
          className='shrink-0 text-ink-400 hover:text-coral-500'
        >
          {entry.favorite ? (
            <Star size={14} fill='currentColor' className='text-coral-500' />
          ) : (
            <StarOff size={14} />
          )}
        </button>
        <Pill tone='neutral' className='font-mono'>
          {entry.source} → {entry.target}
        </Pill>
        <span className='font-mono text-[12px] text-ink-700 dark:text-ink-300 truncate flex-1'>
          {entry.query.replace(/\s+/g, ' ')}
        </span>
        <span className='flex items-center gap-1.5 text-[11px] text-ink-500 font-mono'>
          <Clock size={11} />
          {formatRelative(entry.createdAt)}
        </span>
        <span className='text-[11px] text-ink-500 font-mono tabular-nums'>
          {formatDuration(entry.took)}
        </span>
        {expanded ? (
          <ChevronDown size={14} className='text-ink-400' />
        ) : (
          <ChevronRight size={14} className='text-ink-400' />
        )}
      </div>
      {expanded && (
        <div className='grid md:grid-cols-2 divide-y md:divide-y-0 md:divide-x divide-ink-200 dark:divide-ink-800 bg-ink-50/40 dark:bg-ink-950/40 border-t border-ink-200 dark:border-ink-800'>
          <div>
            <div className='px-5 py-2 text-xs font-medium text-ink-600 dark:text-ink-400 border-b border-ink-200 dark:border-ink-800'>
              {parsed.sourceLabel || t('history.source')}
            </div>
            <pre className='font-mono text-[12.5px] leading-relaxed p-5 overflow-auto whitespace-pre text-ink-700 dark:text-ink-300'>
              {parsed.original || entry.query}
            </pre>
          </div>
          <div>
            <div className='px-5 py-2 text-xs font-medium text-coral-700 dark:text-coral-300 border-b border-ink-200 dark:border-ink-800 flex items-center'>
              {parsed.targetLabel || t('history.translated')}
              <div
                className='ml-auto flex items-center gap-1.5'
                onClick={(e) => e.stopPropagation()}
              >
                <Button variant='ghost' size='sm' onClick={copy}>
                  {copied ? <Check size={12} /> : <Copy size={12} />}
                  {copied ? t('translate.copied') : t('translate.copy')}
                </Button>
                <Button variant='ghost' size='sm' onClick={onRerun}>
                  <Play size={12} />
                  {t('history.rerun')}
                </Button>
                <Button
                  variant='ghost'
                  size='sm'
                  onClick={() => removeHistory(entry.id)}
                >
                  <Trash2 size={12} />
                  {t('history.delete')}
                </Button>
              </div>
            </div>
            <pre className='font-mono text-[12.5px] leading-relaxed p-5 overflow-auto whitespace-pre text-ink-800 dark:text-ink-100'>
              {parsed.translated || entry.response}
            </pre>
          </div>
        </div>
      )}
    </li>
  );
}
