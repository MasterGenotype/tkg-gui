use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::thread;
use std::path::Path;

pub enum CloneMsg {
    Line(String),
    Exit(i32),
    SpawnError(String),
}

/// Clone https://github.com/Frogging-Family/linux-tkg into `dest`.
/// Runs in a spawned thread and streams output via `tx`.
pub fn clone_linux_tkg(dest: PathBuf, tx: Sender<CloneMsg>) {
    thread::spawn(move || {
        // Ensure the parent directory exists
        if let Some(parent) = dest.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                let _ = tx.send(CloneMsg::SpawnError(format!(
                    "Failed to create directory {}: {}",
                    parent.display(),
                    e
                )));
                return;
            }
        }

        let result = Command::new("git")
            .args([
                "clone",
                "--depth=1",
                "https://github.com/Frogging-Family/linux-tkg",
            ])
            .arg(&dest)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();

        match result {
            Ok(mut child) => {
                let stdout = child.stdout.take();
                let stderr = child.stderr.take();

                let tx_out = tx.clone();
                let out_handle = stdout.map(|out| {
                    thread::spawn(move || {
                        for line in BufReader::new(out).lines().map_while(Result::ok) {
                            let _ = tx_out.send(CloneMsg::Line(line));
                        }
                    })
                });

                let tx_err = tx.clone();
                let err_handle = stderr.map(|err| {
                    thread::spawn(move || {
                        for line in BufReader::new(err).lines().map_while(Result::ok) {
                            let _ = tx_err.send(CloneMsg::Line(line));
                        }
                    })
                });

                if let Some(h) = out_handle {
                    let _ = h.join();
                }
                if let Some(h) = err_handle {
                    let _ = h.join();
                }

                match child.wait() {
                    Ok(status) => {
                        let _ = tx.send(CloneMsg::Exit(status.code().unwrap_or(-1)));
                    }
                    Err(e) => {
                        let _ = tx.send(CloneMsg::SpawnError(e.to_string()));
                    }
                }
            }
            Err(e) => {
                let _ = tx.send(CloneMsg::SpawnError(format!(
                    "Failed to spawn git: {}",
                    e
                )));
            }
        }
    });
}

/// Copy an existing linux-tkg directory into `dest`.
/// Runs in a spawned thread and streams output via `tx`.
pub fn copy_linux_tkg(source: &Path, dest: &Path, tx: Sender<CloneMsg>) {
    let source = source.to_path_buf();
    let dest = dest.to_path_buf();

    thread::spawn(move || {
        let _ = tx.send(CloneMsg::Line(format!(
            "Copying {} → {}",
            source.display(),
            dest.display()
        )));

        if let Some(parent) = dest.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                let _ = tx.send(CloneMsg::SpawnError(format!(
                    "Failed to create directory {}: {}",
                    parent.display(),
                    e
                )));
                return;
            }
        }

        let result = Command::new("cp")
            .arg("-a")
            .arg("--")
            .arg(&source)
            .arg(&dest)
            .output();

        match result {
            Ok(output) => {
                if !output.stderr.is_empty() {
                    if let Ok(stderr) = String::from_utf8(output.stderr) {
                        for line in stderr.lines() {
                            let _ = tx.send(CloneMsg::Line(line.to_string()));
                        }
                    }
                }
                let code = output.status.code().unwrap_or(-1);
                let _ = tx.send(CloneMsg::Exit(code));
            }
            Err(e) => {
                let _ = tx.send(CloneMsg::SpawnError(format!("Failed to run cp: {}", e)));
            }
        }
    });
}
