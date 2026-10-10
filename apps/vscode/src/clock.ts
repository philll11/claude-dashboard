// The moment ages and reset times are worked out from: now, or the moment CLAUDE_DASHBOARD_NOW
// fixes for tests, as it does for claude-dashboard itself.

export function now(): number {
  const fixed = process.env.CLAUDE_DASHBOARD_NOW;
  const ms = fixed ? Date.parse(fixed) : Number.NaN;
  return Number.isNaN(ms) ? Date.now() : ms;
}
