use anyhow::{Result, bail};
use windows::{
    Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
    core::HSTRING,
};

pub fn launch(path: &str, args: &[String]) -> Result<()> {
    if path.trim().is_empty() {
        bail!("cannot launch an empty path");
    }

    let operation = HSTRING::from("open");
    let file = HSTRING::from(path);
    let parameters = HSTRING::from(
        args.iter()
            .map(|arg| quote_arg(arg))
            .collect::<Vec<_>>()
            .join(" "),
    );
    let directory = HSTRING::new();

    // SAFETY: every string is a live null-terminated HSTRING for the duration of the
    // call. Path and arguments are passed separately so no shell command string is
    // constructed.
    let result = unsafe {
        ShellExecuteW(
            None,
            &operation,
            &file,
            &parameters,
            &directory,
            SW_SHOWNORMAL,
        )
    };
    let code = result.0 as isize;
    if code <= 32 {
        bail!("ShellExecuteW failed with code {code}");
    }
    Ok(())
}

fn quote_arg(arg: &str) -> String {
    if arg.is_empty() {
        return "\"\"".to_string();
    }
    if !arg
        .chars()
        .any(|character| character.is_whitespace() || character == '"')
    {
        return arg.to_string();
    }

    let mut quoted = String::from("\"");
    let mut backslashes = 0usize;
    for character in arg.chars() {
        match character {
            '\\' => {
                backslashes += 1;
                quoted.push('\\');
            }
            '"' => {
                quoted.push_str(&"\\".repeat(backslashes + 1));
                quoted.push('"');
                backslashes = 0;
            }
            _ => {
                backslashes = 0;
                quoted.push(character);
            }
        }
    }
    quoted.push_str(&"\\".repeat(backslashes));
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_arguments_with_spaces_or_quotes() {
        assert_eq!(quote_arg("plain"), "plain");
        assert_eq!(quote_arg("two words"), "\"two words\"");
        assert_eq!(quote_arg("say \"hi\""), "\"say \\\"hi\\\"\"");
        assert_eq!(quote_arg(""), "\"\"");
    }
}
