import './globals.css';
import { Inter, JetBrains_Mono } from 'next/font/google';
import Shell from '@/app/components/Shell';
import { I18nProvider } from '@/app/lib/i18n';

const inter = Inter({
  subsets: ['latin'],
  variable: '--font-inter',
  display: 'swap',
});

const mono = JetBrains_Mono({
  subsets: ['latin'],
  variable: '--font-mono',
  display: 'swap',
});

export const metadata = {
  title: 'Coral — SQL translation workspace',
  description:
    'Translate, validate, and visualize SQL across Hive, Spark, Trino, and GaussDB.',
};

export default function RootLayout({ children }) {
  return (
    <html lang='en' className={`${inter.variable} ${mono.variable}`}>
      <body className='font-sans bg-white text-ink-900 antialiased'>
        <I18nProvider>
          <Shell>{children}</Shell>
        </I18nProvider>
      </body>
    </html>
  );
}
