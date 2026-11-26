#!/usr/bin/env node
// 注释纪律检查：注释只描述「现在是什么」，不描述「改过什么」。
// 零依赖，三个仓库通用。退出码 1 表示有违规。
import fs from "node:fs";
import path from "node:path";

const ROOT = process.argv[2] || process.cwd();
const SKIP = new Set(["node_modules", ".git", "dist", "dist-electron", "target", ".vite", "coverage", ".audit"]);
const SKIP_PATH = [
  /\/src\/lib\/generated\//,
  /\/protocol\/bindings\//,
  /\/scripts\/protocol-bindings\//,
  /check-comments\.mjs$/,
];
const EXT = new Set([".ts", ".tsx", ".mjs", ".js", ".cjs", ".rs", ".css", ".nsh", ".nsi"]);

// 单行注释最大字符数
const MAX_LINE = 100;

// 禁止的句式。核心：禁止描述「修改过程 / 演进 / 外部编号」。
const RULES = [
  // ── 外部编号：仓库内无定义可查 ──
  { re: /决策\s*\d+/, kind: "外部编号", why: "「决策 N」无定义可查" },
  { re: /阶段\s*[A-Z]\b/, kind: "外部编号", why: "「阶段 X」无定义可查" },
  { re: /第[一二三四五六七八九十\d]+\s*批/, kind: "外部编号", why: "「第 N 批」无定义可查" },
  { re: /任务\s*\d+/, kind: "外部编号", why: "「任务 N」无定义可查" },
  { re: /\b[A-Z]\d+\s*首批/, kind: "外部编号", why: "「XN 首批」无定义可查" },
  { re: /设计规范\s*\d+/, kind: "外部编号", why: "「UI 设计规范 N」仓库内不存在" },
  { re: /\bStep\s*\d+/, kind: "外部编号", why: "「StepN」无定义可查" },

  // ── 修改过程：注释不该记录「这一版改了什么」 ──
  // 只抓明确的版本演进表述；「不再 X」这类描述当前行为的写法不在此列。
  // 只抓「把某实现替换成另一实现」的叙述；「换成 X 就会 Y」这类条件说明不算。
  { re: /(改由|移入(?!口)|搬出|搬入|原样搬出|抽到独立)/, kind: "修改过程", why: "描述改动动作" },
  { re: /(原先|原来|此前)是?(用|按|靠|放在|写在|由)/, kind: "修改过程", why: "描述旧实现" },
  { re: /(原先|原来|此前是|之前是|以前是|原本)/, kind: "修改过程", why: "描述旧状态" },
  { re: /(已改为|已迁移|已废弃|已移除|已替换|不再经|不再由)/, kind: "修改过程", why: "描述变更结果" },
  { re: /(重构前|重构后|演进|升级为|替换为|替代原先)/, kind: "修改过程", why: "描述演进过程" },
  { re: /(本版|本次提交|上一版|旧实现是|新实现中)/, kind: "修改过程", why: "描述版本差异" },
  { re: /(踩过|踩的坑|踩坑| stumbled|曾出现过)/, kind: "修改过程", why: "描述历史事故" },
  { re: /★/, kind: "装饰符号", why: "去掉 ★ 标记" },

  { re: /SAFETY:[^\n]*同[上前]/, kind: "偷懒注释", why: "SAFETY 须逐块说明，不可引用上文" },

  { re: /规范\s*\d+(\.\d+)*/, kind: "外部编号", why: "「规范 N」是代码反推的产物，引用它构成循环论证" },

  // ── 文风：优先正向书面陈述 ──
  { re: /——/, kind: "破折号", why: "改用句号或逗号断句" },
  { re: /(不要|不能|不可以|不准|^别在|。别在|；别在|，别在)/, kind: "否定句式", why: "改用正向陈述" },
  { re: /(不再|无需|不必|不用|不需要|无须)/, kind: "否定句式", why: "改用正向陈述" },

  // ── 空话：过程性叙述 ──
  { re: /^(这里|此处)?(需要|要注意|注意)?(说明|解释)(一下|的是)?[：:]/, kind: "空话", why: "注释开场白，无信息量" },
];

function walk(dir, acc = []) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    if (SKIP.has(e.name)) continue;
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, acc);
    else if (EXT.has(path.extname(e.name))) acc.push(p);
  }
  return acc;
}

function extractComments(file) {
  const ext = path.extname(file);
  const lines = fs.readFileSync(file, "utf8").split(/\r?\n/);
  const out = [];
  let inBlock = false;
  for (let i = 0; i < lines.length; i++) {
    const t = lines[i].trim();
    let body = null;
    if (ext === ".rs") {
      if (t.startsWith("//")) body = t.replace(/^\/[/!]?\s?/, "");
    } else if (ext === ".nsh" || ext === ".nsi") {
      if (t.startsWith(";")) body = t.replace(/^;+\s?/, "");
      else if (t.startsWith("#")) body = t.replace(/^#+\s?/, "");
    } else {
      if (inBlock) { body = t.replace(/^\*+\s?/, "").replace(/\*\/$/, ""); if (t.includes("*/")) inBlock = false; }
      else if (t.startsWith("//")) body = t.replace(/^\/+\s?/, "");
      else if (t.startsWith("/*")) { body = t.replace(/^\/\*+\s?/, ""); if (!t.includes("*/")) inBlock = true; }
    }
    if (body === null) continue;
    out.push({ line: i + 1, text: body.trimEnd() });
  }
  return out;
}

const violations = [];
for (const f of walk(ROOT).sort()) {
  const rel = path.relative(ROOT, f).replace(/\\/g, "/");
  if (SKIP_PATH.some((re) => re.test("/" + rel))) continue;
  for (const c of extractComments(f)) {
    const body = c.text.trim();
    if (body.length === 0) continue;
    for (const rule of RULES) {
      if (rule.re.test(body)) {
        violations.push({ file: rel, line: c.line, kind: rule.kind, msg: rule.why, text: body.slice(0, 100) });
        break;
      }
    }
    const len = [...body].length;
    if (len > MAX_LINE) violations.push({ file: rel, line: c.line, kind: "单行过长", msg: len + " > " + MAX_LINE + " 字", text: body.slice(0, 80) + "…" });
  }
}

if (violations.length === 0) {
  console.log("✓ 注释纪律检查通过");
  process.exit(0);
}

const byKind = new Map();
for (const v of violations) {
  if (!byKind.has(v.kind)) byKind.set(v.kind, []);
  byKind.get(v.kind).push(v);
}
const SHOW = Number(process.env.SHOW || 20);
console.error("✗ 注释纪律检查未通过，「" + violations.length + "」处违规：");
console.error("");
for (const [kind, vs] of byKind) {
  console.error("[" + kind + "] " + vs.length + " 处");
  for (const v of vs.slice(0, SHOW)) {
    console.error("  " + v.file + ":" + v.line + "  " + v.msg);
    console.error("    " + v.text);
  }
  if (vs.length > SHOW) console.error("  …还有 " + (vs.length - SHOW) + " 处");
  console.error("");
}
process.exit(1);