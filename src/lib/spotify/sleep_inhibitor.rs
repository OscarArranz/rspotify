//! Prevent system suspend while local playback is active, without keeping the display awake.

use std::process::{Child, Command, Stdio};

#[derive(Default)]
pub(super) struct SleepInhibitor {
    child: Option<Child>,
}

impl SleepInhibitor {
    pub(super) fn set_playing(&mut self, playing: bool) {
        #[cfg(target_os = "linux")]
        {
            if playing && self.child.is_none() {
                self.child = Command::new("systemd-inhibit")
                    .args([
                        "--what=sleep",
                        "--who=rspotify",
                        "--why=Audio playback",
                        "--mode=block",
                        "sleep",
                        "infinity",
                    ])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .ok();
            } else if !playing {
                self.release();
            }
        }
        #[cfg(not(target_os = "linux"))]
        let _ = playing;
    }

    fn release(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for SleepInhibitor {
    fn drop(&mut self) {
        self.release();
    }
}
