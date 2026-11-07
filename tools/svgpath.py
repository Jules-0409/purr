import re, math
TOK = re.compile(r'[MmLlHhVvCcSsQqTtAaZz]|[-+]?(?:\d*\.\d+|\d+\.?)(?:[eE][-+]?\d+)?')
ARGS = {'M':2,'L':2,'H':1,'V':1,'C':6,'S':4,'Q':4,'T':2,'A':7,'Z':0}
def parse(d):
    """Return list of absolute segments: ('M',x,y) ('L',x,y) ('C',x1,y1,x2,y2,x,y) ('Q',..) ('A',rx,ry,rot,la,sw,x,y) ('Z',)"""
    toks = TOK.findall(d); i = 0; out = []; cx = cy = sx = sy = 0; cmd = None; last_c = None; last_q = None
    def num():
        nonlocal i; v = float(toks[i]); i += 1; return v
    while i < len(toks):
        t = toks[i]
        if re.match(r'[A-Za-z]', t): cmd = t; i += 1
        elif cmd is None: raise ValueError(d)
        C = cmd.upper(); rel = cmd.islower()
        if C == 'Z':
            out.append(('Z',)); cx, cy = sx, sy; last_c = last_q = None
            continue
        if C == 'A':
            # flags may be glued, handle generally via num()
            rx, ry, rot = num(), num(), num()
            la = num(); sw = num(); x, y = num(), num()
            if rel: x += cx; y += cy
            out.append(('A', rx, ry, rot, la, sw, x, y)); cx, cy = x, y; last_c = last_q = None
        elif C == 'M':
            x, y = num(), num()
            if rel: x += cx; y += cy
            out.append(('M', x, y)); cx, cy = sx, sy = x, y; cmd = 'l' if rel else 'L'; last_c = last_q = None
        elif C == 'L':
            x, y = num(), num()
            if rel: x += cx; y += cy
            out.append(('L', x, y)); cx, cy = x, y; last_c = last_q = None
        elif C == 'H':
            x = num(); x = x + cx if rel else x
            out.append(('L', x, cy)); cx = x; last_c = last_q = None
        elif C == 'V':
            y = num(); y = y + cy if rel else y
            out.append(('L', cx, y)); cy = y; last_c = last_q = None
        elif C == 'C':
            a = [num() for _ in range(6)]
            if rel: a = [a[k] + (cx if k % 2 == 0 else cy) for k in range(6)]
            out.append(('C', *a)); last_c = (a[2], a[3]); cx, cy = a[4], a[5]; last_q = None
        elif C == 'S':
            a = [num() for _ in range(4)]
            if rel: a = [a[k] + (cx if k % 2 == 0 else cy) for k in range(4)]
            x1, y1 = (2*cx - last_c[0], 2*cy - last_c[1]) if last_c else (cx, cy)
            out.append(('C', x1, y1, *a)); last_c = (a[0], a[1]); cx, cy = a[2], a[3]; last_q = None
        elif C == 'Q':
            a = [num() for _ in range(4)]
            if rel: a = [a[k] + (cx if k % 2 == 0 else cy) for k in range(4)]
            out.append(('Q', *a)); last_q = (a[0], a[1]); cx, cy = a[2], a[3]; last_c = None
        elif C == 'T':
            a = [num() for _ in range(2)]
            if rel: a = [a[0] + cx, a[1] + cy]
            x1, y1 = (2*cx - last_q[0], 2*cy - last_q[1]) if last_q else (cx, cy)
            out.append(('Q', x1, y1, *a)); last_q = (x1, y1); cx, cy = a; last_c = None
    return out

def to_abs_string(segs):
    f = lambda v: ('%.3f' % v).rstrip('0').rstrip('.')
    s = []
    for g in segs:
        if g[0] == 'Z': s.append('Z')
        elif g[0] == 'A': s.append('A%s %s %s %d %d %s %s' % (f(g[1]), f(g[2]), f(g[3]), int(g[4]), int(g[5]), f(g[6]), f(g[7])))
        else: s.append(g[0] + ' '.join(f(v) for v in g[1:]))
    return ' '.join(s)

def slint_rotated(segs, angle, ox, oy, dy="0", blink=False):
    """Exact SVG affine rotation, including elliptical arc axes, in path data."""
    number = lambda v: ('%.6f' % v).rstrip('0').rstrip('.') or '0'
    fragments = []
    def literal(value):
        fragments.append(repr(value).replace("'", '"'))
    def value(expression):
        fragments.append("(" + expression + ")")
    def point(x, y):
        yy = number(y) if dy == "0" else f"({number(y)} + {dy})"
        args = f"{number(x)},{yy},{angle},{number(ox)},{number(oy)}"
        value("Anim.rx(" + args + ")"); literal(" ")
        value("Anim.ry(" + args + ")")
    for segment in segs:
        command, *values = segment
        literal(command)
        if command == 'A':
            rx, ry, axis, large, sweep, x, y = values
            literal(number(rx) + " ")
            if blink: value(number(ry) + " * root.bs")
            else: literal(number(ry))
            literal(" "); value(f"{number(axis)} + {angle}")
            literal(f" {int(large)} {int(sweep)} "); point(x, y)
        else:
            for i in range(0, len(values), 2):
                if i: literal(" ")
                point(values[i], values[i+1])
        literal(" ")
    return " + ".join(fragments)

def flatten(segs, n=24):
    pts = []; polys = []; cx = cy = 0
    for g in segs:
        if g[0] == 'M':
            if len(pts) > 1: polys.append(pts)
            pts = [(g[1], g[2])]; cx, cy = g[1], g[2]
        elif g[0] == 'L': pts.append((g[1], g[2])); cx, cy = g[1], g[2]
        elif g[0] == 'C':
            x0, y0 = cx, cy; x1, y1, x2, y2, x3, y3 = g[1:]
            for k in range(1, n + 1):
                t = k / n; u = 1 - t
                pts.append((u**3*x0 + 3*u*u*t*x1 + 3*u*t*t*x2 + t**3*x3, u**3*y0 + 3*u*u*t*y1 + 3*u*t*t*y2 + t**3*y3))
            cx, cy = x3, y3
        elif g[0] == 'Q':
            x0, y0 = cx, cy; x1, y1, x2, y2 = g[1:]
            for k in range(1, n + 1):
                t = k / n; u = 1 - t
                pts.append((u*u*x0 + 2*u*t*x1 + t*t*x2, u*u*y0 + 2*u*t*y1 + t*t*y2))
            cx, cy = x2, y2
        elif g[0] == 'A':
            pts.append((g[6], g[7])); cx, cy = g[6], g[7]
    if len(pts) > 1: polys.append(pts)
    return polys

def bbox(segs):
    xs = []; ys = []
    for poly in flatten(segs):
        for x, y in poly: xs.append(x); ys.append(y)
    return min(xs), min(ys), max(xs), max(ys)

def dash(poly, on, off, phase):
    """phase: distance into pattern at path start. returns list of polylines"""
    per = on + off; out = []; cur = None
    d = 0.0
    def state(dist):
        p = (dist + phase) % per
        return p < on
    # walk with small steps
    for (x0, y0), (x1, y1) in zip(poly, poly[1:]):
        L = math.hypot(x1 - x0, y1 - y0); steps = max(1, int(L / 0.05))
        for k in range(steps + 1 if not out and cur is None else steps + 1):
            t = k / steps; x = x0 + (x1 - x0) * t; y = y0 + (y1 - y0) * t; dd = d + L * t
            if state(dd):
                if cur is None: cur = [(x, y)]
                else: cur.append((x, y))
            else:
                if cur is not None:
                    if len(cur) > 1: out.append(cur)
                    cur = None
        d += L
    if cur is not None and len(cur) > 1: out.append(cur)
    # simplify each polyline (drop nearly-collinear points)
    res = []
    for pl in out:
        s = [pl[0]]
        for i in range(1, len(pl) - 1):
            if math.hypot(pl[i][0] - s[-1][0], pl[i][1] - s[-1][1]) > 0.35: s.append(pl[i])
        s.append(pl[-1]); res.append(s)
    return res
