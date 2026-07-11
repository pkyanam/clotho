"use client";

import { Badge } from "@cloudflare/kumo";
import type { HubImportJob } from "@clotho/sdk-js";
import { useRouter } from "next/navigation";
import { useEffect } from "react";


function formatBytes(bytes: number) {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  const unit = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / 1024 ** unit;
  return `${value >= 10 || unit === 0 ? value.toFixed(0) : value.toFixed(1)} ${units[unit]}`;
}

function shortId(id: string) {
  return id.slice(0, 12);
}

function formatDate(timestamp: string) {
  return timestamp.slice(0, 16).replace("T", " ");
}

export function HubImportJobs({ jobs }: { jobs: HubImportJob[] }) {
  const router = useRouter();
  const active = jobs.some((job) => job.status === "queued" || job.status === "running");

  useEffect(() => {
    if (!active) return;
    const timer = window.setInterval(() => router.refresh(), 2000);
    return () => window.clearInterval(timer);
  }, [active, router]);

  if (jobs.length === 0) {
    return (
      <p className="mb-5 text-[0.8125rem] text-kumo-inactive">
        no imports yet. queued transfers and restart recovery will appear here.
      </p>
    );
  }

  return (
    <ul className="mb-6 divide-y divide-kumo-hairline border border-kumo-hairline">
      {jobs.slice(0, 8).map((job) => {
        const progress = job.logical_bytes
          ? Math.min(100, Math.round((job.bytes_imported / job.logical_bytes) * 100))
          : job.status === "succeeded"
            ? 100
            : 0;
        return (
          <li key={job.id} className="px-3 py-3 text-[0.8125rem]">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <span className="text-kumo-default">
                {job.source_repo_id}@{job.source_revision}
              </span>
              <span className="flex items-center gap-2">
                <Badge variant="outline">{job.status}</Badge>
                <span className="text-kumo-inactive">{formatDate(job.created_at)} UTC</span>
              </span>
            </div>
            <div className="mt-2 h-1 bg-kumo-base" aria-label={`${progress}% imported`}>
              <div className="h-full bg-kumo-contrast" style={{ width: `${progress}%` }} />
            </div>
            <div className="mt-2 flex flex-wrap justify-between gap-2 text-kumo-inactive">
              <span>
                {job.files_imported}/{job.files_total || "?"} files · {formatBytes(job.bytes_imported)}
                {job.logical_bytes ? ` / ${formatBytes(job.logical_bytes)}` : ""} · {job.arachne_files} Arachne
              </span>
              {job.commit_id && <span>commit {shortId(job.commit_id)}</span>}
            </div>
            {job.error && <p className="mt-2 break-words text-kumo-inactive">{job.error}</p>}
          </li>
        );
      })}
    </ul>
  );
}
