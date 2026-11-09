// The browser preview's bridge answers only the preview page (ARCH-009):
// any web page a developer opened while `npm run preview` ran could move the
// jobs folder, copy files into the library or write files.
import http from "node:http";
import { expect, test } from "@playwright/test";
import { BRIDGE_PORT, BRIDGE_URL, VITE_PORT } from "./env";

async function post(origin: string | null) {
  const headers: Record<string, string> = { "Content-Type": "text/plain" };
  if (origin) headers.Origin = origin;
  const res = await fetch(`${BRIDGE_URL}/projects_dir`, { method: "POST", body: "{}", headers });
  return { status: res.status, allow: res.headers.get("access-control-allow-origin"), body: await res.text() };
}

test("the preview bridge refuses other web pages", async () => {
  for (const origin of ["https://evil.example", `http://localhost:${VITE_PORT + 1}`, "null"]) {
    const r = await post(origin);
    expect(r.status, origin).toBe(403);
    expect(r.allow, origin).toBeNull();
    expect(r.body, origin).not.toContain("library");
  }
  // DNS rebinding: a page of another name that resolves to this computer.
  // (fetch always sends the real Host, so a plain request with another one.)
  const rebound = await new Promise<number>((resolve, reject) => {
    const req = http.request(
      { host: "127.0.0.1", port: BRIDGE_PORT, path: "/projects_dir", method: "POST", headers: { Host: `evil.example:${BRIDGE_PORT}` } },
      (res) => resolve(res.statusCode ?? 0),
    );
    req.on("error", reject);
    req.end("{}");
  });
  expect(rebound).toBe(403);
  for (const origin of [`http://localhost:${VITE_PORT}`, `http://127.0.0.1:${VITE_PORT}`]) {
    const r = await post(origin);
    expect(r.status, origin).toBe(200);
    expect(r.allow, origin).toBe(origin);
  }
  // Tools and tests without a browser send no Origin.
  expect((await post(null)).status).toBe(200);
});
