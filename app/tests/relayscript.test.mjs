// The relay script (integrations/report-relay/Code.gs), plan-delivery D15,
// run under vitest (Q7). The script is loaded into a Node `vm` context with
// stubbed Google services; nothing leaves the process. Assumed in the script:
// plain top-level functions doPost, doGet, setup, sweep; Script Properties
// KEY, FOLDER_ID, GITHUB_TOKEN, GITHUB_REPO, MAX_PER_DAY, MAX_MB_PER_DAY.
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import vm from "node:vm";
import { describe, expect, it } from "vitest";

const dir = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "integrations", "report-relay");
const CODE = readFileSync(join(dir, "Code.gs"), "utf8");

const KEY = "relay-test-key-0123456789abcdef";
const TOKEN = "github_pat_SECRET_TOKEN_VALUE";
const ID = "0123456789abcdef0123456789abcdef";
const NAME = "tectora-report-20261128-143012-novak-dum.zip";

function zipB64(extra = "data") {
  return Buffer.concat([Buffer.from([0x50, 0x4b, 0x03, 0x04]), Buffer.from(extra)]).toString("base64");
}

function world(props = {}, opts = {}) {
  const properties = new Map(
    Object.entries({ KEY, FOLDER_ID: "FOLDER", GITHUB_TOKEN: TOKEN, ...props }),
  );
  const files = [];
  const calls = { folder: 0, fetch: [], triggers: [] };
  const github = { status: opts.githubStatus ?? 201, issue: 12, failIf: null };
  const logs = { log: [], error: [], warn: [], info: [] };
  // Hooks a test sets to make Drive misbehave: (file, text) => throw.
  const hooks = { setDescription: null };
  const cache = new Map();
  let nextFile = 1;

  const makeFile = (blob, name) => {
    const f = {
      id: `FILE${nextFile++}`,
      name,
      description: "",
      bytes: blob?.bytes ?? [],
      getId() { return this.id; },
      getName() { return this.name; },
      getUrl() { return `https://drive.google.com/file/d/${this.id}/view`; },
      getSize() { return this.bytes.length; },
      setDescription(d) {
        hooks.setDescription?.(this, d);
        this.description = d;
        return this;
      },
      getDescription() { return this.description; },
      getBlob() { return { bytes: this.bytes }; },
      setName(n) { this.name = n; return this; },
    };
    files.push(f);
    return f;
  };
  const iterator = (list) => {
    let i = 0;
    return { hasNext: () => i < list.length, next: () => list[i++] };
  };
  const folder = {
    createFile(a, b, c) {
      // createFile(blob) or createFile(name, content, mime)
      if (typeof a === "string") return makeFile({ bytes: Array.from(Buffer.from(String(b))) }, a);
      return makeFile(a, a.name ?? "");
    },
    getFilesByName: (n) => iterator(files.filter((f) => f.name === n)),
    getFiles: () => iterator([...files]),
    getName: () => "Reports",
  };

  const sandbox = {
    console: {
      log: (...a) => logs.log.push(a.join(" ")),
      info: (...a) => logs.info.push(a.join(" ")),
      warn: (...a) => logs.warn.push(a.join(" ")),
      error: (...a) => logs.error.push(a.join(" ")),
    },
    Logger: { log: (...a) => logs.log.push(a.join(" ")) },
    PropertiesService: {
      getScriptProperties: () => ({
        getProperty: (k) => (properties.has(k) ? properties.get(k) : null),
        setProperty: (k, v) => void properties.set(k, String(v)),
        setProperties: (o) => Object.entries(o).forEach(([k, v]) => properties.set(k, String(v))),
        getProperties: () => Object.fromEntries(properties),
        deleteProperty: (k) => void properties.delete(k),
      }),
    },
    LockService: {
      getScriptLock: () => ({ waitLock() {}, tryLock: () => true, releaseLock() {}, hasLock: () => true }),
    },
    CacheService: {
      getScriptCache: () => ({
        get: (k) => (cache.has(k) ? cache.get(k) : null),
        put: (k, v) => void cache.set(k, String(v)),
        remove: (k) => void cache.delete(k),
      }),
    },
    ContentService: {
      MimeType: { JSON: "application/json", TEXT: "text/plain" },
      createTextOutput: (text) => ({
        text,
        getContent: () => text,
        setMimeType() { return this; },
      }),
    },
    Utilities: {
      // Google's own functions give signed bytes.
      base64Decode: (s) => Array.from(Buffer.from(s, "base64"), (b) => (b > 127 ? b - 256 : b)),
      base64Encode: (b) => Buffer.from(b).toString("base64"),
      newBlob: (bytes, mime, name) => ({ bytes, mime, name, getBytes: () => bytes, getName: () => name }),
      getUuid: () => "00000000-0000-0000-0000-000000000000",
      sleep() {},
      formatDate: (d, _tz, fmt) => (fmt.startsWith("yyyy-MM-dd") ? new Date(d).toISOString().slice(0, 10) : String(d)),
    },
    DriveApp: {
      getFolderById: (id) => {
        calls.folder++;
        if (id !== "FOLDER") throw new Error("no such folder");
        return folder;
      },
    },
    UrlFetchApp: {
      fetch: (url, o = {}) => {
        calls.fetch.push({ url, ...o });
        const poisoned = github.failIf && o.payload ? github.failIf(JSON.parse(o.payload)) : false;
        const status = poisoned ? 500 : github.status;
        const body = status === 201 ? { html_url: `https://github.com/jakubkodajek/Tectora/issues/${github.issue}`, number: github.issue } : { message: "boom" };
        return { getResponseCode: () => status, getContentText: () => JSON.stringify(body) };
      },
    },
    ScriptApp: {
      getProjectTriggers: () => [],
      newTrigger: (fn) => {
        const t = { fn };
        const chain = new Proxy(t, {
          get: (target, p) => (p === "create" ? () => (calls.triggers.push(target), target) : () => chain),
        });
        return chain;
      },
      deleteTrigger() {},
    },
    Session: { getScriptTimeZone: () => "UTC" },
  };
  vm.createContext(sandbox);
  vm.runInContext(CODE, sandbox, { filename: "Code.gs" });
  return { sandbox, properties, files, calls, github, cache, logs, hooks };
}

function post(w, body, { raw = false } = {}) {
  const contents = raw ? body : JSON.stringify(body);
  const out = w.sandbox.doPost({ postData: { contents, type: "application/json" }, parameter: {} });
  return JSON.parse(out.getContent());
}

function good(over = {}) {
  return {
    v: 1,
    key: KEY,
    id: ID,
    file_name: NAME,
    size: 8,
    meta: { description: "The gable is wrong\nsecond line", app_version: "0.11.0", source: "app", reporter: "user", os: "macos", job_name: "Novák dům" },
    zip_b64: zipB64(),
    ...over,
  };
}

const withId = (n) => ({ id: n.toString(16).padStart(32, "0") });

describe("Code.gs", () => {
  it("parses as JavaScript", () => {
    const d = mkdtempSync(join(tmpdir(), "tectora-relay-script-"));
    const f = join(d, "Code.js");
    writeFileSync(f, CODE);
    expect(() => execFileSync(process.execPath, ["--check", f], { stdio: "pipe" })).not.toThrow();
  });

  it("has a manifest that is open to anyone, runs as the owner and asks for little", () => {
    const m = JSON.parse(readFileSync(join(dir, "appsscript.json"), "utf8"));
    expect(m.webapp).toMatchObject({ executeAs: "USER_DEPLOYING", access: "ANYONE_ANONYMOUS" });
    expect(m.runtimeVersion).toBe("V8");
    const allowed = [
      "https://www.googleapis.com/auth/drive",
      "https://www.googleapis.com/auth/drive.file",
      "https://www.googleapis.com/auth/script.external_request",
      "https://www.googleapis.com/auth/script.scriptapp",
    ];
    expect(m.oauthScopes.length).toBeGreaterThan(1);
    for (const s of m.oauthScopes) expect(allowed, s).toContain(s);
    expect(m.oauthScopes).toContain("https://www.googleapis.com/auth/script.external_request");
  });

  it("answers GET with a bare ok and no data", () => {
    const w = world();
    const r = JSON.parse(w.sandbox.doGet({}).getContent());
    expect(r).toEqual({ ok: true, relay: "tectora" });
    expect(w.calls.folder).toBe(0);
  });

  it("refuses a wrong key before touching Drive or GitHub", () => {
    const w = world();
    for (const key of ["wrong", "", KEY + "x", KEY.slice(1), 12345, null, undefined, { a: 1 }]) {
      const r = post(w, good({ key }));
      expect(r, String(key)).toMatchObject({ ok: false, error: "bad_key" });
    }
    expect(w.calls.folder).toBe(0);
    expect(w.calls.fetch).toHaveLength(0);
    expect(w.files).toHaveLength(0);
  });

  it("refuses everything when the script has no key set", () => {
    const w = world({ KEY: "" });
    expect(post(w, good({ key: "" }))).toMatchObject({ ok: false });
    expect(w.files).toHaveLength(0);
  });

  it("files one report and opens one issue with the text in a fence", () => {
    const w = world();
    const description = "Gable wrong @octocat please look\n```js\ncode\n```\nend";
    const r = post(w, good({ meta: { description, source: "app", reporter: "user", job_name: "Novák dům", app_version: "0.11.0" } }));
    expect(r.ok).toBe(true);
    expect(r.file_id).toBe(w.files[0].id);
    expect(r.issue_url).toBe("https://github.com/jakubkodajek/Tectora/issues/12");
    expect(w.files).toHaveLength(1);
    expect(w.files[0].name).toBe(`tectora-report-20261128-143012-novak-dum-${ID.slice(0, 8)}.zip`);
    expect(w.files[0].bytes.length).toBeGreaterThan(4);

    expect(w.calls.fetch).toHaveLength(1);
    const f = w.calls.fetch[0];
    expect(f.url).toBe("https://api.github.com/repos/jakubkodajek/Tectora/issues");
    expect(String(f.method).toLowerCase()).toBe("post");
    expect(JSON.stringify(f.headers)).toContain(TOKEN);
    const issue = JSON.parse(f.payload);
    expect(issue.title).toMatch(/^\[report\] Gable wrong/);
    expect(issue.title.length).toBeLessThanOrEqual("[report] ".length + 80);
    expect(issue.labels).toContain("bug-report");
    expect(issue.assignees).toContain("jakubkodajek");
    // The description sits in a fence longer than any backtick run inside it.
    const m = issue.body.match(/^(`{4,})[^\n]*\n([\s\S]*?)\n\1\s*$/m);
    expect(m, issue.body).not.toBeNull();
    expect(m[2]).toContain("code");
    // The Drive link and the report id are there; no mention can notify anyone.
    expect(issue.body).toContain(w.files[0].getUrl());
    expect(issue.body).toContain(ID);
    expect(issue.body).not.toMatch(/@[A-Za-z0-9]/);
    // The script's secrets never come back or land in the issue.
    expect(JSON.stringify(r)).not.toContain(TOKEN);
    expect(JSON.stringify(r)).not.toContain(KEY);
    expect(issue.body).not.toContain(TOKEN);
    expect(issue.body).not.toContain(KEY);
    expect(w.files[0].description).toContain("issue: https://github.com/jakubkodajek/Tectora/issues/12");
  });

  it("marks a probe report as a test and uses the repository from the properties", () => {
    const w = world({ GITHUB_REPO: "someone/else" });
    post(w, good({ meta: { description: "probe", test: true, source: "probe" } }));
    const f = w.calls.fetch[0];
    expect(f.url).toBe("https://api.github.com/repos/someone/else/issues");
    expect(JSON.parse(f.payload).title).toMatch(/^\[test\]/);
  });

  it("answers a repeated id with the first result and creates nothing twice", () => {
    const w = world();
    const first = post(w, good());
    const again = post(w, good());
    expect(again).toMatchObject({ ok: true, duplicate: true, file_id: first.file_id, issue_url: first.issue_url });
    expect(w.files).toHaveLength(1);
    expect(w.calls.fetch).toHaveLength(1);
  });

  it("answers rate_limited over the day limit and stores nothing more", () => {
    const w = world({ MAX_PER_DAY: "2" });
    expect(post(w, good(withId(1))).ok).toBe(true);
    expect(post(w, good(withId(2))).ok).toBe(true);
    const r = post(w, good(withId(3)));
    expect(r).toMatchObject({ ok: false, error: "rate_limited" });
    expect(r.retry_after_s).toBeGreaterThan(0);
    expect(r.retry_after_s).toBeLessThanOrEqual(86400);
    expect(w.files).toHaveLength(2);
    expect(w.calls.fetch).toHaveLength(2);
    // A repeat of an accepted report is still answered, not limited.
    expect(post(w, good(withId(1)))).toMatchObject({ ok: true, duplicate: true });
  });

  it("limits the bytes per day too", () => {
    const w = world({ MAX_MB_PER_DAY: "1" });
    const big = zipB64("x".repeat(700 * 1024));
    expect(post(w, good({ ...withId(1), zip_b64: big })).ok).toBe(true);
    expect(post(w, good({ ...withId(2), zip_b64: big }))).toMatchObject({ ok: false, error: "rate_limited" });
    expect(w.files).toHaveLength(1);
  });

  it("limits ten uploads a minute", () => {
    const w = world({ MAX_PER_DAY: "1000" });
    let limited = 0;
    for (let i = 1; i <= 12; i++) {
      const r = post(w, good(withId(i)));
      if (!r.ok) {
        expect(r.error).toBe("rate_limited");
        limited++;
      }
    }
    expect(limited).toBe(2);
    expect(w.files).toHaveLength(10);
  });

  it("keeps the file and says ok with no issue when GitHub fails, then the sweep files the issue", () => {
    const w = world({}, { githubStatus: 500 });
    const r = post(w, good());
    expect(r).toMatchObject({ ok: true, issue_url: null });
    expect(r.file_id).toBe(w.files[0].id);
    expect(w.files[0].description).toContain("issue: pending");
    // Back up: the daily sweep retries.
    w.github.status = 201;
    w.sandbox.sweep();
    expect(w.files[0].description).toContain("issue: https://github.com/jakubkodajek/Tectora/issues/12");
    expect(w.files[0].description).not.toContain("pending");
    // A second sweep finds nothing pending and opens nothing.
    const before = w.calls.fetch.length;
    w.sandbox.sweep();
    expect(w.calls.fetch.length).toBe(before);
  });

  it("retries an issue without labels when GitHub refuses the label", () => {
    const w = world({}, { githubStatus: 422 });
    post(w, good());
    expect(w.calls.fetch).toHaveLength(2);
    expect(JSON.parse(w.calls.fetch[1].payload).labels ?? []).toEqual([]);
  });

  it("answers bad_request or too_large for garbage and never throws", () => {
    const w = world();
    const raws = ["not json", "", "[1]", "null", "7", '"s"', "{", "\u0000", '{"v":2}', "{}"];
    for (const raw of raws) {
      const r = post(w, raw, { raw: true });
      expect(r, raw).toMatchObject({ ok: false });
      expect(["bad_request", "bad_key"], raw).toContain(r.error);
    }
    expect(JSON.parse(w.sandbox.doPost({}).getContent())).toMatchObject({ ok: false });
    expect(JSON.parse(w.sandbox.doPost({ postData: {} }).getContent())).toMatchObject({ ok: false });
    expect(JSON.parse(w.sandbox.doPost(undefined).getContent())).toMatchObject({ ok: false });
    const bad = [
      { v: 2 },
      { id: "short" },
      { id: ID.toUpperCase() },
      { id: `${ID}/../x` },
      { file_name: "../evil.zip" },
      { file_name: "evil.exe" },
      { file_name: "tectora-report-x.zip" },
      { zip_b64: Buffer.from("not a zip at all").toString("base64") },
      { zip_b64: "" },
      { zip_b64: 5 },
      { meta: "text" },
      { meta: [1] },
      { meta: null },
    ];
    for (const over of bad) {
      const r = post(w, good(over));
      expect(r, JSON.stringify(over).slice(0, 80)).toMatchObject({ ok: false, error: "bad_request" });
    }
    expect(w.files).toHaveLength(0);
    expect(w.calls.fetch).toHaveLength(0);
  });

  it("refuses an upload over the size cap before decoding it", () => {
    const w = world();
    const r = post(w, good({ zip_b64: "A".repeat(30 * 1024 * 1024) }));
    expect(r).toMatchObject({ ok: false, error: "too_large" });
    expect(w.files).toHaveLength(0);
  });

  it("caps and cleans what goes into the issue", () => {
    const w = world();
    post(w, good({ meta: { description: "x".repeat(20000) + "‮", reporter: "y".repeat(5000), job_name: "z".repeat(5000), source: "app" } }));
    const issue = JSON.parse(w.calls.fetch[0].payload);
    expect(issue.body.length).toBeLessThan(12000);
    expect(issue.body).not.toContain("‮");
  });

  it("setup checks the properties and installs the daily sweep", () => {
    const w = world();
    w.sandbox.setup();
    expect(w.calls.triggers.map((t) => t.fn)).toContain("sweep");
    const missing = world({ FOLDER_ID: "" });
    expect(() => missing.sandbox.setup()).toThrow();
  });

  // ---- fix round 2 (target/work/bug-report/fix-r2.md) ----

  // ERR-003: a GitHub refusal is logged and recorded on the Drive file; the
  // sweep retries it and says what it did.
  it("logs a GitHub refusal, records it on the file and lets the sweep retry it", () => {
    for (const status of [401, 403, 404]) {
      const w = world({}, { githubStatus: status });
      const r = post(w, good());
      expect(r, String(status)).toMatchObject({ ok: true, issue_url: null });
      expect(w.logs.error.join("\n"), `${status} is not logged with console.error`).toContain(String(status));
      expect(w.files[0].description).toContain(`issue: error:${status}`);
      // The sweep retries a refused issue, not only a pending one.
      w.github.status = 201;
      w.sandbox.sweep();
      expect(w.files[0].description, String(status)).toContain("issue: https://github.com/jakubkodajek/Tectora/issues/12");
    }
  });

  it("logs a missing token too, and keeps the report", () => {
    const w = world({ GITHUB_TOKEN: "" });
    const r = post(w, good());
    expect(r).toMatchObject({ ok: true, issue_url: null });
    expect(w.logs.error.length).toBeGreaterThan(0);
    expect(w.files).toHaveLength(1);
    expect(w.files[0].description).toMatch(/issue: (pending|error:)/);
  });

  it("says in its log what the sweep did", () => {
    const w = world({}, { githubStatus: 500 });
    post(w, good());
    w.github.status = 201;
    w.sandbox.sweep();
    const log = [...w.logs.log, ...w.logs.info].join("\n");
    expect(log).toMatch(/sweep/i);
    expect(log).toMatch(/\b1\b/);
  });

  // ERR-004 / BUG-6: idempotent by the whole report id.
  it("does not file a second copy when only the first eight characters of the id match", () => {
    const w = world();
    const a = `aaaaaaaa${"0".repeat(24)}`;
    const b = `aaaaaaaa${"1".repeat(24)}`;
    expect(post(w, good({ id: a })).ok).toBe(true);
    const first = post(w, good({ id: b }));
    expect(first.ok).toBe(true);
    expect(w.files).toHaveLength(2);
    const again = post(w, good({ id: b }));
    expect(again).toMatchObject({ ok: true, duplicate: true, file_id: first.file_id });
    expect(w.files).toHaveLength(2);
    expect(w.calls.fetch).toHaveLength(2);
  });

  it("does not file another copy when the first attempt failed after the file was created", () => {
    const w = world();
    let first = true;
    w.hooks.setDescription = () => {
      if (first) {
        first = false;
        throw new Error("Drive quota");
      }
    };
    const r1 = post(w, good());
    expect(r1.ok === false || r1.ok === true).toBe(true);
    const r2 = post(w, good());
    expect(r2.ok).toBe(true);
    expect(w.files, "a retry must not leave a second Drive copy").toHaveLength(1);
    expect(w.calls.fetch.length, "one issue only").toBeLessThanOrEqual(1);
  });

  // ERR-005: the issue exists; a failing final write must not make a second one.
  it("answers ok with the issue link when the last description write fails, and the sweep opens no second issue", () => {
    const w = world();
    w.hooks.setDescription = (_file, text) => {
      if (String(text).includes("issue: https://")) throw new Error("Drive hiccup");
    };
    const r = post(w, good());
    expect(r).toMatchObject({ ok: true, issue_url: "https://github.com/jakubkodajek/Tectora/issues/12" });
    w.hooks.setDescription = null;
    w.sandbox.sweep();
    expect(w.calls.fetch, "the report got a second issue").toHaveLength(1);
  });

  // BUG-5: nothing in meta can make the issue fail, and poison cannot starve the sweep.
  it("opens the issue whatever the meta values are", () => {
    const w = world({ MAX_PER_DAY: "1000" });
    const weird = [
      { created_ms: 1e300 },
      { created_ms: -1e300 },
      { created_ms: "soon" },
      { created_ms: { a: 1 } },
      { description: 12345 },
      { description: { a: 1 } },
      { description: ["x"] },
      { reporter: { x: 1 }, job_name: 7, os: [1], install: null, source: {} },
      { included: "yes", left_out: "no" },
    ];
    weird.forEach((m, i) => {
      w.cache.clear();
      const r = post(w, good({ ...withId(i + 1), meta: { description: "ok", source: "app", ...m } }));
      expect(r, JSON.stringify(m)).toMatchObject({ ok: true });
      expect(String(r.issue_url), `no issue for ${JSON.stringify(m)}`).toMatch(/^https:\/\/github\.com\//);
    });
  });

  it("does not let reports that cannot get an issue use up the sweep's limit", () => {
    const w = world({ MAX_PER_DAY: "1000" }, { githubStatus: 500 });
    for (let i = 1; i <= 51; i++) {
      w.cache.clear();
      post(w, good({ ...withId(i), meta: { description: "POISON", source: "app" } }));
    }
    w.cache.clear();
    const legit = post(w, good({ ...withId(900), meta: { description: "genuine report", source: "app" } }));
    expect(legit.ok).toBe(true);
    // Now only the poisoned ones are refused by GitHub.
    w.github.status = 201;
    w.github.failIf = (issue) => /POISON/.test(JSON.stringify(issue));
    w.sandbox.sweep();
    const genuine = w.files.find((f) => f.id === legit.file_id);
    expect(genuine.description, "the genuine report never got its issue").toContain("issue: https://");
  });

  it("stops a sweep before the script's own time limit", () => {
    const src = CODE;
    expect(src, "the sweep has no time bound").toMatch(/Date\.now\(\)[^;\n]*(MAX|LIMIT|DEADLINE|budget|elapsed)|(MAX|LIMIT|DEADLINE)[A-Z_]*[^;\n]*Date\.now\(\)/i);
  });
});
