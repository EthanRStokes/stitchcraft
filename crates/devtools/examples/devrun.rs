//! Drives the dev tools from a shell, the way the editor does:
//!
//! ```text
//! devrun <project.stitch> <build|client|server> <fabric|neoforge> [debug] [seconds]
//! ```
//!
//! Prints every event as JSON. In a debug run it continues past each pause
//! and asks for the variables once the game has connected. After `seconds`
//! (default 600) it stops the job.

use std::time::{Duration, Instant};
use stitchcraft_blocks::{ModProject, reporters};
use stitchcraft_devtools::{Action, DevTools, Event, Loader, data_dir};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [path, action, loader, rest @ ..] = &args[..] else {
        eprintln!("usage: devrun <project.stitch> <build|client|server> <fabric|neoforge> [debug] [seconds]");
        std::process::exit(2);
    };
    reporters::register();
    let mut project: ModProject =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    project.migrate_after_load();
    let debug = rest.iter().any(|a| a == "debug");
    let seconds: u64 = rest.iter().find_map(|a| a.parse().ok()).unwrap_or(600);
    let action = Action::parse(action).unwrap();
    let loader = Loader::parse(loader).unwrap();

    let mut tools = DevTools::new(data_dir());
    if action == Action::Server && !tools.eula_accepted(&project.mod_id, loader) {
        tools.accept_eula(&project.mod_id, loader).unwrap();
    }
    let job = tools
        .start(&project, action, &[loader], debug)
        .unwrap_or_else(|refusal| panic!("refused: {refusal:?}"));
    // BREAK=<id>,<id> sets breakpoints, which this continues past.
    if let Ok(ids) = std::env::var("BREAK") {
        tools.set_breakpoints(ids.split(',').map(str::to_string).collect());
    }
    let start = Instant::now();
    let mut asked = false;
    loop {
        for event in tools.poll() {
            println!("{}", serde_json::to_string(&event).unwrap());
            match &event {
                Event::Paused { side, .. } => tools.debug_command(Some(side), "continue"),
                Event::Connected { side } if !asked => {
                    asked = true;
                    tools.debug_command(Some(side), "vars");
                }
                Event::Finished { .. } => return,
                _ => {}
            }
        }
        if start.elapsed() > Duration::from_secs(seconds) {
            tools.stop(job);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
