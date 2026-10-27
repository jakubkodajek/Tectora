r"""Calculate many .DAD files with DACAD under Wine, several at a time.

Each worker gets its own Xvfb display, Wine prefix and DACAD folder (copied
from the given ones), and runs tools/dacad_wine_calc.sh on one file at a
time with a timeout, so a hung error dialog costs one file, not the batch.

    python tools/dacad_batch.py --prefix PREFIX --dacad DACAD_DIR \\
        --workers 4 --out OUT_DIR IN_DIR/*.DAD

Inputs are copied to OUT_DIR and calculated there; OUT_DIR/status.json
records ok / dacad_error (DACAD's own error box, e.g. E0002) / not_saved /
no_result (saved without roof lines) / timeout per file. Files already
marked ok are skipped, so an interrupted batch can be resumed.

DACAD's roof core reads %TEMP%\<name>.RD2 and writes <name>.SOL/.SO2, which
DACAD deletes again; with --keep-core they are copied to OUT_DIR/core/ while
the calculation runs (polled), for diffs below DACAD's post-processing.
"""
from __future__ import annotations

import argparse
import json
import os
import queue
import shutil
import subprocess
import sys
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))
from tectora.io.dacad_dad import read_dad  # noqa: E402

CALC = Path(__file__).resolve().parent / "dacad_wine_calc.sh"


CORE_EXT = (".RD2", ".SOL", ".SO2", ".INT", ".W3D")


def _keep_core(temp: Path, dst: Path, stem: str, stop: threading.Event) -> None:
    """Copy the roof core's temp files for *stem* until *stop* is set.

    A file is copied once it is unchanged over two polls; a copy that a
    later one extends was a partial write (DACAD writes in 4 KiB chunks)
    and is replaced; dst/<stem><ext> is the last such version, and each earlier
    distinct one (e.g. the core's .SOL before the multi-roof stage rewrites
    it) is kept as dst/<stem>.v<n><ext>."""
    dst.mkdir(parents=True, exist_ok=True)
    seen: dict[str, tuple] = {}
    kept: dict[str, list[bytes]] = {}
    while not stop.is_set():
        for ext in CORE_EXT:
            f = temp / (stem + ext)
            try:
                st = f.stat()
            except OSError:
                continue
            key = (st.st_size, st.st_mtime_ns)
            if not st.st_size or seen.get(ext) != key:
                seen[ext] = key
                continue
            try:
                data = f.read_bytes()
            except OSError:
                continue
            versions = kept.setdefault(ext, [])
            if versions and versions[-1] == data:
                continue
            if versions and data.startswith(versions[-1]):
                versions[-1] = data  # the earlier copy caught a partial write
            else:
                if versions:
                    (dst / f"{stem}.v{len(versions)}{ext}").write_bytes(versions[-1])
                versions.append(data)
            (dst / (stem + ext)).write_bytes(data)
        stop.wait(0.01)


def _worker(n: int, a, q: queue.Queue, status: dict, lock: threading.Lock) -> None:
    base = Path(a.workdir) / f"w{n}"
    prefix, dacad = base / "prefix", base / "DACAD"
    if not prefix.exists():
        base.mkdir(parents=True, exist_ok=True)
        shutil.copytree(a.prefix, prefix, symlinks=True)
        shutil.copytree(a.dacad, dacad)
    display = f":{a.display_base + n}"
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
            stop = threading.Event()
            if a.keep_core:
                temp = prefix / "drive_c" / "users" / os.environ.get("USER", "root") / "Temp"
                threading.Thread(target=_keep_core, args=(temp, Path(a.out) / "core", f.stem, stop),
                                 daemon=True).start()
            try:
                r = subprocess.run([str(CALC), str(f)], env=env, capture_output=True, text=True,
                                   timeout=a.timeout)
                st = ("ok" if "ok " in r.stdout else
                      "dacad_error" if "DACAD ERROR" in r.stderr else "not_saved")
                # A lost Ctrl+B saves the file without results (s1 R1_0554).
                if st == "ok" and not read_dad(str(f)).lines:
                    st = "no_result"
            except subprocess.TimeoutExpired:
                st = "timeout"
            stop.set()
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
    ap.add_argument("--display-base", type=int, default=20)
    ap.add_argument("--keep-core", action="store_true",
                    help="copy the roof core's RD2/SOL/SO2 files to OUT/core/")
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
