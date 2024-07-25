use async_process::Command;
use percent_encoding_rfc3986::percent_decode_str;
use std::fmt;
use std::process::Stdio;

use async_std::{
    io::{stdin, stdout, BufReader},
    prelude::*,
};
use serde::{Deserialize, Serialize};

#[derive(Default, Deserialize, Serialize)]
struct ConfigRon {
    title: Option<String>,
    description: Option<String>,
}

enum IpcIn<'a> {
    // https://www.gnupg.org/documentation/manuals/assuan/Client-requests.html#Client-requests
    Comment,

    Bye,
    Reset,
    End,
    Help,
    Option((&'a str, Option<&'a str>)),
    Nop,

    GetPin,

    SetPrompt(&'a str),
    SetDesc(&'a str),
    SetTitle(&'a str),

    Unknown(&'a str),
}

#[allow(dead_code)]
enum IpcOut<'a> {
    Comment(Option<&'a str>),

    Bye(Option<&'a str>),

    Ok(Option<&'a str>),
    Err((&'a str, String)),

    D(&'a str),
    End,
    Option((&'a str, Option<&'a str>)),
}

impl fmt::Display for IpcOut<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self {
            Self::Ok(None) => write!(f, "OK"),
            Self::Ok(Some(arg)) => write!(f, "OK {}", arg),

            Self::Err((id, desc)) => write!(f, "ERR {} {}", id, desc),

            Self::Option((s, None)) => write!(f, "OPTION {}", s),
            Self::Option((s, Some(v))) => write!(f, "OPTION {}={}", s, v),

            Self::D(arg) => write!(f, "D {}", arg),
            Self::End => write!(f, "END"),

            Self::Comment(None) => write!(f, "#"),
            Self::Comment(Some(arg)) => write!(f, "# {}", arg),

            Self::Bye(None) => write!(f, "BYE"),
            Self::Bye(Some(v)) => write!(f, "BYE {}", v),
        }
    }
}

impl<'a> From<&'a str> for IpcIn<'a> {
    fn from(command: &'a str) -> IpcIn<'a> {
        match &command[..1] {
            "#" => Self::Comment,
            _ => match command.split_once(' ').or(Some((command, ""))) {
                Some(("BYE", _)) => Self::Bye,
                Some(("RESET", _)) => Self::Reset,
                Some(("END", _)) => Self::End,
                Some(("HELP", _)) => Self::Help,
                Some(("NOP", _)) => Self::Nop,

                Some(("GETPIN", _)) => Self::GetPin,
                Some(("SETDESC", arg)) => Self::SetDesc(arg),
                Some(("SETTITLE", arg)) => Self::SetTitle(arg),
                Some(("SETPROMPT", arg)) => Self::SetPrompt(arg),

                Some(("OPTION", arg)) => match arg.split_once('=') {
                    Some((k, v)) => Self::Option((k, Some(v))),
                    None => Self::Option((arg, None)),
                },

                _ => Self::Unknown(command),
            },
        }
    }
}

#[async_std::main]
async fn main() {
    let mut config = ConfigRon::default();

    let mut lines = BufReader::new(stdin()).lines();
    let mut stdout = stdout();

    let _ = writeln!(stdout, "{}", IpcOut::Ok(Some("Pleased to meet you"))).await;
    while let Some(line) = lines.next().await {
        let l = match line {
            Ok(l) => l,
            Err(e) => {
                let _ = writeln!(stdout, "{}", IpcOut::Err(("536871187", e.to_string(),)),).await;
                break;
            }
        };

        let command = IpcIn::from(l.as_str());
        match command {
            IpcIn::Comment => continue,

            IpcIn::Bye => {
                let _ = writeln!(stdout, "{}", IpcOut::Ok(Some("closing connection"))).await;
                break;
            }

            IpcIn::Reset => {
                config = ConfigRon::default();
                let _ = writeln!(stdout, "{}", IpcOut::Ok(None)).await;
            }

            IpcIn::End => {
                todo!();
            }

            IpcIn::Help => {
                todo!();
            }

            IpcIn::Option(_p) => {
                let _ = writeln!(stdout, "{}", IpcOut::Ok(None)).await;
            }

            IpcIn::Nop => {
                let _ = writeln!(stdout, "{}", IpcOut::Ok(None)).await;
            }

            IpcIn::GetPin => match anyrun(&config).await {
                Ok(s) => {
                    let _ = writeln!(stdout, "{}", s).await;
                    let _ = writeln!(stdout, "{}", IpcOut::Ok(None)).await;
                }
                Err(e) => {
                    let _ = writeln!(stdout, "{}", e).await;
                    break;
                }
            },

            IpcIn::SetDesc(d) => {
                let decoded = match percent_decode_str(d) {
                    Ok(decoder) => match decoder.decode_utf8() {
                        Ok(v) => v.to_string(),
                        Err(_) => d.to_string(),
                    },
                    Err(_) => d.to_string(),
                };

                config.description = Some(decoded);
                let _ = writeln!(stdout, "{}", IpcOut::Ok(None)).await;
            }

            IpcIn::SetTitle(t) => {
                let decoded = match percent_decode_str(t) {
                    Ok(decoder) => match decoder.decode_utf8() {
                        Ok(v) => v.to_string(),
                        Err(_) => t.to_string(),
                    },
                    Err(_) => t.to_string(),
                };

                config.title = Some(decoded);
                let _ = writeln!(stdout, "{}", IpcOut::Ok(None)).await;
            }

            IpcIn::SetPrompt(_p) => {
                let _ = writeln!(stdout, "{}", IpcOut::Ok(None)).await;
            }

            IpcIn::Unknown(v) => {
                let _ = writeln!(stdout, "{}", IpcOut::Comment(Some(v))).await;
                let _ = writeln!(stdout, "{}", IpcOut::Ok(None)).await;
                // pinentry will break when atually returning an error
                // I need to look into that
                // let _ = writeln!(
                //     stdout,
                //     "{}",
                //     IPCOut::Err((
                //         "536871187",
                //         "Unknown IPC command <User defined source 1>".into()
                //     )),
                // )
                // .await;
            }
        }
    }
}

async fn anyrun<'a>(config: &ConfigRon) -> Result<String, IpcOut<'a>> {
    let mut process = match Command::new("anyrun")
        .args([
            "--plugins",
            "libpinentry.so",
            "--show-results-immediately",
            "true",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return Result::Err(IpcOut::Err(("ERR", e.to_string()))),
    };

    let mut anyrun_stdin = process.stdin.take().unwrap();
    let c = match ron::to_string(config) {
        Ok(s) => s,
        Err(e) => return Result::Err(IpcOut::Err(("ERR", e.to_string()))),
    };

    let _ = writeln!(anyrun_stdin, "{}", c).await;
    if let Err(e) = process.status().await {
        return Result::Err(IpcOut::Err(("ERR", e.to_string())));
    }

    let mut lines = BufReader::new(process.stdout.take().unwrap()).lines();
    if let Some(line) = lines.next().await {
        return match line {
            Ok(v) => Result::Ok(v),
            Err(e) => return Result::Err(IpcOut::Err(("ERR", e.to_string()))),
        };
    }

    Result::Err(IpcOut::Err((
        "83886179",
        "Operation cancelled <pinentry-anyrun>".into(),
    )))
}
