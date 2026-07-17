#!/usr/bin/env node
// Security review tool — example tool primitive for Coral.
//
// Usage: node index.js <target_dir> [severity]
// Coral's MCP config auto-generates the launch command pointing at this script.
//
// This is an example stub. A real implementation would walk the directory,
// check for known vulnerability patterns, and output a findings report.

const target = process.argv[2] || ".";
const severity = process.argv[3] || "medium";
const levels = { low: 1, medium: 2, high: 3, critical: 4 };
const min = levels[severity] || 0;

console.log(`🔍 Scanning ${target} (${severity}+)...`);

// Stub: in a real tool, this would run actual checks
const findings = [
  { file: "config.json", rule: "hardcoded-secret", level: "high", line: 12 },
  { file: "Dockerfile",  rule: "root-user",         level: "medium", line: 1 },
  { file: "package.json", rule: "outdated-dep",      level: "low",  line: 8 },
];

for (const f of findings) {
  if (levels[f.level] >= min) {
    console.log(`  [${f.level}] ${f.file}:${f.line} — ${f.rule}`);
  }
}

console.log(`Done. ${findings.filter(f => levels[f.level] >= min).length} findings reported.`);
