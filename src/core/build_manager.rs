use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::thread;

pub enum BuildMsg {
    Line(String),
    Exit(i32),
    SpawnError(String),
}

pub fn start_build(work_dir: PathBuf, tx: Sender<BuildMsg>) {
    thread::spawn(move || {
        let result = Command::new("makepkg")
            .arg("-si")
            .current_dir(&work_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();

        match result {
            Ok(mut child) => {
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
}
