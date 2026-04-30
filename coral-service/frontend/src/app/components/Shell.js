'use client';

import Link from 'next/link';
import { usePathname, useRouter } from 'next/navigation';
import { useEffect, useState } from 'react';
import {
  ArrowLeftRight,
  CheckCircle2,
  Workflow,
  History,
  Library,
  Settings,
  Moon,
  Sun,
  Search,
  Sparkles,
  Languages,
} from 'lucide-react';
import CommandPalette from '@/app/components/CommandPalette';
import HistoryDrawer from '@/app/components/HistoryDrawer';
import { readHistory } from '@/app/lib/client';
import { useI18n } from '@/app/lib/i18n';

export default function Shell({ children }) {
  const pathname = usePathname();
  const router = useRouter();
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [recentOpen, setRecentOpen] = useState(false);
  const [historyCount, setHistoryCount] = useState(0);
  const [dark, setDark] = useState(false);
  const { lang, setLang, t } = useI18n();

  const NAV = [
    { href: '/', labelKey: 'nav.translate', icon: ArrowLeftRight },
    { href: '/validate', labelKey: 'nav.validate', icon: CheckCircle2 },
    { href: '/visualize', labelKey: 'nav.visualize', icon: Workflow },
    { href: '/functions', labelKey: 'nav.functions', icon: Library },
    { href: '/settings', labelKey: 'nav.settings', icon: Settings },
  ];

  useEffect(() => {
    const saved =
      typeof window !== 'undefined'
        ? localStorage.getItem('coral.theme')
        : null;
    const isDark = saved === 'dark';
    setDark(isDark);
    document.documentElement.classList.toggle('dark', isDark);
  }, []);

  useEffect(() => {
    setHistoryCount(readHistory().length);
    const refresh = () => setHistoryCount(readHistory().length);
    window.addEventListener('coral:history-changed', refresh);
    return () => window.removeEventListener('coral:history-changed', refresh);
  }, []);

  useEffect(() => {
    const onKey = (e) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        setPaletteOpen((v) => !v);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  function toggleTheme() {
    const next = !dark;
    setDark(next);
    document.documentElement.classList.toggle('dark', next);
    localStorage.setItem('coral.theme', next ? 'dark' : 'light');
  }

  function toggleLang() {
    setLang(lang === 'en' ? 'zh' : 'en');
  }

  function pickRecent(entry) {
    if (pathname === '/') {
      window.dispatchEvent(
        new CustomEvent('coral:load-history-entry', { detail: entry }),
      );
      return;
    }
    sessionStorage.setItem(
      'coral.pending-history-entry',
      JSON.stringify(entry),
    );
    router.push('/');
  }

  return (
    <div className='min-h-screen grid grid-cols-[240px_1fr] dark:bg-ink-950 dark:text-ink-50'>
      {/* Sidebar */}
      <aside className='border-r border-ink-200 dark:border-ink-800 bg-ink-50/60 dark:bg-ink-900/40 flex flex-col'>
        <Link
          href='/'
          className='h-14 flex items-center gap-2 px-5 border-b border-ink-200 dark:border-ink-800'
        >
          <span className='h-7 w-7 rounded-md bg-coral-500 text-white grid place-items-center shadow-sm'>
            <Sparkles size={16} strokeWidth={2.5} />
          </span>
          <span className='font-semibold tracking-tight'>coral</span>
          <span className='ml-auto text-[10px] font-mono text-ink-500'>
            v0.3
          </span>
        </Link>

        <nav className='flex-1 p-3 space-y-0.5'>
          {NAV.map((item) => {
            const active =
              item.href === '/'
                ? pathname === '/'
                : pathname.startsWith(item.href);
            const Icon = item.icon;
            return (
              <Link
                key={item.href}
                href={item.href}
                className={`flex items-center gap-2.5 px-3 py-2 rounded-md text-sm transition
                  ${
                    active
                      ? 'bg-white dark:bg-ink-800 text-ink-900 dark:text-ink-50 shadow-card font-medium'
                      : 'text-ink-600 dark:text-ink-400 hover:text-ink-900 dark:hover:text-ink-50 hover:bg-white/60 dark:hover:bg-ink-800/60'
                  }`}
              >
                <Icon size={16} strokeWidth={2} />
                <span>{t(item.labelKey)}</span>
              </Link>
            );
          })}
          <button
            type='button'
            onClick={() => setRecentOpen(true)}
            className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-md text-sm transition ${
              recentOpen
                ? 'bg-white dark:bg-ink-800 text-ink-900 dark:text-ink-50 shadow-card font-medium'
                : 'text-ink-600 dark:text-ink-400 hover:text-ink-900 dark:hover:text-ink-50 hover:bg-white/60 dark:hover:bg-ink-800/60'
            }`}
          >
            <History size={16} strokeWidth={2} />
            <span>{t('translate.openRecent')}</span>
            {historyCount > 0 && (
              <span className='ml-auto inline-flex items-center justify-center min-w-[18px] h-[18px] px-1 text-[10px] font-mono rounded-full bg-coral-500 text-white'>
                {historyCount}
              </span>
            )}
          </button>
        </nav>

        <div className='p-3 border-t border-ink-200 dark:border-ink-800 text-[11px] text-ink-500 flex items-center gap-2'>
          <span className='inline-block h-1.5 w-1.5 rounded-full bg-green-500' />
          <span>{t('shell.connected')}</span>
          <span className='ml-auto font-mono'>rust</span>
        </div>
      </aside>

      {/* Main */}
      <div className='flex flex-col min-w-0'>
        {/* Topbar */}
        <header className='h-14 border-b border-ink-200 dark:border-ink-800 flex items-center gap-3 px-6'>
          <button
            onClick={() => setPaletteOpen(true)}
            className='flex items-center gap-2 text-sm text-ink-500 dark:text-ink-400 hover:text-ink-900 dark:hover:text-ink-50 bg-ink-50 dark:bg-ink-900 border border-ink-200 dark:border-ink-800 rounded-md px-3 py-1.5 min-w-[260px] transition'
          >
            <Search size={14} />
            <span className='flex-1 text-left'>{t('shell.search')}</span>
            <kbd className='font-mono text-[10px] bg-white dark:bg-ink-800 border border-ink-200 dark:border-ink-700 rounded px-1.5 py-0.5 text-ink-500'>
              ⌘K
            </kbd>
          </button>

          <div className='ml-auto flex items-center gap-2'>
            <button
              onClick={toggleLang}
              aria-label={t('shell.toggleLang')}
              title={t('shell.toggleLang')}
              className='h-8 px-2 gap-1.5 flex items-center rounded-md border border-ink-200 dark:border-ink-800 text-ink-600 dark:text-ink-400 hover:text-ink-900 dark:hover:text-ink-50 hover:bg-ink-50 dark:hover:bg-ink-900 transition text-xs font-medium'
            >
              <Languages size={14} />
              <span className='font-mono'>{lang === 'en' ? 'EN' : '中文'}</span>
            </button>
            <button
              onClick={toggleTheme}
              aria-label={t('shell.toggleTheme')}
              title={t('shell.toggleTheme')}
              className='h-8 w-8 grid place-items-center rounded-md border border-ink-200 dark:border-ink-800 text-ink-600 dark:text-ink-400 hover:text-ink-900 dark:hover:text-ink-50 hover:bg-ink-50 dark:hover:bg-ink-900 transition'
            >
              {dark ? <Sun size={15} /> : <Moon size={15} />}
            </button>
          </div>
        </header>

        <main className='flex-1 overflow-auto'>
          <div className='max-w-6xl mx-auto px-6 py-8'>{children}</div>
        </main>
      </div>

      <CommandPalette
        open={paletteOpen}
        onOpenChange={setPaletteOpen}
        onOpenRecent={() => setRecentOpen(true)}
      />
      <HistoryDrawer
        open={recentOpen}
        onClose={() => setRecentOpen(false)}
        onPick={pickRecent}
      />
    </div>
  );
}
