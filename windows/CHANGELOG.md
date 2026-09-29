# Changelog — Windows

The Windows port versions on its own, with `windows-v*` tags. The macOS app has
its own `CHANGELOG.md` at the repository root and its own `macos-v*` tags:
a fix on one platform never waits for the other's calendar.

> ⚠️ `releases/latest` is **ambiguous** in this repository — it resolves to
> whichever platform released last. Link to a tag, never to `/latest`.

## [Unreleased]

### `claude <group>` now works in the Command Prompt

It did not, and the failure was silent in the worst way. The integration covered
the PowerShell `$PROFILE` and Git Bash's `~/.bashrc`; `cmd.exe` has neither, so
`claude work` fell through to `claude.exe` on the PATH and the group name became
the first prompt. The session then opened in the **default** profile and spent
that group's quota, while the accounts you asked for stayed at zero forever and
their rotation never fired. `router doctor` said "all good" — it never asked
which shell you were in, and that was the one case it could not see.

- **Enable in cmd**, in Groups → Terminal integration. It writes `AutoRun`, which
  defines a `doskey` macro. It is a separate, opt-in button because `AutoRun` is a
  global setting of your account, shared with clink, ConEmu and Anaconda —
  whatever is already there is kept, including the `&&` of a chained command.
  A `doskey` macro only exists in an interactive console, so the `cmd /c` that
  npm, MSBuild and VS Code tasks run still reaches the real `claude` untouched.
- **`router doctor` names the shell you are in**, and fails when that shell has no
  integration. It also stops blaming "a terminal opened before the integration"
  for what is an uncovered shell — in cmd, opening another terminal changes
  nothing, and that advice sent people in circles.
- **Diagnose**, in the app: the doctor's output without leaving the window. It was
  unreachable for exactly the people who needed it — the app never exposed it,
  `router.exe` is not on the PATH, and the documented invocation is PowerShell
  syntax, which errors in cmd.
- **`router uninstall-integration`**, also run by the uninstaller before it
  deletes the program. Uninstalling used to leave a broken `claude` function
  warning in red on every invocation, forever.

### Fixed

- **Rotation no longer undoes a manual choice.** Activating an account that had
  not served yet — and therefore had no sample — let the engine fall through to
  the first account in the order on the next loop, three minutes later, with
  nothing on screen explaining it. Switching now requires proof that the active
  account went over.
- **"Installed ✓" with no shell covered.** `all()` over an empty list is `true`,
  and the list is empty exactly when the Documents folder is not found.
- **A shim fix never reached anyone who had already installed.** Staleness was
  decided by whether the script mentioned the router's path; since the install
  folder does not change between versions, the old script always did.
- **The flyout cut off its own footer.** The ceiling was a fixed 900 logical
  pixels; at 150% scaling the usable height is 688, so Groups/Settings/Quit went
  off screen.
- **An open login dialog froze rotation for every group**, with no time limit.

## [1.0.0] — 2026-09-24

First Windows release. The port reads and writes the same files as the macOS
app, so a group and its accounts mean the same thing on both.

### The app

- **Groups of accounts that take turns.** Create groups, sign accounts in
  through the official flow inside the app, set the order of preference and the
  threshold. The engine swaps the active account on its own when the threshold
  is hit, without ending the session.
- **The tray**, with the ring drawn at the taskbar's small-icon size and in its
  theme. Its tooltip gives, per group, the account, the window, the percentage,
  where the number came from and how old it is.
- **A flyout** with the accounts table, next to the icon, also from the Windows
  11 overflow.
- **The Groups window**: create, rename, default or not, delete, auto-switch,
  threshold, reorder by drag or keyboard, use, sign in again, remove, and
  measure accounts. Every usage number carries its window, its reset time, its
  source and its age.
- **Terminal integration per shell** — PowerShell 7, Windows PowerShell 5.1 and
  Git Bash — each problem shipped with its fix: an execution policy that blocks
  the profile, a `.bash_profile` that ignores `.bashrc`, a `claude` function you
  already had (chained, not replaced).
- **A complete status line** in a group's sessions by default —
  `● group │ model │ branch │ context │ 5h … │ 7d … │ $cost │ e-mail` — with
  every item switchable in **Settings → Status line**, a live preview, or your
  own status-line command running after the sensor. The sensor behind it is the
  same in every mode.
- **`router.exe`** with `statusline`, `launch`, `is-group`, `rotate`, `measure`
  and `doctor`.

### The installer

NSIS, per user, in `%LOCALAPPDATA%\FalcaoTokenRouter`, no administrator rights,
English and Brazilian Portuguese, with the WebView2 bootstrapper. It is **not
code-signed**, so SmartScreen stops it once: *More info → Run anyway*.

An open `claude <group>` session keeps a `router.exe` running, and Windows will
not overwrite a running executable — the installer moves it out of the way
instead, so an update over a live session works.

### What it does not do

The inherited token meter (JSONL cost, pricing, alerts), code signing and arm64
are out of scope for this first release.

[1.0.0]: https://github.com/Falkzera/falcao-token-router/releases/tag/windows-v1.0.0
