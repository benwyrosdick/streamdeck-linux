//! Soundboard playback. The audio sink lives on its own thread because the
//! output stream is not `Send`; actions talk to it through a channel.

use anyhow::{anyhow, Result};
use rodio::Source;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::OnceLock;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct PlayRequest {
    /// Playback id; the soundboard uses the action context so a second press can stop it.
    pub id: String,
    pub path: PathBuf,
    /// 0.0–1.0
    pub volume: f32,
    pub fade_in: Duration,
    pub fade_out: Duration,
    pub looping: bool,
    /// Stop an earlier playback with the same id before starting.
    pub restart: bool,
}

enum Cmd {
    Play(PlayRequest, mpsc::Sender<Result<()>>),
    Stop(String, Duration),
    StopAll,
    IsPlaying(String, mpsc::Sender<bool>),
}

fn worker() -> &'static mpsc::Sender<Cmd> {
    static TX: OnceLock<mpsc::Sender<Cmd>> = OnceLock::new();
    TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Cmd>();
        std::thread::Builder::new()
            .name("audio".into())
            .spawn(move || {
                let mut sink: Option<rodio::MixerDeviceSink> = None;
                let mut players: HashMap<String, Vec<rodio::Player>> = HashMap::new();
                for cmd in rx {
                    players.retain(|_, v| {
                        v.retain(|p| !p.empty());
                        !v.is_empty()
                    });
                    match cmd {
                        Cmd::Play(req, done) => {
                            if sink.is_none() {
                                match rodio::DeviceSinkBuilder::open_default_sink() {
                                    Ok(mut s) => {
                                        s.log_on_drop(false);
                                        sink = Some(s);
                                    }
                                    Err(e) => {
                                        done.send(Err(anyhow!("no audio output: {e}"))).ok();
                                        continue;
                                    }
                                }
                            }
                            let mixer = sink.as_ref().unwrap().mixer();
                            if req.restart {
                                if let Some(list) = players.remove(&req.id) {
                                    fade_and_stop(list, req.fade_out);
                                }
                            }
                            let res = (|| -> Result<rodio::Player> {
                                let file = std::fs::File::open(&req.path).map_err(|e| anyhow!("{}: {e}", req.path.display()))?;
                                let decoder = rodio::Decoder::try_from(file).map_err(|e| anyhow!("cannot decode {}: {e}", req.path.display()))?;
                                let player = rodio::Player::connect_new(mixer);
                                player.set_volume(req.volume);
                                if req.looping {
                                    let src = decoder.repeat_infinite();
                                    if req.fade_in > Duration::ZERO {
                                        player.append(src.fade_in(req.fade_in));
                                    } else {
                                        player.append(src);
                                    }
                                } else if req.fade_in > Duration::ZERO {
                                    player.append(decoder.fade_in(req.fade_in));
                                } else {
                                    player.append(decoder);
                                }
                                Ok(player)
                            })();
                            match res {
                                Ok(p) => {
                                    players.entry(req.id.clone()).or_default().push(p);
                                    done.send(Ok(())).ok();
                                }
                                Err(e) => {
                                    done.send(Err(e)).ok();
                                }
                            }
                        }
                        Cmd::Stop(id, fade) => {
                            if let Some(list) = players.remove(&id) {
                                fade_and_stop(list, fade);
                            }
                        }
                        Cmd::StopAll => {
                            for (_, list) in players.drain() {
                                for p in list {
                                    p.stop();
                                }
                            }
                        }
                        Cmd::IsPlaying(id, reply) => {
                            reply.send(players.get(&id).map(|v| v.iter().any(|p| !p.empty())).unwrap_or(false)).ok();
                        }
                    }
                }
            })
            .expect("audio thread");
        tx
    })
}

fn fade_and_stop(list: Vec<rodio::Player>, fade: Duration) {
    if fade.is_zero() {
        for p in list {
            p.stop();
        }
        return;
    }
    // Fade on a helper thread so the audio thread keeps serving presses.
    std::thread::spawn(move || {
        let steps = 20;
        let start: Vec<f32> = list.iter().map(|p| p.volume() as f32).collect();
        for i in (0..steps).rev() {
            for (p, v) in list.iter().zip(&start) {
                p.set_volume(*v * i as f32 / steps as f32);
            }
            std::thread::sleep(fade / steps);
        }
        for p in list {
            p.stop();
        }
    });
}

pub async fn play(req: PlayRequest) -> Result<()> {
    let (tx, rx) = mpsc::channel();
    worker().send(Cmd::Play(req, tx)).map_err(|_| anyhow!("audio thread stopped"))?;
    tokio::task::spawn_blocking(move || rx.recv_timeout(Duration::from_secs(5)).unwrap_or(Ok(()))).await?
}

pub fn stop(id: &str, fade: Duration) {
    worker().send(Cmd::Stop(id.to_string(), fade)).ok();
}

pub fn stop_all() {
    worker().send(Cmd::StopAll).ok();
}

pub async fn is_playing(id: &str) -> bool {
    let (tx, rx) = mpsc::channel();
    if worker().send(Cmd::IsPlaying(id.to_string(), tx)).is_err() {
        return false;
    }
    tokio::task::spawn_blocking(move || rx.recv_timeout(Duration::from_secs(1)).unwrap_or(false)).await.unwrap_or(false)
}

const AUDIO_EXT: &[&str] = &["mp3", "wav", "ogg", "oga", "flac", "m4a", "aac", "opus", "mp4"];

/// A soundboard path may be a folder, in which case a random file plays.
pub fn pick_file(path: &Path) -> Option<PathBuf> {
    if path.is_file() {
        return Some(path.to_path_buf());
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(path)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()).map(|e| AUDIO_EXT.contains(&e.to_lowercase().as_str())).unwrap_or(false))
        .collect();
    if files.is_empty() {
        return None;
    }
    use rand::Rng;
    let i = rand::rng().random_range(0..files.len());
    Some(files.swap_remove(i))
}
