"""Four independent horizontal bar charts of the 2.1.1 shell benchmark, as one SVG file.

    python bench_charts.py OUT.svg
"""
import sys

SHELLS = ["cash 2.1.1", "Git Bash", "PowerShell 7", "WSL bash", "Linux native"]
CHARTS = [
    ("Startup", "ms", [20.9, 26.5, 258.6, 3.4, 1.1], None),
    ("Interpreter, per step", "µs", [2.9, 5.6, 6.9, 2.5, 2.6], None),
    ("$(...)", "µs", [74, 20850, 113, 607, 400], 700),
    ("Start a program", "ms", [19.4, 45.9, 22.3, 0.86, 0.63], None),
]


def fmt(v):
    if v >= 1000:
        return f"{v:,.0f}"
    if v >= 50:
        return f"{v:.0f}" if v == int(v) else f"{v:.1f}"
    if v < 1:
        return f"{v:.2f}"
    return f"{v:.1f}"


PANEL_W, PANEL_H = 420, 200
LABEL_W, BAR_MAX, ROW_H, TOP = 100, 230, 30, 44
out = []
W, H = PANEL_W * 2 + 20, PANEL_H * 2 + 30
out.append(f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" '
           f'viewBox="0 0 {W} {H}" font-family="Segoe UI, Arial, sans-serif">')
out.append(f'<rect width="{W}" height="{H}" fill="#ffffff"/>')
for n, (name, unit, values, cap) in enumerate(CHARTS):
    ox = 10 + (n % 2) * (PANEL_W + 10)
    oy = 10 + (n // 2) * (PANEL_H + 10)
    out.append(f'<text x="{ox}" y="{oy + 20}" font-size="15" font-weight="600" fill="#111">'
               f'{name.replace("&", "&amp;")} <tspan font-weight="400" fill="#555">'
               f'({unit}, lower is better)</tspan></text>')
    top = max(v for v in values if cap is None or v <= cap) if cap else max(values)
    scale_max = cap or top
    x0 = ox + LABEL_W
    out.append(f'<line x1="{x0}" y1="{oy + TOP - 6}" x2="{x0}" y2="{oy + TOP + 5 * ROW_H}" '
               f'stroke="#999" stroke-width="1"/>')
    for i, (shell, v) in enumerate(zip(SHELLS, values)):
        y = oy + TOP + i * ROW_H
        cut = cap is not None and v > cap
        length = BAR_MAX + 30 if cut else max(2, v / scale_max * BAR_MAX)
        colour = "#2a78d6" if i == 0 else "#b4b2a9"
        weight = "600" if i == 0 else "400"
        out.append(f'<text x="{x0 - 8}" y="{y + 15}" text-anchor="end" dominant-baseline="middle" '
                   f'font-size="13" font-weight="{weight}" fill="#222">{shell}</text>')
        out.append(f'<rect x="{x0}" y="{y + 5}" width="{length:.1f}" height="20" rx="3" fill="{colour}"/>')
        if cut:
            bx = x0 + length - 45
            out.append(f'<path d="M{bx},{y + 2} l6,26 M{bx + 9},{y + 2} l6,26" stroke="#ffffff" stroke-width="4"/>')
            out.append(f'<text x="{x0 + 8}" y="{y + 15}" dominant-baseline="middle" font-size="13" '
                       f'fill="#222">{fmt(v)} (bar cut)</text>')
        else:
            out.append(f'<text x="{x0 + length + 6:.1f}" y="{y + 15}" dominant-baseline="middle" '
                       f'font-size="13" fill="#111">{fmt(v)}</text>')
out.append("</svg>")
with open(sys.argv[1], "w", encoding="utf-8", newline="\n") as f:
    f.write("\n".join(out))
print("written", sys.argv[1])
