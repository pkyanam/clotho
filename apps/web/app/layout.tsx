import type { Metadata } from "next";
import localFont from "next/font/local";
import Script from "next/script";
import "./globals.css";

import { AppShell } from "src/components/app-shell";
import { ClothoClerkProvider } from "src/components/clerk-provider";
import { ThemeProvider } from "src/components/theme-provider";
import { THEME_INIT_SCRIPT } from "src/lib/theme";

// geist pixel — square element shape, the one typeface (see packages/ui)
const geistPixel = localFont({
  variable: "--font-geist-pixel",
  display: "swap",
  src: [
    {
      path: "../../../packages/ui/fonts/GeistPixel-Square.ttf",
      weight: "400",
      style: "normal",
    },
  ],
});

export const metadata: Metadata = {
  title: "clotho",
  description: "version control for humans and ai agents.",
};

export default function RootLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  return (
    <html
      lang="en"
      data-mode="dark"
      suppressHydrationWarning
      className={`${geistPixel.variable} h-full antialiased`}
    >
      <body className="min-h-full bg-kumo-canvas text-kumo-default">
        <Script
          id="clotho-theme-init"
          strategy="beforeInteractive"
          dangerouslySetInnerHTML={{ __html: THEME_INIT_SCRIPT }}
        />
        <ThemeProvider>
          <ClothoClerkProvider>
            <AppShell>{children}</AppShell>
          </ClothoClerkProvider>
        </ThemeProvider>
      </body>
    </html>
  );
}
