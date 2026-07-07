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
  title: "clotho — version control for humans and agents",
  description:
    "the version control platform built for the world as it actually is now: humans and agents, working together, on the same repo, at the same time.",
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
        {children}
      </body>
    </html>
  );
}
