'use client';

import { useState } from 'react';
import { CheckCircle2, XCircle, Loader2, Play, Wand2 } from 'lucide-react';
import {
  Card,
  CardBody,
  CardHeader,
  PageHeader,
  Button,
  Pill,
  CodeBlock,
} from '@/app/components/ui';
import { API_BASE, formatSql } from '@/app/lib/client';
import { useT } from '@/app/lib/i18n';
import SqlEditor from '@/app/components/SqlEditor';

const SAMPLE = `SELECT a FROM t WHERE id > 10;
SELECT COUNT(*) FROM t GROUP BY dept`;

export default function ValidatePage() {
  const t = useT();
  const [query, setQuery] = useState(SAMPLE);
  const [loading, setLoading] = useState(false);
  const [result, setResult] = useState(null);
  const [error, setError] = useState(null);
  const [formatError, setFormatError] = useState(null);

  function onFormat() {
    if (!query.trim()) return;
    try {
      setQuery(formatSql(query, 'postgresql'));
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
      const resp = await fetch(API_BASE + '/api/translations/validate', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ query }),
      });
      const json = await resp.json();
      setResult(json);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div>
      <PageHeader
        title={t('validate.title')}
        subtitle={t('validate.subtitle')}
      />

      <Card>
        <CardHeader>
          <Pill tone='coral'>{t('validate.parseCheck')}</Pill>
          <span className='text-xs text-ink-500 ml-auto'>
            {t('validate.hint')}
          </span>
        </CardHeader>
        <SqlEditor
          value={query}
          onChange={setQuery}
          dialect='postgresql'
          onSubmit={onSubmit}
          onFormat={onFormat}
          minHeight='220px'
          placeholder={t('validate.placeholder')}
        />
        {formatError && (
          <div className='px-5 py-2 text-xs text-amber-700 dark:text-amber-400 bg-amber-50/70 dark:bg-amber-900/20 border-t border-amber-200 dark:border-amber-800/40'>
            {t('translate.formatFailed')}
          </div>
        )}
        <div className='px-5 py-3 border-t border-ink-200 dark:border-ink-800 flex items-center gap-3 bg-ink-50/40 dark:bg-ink-950/40'>
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
          <span className='ml-auto' />
          <Button
            type='button'
            onClick={onSubmit}
            disabled={loading || !query.trim()}
          >
            {loading ? (
              <>
                <Loader2 size={14} className='animate-spin' />
                {t('validate.running')}
              </>
            ) : (
              <>
                <Play size={14} />
                {t('validate.run')}
              </>
            )}
          </Button>
        </div>
      </Card>

      {error && (
        <Card className='mt-5 border-red-200 dark:border-red-800/60'>
          <CardBody>
            <div className='flex items-center gap-2 text-red-700 dark:text-red-300'>
              <XCircle size={16} />
              <span className='font-medium'>{t('validate.failed')}</span>
            </div>
            <CodeBlock className='mt-3 !bg-red-50/50 dark:!bg-red-950/30 !border-red-200 dark:!border-red-900/40'>
              {error}
            </CodeBlock>
          </CardBody>
        </Card>
      )}

      {result && (
        <Card className='mt-5'>
          <CardHeader>
            {result.parses ? (
              <>
                <CheckCircle2 size={16} className='text-green-600' />
                <span className='font-medium text-green-700 dark:text-green-400'>
                  {t('validate.ok')}
                </span>
                <Pill tone='green' className='ml-auto font-mono'>
                  {t('validate.statementCount', { n: result.statementCount })}
                </Pill>
              </>
            ) : (
              <>
                <XCircle size={16} className='text-red-600' />
                <span className='font-medium text-red-700 dark:text-red-400'>
                  {t('validate.parseError')}
                </span>
              </>
            )}
          </CardHeader>
          <CardBody>
            {result.parses ? (
              <p className='text-sm text-ink-600 dark:text-ink-300'>
                {t('validate.okDetail')}
              </p>
            ) : (
              <CodeBlock className='!bg-red-50/50 dark:!bg-red-950/30 !border-red-200 dark:!border-red-900/40'>
                {result.parseError || t('validate.unknownError')}
              </CodeBlock>
            )}
          </CardBody>
        </Card>
      )}
    </div>
  );
}
