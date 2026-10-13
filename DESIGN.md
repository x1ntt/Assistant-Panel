# Assistant Panel Design

## 1. Objective

Preserve the existing compact desktop-tool language. Network data must be readable, directionally explicit and accompanied by reliable operation feedback.

## 2. Product Context

A Windows personal assistant side panel, normally 420px wide. Prefer the existing port-forward table to dashboard or marketing layouts.

## 3. Visual Foundations

Use the semantic light/dark variables in src/assets/main.css: background, foreground, primary, muted, accent, destructive, border and ring. No additional hard-coded palette. Light background/foreground/accent are oklch(1 0 0), oklch(0.145 0 0), oklch(0.97 0 0); dark equivalents are oklch(0.145 0 0), oklch(0.985 0 0), oklch(0.269 0 0). Inherit the global sans font and system fallback; do not introduce external fonts. Titles are 10px, tables 9px, secondary labels 8–9px. Reuse px-1/py-0.5 cells and px-2/py-1 card spacing. Use existing radius tokens, ghost buttons and Lucide icons. Highlight rows using primary/10 and a persistent leading marker; hover must not erase highlight.

## 4. Accessibility

Retain visible keyboard focus; icon controls need title and aria-label, highlight needs aria-pressed. Errors need readable text, not color alone. Existing compact desktop density is intentional; do not claim touch-size compliance.

## 5. Voice & Tone

Short Chinese labels: 网络流量、接口、速度、流量、置顶、再次置顶、高亮、取消高亮、展开、收起、不可用、重试. Explain that totals are accumulated since monitoring began. Avoid promotional copy.

## 6. Implementation Practices

Native tables with existing Button conventions. No new UI framework, images or animation. Long names truncate with full-name titles; numeric columns remain legible. Network traffic defaults to three rows with a body-level expand/collapse control and no header add action. Pinned interfaces appear first in most-recently-pinned order, persisted across restarts; pinning again promotes to first rather than toggling.

## 7. Anti-Patterns

No KPI card row, charts, gradients or invented upload/download colors. Not every action is primary. Never represent failed sampling as a successful zero or stale values as current. Collapse changes visibility, not sampling. Highlight and pinning have separate meanings.

## 8. Decision-Making

Filter-module interfaces are hidden by default using Windows FilterInterface metadata, not names or hardware status. A compact body-level show/hide button exposes the filter count. Apply display filtering before the three-row summary; retain backend ordering, pin/highlight state and sampling. Count badges represent the filtered list, with total/hidden counts stated alongside the control.

Data correctness precedes operation consistency, which precedes visual consistency and compactness. Preserve the reference table rather than introducing a second design system.

## 9. Workflow

Source backup first, then backend and IPC, UI integration, tests and packaging. Loading/error/empty/unavailable states are part of delivery.
