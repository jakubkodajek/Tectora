#!/usr/bin/env python3
"""Compares the output of the int_w3d example with reference files.

    python3 int_w3d_compare.py INT_W3D_BINARY CASES OUTDIR [--diff NAME]

CASES holds one case per line, tab separated:

    name  rd2  so2  ref_int  ref_w3d  ref_sol

Any reference may be "-" (not compared); rd2 "-" skips the INT. The core
SOL is always compared with the input SO2 (its writer must reproduce it).
Prints one row per case and file kind (BYTE-EXACT, NUM <max diff> in <n>
tokens, DIFF <first difference>, MISSING) and a summary.
"""
import difflib
import os
import subprocess
import sys


def read(p):
    with open(p, 'rb') as f:
        return f.read().decode('cp1252')


def numeric(a, b):
    ta, tb = a.split(), b.split()
    if len(ta) != len(tb):
        return None, 'token count %d vs %d' % (len(ta), len(tb))
    worst, nd = 0.0, 0
    for k, (x, y) in enumerate(zip(ta, tb)):
        if x == y:
            continue
        try:
            d = abs(float(x) - float(y))
        except ValueError:
            return None, 'token %d: %r vs %r' % (k, x, y)
        worst = max(worst, d)
        nd += 1
    return worst, '%.2g in %d tokens' % (worst, nd)


def main():
    if len(sys.argv) < 4:
        print(__doc__)
        return 2
    binary, cases_file, outdir = sys.argv[1:4]
    show = sys.argv[sys.argv.index('--diff') + 1] if '--diff' in sys.argv else None
    os.makedirs(outdir, exist_ok=True)
    cases = []
    for line in read(cases_file).splitlines():
        v = line.split('\t')
        if len(v) == 6:
            cases.append(v)
    with open(os.path.join(outdir, 'list.txt'), 'w') as f:
        for name, rd2, so2, *_ in cases:
            f.write('%s\t%s\t%s\n' % (rd2, so2, os.path.join(outdir, name)))
    subprocess.run([binary, '--list', os.path.join(outdir, 'list.txt')])
    summary = {}
    for name, rd2, so2, rint, rw3d, rsol in cases:
        for kind, ref in (('SO2', so2), ('INT', rint), ('W3D', rw3d), ('SOL', rsol)):
            if ref == '-':
                continue
            got_p = os.path.join(outdir, name + '.' + kind)
            if not os.path.exists(got_p):
                res, msg = 'MISSING', ''
            else:
                got, want = read(got_p), read(ref)
                if got == want:
                    res, msg = 'BYTE-EXACT', ''
                else:
                    worst, msg = numeric(got, want)
                    res = 'DIFF' if worst is None else 'NUM'
                    if show == name:
                        for l in difflib.unified_diff(want.split('\r\n'), got.split('\r\n'),
                                                      'ref', 'got', lineterm='', n=2):
                            print(l)
            summary.setdefault(kind, {}).setdefault(res, 0)
            summary[kind][res] += 1
            if res != 'BYTE-EXACT' or '-v' in sys.argv:
                print('%-40s %s %-10s %s' % (name, kind, res, msg))
    for kind, d in summary.items():
        print('%s: %s' % (kind, ', '.join('%s %d' % kv for kv in sorted(d.items()))))
    return 0


if __name__ == '__main__':
    sys.exit(main())
