//! A tiny, dependency-free MCP server over stdio.
//!
//! Claude launches `openclip-mcp` itself (Claude Desktop / Claude Code), so there is nothing to
//! host or deploy. Tools are designed to keep token use low: ranked, sentence-aligned candidates
//! instead of the full transcript, ids instead of timestamps, plain text instead of JSON.

use crate::moments::{candidates, Ctx};
use crate::project::{new_id, Clip, Origin};
use crate::store;
use crate::transcript::{fmt_ts, join, sentences, Sentence, Transcript, Word};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

const INSTRUCTIONS: &str = "OpenClip turns a long 16:9 talk (e.g. a church service) into 9:16 clips. \
The video is already transcribed in the OpenClip app. Workflow: (1) get_candidates returns ~20 locally pre-ranked, sentence-aligned moments with their text; \
(2) choose the 5-10 that work completely on their own: a strong first line, one complete idea, a payoff at the end, 20-75 seconds; \
(3) save_clips with the sentence ids (from/to), a short title and a one-line reason. The clips appear live in the app for the user to preview, caption and export. \
Only call get_transcript when candidates look weak or the user asks about a specific topic or time range. Never paste transcript text back to the user; confirm briefly.";

fn tools() -> Value {
    json!([
      {
        "name": "list_projects",
        "description": "List OpenClip projects (id, title, length, clip counts). Usually unnecessary: tools default to the project open in the app.",
        "inputSchema": {"type":"object","properties":{}}
      },
      {
        "name": "get_candidates",
        "description": "Best starting point. Returns the top pre-ranked, sentence-aligned moments of the talk (id range, start time, length, local score and the spoken text), already filtered to avoid clips you saved earlier. Costs a few thousand tokens regardless of video length.",
        "inputSchema": {"type":"object","properties":{
          "project":{"type":"string","description":"Project id or title fragment. Default: the project open in the app."},
          "n":{"type":"integer","description":"How many moments (default 20, max 40)."},
          "min_s":{"type":"integer","description":"Minimum clip length in seconds (default 20)."},
          "max_s":{"type":"integer","description":"Maximum clip length in seconds (default 75)."}
        }}
      },
      {
        "name": "get_transcript",
        "description": "Compact transcript, one line per sentence as `id m:ss text`. Use a time range; the full talk can be tens of thousands of tokens.",
        "inputSchema": {"type":"object","properties":{
          "project":{"type":"string"},
          "from_s":{"type":"number","description":"Start, seconds into the video."},
          "to_s":{"type":"number","description":"End, seconds into the video."},
          "from":{"type":"integer","description":"First sentence id (alternative to from_s)."},
          "to":{"type":"integer","description":"Last sentence id (alternative to to_s)."}
        }}
      },
      {
        "name": "save_clips",
        "description": "Save chosen clips. They show up in the OpenClip app immediately. Boundaries snap to sentence starts/ends with a little breathing room. Pass `id` (from get_clips) to revise a clip you saved earlier.",
        "inputSchema": {"type":"object","required":["clips"],"properties":{
          "project":{"type":"string"},
          "clips":{"type":"array","items":{"type":"object","required":["from","to","title"],"properties":{
            "from":{"type":"integer","description":"First sentence id."},
            "to":{"type":"integer","description":"Last sentence id (inclusive)."},
            "title":{"type":"string","description":"Short, punchy title (max ~8 words)."},
            "why":{"type":"string","description":"One line: why this works as a standalone clip."},
            "score":{"type":"integer","description":"0-100 your confidence it will perform."},
            "id":{"type":"string","description":"Existing clip id to revise."}
          }}}
        }}
      },
      {
        "name": "get_clips",
        "description": "List clips already saved for the project (id, title, time range, whether the user ticked it for export).",
        "inputSchema": {"type":"object","properties":{"project":{"type":"string"}}}
      }
    ])
}

fn prompts() -> Value {
    json!([{
      "name": "find_clips",
      "description": "Find the best vertical clips in the current OpenClip project.",
      "arguments": [
        {"name":"count","description":"How many clips (default 5-10)","required":false},
        {"name":"focus","description":"Optional theme or audience to favour","required":false}
      ]
    }])
}

fn text_result(t: impl Into<String>) -> Value {
    json!({"content":[{"type":"text","text": t.into()}]})
}
fn error_result(t: impl Into<String>) -> Value {
    json!({"content":[{"type":"text","text": t.into()}], "isError": true})
}

struct Loaded {
    dir: PathBuf,
    title: String,
    words: Vec<Word>,
    sents: Vec<Sentence>,
}

fn load(args: &Value) -> Result<Loaded, String> {
    let which = args.get("project").and_then(Value::as_str);
    let dir = store::resolve(which)?;
    let p = store::load_project(&dir)?;
    if !p.transcribed {
        return Err(format!("\"{}\" is still being transcribed in OpenClip. Try again in a minute.", p.title));
    }
    let t: Transcript = store::load_transcript(&dir)?;
    let sents = sentences(&t.words);
    if sents.is_empty() {
        return Err("The transcript is empty (no speech found).".into());
    }
    Ok(Loaded { dir, title: p.title, words: t.words, sents })
}

fn clip_text(l: &Loaded, s0: usize, s1: usize, max_words: usize) -> String {
    let ws = &l.words[l.sents[s0].w0 as usize..=l.sents[s1].w1 as usize];
    if ws.len() <= max_words {
        return join(ws);
    }
    let head = max_words * 3 / 5;
    let tail = max_words - head;
    format!("{} … {}", join(&ws[..head]), join(&ws[ws.len() - tail..]))
}

fn overlaps_existing(dir: &Path, t0: u32, t1: u32) -> bool {
    let mut spans: Vec<(u64, u64)> = Vec::new();
    if let Ok(p) = store::load_project(dir) {
        spans.extend(p.clips.iter().map(|c| (c.start_ms, c.end_ms)));
    }
    spans.extend(store::load_suggestions(dir).clips.iter().map(|c| (c.start_ms, c.end_ms)));
    let (a, b) = (t0 as u64, t1 as u64);
    spans.iter().any(|&(s, e)| {
        let lo = s.max(a);
        let hi = e.min(b);
        hi > lo && (hi - lo) * 5 > (b - a).min(e - s)
    })
}

fn tool_list_projects() -> Result<Value, String> {
    let all = store::list();
    if all.is_empty() {
        return Ok(text_result("No projects. Open OpenClip and drop a video in."));
    }
    let cur = std::fs::read_to_string(store::home().join("current")).unwrap_or_default();
    let mut s = String::new();
    for p in all.iter().take(20) {
        s.push_str(&format!(
            "{} · {} · {} · {} clips ({} picked){}{}\n",
            p.id,
            p.title,
            fmt_ts(p.duration_ms),
            p.clips,
            p.picked,
            if p.transcribed { "" } else { " · transcribing" },
            if cur.trim() == p.dir { " · open in app" } else { "" }
        ));
    }
    Ok(text_result(s))
}

fn tool_candidates(args: &Value) -> Result<Value, String> {
    let l = load(args)?;
    let n = args.get("n").and_then(Value::as_u64).unwrap_or(20).clamp(1, 40) as usize;
    let min_s = args.get("min_s").and_then(Value::as_u64).unwrap_or(20).clamp(5, 120) as u32;
    let max_s = args.get("max_s").and_then(Value::as_u64).unwrap_or(75).clamp(min_s as u64 + 5, 180) as u32;
    let energy = store::load_energy(&l.dir);
    let faces = store::load_faces(&l.dir);
    let ctx = Ctx { words: &l.words, sents: &l.sents, energy: energy.as_ref(), faces: faces.as_ref() };
    let mut c = candidates(&ctx, n + 12, min_s * 1000, max_s * 1000);
    c.retain(|x| !overlaps_existing(&l.dir, x.t0, x.t1));
    c.truncate(n);
    let total = l.words.last().map(|w| w.2).unwrap_or(0);
    let mut out = format!(
        "{} · {} · {} sentences · top {} moments (ids are sentence numbers; local score 0-100). Choose 5-10 and call save_clips.\n",
        l.title,
        fmt_ts(total as u64),
        l.sents.len(),
        c.len()
    );
    for (i, m) in c.iter().enumerate() {
        out.push_str(&format!(
            "\n[{}] s{}-{} · {} · {}s · {:.0}\n{}\n",
            i + 1,
            m.s0,
            m.s1,
            fmt_ts(m.t0 as u64),
            (m.t1 - m.t0 + 500) / 1000,
            m.score,
            clip_text(&l, m.s0 as usize, m.s1 as usize, 150)
        ));
    }
    Ok(text_result(out))
}

fn tool_transcript(args: &Value) -> Result<Value, String> {
    let l = load(args)?;
    let n = l.sents.len();
    let (mut a, mut b) = (0usize, n - 1);
    if let Some(f) = args.get("from").and_then(Value::as_u64) {
        a = (f as usize).min(n - 1);
    }
    if let Some(t) = args.get("to").and_then(Value::as_u64) {
        b = (t as usize).min(n - 1);
    }
    if let Some(s) = args.get("from_s").and_then(Value::as_f64) {
        a = l.sents.iter().position(|x| x.t1 as f64 >= s * 1000.0).unwrap_or(n - 1);
    }
    if let Some(s) = args.get("to_s").and_then(Value::as_f64) {
        b = l.sents.iter().rposition(|x| x.t0 as f64 <= s * 1000.0).unwrap_or(0);
    }
    if b < a {
        b = a;
    }
    let mut out = String::new();
    let mut last = a;
    for i in a..=b {
        let s = &l.sents[i];
        let line = format!("{} {} {}\n", i, fmt_ts(s.t0 as u64), join(&l.words[s.w0 as usize..=s.w1 as usize]));
        if out.len() + line.len() > 60_000 {
            out.push_str(&format!("… truncated; continue with from={}\n", i));
            break;
        }
        out.push_str(&line);
        last = i;
    }
    let _ = last;
    Ok(text_result(out))
}

fn tool_save(args: &Value) -> Result<Value, String> {
    let l = load(args)?;
    let items = args.get("clips").and_then(Value::as_array).ok_or("`clips` must be an array")?;
    if items.is_empty() {
        return Err("`clips` is empty".into());
    }
    let mut sug = store::load_suggestions(&l.dir);
    let existing = store::load_project(&l.dir).map(|p| p.clips).unwrap_or_default();
    sug.rev += 1;
    let mut report = String::new();
    let mut saved = 0;
    for (k, it) in items.iter().enumerate() {
        let from = it.get("from").and_then(Value::as_u64);
        let to = it.get("to").and_then(Value::as_u64);
        let (Some(from), Some(to)) = (from, to) else {
            report.push_str(&format!("#{}: needs `from` and `to` sentence ids\n", k + 1));
            continue;
        };
        let (mut a, mut b) = (from as usize, to as usize);
        if a > b {
            std::mem::swap(&mut a, &mut b);
        }
        if b >= l.sents.len() {
            report.push_str(&format!("#{}: sentence {} is out of range (0-{})\n", k + 1, b, l.sents.len() - 1));
            continue;
        }
        // breathing room, never into a neighbouring word
        let w0 = l.sents[a].w0 as usize;
        let w1 = l.sents[b].w1 as usize;
        let mut start = l.sents[a].t0.saturating_sub(180);
        if w0 > 0 {
            start = start.max(l.words[w0 - 1].2);
        }
        let mut end = l.sents[b].t1 + 380;
        if w1 + 1 < l.words.len() {
            end = end.min(l.words[w1 + 1].1.saturating_sub(20).max(l.sents[b].t1));
        }
        let secs = (end - start) as f32 / 1000.0;
        if secs > 180.0 {
            report.push_str(&format!("#{}: {:.0}s is too long for a short clip (max 180s)\n", k + 1, secs));
            continue;
        }
        let title = it.get("title").and_then(Value::as_str).unwrap_or("Clip").trim().to_string();
        let why = it.get("why").and_then(Value::as_str).unwrap_or("").trim().to_string();
        let score = it.get("score").and_then(Value::as_f64).unwrap_or(75.0).clamp(0.0, 100.0) as f32;
        let id_in = it.get("id").and_then(Value::as_str);

        let slot = id_in.and_then(|id| sug.clips.iter().position(|c| c.id == id));
        let known = id_in.map(|id| existing.iter().any(|c| c.id == id) || slot.is_some()).unwrap_or(false);
        if id_in.is_some() && !known {
            report.push_str(&format!("#{}: unknown id {}\n", k + 1, id_in.unwrap()));
            continue;
        }
        let mut clip = match slot {
            Some(i) => sug.clips[i].clone(),
            None => existing
                .iter()
                .find(|c| Some(c.id.as_str()) == id_in)
                .cloned()
                .unwrap_or_else(|| Clip { id: new_id("c"), picked: true, ..Clip::default() }),
        };
        clip.title = title;
        clip.why = why;
        clip.score = score;
        clip.start_ms = start as u64;
        clip.end_ms = end as u64;
        clip.origin = Origin::Claude;
        clip.srev = sug.rev;
        report.push_str(&format!("{} · {} · {}–{} · {:.0}s\n", clip.id, clip.title, fmt_ts(start as u64), fmt_ts(end as u64), secs));
        match slot {
            Some(i) => sug.clips[i] = clip,
            None => sug.clips.push(clip),
        }
        saved += 1;
    }
    if saved == 0 {
        return Err(format!("Nothing saved.\n{report}"));
    }
    store::save_suggestions(&l.dir, &sug)?;
    Ok(text_result(format!("Saved {saved} clip(s) to \"{}\". They are live in the OpenClip app.\n{report}", l.title)))
}

fn tool_get_clips(args: &Value) -> Result<Value, String> {
    let which = args.get("project").and_then(Value::as_str);
    let dir = store::resolve(which)?;
    let p = store::load_project(&dir)?;
    let sug = store::load_suggestions(&dir);
    let mut seen: Vec<&str> = Vec::new();
    let mut out = String::new();
    for c in p.clips.iter().chain(sug.clips.iter()) {
        if seen.contains(&c.id.as_str()) {
            continue;
        }
        seen.push(&c.id);
        let state = if c.dismissed { "dismissed" } else if c.picked { "picked" } else { "unpicked" };
        out.push_str(&format!("{} · {} · {}–{} · {}\n", c.id, c.title, fmt_ts(c.start_ms), fmt_ts(c.end_ms), state));
    }
    if out.is_empty() {
        out.push_str("No clips yet.");
    }
    Ok(text_result(out))
}

fn call_tool(name: &str, args: &Value) -> Value {
    store::mark_mcp_seen(name);
    let r = match name {
        "list_projects" => tool_list_projects(),
        "get_candidates" => tool_candidates(args),
        "get_transcript" => tool_transcript(args),
        "save_clips" => tool_save(args),
        "get_clips" => tool_get_clips(args),
        _ => Err(format!("unknown tool {name}")),
    };
    r.unwrap_or_else(error_result)
}

/// One JSON-RPC message in, zero or one out.
pub fn handle(msg: &Value) -> Option<Value> {
    let method = msg.get("method").and_then(Value::as_str)?;
    let id = msg.get("id").cloned();
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    // notifications carry no id and get no reply
    let id = match id {
        Some(i) => i,
        None => return None,
    };
    let ok = |result: Value| Some(json!({"jsonrpc":"2.0","id":id,"result":result}));
    match method {
        "initialize" => {
            let want = params.get("protocolVersion").and_then(Value::as_str).unwrap_or("");
            let ver = ["2025-06-18", "2025-03-26", "2024-11-05"].iter().find(|v| **v == want).copied().unwrap_or("2025-06-18");
            ok(json!({
                "protocolVersion": ver,
                "capabilities": {"tools": {}, "prompts": {}},
                "serverInfo": {"name": "openclip", "version": env!("CARGO_PKG_VERSION")},
                "instructions": INSTRUCTIONS
            }))
        }
        "ping" => ok(json!({})),
        "tools/list" => ok(json!({"tools": tools()})),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            ok(call_tool(name, &args))
        }
        "prompts/list" => ok(json!({"prompts": prompts()})),
        "prompts/get" => {
            let a = params.get("arguments").cloned().unwrap_or(json!({}));
            let count = a.get("count").and_then(Value::as_str).unwrap_or("5-10");
            let mut t = format!("Find the {count} best vertical clips in my current OpenClip project: call get_candidates, choose moments that stand completely on their own, then save_clips. Keep it brief.");
            if let Some(f) = a.get("focus").and_then(Value::as_str).filter(|s| !s.is_empty()) {
                t.push_str(&format!(" Favour: {f}."));
            }
            ok(json!({"description":"Find clips","messages":[{"role":"user","content":{"type":"text","text":t}}]}))
        }
        "resources/list" => ok(json!({"resources": []})),
        "resources/templates/list" => ok(json!({"resourceTemplates": []})),
        _ => Some(json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":format!("method not found: {method}")}})),
    }
}

/// Run until stdin closes.
pub fn run_stdio() {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(v) => handle(&v),
            Err(e) => Some(json!({"jsonrpc":"2.0","id":Value::Null,"error":{"code":-32700,"message":format!("parse error: {e}")}})),
        };
        if let Some(r) = reply {
            let _ = writeln!(out, "{}", r);
            let _ = out.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handshake_and_listing() {
        let r = handle(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}})).unwrap();
        assert_eq!(r["result"]["protocolVersion"], "2024-11-05");
        assert!(handle(&json!({"jsonrpc":"2.0","method":"notifications/initialized"})).is_none());
        let r = handle(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})).unwrap();
        assert_eq!(r["result"]["tools"].as_array().unwrap().len(), 5);
    }

    #[test]
    fn end_to_end_with_a_project() {
        let tmp = std::env::temp_dir().join(format!("cc-test-{}", new_id("")));
        std::env::set_var("CLIPCRAFT_HOME", &tmp);
        let dir = tmp.join("t.openclip");
        std::fs::create_dir_all(&dir).unwrap();
        let mut p = crate::project::Project { id: "p1".into(), title: "Test talk".into(), transcribed: true, ..Default::default() };
        p.source.duration_ms = 100_000;
        store::save_project(&dir, &p).unwrap();
        let text = "Why do you keep waiting? You were never meant to live small. Everything changes when you choose. Choose today. \
                    Nothing else will ever be enough for you. That is the truth about life and love and hope for all of us here today.";
        let words = crate::transcript::tests::synth(&text.repeat(6), 0);
        store::save_transcript(&dir, &Transcript { model: "x".into(), words }).unwrap();

        let r = call_tool("get_candidates", &json!({"n": 3, "min_s": 10, "max_s": 40}));
        let t = r["content"][0]["text"].as_str().unwrap();
        assert!(t.contains("[1] s"), "{t}");
        let r = call_tool("save_clips", &json!({"clips":[{"from":0,"to":3,"title":"Never live small","why":"hook"}]}));
        assert!(r.get("isError").is_none(), "{r}");
        let sug = store::load_suggestions(&dir);
        assert_eq!(sug.clips.len(), 1);
        assert_eq!(sug.clips[0].origin, Origin::Claude);
        let r = call_tool("get_clips", &json!({}));
        assert!(r["content"][0]["text"].as_str().unwrap().contains("Never live small"));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
