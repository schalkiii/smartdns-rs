import type { Metadata } from 'next';
import Providers from './providers';

export const metadata: Metadata = {
  title: 'SmartDNS Dashboard',
  description: 'SmartDNS Dashboard - DNS Server Management',
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="zh-CN">
      <body>
        <Providers>{children}</Providers>
      </body>
    </html>
  );
}