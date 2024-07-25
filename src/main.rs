use async_process::Command;
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

#[allow(dead_code)]
enum IPCIn<'a> {
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
enum IPCOut<'a> {
    Comment(Option<&'a str>),

    Bye(Option<&'a str>),

    Ok(Option<&'a str>),
    Err((&'a str, String)),

    D(&'a str),
    End,
    Option((&'a str, Option<&'a str>)),
}

impl fmt::Display for IPCOut<'_> {
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
            // Self::GetPin => write!(f, "GETPIN"),
            // Self::SetPrompt(arg) => write!(f, "SETPROMPT {}", arg),
            // IPC::SetTitle(arg) => write!(f, "SETTITLE {}", arg),
            // IPC::SetDesc(arg) => write!(f, "SETDESC {}", arg),
            //
            // IPC::Err((id, desc)) => write!(f, "ERR {} {}", id, desc),
            // IPC::Unknown => write!(f, "ERR 536871187 Unknown IPC command"),
        }
    }
}

impl<'a> From<&'a str> for IPCIn<'a> {
    fn from(command: &'a str) -> IPCIn<'a> {
        match &command[..1] {
            "#" => Self::Comment,
            _ => match command
                .split_once(" ")
                .or(Some((command.to_string().as_str(), "")))
            {
                Some(("BYE", _)) => Self::Bye,
                Some(("RESET", _)) => Self::Reset,
                Some(("END", _)) => Self::End,
                Some(("HELP", _)) => Self::Help,
                Some(("NOP", _)) => Self::Nop,

                Some(("GETPIN", _)) => Self::GetPin,
                Some(("SETDESC", arg)) => Self::SetDesc(arg),
                Some(("SETTITLE", arg)) => Self::SetTitle(arg),
                Some(("SETPROMPT", arg)) => Self::SetPrompt(arg),

                Some(("OPTION", arg)) => match arg.split_once("=") {
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

    let _ = writeln!(stdout, "{}", IPCOut::Ok(Some("Pleased to meet you"))).await;
    while let Some(line) = lines.next().await {
        let l = match line {
            Ok(l) => l,
            Err(e) => {
                let _ = writeln!(stdout, "{}", IPCOut::Err(("536871187", e.to_string(),)),).await;
                break;
            }
        };

        let command = IPCIn::from(l.as_str());
        match command {
            IPCIn::Comment => continue,

            IPCIn::Bye => {
                let _ = writeln!(stdout, "{}", IPCOut::Ok(Some("closing connection"))).await;
                break;
            }

            IPCIn::Reset => {
                config = ConfigRon::default();
                let _ = writeln!(stdout, "{}", IPCOut::Ok(Some("closing connection"))).await;
            }

            IPCIn::End => {
                todo!();
            }

            IPCIn::Help => {
                todo!();
            }

            IPCIn::Option(_p) => {
                let _ = writeln!(stdout, "{}", IPCOut::Ok(None)).await;
            }

            IPCIn::Nop => {
                let _ = writeln!(stdout, "{}", IPCOut::Ok(None)).await;
            }

            IPCIn::GetPin => match anyrun(&config).await {
                Ok(s) => {
                    let _ = writeln!(stdout, "{}", s).await;
                    let _ = writeln!(stdout, "{}", IPCOut::Ok(None)).await;
                }
                Err(e) => {
                    let _ = writeln!(stdout, "{}", e).await;
                    break;
                }
            },

            IPCIn::SetDesc(d) => {
                config.description = Some(d.to_string());
                let _ = writeln!(stdout, "{}", IPCOut::Ok(None)).await;
            }

            IPCIn::SetTitle(t) => {
                config.title = Some(t.to_string());
                let _ = writeln!(stdout, "{}", IPCOut::Ok(None)).await;
            }

            IPCIn::SetPrompt(_p) => {
                let _ = writeln!(stdout, "{}", IPCOut::Ok(None)).await;
            }

            IPCIn::Unknown(v) => {
                let _ = writeln!(stdout, "{}", IPCOut::Comment(Some(v))).await;
                let _ = writeln!(
                    stdout,
                    "{}",
                    IPCOut::Err((
                        "536871187",
                        "Unknown IPC command <User defined source 1>".into()
                    )),
                )
                .await;
            }
        }
    }
}

async fn anyrun<'a>(config: &ConfigRon) -> Result<String, IPCOut<'a>> {
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
        Err(e) => return Result::Err(IPCOut::Err(("ERR", e.to_string()))),
    };

    let mut anyrun_stdin = process.stdin.take().unwrap();
    let c = match ron::to_string(config) {
        Ok(s) => s,
        Err(e) => return Result::Err(IPCOut::Err(("ERR", e.to_string()))),
    };

    let _ = writeln!(anyrun_stdin, "{}", c).await;
    match process.status().await {
        Err(e) => return Result::Err(IPCOut::Err(("ERR", e.to_string()))),
        _ => {}
    }

    let mut lines = BufReader::new(process.stdout.take().unwrap()).lines();
    while let Some(line) = lines.next().await {
        return match line {
            Ok(v) => Result::Ok(v),
            Err(e) => return Result::Err(IPCOut::Err(("ERR", e.to_string()))),
        };
    }

    Result::Err(IPCOut::Err(("ERR", "Empty input".into())))
}
