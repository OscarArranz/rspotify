//! Bounded, muted Canvas decoding. FFmpeg is optional; no protected media support.

use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::RenderImage;
use image::{Delay, Frame, RgbaImage};

use crate::lib::spotify::home::trusted_video_url;

const WIDTH: u32 = 256;
const HEIGHT: u32 = 320;
const FRAMES: usize = 48;
const MAX_DOWNLOAD: u64 = 16 * 1024 * 1024;

pub(super) fn decode(url: &str) -> Option<Arc<RenderImage>> {
    if !trusted_video_url(url) {
        return None;
    }
    // Do not allow CDN redirects to arbitrary hosts or local network resources.
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() < 5 && trusted_video_url(attempt.url().as_str()) {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()
        .ok()?;
    let response = client.get(url).send().ok()?.error_for_status().ok()?;
    if response
        .content_length()
        .is_some_and(|size| size > MAX_DOWNLOAD)
    {
        return None;
    }
    let mut bytes = Vec::new();
    response
        .take(MAX_DOWNLOAD + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_DOWNLOAD {
        return None;
    }

    let mut child = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-nostdin",
            "-threads",
            "1",
            "-protocol_whitelist",
            "pipe",
            "-i",
            "pipe:0",
            "-an",
            "-sn",
            "-vf",
            "fps=8,scale=256:320:force_original_aspect_ratio=increase,crop=256:320",
            "-frames:v",
            "48",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "bgra",
            "pipe:1",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdin = child.stdin.take()?;
    let stdout = child.stdout.take()?;
    let input = std::thread::spawn(move || {
        let _ = stdin.write_all(&bytes);
    });
    let frame_size = (WIDTH * HEIGHT * 4) as usize;
    let output = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .take((frame_size * FRAMES + 1) as u64)
            .read_to_end(&mut bytes)
            .ok()?;
        Some(bytes)
    });
    let deadline = Instant::now() + Duration::from_secs(20);
    let success = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(25)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };
    let _ = input.join();
    let bytes = output.join().ok()??;
    if !success || bytes.len() > frame_size * FRAMES || bytes.len() % frame_size != 0 {
        return None;
    }
    let frames: Vec<_> = bytes
        .chunks_exact(frame_size)
        .filter_map(|data| {
            Some(Frame::from_parts(
                RgbaImage::from_raw(WIDTH, HEIGHT, data.to_vec())?,
                0,
                0,
                Delay::from_numer_denom_ms(125, 1),
            ))
        })
        .collect();
    if frames.is_empty() {
        return None;
    }
    Some(Arc::new(RenderImage::new(frames)))
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "requires saved session, read-only network access and optional FFmpeg; no audio or desktop interaction"]
    fn smoke_native_home_canvas_decode() {
        let client = crate::lib::spotify::NativeSpotify::restore().expect("saved session");
        let home = client.home("").expect("Home");
        let url = home
            .panels
            .iter()
            .find_map(|p| p.video_url.as_deref())
            .expect("Canvas source");
        let frames = super::decode(url).expect("bounded Canvas decoding with FFmpeg");
        assert!(frames.frame_count() > 1 && frames.frame_count() <= 48);
        eprintln!(
            "Decoded {} muted Canvas frames; no playback started",
            frames.frame_count()
        );
    }

    #[test]
    fn rejects_non_cdn_inputs_before_decoding() {
        assert!(super::decode("file:///etc/passwd").is_none());
        assert!(super::decode("https://example.com/video.mp4").is_none());
    }
}
