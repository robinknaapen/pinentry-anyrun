use clap::Parser;
use std::process::Stdio;

use assuan_rs::{
    errors,
    errors::GpgErrorCode,
    response::{self, Response, ResponseErr},
    server::{
        self, HandlerRequest, HandlerResult, HelpResult, OptionRequest, OptionResult, ServerError,
    },
};
use async_process::Command;
use async_std::{
    io::{stdin, stdout, BufReader},
    prelude::*,
};

use percent_encoding_rfc3986::percent_decode_str;
use serde::{Deserialize, Serialize};

#[derive(Parser)]
struct Args {
    #[arg(long, default_value_t = String::from("anyrun"))]
    anyrun: String,
}

#[derive(Default, Deserialize, Serialize)]
struct ConfigRon {
    title: Option<String>,
    description: Option<String>,
}

struct HandlerConfig {
    args: Args,
    ron: ConfigRon,
}

struct Handler {
    config: HandlerConfig,
}

impl server::Handler for Handler {
    async fn option(&mut self, _o: OptionRequest<'_>) -> OptionResult {
        Ok(Response::Ok(None))
    }

    fn help(&mut self) -> HelpResult {
        None
    }

    async fn handle(&mut self, r: HandlerRequest<'_>) -> HandlerResult {
        match r {
            ("SETPROMPT", _) => Ok(Some(Response::Ok(None))),
            ("SETOK", _) => Ok(Some(Response::Ok(None))),
            ("SETCANCEL", _) => Ok(Some(Response::Ok(None))),
            ("SETNOTOK", _) => Ok(Some(Response::Ok(None))),
            ("SETERROR", _) => Ok(Some(Response::Ok(None))),
            ("SETQUALITYBAR", _) => Ok(Some(Response::Ok(None))),
            ("SETQUALITYBAR_TT", _) => Ok(Some(Response::Ok(None))),
            ("CONFIRM", _) => Ok(Some(Response::Ok(None))),
            ("MESSAGE", _) => Ok(Some(Response::Ok(None))),

            ("GETPIN", _) => match anyrun(&self.config).await {
                Ok(v) => Ok(Some(Response::D(v))),
                Err((e, s)) => Err((e, Some(s))),
            },
            ("SETTITLE", Some(d)) => {
                let decoded = match percent_decode_str(d) {
                    Ok(decoder) => match decoder.decode_utf8() {
                        Ok(v) => v.to_string(),
                        Err(_) => d.to_string(),
                    },
                    Err(_) => d.to_string(),
                };

                self.config.ron.title = Some(decoded);
                Ok(Some(Response::Ok(None)))
            }
            ("SETDESC", Some(d)) => {
                let decoded = match percent_decode_str(d) {
                    Ok(decoder) => match decoder.decode_utf8() {
                        Ok(v) => v.to_string(),
                        Err(_) => d.to_string(),
                    },
                    Err(_) => d.to_string(),
                };

                self.config.ron.description = Some(decoded);
                Ok(Some(Response::Ok(None)))
            }

            _ => Err((ResponseErr::Gpg(errors::GpgErrorCode::UnknownCommand), None)),
        }
    }

    fn reset(&mut self) {
        self.config.ron = ConfigRon::default();
    }
}

#[async_std::main]
async fn main() -> Result<(), ServerError> {
    let reader = BufReader::new(stdin()).lines();
    let writer = stdout();

    let handler = Handler {
        config: HandlerConfig {
            args: Args::parse(),
            ron: ConfigRon::default(),
        },
    };

    server::start(reader, writer, handler).await
}

async fn anyrun<'a>(config: &HandlerConfig) -> Result<String, (response::ResponseErr, String)> {
    let mut process = match Command::new(config.args.anyrun.clone())
        .args([
            "--plugins",
            "libpinentry.so",
            "--show-results-immediately",
            "true",
            "--mode",
            "pinentry",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            return Err((
                response::ResponseErr::Gpg(GpgErrorCode::Unexpected),
                format!("error while spawning anyrun: {}", e),
            ))
        }
    };

    let mut anyrun_stdin = process.stdin.take().unwrap();
    let c = match ron::to_string(&config.ron) {
        Ok(s) => s,
        Err(e) => {
            return Err((
                response::ResponseErr::Gpg(GpgErrorCode::Unexpected),
                format!("error while serializing ron {}", e),
            ))
        }
    };

    let _ = writeln!(anyrun_stdin, "{}", c).await;
    if let Err(e) = process.status().await {
        return Err((
            response::ResponseErr::Gpg(GpgErrorCode::Unexpected),
            format!("error while reading anyrun output: {}", e),
        ));
    }

    let mut lines = BufReader::new(process.stdout.take().unwrap()).lines();
    if let Some(line) = lines.next().await {
        return match line {
            Ok(v) => Ok(v),
            Err(e) => Err((
                response::ResponseErr::Gpg(GpgErrorCode::Unexpected),
                format!("error while reading anyrun output: {}", e),
            )),
        };
    }

    Ok(String::new())
}
