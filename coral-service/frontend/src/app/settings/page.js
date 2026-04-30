'use client';

import { useEffect, useState } from 'react';
import {
  Play,
  Loader2,
  CheckCircle2,
  XCircle,
  Moon,
  Sun,
  Database,
  Info,
  Languages,
  Wand2,
} from 'lucide-react';
import {
  Card,
  CardBody,
  CardHeader,
  PageHeader,
  Button,
  Pill,
  CodeBlock,
} from '@/app/components/ui';
import SqlEditor from '@/app/components/SqlEditor';
import { API_BASE, formatSql } from '@/app/lib/client';
import { useI18n, useT } from '@/app/lib/i18n';

export default function SettingsPage() {
  const t = useT();
  const [tab, setTab] = useState('general');

  return (
    <div>
      <PageHeader
        title={t('settings.title')}
        subtitle={t('settings.subtitle')}
      />

      <div className='flex gap-1 mb-4 border-b border-ink-200 dark:border-ink-800'>
        <TabBtn active={tab === 'general'} onClick={() => setTab('general')}>
          {t('settings.tabGeneral')}
        </TabBtn>
        <TabBtn active={tab === 'catalog'} onClick={() => setTab('catalog')}>
          {t('settings.tabCatalog')}
        </TabBtn>
        <TabBtn active={tab === 'about'} onClick={() => setTab('about')}>
          {t('settings.tabAbout')}
        </TabBtn>
      </div>

      {tab === 'general' && <GeneralTab />}
      {tab === 'catalog' && <CatalogTab />}
      {tab === 'about' && <AboutTab />}
    </div>
  );
}

function TabBtn({ active, children, ...rest }) {
  return (
    <button
      {...rest}
      className={`px-3 py-2 text-sm border-b-2 -mb-px transition ${
        active
          ? 'border-coral-500 text-ink-900 dark:text-ink-50 font-medium'
          : 'border-transparent text-ink-500 hover:text-ink-800 dark:hover:text-ink-100'
      }`}
    >
      {children}
    </button>
  );
}

function GeneralTab() {
  const t = useT();
  const { lang, setLang } = useI18n();
  const [dark, setDark] = useState(false);

  useEffect(() => {
    setDark(document.documentElement.classList.contains('dark'));
  }, []);

  function applyTheme(next) {
    document.documentElement.classList.toggle('dark', next);
    localStorage.setItem('coral.theme', next ? 'dark' : 'light');
    setDark(next);
  }

  return (
    <Card>
      <CardBody>
        {/* Language */}
        <div className='flex items-center justify-between py-3'>
          <div>
            <div className='text-sm font-medium text-ink-900 dark:text-ink-50'>
              {t('settings.language')}
            </div>
            <div className='text-xs text-ink-500 mt-0.5'>
              {t('settings.languageDesc')}
            </div>
          </div>
          <div className='flex gap-2'>
            <Button
              variant={lang === 'en' ? 'primary' : 'secondary'}
              size='sm'
              onClick={() => setLang('en')}
            >
              <Languages size={14} /> English
            </Button>
            <Button
              variant={lang === 'zh' ? 'primary' : 'secondary'}
              size='sm'
              onClick={() => setLang('zh')}
            >
              <Languages size={14} /> 中文
            </Button>
          </div>
        </div>

        {/* Theme */}
        <div className='flex items-center justify-between py-3 border-t border-ink-200 dark:border-ink-800'>
          <div>
            <div className='text-sm font-medium text-ink-900 dark:text-ink-50'>
              {t('settings.theme')}
            </div>
            <div className='text-xs text-ink-500 mt-0.5'>
              {t('settings.themeDesc')}
            </div>
          </div>
          <div className='flex gap-2'>
            <Button
              variant={!dark ? 'primary' : 'secondary'}
              size='sm'
              onClick={() => applyTheme(false)}
            >
              <Sun size={14} /> {t('settings.light')}
            </Button>
            <Button
              variant={dark ? 'primary' : 'secondary'}
              size='sm'
              onClick={() => applyTheme(true)}
            >
              <Moon size={14} /> {t('settings.dark')}
            </Button>
          </div>
        </div>

        {/* Backend */}
        <div className='border-t border-ink-200 dark:border-ink-800 py-3'>
          <div className='text-sm font-medium text-ink-900 dark:text-ink-50'>
            {t('settings.backend')}
          </div>
          <div className='text-xs text-ink-500 mt-0.5'>
            {t('settings.backendDesc')}
          </div>
          <CodeBlock className='mt-2 !text-[12px]'>
            {API_BASE || t('settings.backendUnset')}
          </CodeBlock>
        </div>
      </CardBody>
    </Card>
  );
}

function CatalogTab() {
  const t = useT();
  const [statement, setStatement] = useState(
    'CREATE TABLE hr.employees (id INT, name STRING)',
  );
  const [loading, setLoading] = useState(false);
  const [result, setResult] = useState(null);
  const [error, setError] = useState(null);
  const [formatError, setFormatError] = useState(null);

  function onFormat() {
    if (!statement.trim()) return;
    try {
      setStatement(formatSql(statement, 'postgresql'));
      setFormatError(null);
    } catch (err) {
      setFormatError(String(err.message || err));
      setTimeout(() => setFormatError(null), 4000);
    }
  }

  async function onSubmit(e) {
    e?.preventDefault?.();
    setLoading(true);
    setError(null);
    setResult(null);
    try {
      const resp = await fetch(API_BASE + '/api/catalog-ops/execute', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: statement,
      });
      const text = await resp.text();
      if (!resp.ok) {
        setError(text);
      } else {
        setResult(text);
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }

  const ok = result && result.startsWith('Creation successful');

  return (
    <Card>
      <CardHeader>
        <Database size={16} className='text-coral-600' />
        <span className='font-medium'>{t('settings.catalogTitle')}</span>
        <Pill tone='amber' className='ml-auto'>
          {t('settings.catalogLocalOnly')}
        </Pill>
      </CardHeader>
      <form onSubmit={onSubmit}>
        <SqlEditor
          value={statement}
          onChange={setStatement}
          dialect='postgresql'
          onSubmit={onSubmit}
          onFormat={onFormat}
          minHeight='180px'
          placeholder={t('settings.catalogPlaceholder')}
        />
        {formatError && (
          <div className='px-5 py-2 text-xs text-amber-700 dark:text-amber-400 bg-amber-50/70 dark:bg-amber-900/20 border-t border-amber-200 dark:border-amber-800/40'>
            {t('translate.formatFailed')}
          </div>
        )}
        <div className='px-5 py-3 border-t border-ink-200 dark:border-ink-800 flex items-center gap-3 bg-ink-50/40 dark:bg-ink-950/40'>
          <span className='text-xs text-ink-500'>
            {t('settings.catalogHint')}
          </span>
          <Button
            type='button'
            variant='ghost'
            size='sm'
            onClick={onFormat}
            disabled={!statement.trim()}
            title='⇧⌘F'
            className='ml-auto'
          >
            <Wand2 size={14} />
            {t('translate.format')}
          </Button>
          <Button type='submit' disabled={loading || !statement.trim()}>
            {loading ? (
              <>
                <Loader2 size={14} className='animate-spin' />
                {t('settings.executing')}
              </>
            ) : (
              <>
                <Play size={14} />
                {t('settings.execute')}
              </>
            )}
          </Button>
        </div>
      </form>

      {(result || error) && (
        <div className='border-t border-ink-200 dark:border-ink-800 px-5 py-3'>
          {error ? (
            <div className='flex items-start gap-2 text-sm'>
              <XCircle size={16} className='text-red-600 mt-0.5 shrink-0' />
              <div>
                <div className='font-medium text-red-700 dark:text-red-400'>
                  {t('settings.catalogFailed')}
                </div>
                <div className='text-ink-600 dark:text-ink-300 mt-1 font-mono text-[12.5px] whitespace-pre-wrap'>
                  {error}
                </div>
              </div>
            </div>
          ) : (
            <div className='flex items-start gap-2 text-sm'>
              {ok ? (
                <CheckCircle2
                  size={16}
                  className='text-green-600 mt-0.5 shrink-0'
                />
              ) : (
                <Info size={16} className='text-ink-500 mt-0.5 shrink-0' />
              )}
              <div>
                <div className='font-medium'>
                  {ok ? t('settings.catalogSuccess') : t('settings.response')}
                </div>
                <div className='text-ink-600 dark:text-ink-300 mt-1 font-mono text-[12.5px] whitespace-pre-wrap'>
                  {result}
                </div>
              </div>
            </div>
          )}
        </div>
      )}
    </Card>
  );
}

function AboutTab() {
  const t = useT();
  return (
    <Card>
      <CardBody>
        <div className='flex items-center gap-3'>
          <span className='h-10 w-10 rounded-lg bg-coral-500 text-white grid place-items-center'>
            ◈
          </span>
          <div>
            <div className='text-base font-semibold'>Coral</div>
            <div className='text-xs text-ink-500 font-mono'>
              {t('settings.aboutSub')}
            </div>
          </div>
        </div>
        <p className='text-sm text-ink-600 dark:text-ink-300 mt-4 leading-relaxed'>
          {t('settings.aboutDesc')}
        </p>
        <div className='grid grid-cols-2 gap-3 mt-5 text-sm'>
          <Stat label={t('settings.statFn')} value={t('settings.statFnV')} />
          <Stat
            label={t('settings.statParser')}
            value={t('settings.statParserV')}
          />
          <Stat
            label={t('settings.statPretty')}
            value={t('settings.statPrettyV')}
          />
          <Stat
            label={t('settings.statTargets')}
            value={t('settings.statTargetsV')}
          />
        </div>
      </CardBody>
    </Card>
  );
}

function Stat({ label, value }) {
  return (
    <div className='border border-ink-200 dark:border-ink-800 rounded-md px-4 py-3'>
      <div className='text-[11px] uppercase tracking-wider text-ink-500'>
        {label}
      </div>
      <div className='text-sm font-medium text-ink-800 dark:text-ink-100 mt-0.5 font-mono'>
        {value}
      </div>
    </div>
  );
}
