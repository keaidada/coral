'use client';

import { Command } from 'cmdk';
import { useRouter } from 'next/navigation';
import {
  ArrowLeftRight,
  CheckCircle2,
  Workflow,
  History,
  Library,
  Settings,
  Trash2,
  Moon,
  Languages,
} from 'lucide-react';
import { useEffect, useState } from 'react';
import { clearHistory, readHistory } from '@/app/lib/client';
import { useI18n } from '@/app/lib/i18n';

export default function CommandPalette({ open, onOpenChange, onOpenRecent }) {
  const router = useRouter();
  const [history, setHistory] = useState([]);
  const { lang, setLang, t } = useI18n();

  useEffect(() => {
    if (!open) return;
    setHistory(readHistory().slice(0, 8));
  }, [open]);

  useEffect(() => {
    const onKey = (e) => {
      if (e.key === 'Escape') onOpenChange(false);
    };
    if (open) window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, onOpenChange]);

  if (!open) return null;

  function go(path) {
    onOpenChange(false);
    router.push(path);
  }

  function toggleTheme() {
    const current = document.documentElement.classList.contains('dark');
    document.documentElement.classList.toggle('dark', !current);
    localStorage.setItem('coral.theme', !current ? 'dark' : 'light');
    onOpenChange(false);
    window.dispatchEvent(new Event('storage'));
  }

  function toggleLang() {
    setLang(lang === 'en' ? 'zh' : 'en');
    onOpenChange(false);
  }

  return (
    <div
      cmdk-overlay=''
      onClick={(e) => {
        if (e.target === e.currentTarget) onOpenChange(false);
      }}
      className='grid place-items-start justify-center pt-[10vh]'
    >
      <Command label='Command menu'>
        <Command.Input placeholder={t('cmd.placeholder')} />
        <Command.List>
          <Command.Empty>{t('cmd.empty')}</Command.Empty>

          <Command.Group heading={t('cmd.navigate')}>
            <Command.Item onSelect={() => go('/')}>
              <ArrowLeftRight size={14} /> {t('nav.translate')}
            </Command.Item>
            <Command.Item onSelect={() => go('/validate')}>
              <CheckCircle2 size={14} /> {t('nav.validate')}
            </Command.Item>
            <Command.Item onSelect={() => go('/visualize')}>
              <Workflow size={14} /> {t('nav.visualize')}
            </Command.Item>
            <Command.Item
              onSelect={() => {
                onOpenChange(false);
                onOpenRecent?.();
              }}
            >
              <History size={14} /> {t('translate.openRecent')}
            </Command.Item>
            <Command.Item onSelect={() => go('/functions')}>
              <Library size={14} /> {t('nav.functions')}
            </Command.Item>
            <Command.Item onSelect={() => go('/settings')}>
              <Settings size={14} /> {t('nav.settings')}
            </Command.Item>
          </Command.Group>

          {history.length > 0 && (
            <Command.Group heading={t('cmd.recent')}>
              {history.map((h) => (
                <Command.Item
                  key={h.id}
                  value={`history-${h.id}-${h.query}`}
                  onSelect={() => go(`/history?open=${h.id}`)}
                >
                  <History size={14} />
                  <span className='font-mono text-[12px] truncate max-w-[360px]'>
                    {h.query.replace(/\s+/g, ' ').slice(0, 80)}
                  </span>
                  <span className='ml-auto text-[11px] text-ink-500'>
                    {h.source} → {h.target}
                  </span>
                </Command.Item>
              ))}
            </Command.Group>
          )}

          <Command.Group heading={t('cmd.actions')}>
            <Command.Item onSelect={toggleLang}>
              <Languages size={14} /> {t('cmd.toggleLang')} (
              {lang === 'en' ? '中文' : 'EN'})
            </Command.Item>
            <Command.Item onSelect={toggleTheme}>
              <Moon size={14} /> {t('cmd.toggleTheme')}
            </Command.Item>
            <Command.Item
              onSelect={() => {
                if (confirm(t('history.clearConfirm'))) {
                  clearHistory();
                  onOpenChange(false);
                }
              }}
            >
              <Trash2 size={14} /> {t('cmd.clearHistory')}
            </Command.Item>
          </Command.Group>
        </Command.List>
      </Command>
    </div>
  );
}
