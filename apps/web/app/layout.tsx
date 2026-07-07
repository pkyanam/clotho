import type { Metadata } from "next";
import localFont from "next/font/local";
import "./globals.css";

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
      <body className="flex min-h-full flex-col bg-kumo-canvas text-kumo-default">
        <header className="sticky top-0 z-10 border-b border-kumo-hairline bg-kumo-canvas/70 backdrop-blur-xl">
          <div className="mx-auto flex h-14 max-w-6xl items-center px-6">
            <span className="text-sm">clotho</span>
          </div>
        </header>
        <main className="flex-1">{children}</main>
      </body>
    </html>
  );
}
