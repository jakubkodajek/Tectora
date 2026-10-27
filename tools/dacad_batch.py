"""Calculate many .DAD files with DACAD under Wine, several at a time.

Each worker gets its own Xvfb display, Wine prefix and DACAD folder (copied
from the given ones), and runs tools/dacad_wine_calc.sh on one file at a
time with a timeout, so a hung error dialog costs one file, not the batch.

    python tools/dacad_batch.py --prefix PREFIX --dacad DACAD_DIR \\
        --workers 4 --out OUT_DIR IN_DIR/*.DAD

Inputs are copied to OUT_DIR and calculated there; OUT_DIR/status.json
records ok / not_saved / timeout per file. Files already marked ok are
skipped, so an interrupted batch can be resumed.
"""
from __future__ import annotations

import argparse
import json
import os
import queue
import shutil
import subprocess
import threading
import time
from pathlib import Path

CALC = Path(__file__).resolve().parent / "dacad_wine_calc.sh"


def _worker(n: int, a, q: queue.Queue, status: dict, lock: threading.Lock) -> None:
    base = Path(a.workdir) / f"w{n}"
    prefix, dacad = base / "prefix", base / "DACAD"
    if not prefix.exists():
        base.mkdir(parents=True, exist_ok=True)
        shutil.copytree(a.prefix, prefix, symlinks=True)
        shutil.copytree(a.dacad, dacad)
    display = f":{20 + n}"
    xvfb = subprocess.Popen(["Xvfb", display, "-screen", "0", "1280x1024x24", "+extension", "GLX"],
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    time.sleep(2)
    env = dict(os.environ, DISPLAY=display, WINEPREFIX=str(prefix), DACAD_DIR=str(dacad))
    try:
        while True:
            try:
                f = q.get_nowait()
            except queue.Empty:
                return
            t0 = time.time()
            try:
                r = subprocess.run([str(CALC), str(f)], env=env, capture_output=True, text=True,
                                   timeout=a.timeout)
                st = "ok" if "ok " in r.stdout else "not_saved"
            except subprocess.TimeoutExpired:
                st = "timeout"
            subprocess.run(["wineserver", "-k"], env=env, capture_output=True)
            with lock:
                status[f.name] = {"status": st, "seconds": round(time.time() - t0, 1)}
                Path(a.out, "status.json").write_text(json.dumps(status, indent=1, sort_keys=True))
                done = sum(1 for v in status.values())
            print(f"[w{n}] {f.name} {st} {time.time() - t0:.0f}s ({done})", flush=True)
    finally:
        xvfb.terminate()


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("files", nargs="+", type=Path)
    ap.add_argument("--prefix", required=True)
    ap.add_argument("--dacad", required=True)
    ap.add_argument("--out", required=True, type=Path)
    ap.add_argument("--workdir", default="/tmp/claude-0/wine/workers")
    ap.add_argument("--workers", type=int, default=4)
    ap.add_argument("--timeout", type=int, default=150)
    a = ap.parse_args(argv)
    a.out.mkdir(parents=True, exist_ok=True)
    sp = a.out / "status.json"
    status = json.loads(sp.read_text()) if sp.exists() else {}
    q: queue.Queue = queue.Queue()
    for f in a.files:
        dst = a.out / f.name
        if status.get(f.name, {}).get("status") == "ok":
            continue
        shutil.copy(f, dst)
        q.put(dst)
    lock = threading.Lock()
    ts = [threading.Thread(target=_worker, args=(n, a, q, status, lock)) for n in range(a.workers)]
    for t in ts:
        t.start()
    for t in ts:
        t.join()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
