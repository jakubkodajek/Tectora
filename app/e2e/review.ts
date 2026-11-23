// Helpers of the review guards (Revert, Revert all, the put-back of the
// offer's figures): an offer an older engine stored, another process holding
// the job's lock, answers held at the browser, request timelines. Shared by
// audit-r10-ed and audit-r11-ed.
import { spawn, type ChildProcess } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import type { Page } from "@playwright/test";
import { LIBRARY_DIR } from "./env";
import { bridge, createExample } from "./fixtures";

/** 12 November 2026, the day the "offer" was made. */
export const OFFER_MS = Date.UTC(2026, 10, 12, 10, 0, 0);

/**
 * An offer job whose stored result looks like one engine 0.8.0 stored (as
 * audit-r9-recalc `olderOffer`): roof area `stored` m², taken on OFFER_MS,
 * its origin checked by 0.8.0 and the AI's status change accepted.
 */
export async function olderOffer(mcp: any, name: string, stored = 150): Promise<string> {
  const id = await createExample("gable", name);
  await mcp.call("update_job", { id, status: "offer", summary: "Offer sent." });
  const file = path.join(LIBRARY_DIR, `${id}.tectora`);
  const p = JSON.parse(fs.readFileSync(file, "utf8"));
  const origins = path.join(LIBRARY_DIR, ".activity", `${id}.result.json`);
  const o = JSON.parse(fs.readFileSync(origins, "utf8")) as { taken_ms: number; checked: string }[];
  for (const x of o) {
    if (x.taken_ms === p.result_snapshot.taken_ms) x.taken_ms = OFFER_MS;
    x.checked = "0.8.0";
  }
  p.result_snapshot.engine_version = "0.8.0";
  p.result_snapshot.roof_area = stored;
  p.result_snapshot.taken_ms = OFFER_MS;
  fs.writeFileSync(file, JSON.stringify(p));
  fs.writeFileSync(origins, JSON.stringify(o));
  await acceptAll(id);
  return id;
}

/** Accepts every change in the job's log (no review banner). */
export async function acceptAll(id: string) {
  const log = await bridge<{ ts_ms: number }[]>("project_activity", { id });
  await bridge("accept_changes", { id, tsMs: log.reduce((m, a) => Math.max(m, a.ts_ms), 0) });
}

/**
 * Another process (a slow AI save, a second Tectora on a shared folder)
 * holds the job's lock file for `ms`: every write of the job waits for it.
 * Resolves once the lock is taken. Python's flock, as the audit's
 * reproducer; the E2E suite runs on Linux.
 */
export async function holdLock(id: string, ms: number): Promise<ChildProcess> {
  const file = path.join(LIBRARY_DIR, ".activity", `${id}.lock`);
  const h = spawn(
    "python3",
    ["-I", "-c", "import fcntl,sys,time\nf=open(sys.argv[1],'a')\nfcntl.flock(f,fcntl.LOCK_EX)\nprint('held',flush=True)\ntime.sleep(float(sys.argv[2]))", file, String(ms / 1000)],
    { stdio: ["ignore", "pipe", "inherit"] },
  );
  await new Promise<void>((ok, fail) => {
    h.stdout!.once("data", () => ok());
    h.once("error", fail);
    h.once("exit", (code) => fail(new Error(`lock holder exited with ${code}`)));
  });
  return h;
}

/** Holds the bridge's answers to `cmd` (after the core ran it) until released. */
export async function hold(page: Page, cmd: string, when: (body: any) => boolean = () => true): Promise<{ release: () => void; held: () => number }> {
  let open!: () => void;
  const gate = new Promise<void>((r) => (open = r));
  let n = 0;
  await page.route(`**/${cmd}`, async (route) => {
    let body: any = {};
    try {
      body = JSON.parse(route.request().postData() ?? "{}");
    } catch {
      // Not JSON: not ours to hold.
    }
    if (!when(body)) return route.continue();
    const res = await route.fetch();
    n++;
    await gate;
    await route.fulfill({ response: res }).catch(() => {});
  });
  return { release: open, held: () => n };
}

/** Times (ms since the page's start) the editor sent requests to `cmd` and got their answers. */
export function timeline(page: Page, cmd: string) {
  const sent: number[] = [];
  const answered: number[] = [];
  page.on("request", (r) => {
    if (r.url().endsWith(`/${cmd}`)) sent.push(Date.now());
  });
  page.on("requestfinished", (r) => {
    if (r.url().endsWith(`/${cmd}`)) answered.push(Date.now());
  });
  return { sent, answered };
}

