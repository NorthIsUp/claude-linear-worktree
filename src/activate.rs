use anyhow::{bail, Result};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Bash,
    Zsh,
}

impl Shell {
    pub fn parse(s: &str) -> Result<Self> {
        // Accept a bare name or a path like /bin/zsh.
        let name = Path::new(s)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(s);
        match name {
            "bash" => Ok(Shell::Bash),
            "zsh" => Ok(Shell::Zsh),
            other => bail!("unsupported shell '{other}' (supported: bash, zsh)"),
        }
    }
}

/// Single-quote a string for POSIX shells.
pub fn sh_quote(s: &str) -> String {
    let escaped = s.replace('\'', "'\\''");
    format!("'{escaped}'")
}

/// Render the shell function that wraps the `claude-lwt` binary.
///
/// Behavior of the resulting `clw` function:
///   * `clw activate ...`            — passes through to the binary unchanged.
///   * `clw --print-worktree ...`    — runs the binary directly so its stdout
///     (the worktree path) is captured by `$(...)` instead of being eval'd.
///   * anything else                 — runs the binary with `--emit-shell` and
///     `eval`s the result, so `cd <worktree> && exec claude ...` (or just `cd`
///     when `--no-claude` is set) lands in the parent shell.
pub fn render_function(shell: Shell, binary: &Path) -> String {
    let bin = sh_quote(&binary.display().to_string());
    let _ = shell; // bash/zsh share syntax here
    format!(
        r#"clw() {{
  if [ "$1" = "activate" ]; then
    command {bin} activate "$@"
    return
  fi
  local __clw_arg
  for __clw_arg in "$@"; do
    case "$__clw_arg" in
      --) break ;;
      --print-worktree|--print-worktree=*)
        command {bin} "$@"
        return
        ;;
    esac
  done
  local __clw_out
  __clw_out=$(command {bin} --emit-shell "$@") || return $?
  eval "$__clw_out"
}}
"#
    )
}

/// Handle `clw activate [<shell>|--shell <shell>]` and print the function to
/// stdout. The shell can be a bare name (`bash`, `zsh`), a basename (`sh`-style
/// path is also accepted via `Shell::parse`), or read from `$SHELL`.
pub fn run(argv: &[std::ffi::OsString]) -> Result<()> {
    let mut shell_arg: Option<String> = None;
    let mut i = 0;
    while i < argv.len() {
        let a = argv[i].to_string_lossy().into_owned();
        match a.as_str() {
            "--shell" => {
                i += 1;
                if i >= argv.len() {
                    bail!("--shell requires a value");
                }
                shell_arg = Some(argv[i].to_string_lossy().into_owned());
            }
            s if s.starts_with("--shell=") => {
                shell_arg = Some(s.trim_start_matches("--shell=").to_string());
            }
            "-h" | "--help" => {
                print_help();
                return Ok(());
            }
            other if !other.starts_with('-') && shell_arg.is_none() => {
                shell_arg = Some(other.to_string());
            }
            other => bail!("unexpected argument to activate: {other}"),
        }
        i += 1;
    }

    let shell_src = shell_arg
        .or_else(|| std::env::var("SHELL").ok())
        .ok_or_else(|| anyhow::anyhow!("pass `clw activate <bash|zsh>` (or set $SHELL)"))?;
    let shell = Shell::parse(&shell_src)?;

    let bin = std::env::current_exe()?;
    print!("{}", render_function(shell, &bin));
    Ok(())
}

fn print_help() {
    eprintln!(
        "Usage: clw activate <bash|zsh>\n         clw activate --shell <bash|zsh>\n\n\
         Prints a shell function to stdout. Add to your shell config:\n\n  \
         eval \"$(clw activate $SHELL)\"\n"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn quotes_plain_string() {
        assert_eq!(sh_quote("hello"), "'hello'");
    }

    #[test]
    fn escapes_embedded_single_quote() {
        assert_eq!(sh_quote("a'b"), r#"'a'\''b'"#);
    }

    #[test]
    fn parse_shell_by_basename() {
        assert_eq!(Shell::parse("/bin/zsh").unwrap(), Shell::Zsh);
        assert_eq!(Shell::parse("bash").unwrap(), Shell::Bash);
        assert!(Shell::parse("fish").is_err());
    }

    #[test]
    fn renders_function_with_binary_path() {
        let out = render_function(Shell::Bash, &PathBuf::from("/usr/local/bin/claude-lwt"));
        assert!(out.contains("clw()"));
        assert!(out.contains("'/usr/local/bin/claude-lwt'"));
        assert!(out.contains("--emit-shell"));
        assert!(out.contains("activate"));
    }

    #[test]
    fn rendered_function_short_circuits_on_print_worktree() {
        let out = render_function(Shell::Zsh, &PathBuf::from("/usr/local/bin/claude-lwt"));
        // The print-worktree case must run the binary directly (no
        // --emit-shell, no eval) so `$(clw --print-worktree ...)` captures the
        // raw path.
        let marker = "--print-worktree|--print-worktree=*)";
        assert!(
            out.contains(marker),
            "expected case pattern `{marker}` in rendered function: {out}"
        );
        let print_section = out
            .split(marker)
            .nth(1)
            .expect("function must branch on --print-worktree");
        let until_eval_branch = print_section
            .split("__clw_out=")
            .next()
            .expect("eval branch follows the print-worktree branch");
        assert!(
            !until_eval_branch.contains("--emit-shell"),
            "print-worktree branch should not invoke --emit-shell"
        );
    }

    #[test]
    fn rendered_function_stops_scanning_at_double_dash() {
        let out = render_function(Shell::Bash, &PathBuf::from("/x"));
        // The scan loop must `break` on `--` so `--print-worktree` appearing
        // *after* `--` (i.e. as a passthrough flag to claude) doesn't trigger
        // the short-circuit.
        assert!(out.contains("--) break ;;"));
    }
}
