use dashmap::DashMap;
use std::io::{Read, Write};
use std::sync::Arc;
use teletypewriter::create_pty_with_spawn;
use tokio::sync::mpsc;
use uuid::Uuid;

pub type SessionId = Uuid;

pub struct Session {
    pub pty_writer: std::fs::File,
    pub child_pid: i32,
    pub cols: u16,
    pub rows: u16,
    reader_handle: Option<tokio::task::JoinHandle<()>>,
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Some(handle) = self.reader_handle.take() {
            handle.abort();
        }
        teletypewriter::kill_pid(self.child_pid);
    }
}

#[derive(Clone)]
pub struct SessionManager {
    pub sessions: Arc<DashMap<SessionId, Session>>,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self {
            sessions: Arc::new(DashMap::new()),
        }
    }
}

impl SessionManager {
    pub fn create_session(
        &self,
        cols: u16,
        rows: u16,
    ) -> Result<(SessionId, mpsc::UnboundedReceiver<Vec<u8>>), String> {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());

        let pty = create_pty_with_spawn(
            &shell,
            vec![],
            &None,
            cols,
            rows,
        )
        .map_err(|e| format!("Failed to create PTY: {e}"))?;

        let session_id = Uuid::new_v4();
        let child_pid = *pty.child.pid as i32;

        // Prevent pty drop from sending SIGHUP to the child process.
        // Session::Drop handles cleanup via kill_pid.
        let pty_fd = *pty.child.id;
        std::mem::forget(pty);

        let (write_fd, read_fd) = unsafe {
            let wfd = libc::dup(pty_fd);
            let rfd = libc::dup(pty_fd);
            if wfd < 0 || rfd < 0 {
                return Err("Failed to dup PTY fd".to_string());
            }
            // Set both to blocking mode (PTY may default to non-blocking)
            let flags = libc::fcntl(rfd, libc::F_GETFL);
            libc::fcntl(rfd, libc::F_SETFL, flags & !libc::O_NONBLOCK);
            let flags = libc::fcntl(wfd, libc::F_GETFL);
            libc::fcntl(wfd, libc::F_SETFL, flags & !libc::O_NONBLOCK);
            (wfd, rfd)
        };

        let pty_writer = unsafe {
            use std::os::unix::io::FromRawFd;
            std::fs::File::from_raw_fd(write_fd)
        };

        let (output_tx, output_rx) = mpsc::unbounded_channel();

        // Spawn PTY reader task with pre-dup'd fd
        let tx = output_tx;
        let reader_handle = tokio::task::spawn_blocking(move || {
            let mut reader = unsafe {
                use std::os::unix::io::FromRawFd;
                std::fs::File::from_raw_fd(read_fd)
            };
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        // EIO means PTY closed (child exited)
                        if e.raw_os_error() == Some(libc::EIO) {
                            break;
                        }
                        tracing::error!("PTY read error: {e}");
                        break;
                    }
                }
            }
        });

        let session = Session {
            pty_writer,
            child_pid,
            cols,
            rows,
            reader_handle: Some(reader_handle),
        };

        self.sessions.insert(session_id, session);
        tracing::info!("Created session {session_id} (pid {child_pid})");

        Ok((session_id, output_rx))
    }

    pub fn write_to_session(
        &self,
        session_id: &SessionId,
        data: &[u8],
    ) -> Result<(), String> {
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            session
                .pty_writer
                .write_all(data)
                .map_err(|e| format!("PTY write error: {e}"))
        } else {
            Err(format!("Session {session_id} not found"))
        }
    }

    pub fn resize_session(
        &self,
        session_id: &SessionId,
        cols: u16,
        rows: u16,
    ) -> Result<(), String> {
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            session.cols = cols;
            session.rows = rows;
            // Resize via ioctl
            use std::os::unix::io::AsRawFd;
            let fd = session.pty_writer.as_raw_fd();
            unsafe {
                let ws = libc::winsize {
                    ws_row: rows,
                    ws_col: cols,
                    ws_xpixel: 0,
                    ws_ypixel: 0,
                };
                libc::ioctl(fd, libc::TIOCSWINSZ, &ws);
            }
            Ok(())
        } else {
            Err(format!("Session {session_id} not found"))
        }
    }

    pub fn close_session(&self, session_id: &SessionId) {
        if let Some((_, session)) = self.sessions.remove(session_id) {
            tracing::info!(
                "Closed session {session_id} (pid {})",
                session.child_pid
            );
        }
    }
}
