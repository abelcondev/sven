// Command grok-sdd is the standalone Spec-Driven Development engine for the
// grok-build SDD fork. It manages the persistent OKF knowledge base under
// <workspace>/sdd and prints the single next step of the unified loop, derived
// purely from disk state plus the current git branch.
//
// The grok-build agent drives it through its normal terminal tool (there is no
// MCP layer): the .grok/rules + .grok/skills tell the agent when to run each
// subcommand. Unlike kez, `propose` never calls a model — the agent itself is
// the model, so `propose` just branches and seeds a skeleton the agent expands.
package main

import (
	"context"
	_ "embed"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"

	"grokbuild-sdd/internal/branchguard"
	"grokbuild-sdd/internal/sdd"
)

//go:embed assets/rules_sdd.md
var rulesTemplate string

func main() {
	os.Exit(run(os.Args[1:], os.Stdout, os.Stderr))
}

func run(args []string, stdout, stderr io.Writer) int {
	if len(args) == 0 || args[0] == "-h" || args[0] == "--help" || args[0] == "help" {
		_, _ = io.WriteString(stdout, help)
		return 0
	}
	cmd := args[0]
	rest := args[1:]
	root := resolveRoot(rest)
	if root == "" {
		fmt.Fprintln(stderr, "error: could not resolve a workspace directory")
		return 1
	}

	switch cmd {
	case "init":
		return cmdInit(root, stdout, stderr)
	case "status":
		return cmdStatus(root, stdout, stderr)
	case "next":
		return cmdNext(root, stdout, stderr)
	case "propose":
		return cmdPropose(root, rest, stdout, stderr)
	case "approve":
		return cmdApprove(root, rest, stdout, stderr)
	case "design":
		return cmdDesign(root, rest, stdout, stderr)
	case "approve-design":
		return cmdApproveDesign(root, rest, stdout, stderr)
	case "task":
		return cmdTask(root, rest, stdout, stderr)
	case "done":
		return cmdDone(root, rest, stdout, stderr)
	case "preflight":
		return cmdPreflight(root, stdout, stderr)
	case "ship":
		return cmdShip(root, rest, stdout, stderr)
	case "cleanup":
		return cmdCleanup(root, rest, stdout, stderr)
	case "guard":
		return cmdGuard(root, rest, stdout, stderr)
	case "hook":
		return cmdHook(rest, os.Stdin, stdout, stderr)
	default:
		fmt.Fprintf(stderr, "unknown command %q. Run `grok-sdd --help`.\n", cmd)
		return 2
	}
}

// resolveRoot picks the workspace: -C/--cwd <dir> if given, else the process cwd,
// then walks up to the enclosing git root (or an existing sdd/ dir). Falls back to
// the starting dir so grok-sdd works even outside a repo.
func resolveRoot(args []string) string {
	start, _ := flagValue(args, "-C")
	if start == "" {
		start, _ = flagValue(args, "--cwd")
	}
	if start == "" {
		if wd, err := os.Getwd(); err == nil {
			start = wd
		}
	}
	if start == "" {
		return ""
	}
	abs, err := filepath.Abs(start)
	if err != nil {
		return start
	}
	dir := abs
	for {
		if fi, err := os.Stat(filepath.Join(dir, ".git")); err == nil && (fi.IsDir() || !fi.IsDir()) {
			return dir
		}
		if fi, err := os.Stat(filepath.Join(dir, sdd.DirName)); err == nil && fi.IsDir() {
			return dir
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			break
		}
		dir = parent
	}
	return abs
}

func cmdInit(root string, stdout, stderr io.Writer) int {
	created, skipped, err := sdd.Scaffold(root)
	if err != nil {
		fmt.Fprintln(stderr, "error:", err)
		return 1
	}
	base := filepath.Join(root, sdd.DirName)
	if len(created) == 0 {
		fmt.Fprintf(stdout, "SDD knowledge base already present at %s (nothing to create).\n", base)
	} else {
		fmt.Fprintf(stdout, "Scaffolded OKF SDD knowledge base at %s:\n", base)
		for _, c := range created {
			fmt.Fprintf(stdout, "  + %s\n", c)
		}
	}
	for _, s := range skipped {
		fmt.Fprintf(stdout, "  = %s (kept)\n", s)
	}
	if wrote, err := writeRequireBranchMarker(root); err != nil {
		fmt.Fprintf(stderr, "warning: could not enable the branch guard: %v\n", err)
	} else if wrote {
		fmt.Fprintf(stdout, "  + %s (branch guard on: feature branch + PR required)\n", filepath.Join(".grok-sdd", "require-branch"))
	}
	if wrote, err := writeRulesFile(root); err != nil {
		fmt.Fprintf(stderr, "warning: could not write the SDD rules file: %v\n", err)
	} else if wrote {
		fmt.Fprintf(stdout, "  + %s (SDD policy injected into grok every turn)\n", filepath.Join(".grok", "rules", "sdd.md"))
	}
	if len(created) > 0 {
		fmt.Fprintln(stdout, "\nNext: draft a proposal in sdd/proposal.md, then promote it to sdd/decisions/NNN-name.md on approval.")
	}
	return 0
}

// writeRequireBranchMarker creates <root>/.grok-sdd/require-branch (empty) to opt
// the repo into the feature-branch guard. Reports whether it created the marker.
func writeRequireBranchMarker(root string) (bool, error) {
	dir := filepath.Join(root, ".grok-sdd")
	marker := filepath.Join(dir, "require-branch")
	if _, err := os.Stat(marker); err == nil {
		return false, nil
	}
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return false, err
	}
	if err := os.WriteFile(marker, nil, 0o644); err != nil {
		return false, err
	}
	return true, nil
}

// writeRulesFile writes .grok/rules/sdd.md — the standing SDD policy grok injects
// into context every turn while in this project. Skips an existing file so a
// user's edits are never clobbered.
func writeRulesFile(root string) (bool, error) {
	dir := filepath.Join(root, ".grok", "rules")
	path := filepath.Join(dir, "sdd.md")
	if _, err := os.Stat(path); err == nil {
		return false, nil
	}
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return false, err
	}
	if err := os.WriteFile(path, []byte(rulesTemplate), 0o644); err != nil {
		return false, err
	}
	return true, nil
}

func cmdStatus(root string, stdout, stderr io.Writer) int {
	st, err := sdd.ReadStatus(root)
	if err != nil {
		fmt.Fprintln(stderr, "error:", err)
		return 1
	}
	if !st.Present {
		fmt.Fprintf(stdout, "No SDD knowledge base found. Run `grok-sdd init` to scaffold %s/.\n", sdd.DirName)
		return 0
	}
	var done, pending, other int
	for _, t := range st.Tasks {
		switch t.Status {
		case "done", "completed":
			done++
		case "pending", "todo", "":
			pending++
		default:
			other++
		}
	}
	fmt.Fprintf(stdout, "SDD (OKF) — %s/\n", sdd.DirName)
	fmt.Fprintf(stdout, "  decisions: %d\n", st.Decisions)
	fmt.Fprintf(stdout, "  tasks:     %d (%d done, %d pending, %d other)\n", len(st.Tasks), done, pending, other)
	for _, t := range st.Tasks {
		title := t.Title
		if title != "" {
			title = " — " + title
		}
		tier := ""
		if t.Tier != "" && !isDoneStatus(t.Status) {
			tier = "  {tier: " + string(t.Tier) + "}"
		}
		fmt.Fprintf(stdout, "    [%s] %s%s%s\n", statusOrDash(t.Status), t.Name, title, tier)
	}
	if state, err := sdd.ReadLoopState(root, currentGitBranch(root)); err == nil {
		if state.Branch != "" {
			fmt.Fprintf(stdout, "  branch:    %s\n", state.Branch)
		}
		printNextAction(stdout, state.Next())
	}
	return 0
}

func cmdNext(root string, stdout, stderr io.Writer) int {
	state, err := sdd.ReadLoopState(root, currentGitBranch(root))
	if err != nil {
		fmt.Fprintln(stderr, "error:", err)
		return 1
	}
	printNextAction(stdout, state.Next())
	return 0
}

// cmdPropose branches onto the proposal's feature branch and seeds a skeleton
// proposal.md for the agent to expand with its edit tools. No provider is
// involved — the grok-build agent is the author.
func cmdPropose(root string, args []string, stdout, stderr io.Writer) int {
	description := strings.TrimSpace(strings.Join(nonFlagArgs(args), " "))
	if description == "" {
		fmt.Fprintln(stderr, `usage: grok-sdd propose "<what you want to build>"`)
		return 2
	}
	ensureProposalBranch(root, description, stdout)
	rel, err := sdd.SeedProposal(root, description, time.Now())
	if err != nil {
		fmt.Fprintln(stderr, "error:", err)
		return 1
	}
	fmt.Fprintf(stdout, "Seeded proposal skeleton at %s.\nExpand it in place (edit the file, do not overwrite) — fill the # Proposal, # Context and # Acceptance sections, set `tags: [ui]` if it involves screens. Describe WHAT and WHY, not HOW, and write no code. Then run `grok-sdd approve --title \"…\"`.\n", rel)
	return 0
}

func cmdApprove(root string, args []string, stdout, stderr io.Writer) int {
	title, err := flagValue(args, "--title")
	if err != nil {
		fmt.Fprintln(stderr, err)
		return 2
	}
	rel, err := sdd.Promote(root, title, time.Now())
	if err != nil {
		fmt.Fprintln(stderr, "error:", err)
		return 1
	}
	fmt.Fprintf(stdout, "Approved. Wrote decision %s, appended sdd/log.md, updated sdd/index.md, reset sdd/proposal.md.\n", rel)
	fmt.Fprintf(stdout, "Commit the decision and open its PR as a draft (no `-u`: it writes .git/config, which the sandbox refuses):\n  git add sdd/ && git commit -m %q\n  git push origin HEAD && gh pr create --fill --draft\n", "docs(sdd): "+filepath.Base(rel))
	fmt.Fprintf(stdout, "Run `grok-sdd next` for the next step. Implementation for this decision stays on the same branch and the same draft PR — one PR per proposal, marked ready only when every task is done.\n")
	return 0
}

func cmdTask(root string, args []string, stdout, stderr io.Writer) int {
	positional := nonFlagArgs(args)
	if len(positional) < 2 {
		fmt.Fprintln(stderr, "usage: grok-sdd task <decision-ref> <title...> [--tier trivial|standard|critical]")
		return 2
	}
	tier, err := flagValue(args, "--tier")
	if err != nil {
		fmt.Fprintln(stderr, err)
		return 2
	}
	decisionRef := positional[0]
	title := strings.Join(positional[1:], " ")
	rel, err := sdd.AddTaskWithTier(root, decisionRef, title, sdd.Tier(tier), time.Now())
	if err != nil {
		fmt.Fprintln(stderr, "error:", err)
		return 1
	}
	if strings.TrimSpace(tier) != "" {
		fmt.Fprintf(stdout, "Created task %s (pending, tier %s, linked to %s).\n", rel, tier, decisionRef)
	} else {
		fmt.Fprintf(stdout, "Created task %s (pending, tier inferred, linked to %s).\n", rel, decisionRef)
	}
	return 0
}

func cmdDesign(root string, args []string, stdout, stderr io.Writer) int {
	positional := nonFlagArgs(args)
	if len(positional) < 2 {
		fmt.Fprintln(stderr, "usage: grok-sdd design <decision-ref> <title...>")
		return 2
	}
	decisionRef := positional[0]
	title := strings.Join(positional[1:], " ")
	rel, err := sdd.AddDesign(root, decisionRef, title, time.Now())
	if err != nil {
		fmt.Fprintln(stderr, "error:", err)
		return 1
	}
	fmt.Fprintf(stdout, "Created design %s (in-review, linked to %s). Gather references, write the ASCII wireframe + composition into the file, then run `grok-sdd approve-design %s` once the human approves it.\n", rel, decisionRef, strings.TrimSuffix(filepath.Base(rel), ".md"))
	return 0
}

func cmdApproveDesign(root string, args []string, stdout, stderr io.Writer) int {
	positional := nonFlagArgs(args)
	if len(positional) < 1 {
		fmt.Fprintln(stderr, "usage: grok-sdd approve-design <design-ref>")
		return 2
	}
	rel, err := sdd.ApproveDesign(root, positional[0], time.Now())
	if err != nil {
		fmt.Fprintln(stderr, "error:", err)
		return 1
	}
	fmt.Fprintf(stdout, "Approved design %s, appended sdd/log.md. UI tasks for its decision are unblocked.\n", rel)
	return 0
}

func cmdDone(root string, args []string, stdout, stderr io.Writer) int {
	positional := nonFlagArgs(args)
	if len(positional) < 1 {
		fmt.Fprintln(stderr, `usage: grok-sdd done <task-ref> [--residual "..."]`)
		return 2
	}
	rel, err := sdd.CompleteTask(root, positional[0], time.Now())
	if err != nil {
		fmt.Fprintln(stderr, "error:", err)
		return 1
	}
	fmt.Fprintf(stdout, "Marked %s done, appended sdd/log.md.\n", rel)
	if residuals := flagValues(args, "--residual"); len(residuals) > 0 {
		createResidualTasks(root, rel, residuals, stdout, stderr)
	}
	printProposalProgress(root, rel, stdout)
	printLoopNext(root, stdout)
	return 0
}

func cmdPreflight(root string, stdout, stderr io.Writer) int {
	checks, ok := preflight(root, activeShipProbes)
	printPreflight(stdout, checks)
	if !ok {
		fmt.Fprintln(stderr, "pre-flight failed — fix the ✗ above before pushing")
		return 1
	}
	fmt.Fprintln(stdout, "Pre-flight passed — safe to push.")
	return 0
}

func cmdShip(root string, args []string, stdout, stderr io.Writer) int {
	positional := nonFlagArgs(args)
	if len(positional) < 1 {
		fmt.Fprintln(stderr, `usage: grok-sdd ship <task-ref> [--residual "..."]`)
		return 2
	}
	checks, ok := preflight(root, activeShipProbes)
	printPreflight(stdout, checks)
	if !ok {
		fmt.Fprintln(stderr, "pre-flight failed — fix the ✗ above before shipping. The task was NOT closed.")
		return 1
	}
	fmt.Fprintln(stdout, "Pre-flight passed.")
	if taskIsDone(root, positional[0]) {
		rel := "sdd/tasks/" + refStem(positional[0]) + ".md"
		fmt.Fprintf(stdout, "\n%s is already closed — nothing to re-close.\n", rel)
		printProposalProgress(root, rel, stdout)
		printLoopNext(root, stdout)
		return 0
	}
	return cmdDone(root, args, stdout, stderr)
}

func cmdCleanup(root string, args []string, stdout, stderr io.Writer) int {
	dryRun := hasFlag(args, "--dry-run")
	def := gitDefaultBranch(root)
	if def == "" {
		fmt.Fprintln(stderr, "could not determine the default branch (main/master); run from inside the repo")
		return 1
	}
	merged, err := gitMergedBranches(root, def)
	if err != nil {
		fmt.Fprintln(stderr, "could not list merged branches:", err)
		return 1
	}
	candidates := cleanupCandidates(merged, def, currentGitBranch(root))
	if len(candidates) == 0 {
		fmt.Fprintf(stdout, "No merged proposal branches to clean up (default: %s).\n", def)
		return 0
	}
	if dryRun {
		fmt.Fprintf(stdout, "Merged proposal branches (dry run — nothing deleted):\n")
		for _, b := range candidates {
			fmt.Fprintf(stdout, "  - %s\n", b)
		}
		return 0
	}
	fmt.Fprintf(stdout, "Deleting merged proposal branches (default: %s):\n", def)
	failed := 0
	for _, b := range candidates {
		if err := exec.Command("git", "-C", root, "update-ref", "-d", "refs/heads/"+b).Run(); err != nil {
			fmt.Fprintf(stderr, "  ! %s: %v\n", b, err)
			failed++
			continue
		}
		fmt.Fprintf(stdout, "  - %s (deleted)\n", b)
	}
	if failed > 0 {
		fmt.Fprintf(stderr, "%d branch(es) could not be deleted\n", failed)
		return 1
	}
	return 0
}

// cmdGuard is the entry point for the PreToolUse branch-guard hook. It checks
// whether writing <relpath> is allowed on the current branch and exits non-zero
// (printing the reason) when the guard blocks it, so the hook can deny the tool
// call. Paths that are not gated source, or a repo that has not opted in, exit 0.
func cmdGuard(root string, args []string, stdout, stderr io.Writer) int {
	positional := nonFlagArgs(args)
	if len(positional) < 1 {
		fmt.Fprintln(stderr, "usage: grok-sdd guard <relative-path>")
		return 2
	}
	rel := positional[0]
	if filepath.IsAbs(rel) {
		if r, err := filepath.Rel(root, rel); err == nil {
			rel = r
		}
	}
	if err := branchguard.Check(root, rel); err != nil {
		fmt.Fprintln(stderr, err.Error())
		return 1
	}
	return 0
}

// ---- hook mode (grok-build integration) ----

// hookEnvelope is the subset of the grok-build hook stdin payload the SDD hooks
// read. grok sends camelCase JSON on stdin for every hook event.
type hookEnvelope struct {
	Cwd           string          `json:"cwd"`
	WorkspaceRoot string          `json:"workspaceRoot"`
	ToolName      string          `json:"toolName"`
	ToolInput     json.RawMessage `json:"toolInput"`
}

// hookRoot picks the workspace for a hook run: the envelope's workspaceRoot, then
// cwd, then the process cwd — so the hook targets the repo the agent is in.
func (e hookEnvelope) hookRoot() string {
	if e.WorkspaceRoot != "" {
		return e.WorkspaceRoot
	}
	if e.Cwd != "" {
		return e.Cwd
	}
	if wd, err := os.Getwd(); err == nil {
		return wd
	}
	return "."
}

// cmdHook dispatches the two grok-build hook integrations, reading the event
// envelope from stdin and writing the hook's JSON decision to stdout.
//
//	grok-sdd hook stop         Stop hook: inject the loop's next step each turn
//	grok-sdd hook pretooluse   PreToolUse hook: branch-guard code writes
func cmdHook(args []string, stdin io.Reader, stdout, stderr io.Writer) int {
	if len(args) == 0 {
		fmt.Fprintln(stderr, "usage: grok-sdd hook <stop|pretooluse>")
		return 2
	}
	raw, _ := io.ReadAll(stdin)
	var env hookEnvelope
	_ = json.Unmarshal(raw, &env) // best-effort: an empty/garbled envelope degrades to no-op
	switch args[0] {
	case "stop":
		return hookStop(env, stdout)
	case "pretooluse":
		return hookPreToolUse(env, stdout)
	default:
		fmt.Fprintf(stderr, "unknown hook %q (use stop|pretooluse)\n", args[0])
		return 2
	}
}

// hookStop injects the SDD loop's single next step into the next turn, but only
// inside an SDD project (sdd/index.md present) so non-SDD repos are untouched. It
// emits the grok Stop-hook shape with hookSpecificOutput.additionalContext.
func hookStop(env hookEnvelope, stdout io.Writer) int {
	root := env.hookRoot()
	if _, err := os.Stat(filepath.Join(root, sdd.DirName, "index.md")); err != nil {
		emitJSON(stdout, map[string]any{"continue": true}) // not an SDD project — no injection
		return 0
	}
	state, err := sdd.ReadLoopState(root, currentGitBranch(root))
	if err != nil {
		emitJSON(stdout, map[string]any{"continue": true})
		return 0
	}
	ctx := renderLoopContext(state.Next())
	emitJSON(stdout, map[string]any{
		"continue": true,
		"hookSpecificOutput": map[string]any{
			"additionalContext": ctx,
		},
	})
	return 0
}

// renderLoopContext builds the always-on SDD reminder injected each turn: the
// standing policy plus the one recommended next step, mirroring kez's per-turn
// system-prompt injection.
func renderLoopContext(a sdd.NextAction) string {
	var b strings.Builder
	b.WriteString("[SDD loop] Current position (from grok-sdd next) — do exactly this one step, and stop at human gates. Full policy is in .grok/rules/sdd.md.\n")
	label := "next"
	if a.Gate {
		label = "next (human gate — your call)"
	}
	b.WriteString(label)
	b.WriteString(": ")
	b.WriteString(a.Summary)
	b.WriteString("\n")
	if a.Command != "" {
		b.WriteString("  command: ")
		b.WriteString(a.Command)
		b.WriteString("\n")
	}
	if a.Skill != "" {
		b.WriteString("  skill: load `")
		b.WriteString(a.Skill)
		b.WriteString("` first (its dense phase rules), then act\n")
	}
	if a.Then != "" {
		b.WriteString("  then: ")
		b.WriteString(a.Then)
		b.WriteString("\n")
	}
	return b.String()
}

// gatedEditTools are the grok-build tool names that write file contents and so
// must pass the branch guard. Non-editing tools are never blocked.
var gatedEditTools = map[string]bool{
	"search_replace": true,
	"apply_patch":    true,
	"edit":           true,
	"write":          true,
	"create_file":    true,
	"str_replace":    true,
}

// hookPreToolUse is the branch guard: it denies a code-writing tool call while
// HEAD is on a protected branch in a guard-enabled repo. Fail-open — anything it
// cannot positively identify as a gated write on a protected branch is allowed.
func hookPreToolUse(env hookEnvelope, stdout io.Writer) int {
	if !gatedEditTools[env.ToolName] {
		return allowTool(stdout)
	}
	path := extractToolPath(env.ToolInput)
	if path == "" {
		return allowTool(stdout) // can't determine target — don't block
	}
	root := env.hookRoot()
	rel := path
	if filepath.IsAbs(rel) {
		if r, err := filepath.Rel(root, rel); err == nil {
			rel = r
		}
	}
	if err := branchguard.Check(root, rel); err != nil {
		emitJSON(stdout, map[string]any{"decision": "deny", "reason": err.Error()})
		return 0
	}
	return allowTool(stdout)
}

func allowTool(stdout io.Writer) int {
	emitJSON(stdout, map[string]any{"decision": "allow"})
	return 0
}

// extractToolPath pulls the target file path from an edit tool's input, trying
// the field names grok's edit tools use (file_path, then path).
func extractToolPath(input json.RawMessage) string {
	if len(input) == 0 {
		return ""
	}
	var m map[string]any
	if err := json.Unmarshal(input, &m); err != nil {
		return ""
	}
	for _, key := range []string{"file_path", "path", "filePath", "filename"} {
		if v, ok := m[key].(string); ok && strings.TrimSpace(v) != "" {
			return v
		}
	}
	return ""
}

func emitJSON(stdout io.Writer, v any) {
	b, err := json.Marshal(v)
	if err != nil {
		return
	}
	fmt.Fprintln(stdout, string(b))
}

// ---- shared helpers (ported from kez internal/cli) ----

func printNextAction(stdout io.Writer, action sdd.NextAction) {
	label := "next"
	if action.Gate {
		label = "next (your call)"
	}
	fmt.Fprintf(stdout, "  %s: %s\n", label, action.Summary)
	if action.Command != "" {
		fmt.Fprintf(stdout, "         %s\n", action.Command)
	}
	if action.Skill != "" {
		fmt.Fprintf(stdout, "         skill: %s\n", action.Skill)
	}
	if action.Then != "" {
		fmt.Fprintf(stdout, "         then:  %s\n", action.Then)
	}
}

func printLoopNext(root string, stdout io.Writer) {
	state, err := sdd.ReadLoopState(root, currentGitBranch(root))
	if err != nil {
		return
	}
	fmt.Fprintf(stdout, "\n▶ Next step:\n")
	printNextAction(stdout, state.Next())
}

// currentGitBranch reads <root>/.git/HEAD, returning "" when detached, outside a
// repo, or a worktree gitdir pointer. Dependency-free by design.
func currentGitBranch(root string) string {
	data, err := os.ReadFile(filepath.Join(root, ".git", "HEAD"))
	if err != nil {
		return ""
	}
	head := strings.TrimSpace(string(data))
	const prefix = "ref: refs/heads/"
	if strings.HasPrefix(head, prefix) {
		return strings.TrimSpace(strings.TrimPrefix(head, prefix))
	}
	return ""
}

func createResidualTasks(root, taskRel string, residuals []string, stdout, stderr io.Writer) {
	decision := decisionRefForTask(root, taskRel)
	if decision == "" {
		fmt.Fprintf(stderr, "warning: %s has no decision link; cannot record residuals as follow-up tasks.\n", taskRel)
		return
	}
	fmt.Fprintf(stdout, "\nRecorded %d residual(s) as follow-up tasks on %s:\n", len(residuals), decision)
	for _, r := range residuals {
		text := strings.TrimSpace(r)
		if text == "" {
			continue
		}
		title := text
		if !strings.HasPrefix(strings.ToLower(title), "residual") {
			title = "Residual: " + title
		}
		rel, err := sdd.AddTask(root, decision, title, time.Now())
		if err != nil {
			fmt.Fprintf(stderr, "warning: could not create residual task %q: %v\n", text, err)
			continue
		}
		fmt.Fprintf(stdout, "  + %s\n", rel)
	}
}

func decisionRefForTask(root, taskRel string) string {
	st, err := sdd.ReadStatus(root)
	if err != nil {
		return ""
	}
	want := refStem(taskRel)
	for _, t := range st.Tasks {
		if refStem(t.Name) == want {
			return strings.TrimSpace(t.Decision)
		}
	}
	return ""
}

func printProposalProgress(root, completedRel string, stdout io.Writer) {
	st, err := sdd.ReadStatus(root)
	if err != nil || len(st.Tasks) == 0 {
		return
	}
	completed := refStem(completedRel)
	decision := ""
	for _, t := range st.Tasks {
		if refStem(t.Name) == completed {
			decision = refStem(t.Decision)
			break
		}
	}
	if decision == "" {
		return
	}
	var checklist strings.Builder
	pending := 0
	for _, t := range st.Tasks {
		if refStem(t.Decision) != decision {
			continue
		}
		box := "[x]"
		if !isDoneStatus(t.Status) {
			box = "[ ]"
			pending++
		}
		checklist.WriteString("  - ")
		checklist.WriteString(box)
		checklist.WriteString(" ")
		checklist.WriteString(t.Name)
		if t.Title != "" {
			checklist.WriteString(" — ")
			checklist.WriteString(t.Title)
		}
		checklist.WriteString("\n")
	}
	fmt.Fprintf(stdout, "\nProposal tasks (this PR):\n%s", checklist.String())
	if pending == 0 {
		fmt.Fprintf(stdout, "\nEvery task in this proposal is done. Refresh the PR body with the checklist above, then take it out of draft:\n  gh pr edit --body \"<checklist + manual QA>\"\n  gh pr ready\n")
	} else {
		fmt.Fprintf(stdout, "\n%d task(s) still pending — keep them on this branch (one PR per proposal) and leave the PR a draft. Push and refresh the PR body:\n  git push origin HEAD\n  gh pr edit --body \"<checklist above + manual QA>\"\n", pending)
	}
}

func refStem(ref string) string {
	ref = strings.TrimSpace(ref)
	if i := strings.LastIndexByte(ref, '/'); i >= 0 {
		ref = ref[i+1:]
	}
	return strings.TrimSuffix(ref, ".md")
}

func isDoneStatus(status string) bool {
	switch strings.TrimSpace(status) {
	case "done", "completed":
		return true
	}
	return false
}

func statusOrDash(s string) string {
	if s == "" {
		return "-"
	}
	return s
}

func ensureProposalBranch(root, description string, stdout io.Writer) {
	branch := currentGitBranch(root)
	if branch == "" {
		return
	}
	target := sdd.ProposalBranchName(description)
	if branch == target {
		return
	}
	if !sdd.IsProtectedBranch(branch) && !strings.HasPrefix(branch, "sdd/prop-") {
		return
	}
	if err := checkoutBranch(root, target); err != nil {
		fmt.Fprintf(stdout, "Note: could not open branch %s (%v); writing the proposal on %s.\n", target, err, branch)
		return
	}
	fmt.Fprintf(stdout, "Branched to %s — this proposal's doc, approval, and code land in one PR.\n", target)
}

func checkoutBranch(root, branch string) error {
	exists := exec.Command("git", "-C", root, "rev-parse", "--verify", "--quiet", "refs/heads/"+branch).Run() == nil
	if exists {
		return exec.Command("git", "-C", root, "checkout", branch).Run()
	}
	return exec.Command("git", "-C", root, "checkout", "-b", branch).Run()
}

// ---- ship pre-flight ----

type shipProbes struct {
	branch          func(root string) string
	remoteURL       func(root string) (string, error)
	remoteReachable func(root string) error
	ghAuth          func() (string, error)
}

var activeShipProbes = shipProbes{
	branch:          currentGitBranch,
	remoteURL:       gitRemoteURL,
	remoteReachable: gitRemoteReachable,
	ghAuth:          ghAuthAccount,
}

type preflightCheck struct {
	name   string
	ok     bool
	detail string
	hard   bool
}

func preflight(root string, p shipProbes) (checks []preflightCheck, ok bool) {
	branch := p.branch(root)
	switch {
	case branch == "":
		checks = append(checks, preflightCheck{"branch", false, "not on a branch (detached HEAD or not a git repo) — ship from the proposal's feature branch", true})
	case sdd.IsProtectedBranch(branch):
		checks = append(checks, preflightCheck{"branch", false, "on protected branch " + branch + " — ship from the proposal's feature branch, never the default branch", true})
	default:
		checks = append(checks, preflightCheck{"branch", true, branch, false})
	}
	url, err := p.remoteURL(root)
	if err != nil {
		checks = append(checks, preflightCheck{"remote", false, "no 'origin' remote configured — set one with `git remote add origin <url>`", true})
	} else {
		checks = append(checks, preflightCheck{"remote", true, "origin → " + url, false})
		if rerr := p.remoteReachable(root); rerr != nil {
			checks = append(checks, preflightCheck{"reachable", false, "origin unreachable: " + rerr.Error() + " (repo renamed/deleted, or the authenticated account lacks access)", true})
		} else {
			checks = append(checks, preflightCheck{"reachable", true, "origin responds to ls-remote", false})
		}
	}
	account, err := p.ghAuth()
	if err != nil {
		checks = append(checks, preflightCheck{"gh auth", false, "gh not authenticated: " + err.Error() + " — run `gh auth login`", true})
	} else {
		detail := "authenticated"
		if account != "" {
			detail = "authenticated as " + account
		}
		checks = append(checks, preflightCheck{"gh auth", true, detail, false})
	}
	ok = true
	for _, c := range checks {
		if !c.ok && c.hard {
			ok = false
		}
	}
	return checks, ok
}

func printPreflight(stdout io.Writer, checks []preflightCheck) {
	fmt.Fprintln(stdout, "Ship pre-flight:")
	for _, c := range checks {
		mark := "✓"
		if !c.ok {
			mark = "✗"
		}
		fmt.Fprintf(stdout, "  %s %s: %s\n", mark, c.name, c.detail)
	}
}

func taskIsDone(root, taskRef string) bool {
	st, err := sdd.ReadStatus(root)
	if err != nil {
		return false
	}
	want := refStem(taskRef)
	for _, t := range st.Tasks {
		if refStem(t.Name) == want {
			return isDoneStatus(t.Status)
		}
	}
	return false
}

func gitRemoteURL(root string) (string, error) {
	out, err := exec.Command("git", "-C", root, "remote", "get-url", "origin").Output()
	if err != nil {
		return "", fmt.Errorf("no origin remote")
	}
	return strings.TrimSpace(string(out)), nil
}

func gitRemoteReachable(root string) error {
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
	defer cancel()
	cmd := exec.CommandContext(ctx, "git", "-C", root, "ls-remote", "--exit-code", "origin", "HEAD")
	out, err := cmd.CombinedOutput()
	if ctx.Err() == context.DeadlineExceeded {
		return fmt.Errorf("timed out contacting origin")
	}
	if err != nil {
		return fmt.Errorf("%s", firstNonEmptyLine(string(out), err.Error()))
	}
	return nil
}

func ghAuthAccount() (string, error) {
	out, err := exec.Command("gh", "auth", "status").CombinedOutput()
	if err != nil {
		return "", fmt.Errorf("%s", firstNonEmptyLine(string(out), err.Error()))
	}
	return parseGhAccount(string(out)), nil
}

func parseGhAccount(status string) string {
	for _, line := range strings.Split(status, "\n") {
		if !strings.Contains(line, "Logged in") {
			continue
		}
		i := strings.Index(line, "account ")
		if i < 0 {
			continue
		}
		rest := strings.TrimSpace(line[i+len("account "):])
		if rest == "" {
			continue
		}
		return strings.Fields(rest)[0]
	}
	return ""
}

func firstNonEmptyLine(s, def string) string {
	for _, line := range strings.Split(s, "\n") {
		if t := strings.TrimSpace(line); t != "" {
			return t
		}
	}
	return def
}

// ---- cleanup ----

func cleanupCandidates(merged []string, defaultBranch, current string) []string {
	var out []string
	for _, b := range merged {
		b = strings.TrimSpace(strings.TrimPrefix(strings.TrimSpace(b), "* "))
		if b == "" || b == defaultBranch || b == current {
			continue
		}
		if strings.HasPrefix(b, "sdd/prop-") || strings.HasPrefix(b, "feat/") {
			out = append(out, b)
		}
	}
	return out
}

func gitDefaultBranch(root string) string {
	if out, err := exec.Command("git", "-C", root, "symbolic-ref", "--short", "refs/remotes/origin/HEAD").Output(); err == nil {
		ref := strings.TrimSpace(string(out))
		if i := strings.LastIndex(ref, "/"); i >= 0 {
			ref = ref[i+1:]
		}
		if ref != "" {
			return ref
		}
	}
	for _, b := range []string{"main", "master"} {
		if exec.Command("git", "-C", root, "rev-parse", "--verify", "--quiet", "refs/heads/"+b).Run() == nil {
			return b
		}
	}
	return ""
}

func gitMergedBranches(root, defaultBranch string) ([]string, error) {
	out, err := exec.Command("git", "-C", root, "branch", "--merged", defaultBranch, "--format", "%(refname:short)").Output()
	if err != nil {
		return nil, err
	}
	var branches []string
	for _, line := range strings.Split(string(out), "\n") {
		if s := strings.TrimSpace(line); s != "" {
			branches = append(branches, s)
		}
	}
	return branches, nil
}

// ---- flag helpers ----

func flagValue(args []string, flag string) (string, error) {
	for i, a := range args {
		if a == flag {
			if i+1 < len(args) {
				return args[i+1], nil
			}
			return "", fmt.Errorf("%s requires a value", flag)
		}
		if strings.HasPrefix(a, flag+"=") {
			return strings.TrimPrefix(a, flag+"="), nil
		}
	}
	return "", nil
}

func flagValues(args []string, flag string) []string {
	var out []string
	for i := 0; i < len(args); i++ {
		a := args[i]
		if a == flag {
			if i+1 < len(args) {
				out = append(out, args[i+1])
				i++
			}
			continue
		}
		if strings.HasPrefix(a, flag+"=") {
			out = append(out, strings.TrimPrefix(a, flag+"="))
		}
	}
	return out
}

func nonFlagArgs(args []string) []string {
	var out []string
	skipNext := false
	for _, a := range args {
		if skipNext {
			skipNext = false
			continue
		}
		if strings.HasPrefix(a, "-") {
			if !strings.Contains(a, "=") {
				skipNext = true
			}
			continue
		}
		out = append(out, a)
	}
	return out
}

func hasFlag(args []string, flag string) bool {
	for _, a := range args {
		if a == flag {
			return true
		}
	}
	return false
}

const help = `grok-sdd — Spec-Driven Development engine for the grok-build SDD fork.

Persistent, versioned spec artifacts under <workspace>/sdd:
  proposal.md   the current, in-review proposal (transient; cleared on approval)
  decisions/    approved, numbered architectural decisions (historical)
  designs/      approved UI designs (references + ASCII wireframe) — the gate before UI code
  tasks/        units of work with Given/When/Then acceptance criteria
  log.md        append-only history

Lifecycle: propose → approve (→ decision) → [design → approve-design, for UI] →
task → branch → implement → done/ship, tracked in log.md. The same loop covers
everything — discovery, architecture, foundations, and each feature. init turns on
the feature-branch guard (one PR per proposal). Run "grok-sdd next" any time to see
the one recommended next step from disk state.

Usage:
  grok-sdd init                        Scaffold sdd/ + enable the branch guard (idempotent)
  grok-sdd propose "<what & why>"      Branch + seed sdd/proposal.md for the agent to expand
  grok-sdd approve [--title <text>]    Promote proposal.md → decisions/NNN, update log + index
  grok-sdd design <decision-ref> <t…>  Scaffold an in-review UI design linked to a decision
  grok-sdd approve-design <design-ref> Approve a design, unblocking its decision's UI tasks
  grok-sdd task <decision-ref> <t…> [--tier …]  Scaffold a pending task; --tier trivial|standard|critical (else inferred)
  grok-sdd done <task-ref> [--residual "…"]  Mark a task done; each --residual becomes a follow-up task
  grok-sdd preflight                   Check branch + remote reachability + gh auth before pushing
  grok-sdd ship <task-ref> [--residual "…"]  Pre-flight, then close the task (safe close before push/PR)
  grok-sdd cleanup [--dry-run]         Delete merged proposal branches (config-write-safe, post-merge)
  grok-sdd status                      Report decisions, tasks, and the current loop position
  grok-sdd next                        Print the single recommended next step (resumable)
  grok-sdd guard <path>                Branch-guard check for the PreToolUse hook (exit≠0 = block)

Flags:
  -C, --cwd <dir>   Operate on the workspace containing <dir> (default: current directory)
  -h, --help        Show this help
`
