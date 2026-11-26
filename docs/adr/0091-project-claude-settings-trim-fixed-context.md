# 0091: Project Claude Code settings drop tools this repo never uses

Status: accepted
Date: 2026-11-26

## Context

Every API call re-reads the whole context. Before a session reads anything, Claude Code puts 51 k tokens into it
(`docs/iteration-baseline.md`, "Session cost baseline"): mostly tool schemas and their instructions, and skill
listings. In the 20 sessions measured that was 20 % of the mean call. The largest parts are tools this repo does not
use: Artifact (a 26 KB schema plus 30 KB of instructions, never called), PowerShell (10 KB; Git Bash does the same
work, and `powershell -NoProfile -Command` still runs from it), ReportFindings, ListAgents and ScheduleWakeup (never or
almost never called), and the Unity plugin's skill listing (8 KB; its skills were called 3 times in 47 sessions;
`tools/unity-batch.sh`, `LESSONS-unity.md` and the user-level `unity-cli` skill cover the work).

## Decision

`.claude/settings.json` (checked in, applies to every session in this repo) denies the tools Artifact, PowerShell,
ReportFindings, ListAgents and ScheduleWakeup, and disables the `unity@claude-plugins-official` plugin. Denied
tools are removed from the request, not only refused.

Measured with `claude -p` in an empty project (first-call context, Haiku): 44,990 tokens with no settings, 23,045
with the deny list, 41,885 with the plugin off, 19,941 with both (-25 k tokens, -56 %). In this repo (CLAUDE.md included): 47,262 without the settings, 22,219 with them. On the 20 measured
sessions that is about $22 of $295 (7 %). Turning off claude.ai connectors (`ENABLE_CLAUDEAI_MCP_SERVERS`) or Chrome
changed nothing measurable and is not set.

## Consequences

Sessions that need one of these tools fail with a denial; re-enable it here in a reviewed commit, or for one person
in the gitignored `.claude/settings.local.json`. Re-measure when Claude Code adds large tools: run the same
`claude -p "Reply with OK only." --model haiku --output-format json` in an empty directory and compare the usage.
