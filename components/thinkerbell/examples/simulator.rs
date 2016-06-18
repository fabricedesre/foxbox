#![feature(custom_derive, plugin)]
#![plugin(serde_macros)]

extern crate foxbox_taxonomy;
extern crate foxbox_thinkerbell;

extern crate transformable_channels;

extern crate docopt;
extern crate serde;
extern crate serde_json;

use foxbox_thinkerbell::run::Execution;
use foxbox_thinkerbell::ast::Script;
use foxbox_thinkerbell::fake_env::*;

use foxbox_taxonomy::api::User;
use foxbox_taxonomy::parse::{ Path, Parser };

use std::io::prelude::*;
use std::fs::File;
use std::thread;
use std::time::Duration as StdDuration;
use std::str::FromStr;

use transformable_channels::mpsc::*;

const USAGE: &'static str = "
Usage: simulator [options]...
       simulator --help

-h, --help            Show this message.
-r, --ruleset <path>  Load decision rules from a file.
-e, --events <path>   Load events from a file.
-s, --slowdown <num>  StdDuration of each tick, in floating point seconds. Default: no slowdown.
";


fn main () {
    use foxbox_thinkerbell::run::ExecutionEvent::*;

    println!("Preparing simulator.");
    let (tx, rx) = channel();
    let env = FakeEnv::new(Box::new(tx));
    let (tx_done, rx_done) = channel();
    thread::spawn(move || {
        for event in rx {
            match event {
                FakeEnvEvent::Done => {
                    let _ = tx_done.send(()).unwrap();
                },
                event => println!("<<< {:?}", event)
            }
        }
    });

    let args = docopt::Docopt::new(USAGE)
        .and_then(|d| d.argv(std::env::args().into_iter()).parse())
        .unwrap_or_else(|e| e.exit());

    let slowdown = match args.find("--slowdown") {
        None => StdDuration::new(0, 0),
        Some(value) => {
            let vec = value.as_vec();
            if vec.is_empty() || vec[0].is_empty() {
                StdDuration::new(0, 0)
            } else {
                let s : f64 = FromStr::from_str(vec[0]).unwrap();
                StdDuration::new(s as u64, (s.fract() * 1_000_000.0) as u32)
            }
        }
    };

    let mut runners = Vec::new();

    println!("Loading rulesets.");
    for path in args.get_vec("--ruleset") {
        print!("Loading ruleset from {}\n", path);
        let mut file = File::open(path).unwrap();
        let mut source = String::new();
        file.read_to_string(&mut source).unwrap();
        let script = Script::from_str(&source).unwrap();
        print!("Ruleset loaded, launching... ");

        let mut runner = Execution::<FakeEnv>::new();
        let (tx, rx) = channel();
        runner.start(env.clone(), script, User::None, tx).unwrap();
        match rx.recv().unwrap() {
            Starting { result: Ok(()) } => println!("ready."),
            err => panic!("Could not launch script {:?}", err)
        }
        runners.push(runner);
    }

    println!("Loading sequences of events.");
    for path in args.get_vec("--events") {
        println!("Loading events from {}...", path);
        let mut file = File::open(path).unwrap();
        let mut source = String::new();
        file.read_to_string(&mut source).unwrap();
        let json : serde_json::Value = serde_json::from_str(&source).unwrap();
        let script = Vec::<Instruction>::parse(Path::new(), &json).unwrap();
        println!("Sequence of events loaded, playing...");

        for event in script {
            thread::sleep(slowdown.clone());
            println!(">>> {:?}", event);
            env.execute(event);
            rx_done.recv().unwrap();
        }
    }

    println!("Simulation complete.");
    thread::sleep(StdDuration::new(100, 0));
}

