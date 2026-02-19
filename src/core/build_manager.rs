use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{ChildStdin, Command, Stdio};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;

pub enum BuildMsg {
    Line(String),
    Exit(i32),
    SpawnError(String),
}

/// Handle for sending input to the build process
pub struct BuildHandle {
    stdin: Arc<Mutex<Option<ChildStdin>>>,
}

impl BuildHandle {
    /// Send input to the build process (adds newline automatically)
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

pub fn start_build(work_dir: PathBuf, tx: Sender<BuildMsg>, use_makepkg: bool) -> BuildHandle {
    let stdin_handle: Arc<Mutex<Option<ChildStdin>>> = Arc::new(Mutex::new(None));
    let stdin_clone = stdin_handle.clone();

    thread::spawn(move || {
        // Use makepkg for Arch-based distros, install.sh for others
        let result = if use_makepkg {
            Command::new("makepkg")
                .arg("-si")
                .current_dir(&work_dir)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
        } else {
            Command::new("./install.sh")
                .arg("install")
                .current_dir(&work_dir)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
        };

        match result {
            Ok(mut child) => {
                // Store stdin handle for interactive input
                if let Some(stdin) = child.stdin.take() {
                    if let Ok(mut guard) = stdin_clone.lock() {
                        *guard = Some(stdin);
                    }
                }

                let stdout = child.stdout.take();
                let stderr = child.stderr.take();

                // Spawn thread for stdout
                let tx_stdout = tx.clone();
                let stdout_handle = stdout.map(|out| {
                    thread::spawn(move || {
                        let reader = BufReader::new(out);
                        for line in reader.lines().map_while(Result::ok) {
                            let _ = tx_stdout.send(BuildMsg::Line(line));
                        }
                    })
                });

                // Spawn thread for stderr
                let tx_stderr = tx.clone();
                let stderr_handle = stderr.map(|err| {
                    thread::spawn(move || {
                        let reader = BufReader::new(err);
                        for line in reader.lines().map_while(Result::ok) {
                            let _ = tx_stderr.send(BuildMsg::Line(line));
                        }
                    })
                });

                // Wait for output threads
                if let Some(h) = stdout_handle {
                    let _ = h.join();
                }
                if let Some(h) = stderr_handle {
                    let _ = h.join();
                }

                // Clear stdin handle
                if let Ok(mut guard) = stdin_clone.lock() {
                    *guard = None;
                }

                // Wait for process to exit
                match child.wait() {
                    Ok(status) => {
                        let code = status.code().unwrap_or(-1);
                        let _ = tx.send(BuildMsg::Exit(code));
                    }
                    Err(e) => {
                        let _ = tx.send(BuildMsg::SpawnError(e.to_string()));
                    }
                }
            }
            Err(e) => {
                let _ = tx.send(BuildMsg::SpawnError(e.to_string()));
            }
        }
    });

    BuildHandle { stdin: stdin_handle }
}
