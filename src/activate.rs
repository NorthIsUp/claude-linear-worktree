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
pub fn render_function(shell: Shell, binary: &Path) -> String {
    let bin = sh_quote(&binary.display().to_string());
    let _ = shell; // bash/zsh share syntax here
    format!(
        r#"clw() {{
  if [ "$1" = "activate" ]; then
    command {bin} activate "$@"
    return
  fi
  local __clw_out
  __clw_out=$(command {bin} --emit-shell "$@") || return $?
  eval "$__clw_out"
}}
"#
    )
}

/// Handle `clw activate [--shell <name>]` and print the function to stdout.
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
            other => bail!("unexpected argument to activate: {other}"),
        }
        i += 1;
    }

    let shell_src = shell_arg
        .or_else(|| std::env::var("SHELL").ok())
        .ok_or_else(|| anyhow::anyhow!("pass --shell <bash|zsh> (or set $SHELL)"))?;
    let shell = Shell::parse(&shell_src)?;

    let bin = std::env::current_exe()?;
    print!("{}", render_function(shell, &bin));
    Ok(())
}

fn print_help() {
    eprintln!(
        "Usage: clw activate --shell <bash|zsh>\n\n\
         Prints a shell function to stdout. Add to your shell config:\n\n  \
         eval \"$(clw activate --shell $SHELL)\"\n"
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
}
