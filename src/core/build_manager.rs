use std::io::{BufRead, BufReader, Write};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::PathBuf;
use std::process::{ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Signal every process in group `pgid`. Returns whether the call succeeded.
///
/// A negative pid means "the process group" to `kill(2)`; `killpg` says it
/// directly. Used rather than signalling the child alone because the child is a
/// shell script whose descendants are doing all the work.
fn killpg(pgid: i32, sig: libc::c_int) -> bool {
    // SAFETY: a plain syscall with an integer pgid and signal number; it cannot
    // violate any Rust invariant, and failure is reported via the return value.
    unsafe { libc::killpg(pgid, sig) == 0 }
}

/// Grace period between SIGTERM and SIGKILL when stopping a build.
///
/// `makepkg`/`install.sh` are shell scripts; SIGTERM gives them a chance to run
/// their own traps and tidy up before the group is killed outright.
const TERM_GRACE: Duration = Duration::from_secs(5);

pub enum BuildMsg {
    Line(String),
    Exit(i32),
    /// Killed by a signal — normally SIGTERM/SIGKILL from the Stop button.
    Signalled(i32),
    SpawnError(String),
}

/// Handle for talking to, and stopping, the build process.
pub struct BuildHandle {
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    /// Process-group id of the build, which is also the direct child's pid.
    ///
    /// The group is what matters: `makepkg` spawns `make -jN`, which spawns a
    /// compiler per job. Signalling only the direct child would orphan all of
    /// them and leave the machine compiling after the user pressed Stop.
    pgid: Arc<Mutex<Option<i32>>>,
    /// Set once the child has been reaped, so the delayed SIGKILL cannot land on
    /// a recycled process group.
    exited: Arc<AtomicBool>,
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

    /// Stop the build: SIGTERM the whole process group, then SIGKILL whatever is
    /// left after [`TERM_GRACE`].
    ///
    /// Returns as soon as the SIGTERM is delivered — the escalation happens on a
    /// detached thread so the UI never blocks. The exit is reported through the
    /// normal [`BuildMsg::Exit`] path when the child is reaped.
    ///
    /// Caveat worth surfacing to the user: anything the build started under
    /// `sudo` runs as root, and an unprivileged tkg-gui cannot signal it. If the
    /// build has reached `makepkg`'s install step, the kill may be partial.
    pub fn terminate(&self) -> Result<(), String> {
        let pgid = self
            .pgid
            .lock()
            .ok()
            .and_then(|g| *g)
            .ok_or_else(|| "build process is not running".to_string())?;

        if !killpg(pgid, libc::SIGTERM) {
            return Err(format!(
                "could not signal build process group {pgid}: {}",
                std::io::Error::last_os_error()
            ));
        }

        let exited = self.exited.clone();
        thread::spawn(move || {
            thread::sleep(TERM_GRACE);
            // Skip the escalation if the child was already reaped, so SIGKILL can
            // never land on a process group that has since been recycled.
            if !exited.load(Ordering::SeqCst) {
                killpg(pgid, libc::SIGKILL);
            }
        });
        Ok(())
    }
}

pub fn start_build(work_dir: PathBuf, tx: Sender<BuildMsg>, use_makepkg: bool) -> BuildHandle {
    let stdin_handle: Arc<Mutex<Option<ChildStdin>>> = Arc::new(Mutex::new(None));
    let stdin_clone = stdin_handle.clone();
    let pgid_handle: Arc<Mutex<Option<i32>>> = Arc::new(Mutex::new(None));
    let pgid_clone = pgid_handle.clone();
    let exited = Arc::new(AtomicBool::new(false));
    let exited_clone = exited.clone();

    thread::spawn(move || {
        // Use makepkg for Arch-based distros, install.sh for others
        // `process_group(0)` makes the child its own group leader, so every
        // descendant (make, and one compiler per job) shares its pgid and a
        // single killpg reaches all of them.
        let result = if use_makepkg {
            Command::new("makepkg")
                .arg("-si")
                .current_dir(&work_dir)
                .process_group(0)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
        } else {
            Command::new("./install.sh")
                .arg("install")
                .current_dir(&work_dir)
                .process_group(0)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
        };

        match result {
            Ok(mut child) => {
                // Publish the pgid (== child pid, since it leads its own group)
                // so Stop can signal the whole tree.
                if let Ok(mut guard) = pgid_clone.lock() {
                    *guard = Some(child.id() as i32);
                }

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
                let outcome = child.wait();
                exited_clone.store(true, Ordering::SeqCst);
                if let Ok(mut guard) = pgid_clone.lock() {
                    *guard = None;
                }
                match outcome {
                    Ok(status) => {
                        // A signalled process has no exit code. Report the signal
                        // instead of a bare -1, so a Stop reads as a stop rather
                        // than a mysterious failure.
                        match (status.code(), status.signal()) {
                            (Some(code), _) => {
                                let _ = tx.send(BuildMsg::Exit(code));
                            }
                            (None, Some(sig)) => {
                                let _ = tx.send(BuildMsg::Signalled(sig));
                            }
                            (None, None) => {
                                let _ = tx.send(BuildMsg::Exit(-1));
                            }
                        }
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

    BuildHandle {
        stdin: stdin_handle,
        pgid: pgid_handle,
        exited,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pids whose process group is `pgid`, read from `/proc/<pid>/stat`.
    ///
    /// Field 5 of that file is the process group. Counting group members is how
    /// the claim "Stop reaches every descendant" is actually verified — killing
    /// the direct child alone would leave these behind, which was the bug.
    fn group_members(pgid: i32) -> Vec<i32> {
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir("/proc") else {
            return out;
        };
        for e in entries.filter_map(|e| e.ok()) {
            let name = e.file_name().to_string_lossy().to_string();
            let Ok(pid) = name.parse::<i32>() else {
                continue;
            };
            let Ok(stat) = std::fs::read_to_string(e.path().join("stat")) else {
                continue;
            };
            // comm can contain spaces and parens, so parse after the last ')'.
            let Some(rest) = stat.rsplit_once(')').map(|(_, r)| r) else {
                continue;
            };
            // rest = " S ppid pgrp ..." -> pgrp is the 3rd whitespace field.
            if rest
                .split_whitespace()
                .nth(2)
                .and_then(|f| f.parse::<i32>().ok())
                == Some(pgid)
            {
                out.push(pid);
            }
        }
        out
    }

    fn wait_until<F: Fn() -> bool>(cond: F, limit: Duration) -> bool {
        let start = std::time::Instant::now();
        while start.elapsed() < limit {
            if cond() {
                return true;
            }
            thread::sleep(Duration::from_millis(50));
        }
        cond()
    }

    /// The regression this change exists for: a build spawns a tree of processes
    /// (make, then a compiler per job), and stopping it has to take all of them.
    #[test]
    fn killpg_takes_down_the_whole_process_tree() {
        // A shell with two children, standing in for make + compilers.
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("sleep 30 & sleep 30 & wait")
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("could not spawn test process");
        let pgid = child.id() as i32;

        assert!(
            wait_until(|| group_members(pgid).len() >= 3, Duration::from_secs(5)),
            "expected sh + 2 sleeps in group {pgid}, saw {:?}",
            group_members(pgid)
        );

        assert!(killpg(pgid, libc::SIGKILL), "killpg failed");
        let _ = child.wait(); // reap, so the group really is empty

        assert!(
            wait_until(|| group_members(pgid).is_empty(), Duration::from_secs(5)),
            "group {pgid} still has members after killpg: {:?}",
            group_members(pgid)
        );
    }

    /// Killing only the direct child is what the old Stop effectively did; this
    /// pins down that it is *not* sufficient, so the group behaviour above cannot
    /// be quietly regressed back to a plain child.kill().
    #[test]
    fn killing_only_the_child_leaves_descendants_running() {
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("sleep 30 & sleep 30 & wait")
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("could not spawn test process");
        let pgid = child.id() as i32;
        assert!(wait_until(
            || group_members(pgid).len() >= 3,
            Duration::from_secs(5)
        ));

        child.kill().expect("kill failed");
        let _ = child.wait();

        // The sleeps are orphaned but alive — exactly the old behaviour.
        assert!(
            wait_until(|| !group_members(pgid).is_empty(), Duration::from_secs(2)),
            "expected orphaned descendants to survive a bare child.kill()"
        );

        killpg(pgid, libc::SIGKILL); // clean up after ourselves
        assert!(wait_until(
            || group_members(pgid).is_empty(),
            Duration::from_secs(5)
        ));
    }

    #[test]
    fn terminate_reports_when_nothing_is_running() {
        let h = BuildHandle {
            stdin: Arc::new(Mutex::new(None)),
            pgid: Arc::new(Mutex::new(None)),
            exited: Arc::new(AtomicBool::new(false)),
        };
        assert!(h.terminate().unwrap_err().contains("not running"));
    }
}
