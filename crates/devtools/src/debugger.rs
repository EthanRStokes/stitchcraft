//! The editor's end of the debugger protocol. The game's end, and the
//! protocol itself, are `Dbg.java` - see `stitchcraft_export::runtime::dbg`.
//!
//! The debugger listens on a free localhost port. Each game started for
//! debugging (a client, a server, or both) connects once and says which it
//! is. Breakpoints are kept here and sent to every game, including one that
//! connects later; continue, step and pause go to whichever game asked.

use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread;

/// Something a game said.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    /// A game connected. `side` is `client` or `server`.
    Hello { side: String },
    /// Block `id` ran.
    At { side: String, id: String },
    /// The game stopped before block `id`. `vars` is the canvas's variables.
    Paused { side: String, id: String, vars: serde_json::Value },
    /// The variables, when asked for.
    Vars { side: String, vars: serde_json::Value },
    /// A handler threw while on block `id` (`-` when it was not on one).
    Error { side: String, id: String, message: String },
    /// The game went away.
    Gone { side: String },
}

/// Parses one line from the game. `side` is what that connection said in
/// its hello, used for every message after.
pub fn parse(side: &str, line: &str) -> Option<Message> {
    let (command, rest) = line.split_once(' ').unwrap_or((line, ""));
    let side = side.to_string();
    Some(match command {
        "hello" => Message::Hello {
            side: rest.trim().to_string(),
        },
        "at" => Message::At {
            side,
            id: rest.trim().to_string(),
        },
        "paused" => {
            let (id, json) = rest.split_once(' ').unwrap_or((rest, "{}"));
            Message::Paused {
                side,
                id: id.to_string(),
                vars: serde_json::from_str(json).unwrap_or(serde_json::Value::Null),
            }
        }
        "vars" => Message::Vars {
            side,
            vars: serde_json::from_str(rest).unwrap_or(serde_json::Value::Null),
        },
        "error" => {
            let (id, message) = rest.split_once(' ').unwrap_or((rest, ""));
            Message::Error {
                side,
                id: id.to_string(),
                message: message.to_string(),
            }
        }
        _ => return None,
    })
}

struct Shared {
    connections: Vec<(String, TcpStream)>,
    breakpoints: BTreeSet<String>,
}

/// A listening debugger.
pub struct Debugger {
    port: u16,
    shared: Arc<Mutex<Shared>>,
    messages: Receiver<Message>,
}

impl Debugger {
    /// Starts listening on a free localhost port.
    pub fn start() -> std::io::Result<Debugger> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        let shared = Arc::new(Mutex::new(Shared {
            connections: Vec::new(),
            breakpoints: BTreeSet::new(),
        }));
        let (sender, messages) = channel();
        let accept_shared = shared.clone();
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let _ = stream.set_nodelay(true);
                serve(stream, accept_shared.clone(), sender.clone());
            }
        });
        Ok(Debugger {
            port,
            shared,
            messages,
        })
    }

    /// What a game is started with: `-Dstitchcraft.debug=<this>`.
    pub fn address(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }

    /// Messages since the last call.
    pub fn take_messages(&self) -> Vec<Message> {
        self.messages.try_iter().collect()
    }

    /// Replaces the breakpoint set, in every game now and in every one that
    /// connects later.
    pub fn set_breakpoints(&self, ids: impl IntoIterator<Item = String>) {
        let mut shared = self.shared.lock().unwrap();
        shared.breakpoints = ids.into_iter().collect();
        let line = breakpoints_line(&shared.breakpoints);
        broadcast(&mut shared, None, &line);
    }

    /// Sends `command` (`continue`, `step`, `pause`, `vars`) to the game on
    /// `side`, or to every game when `side` is `None`.
    pub fn send(&self, side: Option<&str>, command: &str) {
        let mut shared = self.shared.lock().unwrap();
        broadcast(&mut shared, side, command);
    }

    /// The sides connected now.
    pub fn sides(&self) -> Vec<String> {
        self.shared
            .lock()
            .unwrap()
            .connections
            .iter()
            .map(|(side, _)| side.clone())
            .collect()
    }
}

fn breakpoints_line(ids: &BTreeSet<String>) -> String {
    let joined: Vec<&str> = ids.iter().map(String::as_str).collect();
    format!("breakpoints {}", joined.join(","))
}

fn broadcast(shared: &mut Shared, side: Option<&str>, line: &str) {
    shared.connections.retain_mut(|(their_side, stream)| {
        if side.is_some_and(|side| side != their_side) {
            return true;
        }
        stream
            .write_all(format!("{line}\n").as_bytes())
            .and_then(|_| stream.flush())
            .is_ok()
    });
}

fn serve(stream: TcpStream, shared: Arc<Mutex<Shared>>, sender: Sender<Message>) {
    thread::spawn(move || {
        let Ok(writer) = stream.try_clone() else {
            return;
        };
        let mut side = String::from("game");
        let mut registered = false;
        for line in BufReader::new(stream).lines() {
            let Ok(line) = line else { break };
            let Some(message) = parse(&side, &line) else {
                continue;
            };
            if let Message::Hello { side: said } = &message {
                side = said.clone();
                if !registered {
                    let mut shared = shared.lock().unwrap();
                    let Ok(mut clone) = writer.try_clone() else { break };
                    // A game that connects mid-session starts with the
                    // breakpoints already set.
                    let line = breakpoints_line(&shared.breakpoints);
                    let _ = clone.write_all(format!("{line}\n").as_bytes());
                    shared.connections.push((side.clone(), clone));
                    registered = true;
                }
            }
            if sender.send(message).is_err() {
                break;
            }
        }
        if registered {
            let mut shared = shared.lock().unwrap();
            if let Some(index) = shared.connections.iter().position(|(s, _)| *s == side) {
                shared.connections.remove(index);
            }
        }
        let _ = sender.send(Message::Gone { side });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn wait_for(debugger: &Debugger, matches: impl Fn(&Message) -> bool) -> Message {
        let start = Instant::now();
        loop {
            for message in debugger.take_messages() {
                if matches(&message) {
                    return message;
                }
            }
            assert!(start.elapsed() < Duration::from_secs(5), "no such message");
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn lines_parse_into_messages() {
        assert_eq!(
            parse("server", "paused abc {\"variables\":{\"score\":3}}"),
            Some(Message::Paused {
                side: "server".into(),
                id: "abc".into(),
                vars: serde_json::json!({"variables": {"score": 3}}),
            })
        );
        assert_eq!(
            parse("client", "error abc NullPointerException: boom"),
            Some(Message::Error {
                side: "client".into(),
                id: "abc".into(),
                message: "NullPointerException: boom".into(),
            })
        );
        assert_eq!(parse("client", "nonsense"), None);
    }

    #[test]
    fn a_game_gets_breakpoints_on_connect_and_commands_after() {
        let debugger = Debugger::start().unwrap();
        debugger.set_breakpoints(["b1".to_string(), "b2".to_string()]);

        let mut game = TcpStream::connect(debugger.address()).unwrap();
        game.write_all(b"hello server\n").unwrap();
        let mut reader = BufReader::new(game.try_clone().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert_eq!(line.trim(), "breakpoints b1,b2");
        wait_for(&debugger, |m| matches!(m, Message::Hello { side } if side == "server"));

        game.write_all(b"at b1\n").unwrap();
        wait_for(&debugger, |m| matches!(m, Message::At { id, side } if id == "b1" && side == "server"));

        debugger.send(Some("client"), "continue");
        debugger.send(Some("server"), "step");
        line.clear();
        reader.read_line(&mut line).unwrap();
        assert_eq!(line.trim(), "step", "a command for the client reached the server");

        drop(reader);
        drop(game);
        wait_for(&debugger, |m| matches!(m, Message::Gone { .. }));
        assert!(debugger.sides().is_empty());
    }
}
