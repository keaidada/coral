'use client';

import { useEffect, useState } from 'react';
import {
  Play,
  Copy,
  Check,
  AlertTriangle,
  Loader2,
  ArrowRight,
  History as HistoryIcon,
  Wand2,
} from 'lucide-react';
import {
  Card,
  CardBody,
  CardHeader,
  PageHeader,
  Button,
  Select,
  Pill,
  CodeBlock,
} from '@/app/components/ui';
import SqlEditor from '@/app/components/SqlEditor';
import HistoryDrawer from '@/app/components/HistoryDrawer';
import {
  API_BASE,
  addHistory,
  formatDuration,
  formatSql,
  parseTranslateBody,
  readHistory,
} from '@/app/lib/client';
import { useT } from '@/app/lib/i18n';

const SOURCES = [
  { value: 'hive', label: 'Hive' },
  { value: 'spark', label: 'Spark' },
  { value: 'trino', label: 'Trino' },
  { value: 'gaussdb', label: 'GaussDB / openGauss' },
];
const TARGETS = [
  { value: 'spark', label: 'Spark' },
  { value: 'trino', label: 'Trino' },
];

const SAMPLE = `SELECT
  e.id,
  NVL(d.name, 'Unknown') AS dept,
  COUNT(*)               AS headcount
FROM hr.employees e
LEFT JOIN hr.departments d ON e.dept_id = d.id
GROUP BY e.id, d.name`;

export default function TranslatePage() {
  const t = useT();
  const [source, setSource] = useState('hive');
  const [target, setTarget] = useState('spark');
  const [rewriteType, setRewriteType] = useState('none');
  const [query, setQuery] = useState(SAMPLE);
  const [loading, setLoading] = useState(false);
  const [result, setResult] = useState(null);
  const [error, setError] = useState(null);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [formatError, setFormatError] = useState(null);
  const [historyCount, setHistoryCount] = useState(0);

  useEffect(() => {
    setHistoryCount(readHistory().length);
    const refresh = () => setHistoryCount(readHistory().length);
    window.addEventListener('coral:history-changed', refresh);
    return () => window.removeEventListener('coral:history-changed', refresh);
  }, []);

  useEffect(() => {
    function applyEntry(h) {
      if (!h) return;
      setQuery(h.query || '');
      setSource(h.source || 'hive');
      setTarget(h.target || 'spark');
      setRewriteType(h.rewriteType || 'none');
      setResult(h);
      setError(null);
      window.scrollTo({ top: 0, behavior: 'smooth' });
    }

    const raw = sessionStorage.getItem('coral.pending-history-entry');
    if (raw) {
      sessionStorage.removeItem('coral.pending-history-entry');
      try {
        applyEntry(JSON.parse(raw));
      } catch {
        applyEntry(null);
      }
    }

    const onPick = (event) => applyEntry(event.detail);
    window.addEventListener('coral:load-history-entry', onPick);
    return () => window.removeEventListener('coral:load-history-entry', onPick);
  }, []);

  async function onSubmit() {
    if (!query.trim()) return;
    setLoading(true);
    setError(null);
    setResult(null);
    const started = performance.now();
    try {
      const resp = await fetch(API_BASE + '/api/translations/translate', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          query,
          sourceLanguage: source,
          targetLanguage: target,
          rewriteType,
        }),
      });
      const body = await resp.text();
      const took = Math.round(performance.now() - started);
      if (!resp.ok) {
        setError({ status: resp.status, message: body });
        return;
      }
      const parsed = parseTranslateBody(body);
      const entry = {
        query,
        source,
        target,
        rewriteType,
        took,
        status: resp.status,
        response: body,
        parsed,
      };
      setResult(entry);
      addHistory(entry);
    } catch (err) {
      setError({ status: 0, message: String(err) });
    } finally {
      setLoading(false);
    }
  }

  function onFormat() {
    if (!query.trim()) return;
    try {
      const formatted = formatSql(query, source);
      setQuery(formatted);
      setFormatError(null);
    } catch (err) {
      setFormatError(String(err.message || err));
      setTimeout(() => setFormatError(null), 4000);
    }
  }

  function loadHistoryEntry(h) {
    setQuery(h.query);
    setSource(h.source);
    setTarget(h.target);
    setRewriteType(h.rewriteType || 'none');
    setResult(h);
    setError(null);
    window.scrollTo({ top: 0, behavior: 'smooth' });
  }

  return (
    <div>
      <PageHeader
        title={t('translate.title')}
        subtitle={t('translate.subtitle')}
        actions={
          <Button
            variant='secondary'
            size='sm'
            onClick={() => setDrawerOpen(true)}
          >
            <HistoryIcon size={14} />
            {t('translate.openRecent')}
            {historyCount > 0 && (
              <span className='ml-1 inline-flex items-center justify-center min-w-[18px] h-[18px] px-1 text-[10px] font-mono rounded-full bg-coral-500 text-white'>
                {historyCount}
              </span>
            )}
          </Button>
        }
      />

      {/* Editor card */}
      <Card>
        <CardHeader>
          <div className='flex items-center gap-3 flex-wrap'>
            <Pill tone='coral'>{t('translate.newTranslation')}</Pill>
            <div className='flex items-center gap-2 text-sm'>
              <span className='text-ink-500'>{t('translate.from')}</span>
              <Select
                value={source}
                onChange={(e) => setSource(e.target.value)}
              >
                {SOURCES.map((s) => (
                  <option key={s.value} value={s.value}>
                    {s.label}
                  </option>
                ))}
              </Select>
              <ArrowRight size={14} className='text-ink-400' />
              <span className='text-ink-500'>{t('translate.to')}</span>
              <Select
                value={target}
                onChange={(e) => setTarget(e.target.value)}
              >
                {TARGETS.map((tt) => (
                  <option key={tt.value} value={tt.value}>
                    {tt.label}
                  </option>
                ))}
              </Select>
            </div>
          </div>
          <div className='ml-auto flex items-center gap-2'>
            <span className='text-xs text-ink-500'>
              {t('translate.rewrite')}
            </span>
            <Select
              value={rewriteType}
              onChange={(e) => setRewriteType(e.target.value)}
            >
              <option value='none'>{t('translate.rewriteNone')}</option>
              <option value='incremental'>
                {t('translate.rewriteIncremental')}
              </option>
            </Select>
          </div>
        </CardHeader>

        <SqlEditor
          value={query}
          onChange={setQuery}
          dialect={source}
          onSubmit={onSubmit}
          onFormat={onFormat}
          minHeight='280px'
          placeholder={t('translate.placeholder')}
        />

        {formatError && (
          <div className='px-5 py-2 text-xs text-amber-700 dark:text-amber-400 bg-amber-50/70 dark:bg-amber-900/20 border-t border-amber-200 dark:border-amber-800/40'>
            {t('translate.formatFailed')}
          </div>
        )}

        <div className='px-5 py-3 border-t border-ink-200 dark:border-ink-800 flex items-center gap-3 bg-ink-50/40 dark:bg-ink-950/40 flex-wrap'>
          <span className='text-xs text-ink-500 font-mono'>
            {t('translate.chars', { n: query.length })} ·{' '}
            {t('translate.tokens', {
              n: query.split(/\s+/).filter(Boolean).length,
            })}
          </span>
          <span className='ml-auto text-xs text-ink-500 hidden sm:inline'>
            {t('translate.hint')}
          </span>
          <Button
            type='button'
            variant='ghost'
            size='sm'
            onClick={onFormat}
            disabled={!query.trim()}
            title='⇧⌘F'
          >
            <Wand2 size={14} />
            {t('translate.format')}
          </Button>
          <Button
            type='button'
            variant='ghost'
            size='sm'
            onClick={() => {
              setQuery('');
              setResult(null);
              setError(null);
            }}
          >
            {t('translate.clear')}
          </Button>
          <Button
            type='button'
            onClick={onSubmit}
            disabled={loading || !query.trim()}
          >
            {loading ? (
              <>
                <Loader2 size={14} className='animate-spin' />
                {t('translate.running')}
              </>
            ) : (
              <>
                <Play size={14} />
                {t('translate.run')}
              </>
            )}
          </Button>
        </div>
      </Card>

      {/* Error */}
      {error && (
        <Card className='mt-5 border-red-200 dark:border-red-800/60'>
          <CardHeader className='bg-red-50/60 dark:bg-red-900/20'>
            <AlertTriangle
              size={16}
              className='text-red-600 dark:text-red-400'
            />
            <span className='font-medium text-red-700 dark:text-red-300'>
              {t('translate.failed')}
            </span>
            <Pill tone='red' className='ml-auto font-mono'>
              HTTP {error.status || 'network'}
            </Pill>
          </CardHeader>
          <CardBody>
            <CodeBlock className='!bg-red-50/50 dark:!bg-red-950/30 !border-red-200 dark:!border-red-900/40'>
              {error.message || 'No error message'}
            </CodeBlock>
          </CardBody>
        </Card>
      )}

      {/* Result — translated only, full-width */}
      {result && !error && (
        <ResultView result={result} key={result.createdAt || result.took} />
      )}

      <HistoryDrawer
        open={drawerOpen}
        onClose={() => setDrawerOpen(false)}
        onPick={loadHistoryEntry}
      />
    </div>
  );
}

function ResultView({ result }) {
  const t = useT();
  const [copied, setCopied] = useState(false);
  const parsed = result.parsed || parseTranslateBody(result.response);
  const translated = parsed.translated || result.response;

  async function copy() {
    await navigator.clipboard.writeText(translated);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  }

  return (
    <Card className='mt-5'>
      <CardHeader>
        <Pill tone='green'>{t('translate.success')}</Pill>
        <span className='text-sm text-ink-600 dark:text-ink-300 font-mono'>
          {result.source} → {result.target}
        </span>
        <span className='ml-auto flex items-center gap-3 text-xs text-ink-500 font-mono'>
          <span>{formatDuration(result.took)}</span>
          {result.rewriteType && result.rewriteType !== 'none' && (
            <Pill tone='amber'>{result.rewriteType}</Pill>
          )}
          <Button variant='secondary' size='sm' onClick={copy}>
            {copied ? (
              <>
                <Check size={13} /> {t('translate.copied')}
              </>
            ) : (
              <>
                <Copy size={13} /> {t('translate.copy')}
              </>
            )}
          </Button>
        </span>
      </CardHeader>
      <div className='px-5 py-2.5 border-b border-ink-200 dark:border-ink-800 bg-coral-50/50 dark:bg-coral-900/10'>
        <span className='text-xs font-medium text-coral-700 dark:text-coral-300'>
          {parsed.targetLabel || t('translate.translated')}
        </span>
      </div>
      <SqlEditor
        value={translated}
        onChange={() => {}}
        dialect={result.target}
        readOnly
        minHeight='160px'
      />
    </Card>
  );
}
