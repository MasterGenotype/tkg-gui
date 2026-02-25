use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{ChildStdin, Command, Stdio};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;

pub enum WineBuildMsg {
    Line(String),
    Exit(i32),
    SpawnError(String),
}

/// Handle for sending interactive input to the wine build process.
pub struct WineBuildHandle {
    stdin: Arc<Mutex<Option<ChildStdin>>>,
}

impl WineBuildHandle {
    pub fn send_input(&self, input: &str) -> Result<(), String> {
        if let Ok(mut guard) = self.stdin.lock() {
            if let Some(stdin) = guard.as_mut() {
                writeln!(stdin, "{}", input).map_err(|e| e.to_string())?;
                stdin.flush().map_err(|e| e.to_string())?;
                return Ok(());
            }
        }
        Err("Process stdin not available".to_string())
    }
}

/// Run `makepkg -si` in `<wine_tkg_path>/wine-tkg-git/` and stream output
/// line-by-line via `tx`. Returns a handle for interactive stdin input.
pub fn start_build(wine_tkg_path: PathBuf, tx: Sender<WineBuildMsg>) -> WineBuildHandle {
    let stdin_handle: Arc<Mutex<Option<ChildStdin>>> = Arc::new(Mutex::new(None));
    let stdin_clone = stdin_handle.clone();

    thread::spawn(move || {
        // The PKGBUILD lives in the inner wine-tkg-git/ subdirectory
        let work_dir = wine_tkg_path.join("wine-tkg-git");

        let result = Command::new("makepkg")
            .arg("-si")
            .current_dir(&work_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();

        match result {
            Ok(mut child) => {
                if let Some(stdin) = child.stdin.take() {
                    if let Ok(mut guard) = stdin_clone.lock() {
                        *guard = Some(stdin);
                    }
                }

                let stdout = child.stdout.take();
                let stderr = child.stderr.take();

                let tx_out = tx.clone();
                let stdout_handle = stdout.map(|out| {
                    thread::spawn(move || {
                        for line in BufReader::new(out).lines().map_while(Result::ok) {
                            let _ = tx_out.send(WineBuildMsg::Line(line));
                        }
                    })
                });

                let tx_err = tx.clone();
                let stderr_handle = stderr.map(|err| {
                    thread::spawn(move || {
                        for line in BufReader::new(err).lines().map_while(Result::ok) {
                            let _ = tx_err.send(WineBuildMsg::Line(line));
                        }
                    })
                });

                if let Some(h) = stdout_handle {
                    let _ = h.join();
                }
                if let Some(h) = stderr_handle {
                    let _ = h.join();
                }

                if let Ok(mut guard) = stdin_clone.lock() {
                    *guard = None;
                }

                match child.wait() {
                    Ok(status) => {
                        let _ = tx.send(WineBuildMsg::Exit(status.code().unwrap_or(-1)));
                    }
                    Err(e) => {
                        let _ = tx.send(WineBuildMsg::SpawnError(e.to_string()));
                    }
                }
            }
            Err(e) => {
                let _ = tx.send(WineBuildMsg::SpawnError(format!(
                    "Failed to spawn makepkg: {}",
                    e
                )));
            }
        }
    });

    WineBuildHandle { stdin: stdin_handle }
}
