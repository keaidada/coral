export function Card({ children, className = '', ...rest }) {
  return (
    <div
      {...rest}
      className={`bg-white dark:bg-ink-900 border border-ink-200 dark:border-ink-800 rounded-lg shadow-card ${className}`}
    >
      {children}
    </div>
  );
}

export function CardHeader({ children, className = '' }) {
  return (
    <div
      className={`px-5 py-4 border-b border-ink-200 dark:border-ink-800 flex items-center gap-3 ${className}`}
    >
      {children}
    </div>
  );
}

export function CardBody({ children, className = '' }) {
  return <div className={`p-5 ${className}`}>{children}</div>;
}

export function PageHeader({ title, subtitle, actions }) {
  return (
    <div className='flex items-start justify-between mb-6'>
      <div>
        <h1 className='text-xl font-semibold tracking-tight text-ink-900 dark:text-ink-50'>
          {title}
        </h1>
        {subtitle && (
          <p className='text-sm text-ink-500 dark:text-ink-400 mt-1'>
            {subtitle}
          </p>
        )}
      </div>
      {actions && <div className='flex items-center gap-2'>{actions}</div>}
    </div>
  );
}

export function Button({
  variant = 'primary',
  size = 'md',
  className = '',
  children,
  ...rest
}) {
  const base =
    'inline-flex items-center justify-center gap-1.5 rounded-md font-medium transition disabled:opacity-50 disabled:cursor-not-allowed';
  const sizes = {
    sm: 'text-xs px-2.5 py-1.5',
    md: 'text-sm px-3.5 py-2',
    lg: 'text-sm px-4 py-2.5',
  };
  const variants = {
    primary:
      'bg-coral-500 text-white hover:bg-coral-600 shadow-sm focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-coral-500',
    secondary:
      'bg-white dark:bg-ink-900 text-ink-800 dark:text-ink-100 border border-ink-200 dark:border-ink-800 hover:bg-ink-50 dark:hover:bg-ink-800',
    ghost:
      'text-ink-700 dark:text-ink-300 hover:bg-ink-100 dark:hover:bg-ink-800',
    danger: 'bg-red-500 text-white hover:bg-red-600',
  };
  return (
    <button
      {...rest}
      className={`${base} ${sizes[size]} ${variants[variant]} ${className}`}
    >
      {children}
    </button>
  );
}

export function Select({ className = '', ...rest }) {
  return (
    <select
      {...rest}
      className={`bg-white dark:bg-ink-900 border border-ink-200 dark:border-ink-800 rounded-md text-sm px-2.5 py-1.5 text-ink-800 dark:text-ink-100 focus:outline-none focus:ring-2 focus:ring-coral-500 focus:border-transparent ${className}`}
    />
  );
}

export function Pill({ tone = 'neutral', children, className = '' }) {
  const tones = {
    neutral:
      'bg-ink-100 dark:bg-ink-800 text-ink-700 dark:text-ink-200 border-ink-200 dark:border-ink-700',
    coral:
      'bg-coral-50 dark:bg-coral-900/30 text-coral-700 dark:text-coral-300 border-coral-200 dark:border-coral-800',
    green:
      'bg-green-50 dark:bg-green-900/30 text-green-700 dark:text-green-300 border-green-200 dark:border-green-800',
    amber:
      'bg-amber-50 dark:bg-amber-900/30 text-amber-700 dark:text-amber-300 border-amber-200 dark:border-amber-800',
    red: 'bg-red-50 dark:bg-red-900/30 text-red-700 dark:text-red-300 border-red-200 dark:border-red-800',
  };
  return (
    <span
      className={`inline-flex items-center gap-1 px-1.5 py-0.5 text-[11px] font-medium rounded border ${tones[tone]} ${className}`}
    >
      {children}
    </span>
  );
}

export function CodeBlock({ children, className = '' }) {
  return (
    <pre
      className={`font-mono text-[13px] leading-relaxed bg-ink-50 dark:bg-ink-950 border border-ink-200 dark:border-ink-800 rounded-md p-4 overflow-auto whitespace-pre text-ink-800 dark:text-ink-100 ${className}`}
    >
      {children}
    </pre>
  );
}
