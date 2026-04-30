'use client';

import { useEffect, useMemo, useState } from 'react';
import { Search, Loader2 } from 'lucide-react';
import { Card, PageHeader, Select, Pill } from '@/app/components/ui';
import { API_BASE } from '@/app/lib/client';
import { useT } from '@/app/lib/i18n';

const DISPOSITIONS = ['passthrough', 'rename', 'custom', 'unsupported'];

function dispositionTone(disp) {
  if (!disp) return 'neutral';
  const d = disp.toLowerCase();
  if (d.startsWith('rename')) return 'coral';
  if (d.startsWith('passthrough')) return 'neutral';
  if (d.startsWith('custom')) return 'amber';
  if (d.startsWith('unsupported')) return 'red';
  return 'neutral';
}

export default function FunctionsPage() {
  const t = useT();
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);
  const [entries, setEntries] = useState([]);
  const [query, setQuery] = useState('');
  const [dispFilter, setDispFilter] = useState('');

  useEffect(() => {
    let cancelled = false;
    (async () => {
      setLoading(true);
      setError(null);
      try {
        const resp = await fetch(API_BASE + '/api/functions');
        if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
        const data = await resp.json();
        if (!cancelled) setEntries(data.entries || []);
      } catch (err) {
        if (!cancelled) setError(String(err));
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return entries.filter((e) => {
      if (dispFilter) {
        const d = (e.disposition || '').toLowerCase();
        if (!d.startsWith(dispFilter.toLowerCase())) return false;
      }
      if (!q) return true;
      return (
        (e.name || '').toLowerCase().includes(q) ||
        (e.disposition || '').toLowerCase().includes(q) ||
        (e.notes || '').toLowerCase().includes(q) ||
        (e.category || '').toLowerCase().includes(q)
      );
    });
  }, [entries, query, dispFilter]);

  return (
    <div>
      <PageHeader
        title={t('functions.title')}
        subtitle={
          loading
            ? t('functions.subtitleLoading')
            : t('functions.subtitle', { n: entries.length })
        }
      />

      <Card>
        <div className='px-4 py-2.5 flex items-center gap-3 border-b border-ink-200 dark:border-ink-800 flex-wrap'>
          <Search size={14} className='text-ink-400' />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t('functions.search')}
            className='flex-1 bg-transparent text-sm outline-none text-ink-800 dark:text-ink-100 min-w-[180px]'
          />
          <span className='text-xs text-ink-500'>
            {t('functions.disposition')}
          </span>
          <Select
            value={dispFilter}
            onChange={(e) => setDispFilter(e.target.value)}
          >
            <option value=''>{t('functions.all')}</option>
            {DISPOSITIONS.map((d) => (
              <option key={d} value={d}>
                {d}
              </option>
            ))}
          </Select>
          <span className='text-xs text-ink-500 font-mono ml-auto'>
            {filtered.length}/{entries.length}
          </span>
        </div>

        {loading ? (
          <div className='py-16 flex items-center justify-center text-ink-500'>
            <Loader2 size={18} className='animate-spin mr-2' />
            {t('functions.loading')}
          </div>
        ) : error ? (
          <div className='p-5 text-sm text-red-600 dark:text-red-400'>
            {t('functions.failed', { err: error })}
          </div>
        ) : (
          <div className='overflow-x-auto'>
            <table className='w-full text-sm'>
              <thead>
                <tr className='text-[11px] uppercase tracking-wider text-ink-500 border-b border-ink-200 dark:border-ink-800'>
                  <th className='text-left font-medium px-5 py-2.5'>
                    {t('functions.colName')}
                  </th>
                  <th className='text-left font-medium px-5 py-2.5'>
                    {t('functions.colDisposition')}
                  </th>
                  <th className='text-left font-medium px-5 py-2.5'>
                    {t('functions.colCategory')}
                  </th>
                  <th className='text-left font-medium px-5 py-2.5'>
                    {t('functions.colNotes')}
                  </th>
                </tr>
              </thead>
              <tbody className='divide-y divide-ink-200 dark:divide-ink-800'>
                {filtered.map((e, i) => (
                  <tr
                    key={`${e.name}-${i}`}
                    className='hover:bg-ink-50/40 dark:hover:bg-ink-950/40'
                  >
                    <td className='px-5 py-2.5 font-mono text-[12.5px] text-ink-800 dark:text-ink-100'>
                      {e.name}
                    </td>
                    <td className='px-5 py-2.5'>
                      <Pill
                        tone={dispositionTone(e.disposition)}
                        className='font-mono'
                      >
                        {e.disposition}
                      </Pill>
                    </td>
                    <td className='px-5 py-2.5 text-ink-600 dark:text-ink-300 font-mono text-[12.5px]'>
                      {e.category || '—'}
                    </td>
                    <td className='px-5 py-2.5 text-ink-600 dark:text-ink-300 max-w-[520px]'>
                      <span className='line-clamp-2'>{e.notes || '—'}</span>
                    </td>
                  </tr>
                ))}
                {filtered.length === 0 && (
                  <tr>
                    <td
                      colSpan={4}
                      className='py-10 text-center text-sm text-ink-500'
                    >
                      {t('functions.noMatch')}
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        )}
      </Card>
    </div>
  );
}
