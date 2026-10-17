//! Headless entry points: `openclip import <video> [model]`, `openclip export <project> [clip-id ...]`,
//! `openclip candidates <project>`. They share the engine with the app, which also makes them
//! handy for scripting and for testing the pipeline without the window.

use crate::engine;
use openclip_core::moments::{candidates, Ctx};
use openclip_core::store;
use openclip_core::transcript::{fmt_ts, join, sentences};
use serde_json::Value;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub fn run(args: &[String]) -> Option<i32> {
    let cmd = args.first()?.as_str();
    match cmd {
        "import" => Some(import(&args[1..])),
        "export" => Some(export(&args[1..])),
        "candidates" => Some(cands(&args[1..])),
        "--version" | "-V" => {
            println!("openclip {}", env!("CARGO_PKG_VERSION"));
            Some(0)
        }
        _ => None,
    }
}

fn import(a: &[String]) -> i32 {
    let Some(video) = a.first() else {
        eprintln!("usage: openclip import <video> [model: base.en | small.en | large-v3-turbo-q5_0]");
        return 2;
    };
    let model = a.get(1).cloned().unwrap_or_else(|| "small.en".into());
    let (dir, _) = match engine::create_project(video) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };
    let t0 = std::time::Instant::now();
    let emit: engine::Emit = Arc::new(|_, v: Value| {
        eprintln!("{} {:.0}%", v["stage"].as_str().unwrap_or(""), v["pct"].as_f64().unwrap_or(0.0));
    });
    let cancel = Arc::new(AtomicBool::new(false));
    match engine::run_pipeline(&dir, &engine::ImportOpts { model, language: "en".into() }, &emit, &cancel) {
        Ok(()) => {
            eprintln!("done in {:.1}s", t0.elapsed().as_secs_f64());
            println!("{}", dir.display());
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

fn export(a: &[String]) -> i32 {
    let Some(dirp) = a.first() else {
        eprintln!("usage: openclip export <project dir> [clip id ...]   (default: all picked clips)");
        return 2;
    };
    let dir = Path::new(dirp);
    let mut proj = match store::load_project(dir) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };
    // merge in anything Claude saved that the app has not picked up yet
    let sug = store::load_suggestions(dir);
    for c in sug.clips {
        if !proj.clips.iter().any(|x| x.id == c.id) {
            proj.clips.push(c);
        }
    }
    let t = match store::load_transcript(dir) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };
    let faces = store::load_faces(dir);
    let ids = &a[1..];
    let clips: Vec<_> = proj.clips.iter().filter(|c| !c.dismissed && if ids.is_empty() { c.picked } else { ids.contains(&c.id) }).cloned().collect();
    if clips.is_empty() {
        eprintln!("no clips to export");
        return 1;
    }
    for c in clips {
        let t0 = std::time::Instant::now();
        match engine::export_clip(dir, &proj, &t.words, faces.as_ref(), &c, &|_| false) {
            Ok(p) => println!("{p}  ({:.1}s)", t0.elapsed().as_secs_f64()),
            Err(e) => {
                eprintln!("{}: {e}", c.title);
                return 1;
            }
        }
    }
    0
}

fn cands(a: &[String]) -> i32 {
    let dir = match store::resolve(a.first().map(String::as_str)) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let t = match store::load_transcript(&dir) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let s = sentences(&t.words);
    let en = store::load_energy(&dir);
    let fc = store::load_faces(&dir);
    let c = candidates(&Ctx { words: &t.words, sents: &s, energy: en.as_ref(), faces: fc.as_ref() }, 12, 20_000, 75_000);
    for (i, m) in c.iter().enumerate() {
        let ws = &t.words[s[m.s0 as usize].w0 as usize..=s[m.s1 as usize].w1 as usize];
        println!("[{}] s{}-{} {} {}s score {:.0}\n   {}\n", i + 1, m.s0, m.s1, fmt_ts(m.t0 as u64), (m.t1 - m.t0) / 1000, m.score, join(ws));
    }
    0
}
