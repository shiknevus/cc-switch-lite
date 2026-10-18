/**
 * 用量统计里几张表（请求日志、供应商、模型、定价）共用的样式：
 * Table headers use 12px --text-2, rows are 36px, and the table stays within the page width.
 * 所有列（含数字列）一律左对齐，列头和内容同一规则。
 */
export const usageTable = {
  /** Outer table wrapper: keep the table inside the page width. */
  scroller: "w-full overflow-x-hidden",
  table: "w-full table-fixed border-collapse text-body tabular-nums",
  headRow: "h-9 border-b border-border text-caption text-fg-2",
  th: "whitespace-nowrap px-1.5 text-start font-medium first:ps-2 last:pe-2",
  row: "h-9 border-b border-border",
  rowInteractive:
    "h-9 cursor-pointer border-b border-border transition-colors hover:bg-subtle",
  td: "whitespace-nowrap px-1.5 first:ps-2 last:pe-2",
  mono: "font-mono text-caption",
  muted: "text-fg-3",
  empty: "py-10 text-center text-body text-fg-3",
  skeleton: "h-[240px] animate-pulse rounded-panel bg-subtle",
} as const;
