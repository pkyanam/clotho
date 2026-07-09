import type { Metadata } from "next";
import localFont from "next/font/local";
import "./globals.css";

import { AppShell } from "src/components/app-shell";

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
      className={`${geistPixel.variable} h-full antialiased`}
    >
      <body className="min-h-full bg-kumo-canvas text-kumo-default">
        <AppShell>{children}</AppShell>
      </body>
    </html>
  );
}
