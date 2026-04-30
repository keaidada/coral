'use client';

import { useEffect, useMemo, useState } from 'react';
import { X, Star, StarOff, Search, Trash2, Clock, Inbox } from 'lucide-react';
import { Pill } from '@/app/components/ui';
import {
  formatDuration,
  formatRelative,
  readHistory,
  removeHistory,
  updateHistory,
} from '@/app/lib/client';
import { useT } from '@/app/lib/i18n';

export default function HistoryDrawer({ open, onClose, onPick }) {
  const t = useT();
  const [entries, setEntries] = useState([]);
  const [query, setQuery] = useState('');

  useEffect(() => {
    if (!open) return;
    setEntries(readHistory());
    const refresh = () => setEntries(readHistory());
    window.addEventListener('coral:history-changed', refresh);
    return () => window.removeEventListener('coral:history-changed', refresh);
  }, [open]);

  useEffect(() => {
    const onKey = (e) => {
      if (e.key === 'Escape' && open) onClose();
    };
    if (open) window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  // Lock body scroll while drawer is open.
  useEffect(() => {
    if (!open) return;
    const prev = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    return () => {
      document.body.style.overflow = prev;
    };
  }, [open]);

  const filtered = useMemo(() => {
    if (!query.trim()) return entries;
    const q = query.toLowerCase();
    return entries.filter(
      (e) =>
        e.query.toLowerCase().includes(q) ||
        `${e.source} ${e.target}`.toLowerCase().includes(q),
    );
  }, [entries, query]);

  return (
    <>
      <div
        className='coral-drawer-overlay'
        data-open={open || undefined}
        onClick={onClose}
      />
      <aside
        className='coral-drawer dark:bg-ink-950 dark:text-ink-50 dark:border-ink-800'
        data-open={open || undefined}
        aria-hidden={!open}
      >
        <header className='h-14 flex items-center gap-3 px-5 border-b border-ink-200 dark:border-ink-800 shrink-0'>
          <Clock size={16} className='text-coral-600' />
          <span className='text-sm font-semibold tracking-tight'>
            {t('translate.recent')}
          </span>
          <span className='text-[11px] text-ink-500 font-mono ml-1'>
            {entries.length}
          </span>
          <button
            onClick={onClose}
            aria-label='Close'
            className='ml-auto h-8 w-8 grid place-items-center rounded-md text-ink-500 hover:text-ink-900 dark:hover:text-ink-50 hover:bg-ink-100 dark:hover:bg-ink-800 transition'
          >
            <X size={15} />
          </button>
        </header>

        <div className='px-4 py-2.5 flex items-center gap-2 border-b border-ink-200 dark:border-ink-800 shrink-0'>
          <Search size={14} className='text-ink-400' />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t('history.filter')}
            className='flex-1 bg-transparent text-sm outline-none text-ink-800 dark:text-ink-100'
          />
        </div>

        <div className='flex-1 overflow-auto'>
          {filtered.length === 0 ? (
            <div className='h-full grid place-items-center text-center text-sm text-ink-500 px-8 py-16'>
              <div>
                <Inbox
                  size={28}
                  className='mx-auto mb-3 text-ink-300 dark:text-ink-700'
                />
                {entries.length === 0
                  ? t('translate.noHistory')
                  : t('history.noMatch')}
              </div>
            </div>
          ) : (
            <ul className='divide-y divide-ink-200 dark:divide-ink-800'>
              {filtered.map((e) => (
                <li
                  key={e.id}
                  className='group px-5 py-3 hover:bg-ink-50/60 dark:hover:bg-ink-900/60 transition cursor-pointer'
                  onClick={() => {
                    onPick(e);
                    onClose();
                  }}
                >
                  <div className='flex items-center gap-2 mb-1.5'>
                    <button
                      onClick={(ev) => {
                        ev.stopPropagation();
                        updateHistory(e.id, { favorite: !e.favorite });
                      }}
                      className='shrink-0 text-ink-400 hover:text-coral-500'
                    >
                      {e.favorite ? (
                        <Star
                          size={13}
                          fill='currentColor'
                          className='text-coral-500'
                        />
                      ) : (
                        <StarOff size={13} />
                      )}
                    </button>
                    <Pill tone='neutral' className='font-mono'>
                      {e.source} → {e.target}
                    </Pill>
                    <span className='ml-auto text-[10px] text-ink-500 font-mono'>
                      {formatRelative(e.createdAt)} · {formatDuration(e.took)}
                    </span>
                    <button
                      onClick={(ev) => {
                        ev.stopPropagation();
                        removeHistory(e.id);
                      }}
                      className='opacity-0 group-hover:opacity-100 text-ink-400 hover:text-red-500 transition'
                      aria-label={t('history.delete')}
                    >
                      <Trash2 size={12} />
                    </button>
                  </div>
                  <pre className='font-mono text-[11.5px] leading-snug text-ink-700 dark:text-ink-300 whitespace-pre-wrap break-all line-clamp-3'>
                    {e.query.replace(/\s+/g, ' ')}
                  </pre>
                </li>
              ))}
            </ul>
          )}
        </div>
      </aside>
    </>
  );
}
