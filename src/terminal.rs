use anyhow::Result;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;

pub struct PtySession {
    master: Box<dyn MasterPty + Send>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    output_rx: mpsc::Receiver<Vec<u8>>,
    pid: Option<u32>,
    child: Arc<Mutex<Box<dyn portable_pty::Child + Send + Sync>>>,
    _reader_handle: thread::JoinHandle<()>,
}

impl PtySession {
    pub fn spawn(shell: &str, cwd: &str, rows: u16, cols: u16) -> Result<Self> {
        let pty_system = native_pty_system();

        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let mut cmd = CommandBuilder::new(shell);
        cmd.cwd(cwd);

        let child = pair.slave.spawn_command(cmd)?;
        let pid = child.process_id();

        drop(pair.slave);

        let writer = pair.master.take_writer()?;
        let mut reader = pair.master.try_clone_reader()?;

        let (tx, rx) = mpsc::channel();
        let reader_handle = thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(Self {
            master: pair.master,
            writer: Arc::new(Mutex::new(writer)),
            output_rx: rx,
            pid,
            child: Arc::new(Mutex::new(child)),
            _reader_handle: reader_handle,
        })
    }

    pub fn write(&self, data: &[u8]) -> Result<()> {
        let mut writer = self.writer.lock().unwrap();
        writer.write_all(data)?;
        writer.flush()?;
        Ok(())
    }

    pub fn read_available(&self) -> Vec<Vec<u8>> {
        let mut chunks = Vec::new();
        while let Ok(chunk) = self.output_rx.try_recv() {
            chunks.push(chunk);
        }
        chunks
    }

    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    /// Read the current working directory of the child process via /proc/PID/cwd
    pub fn cwd(&self) -> Option<String> {
        let pid = self.pid?;
        let link = format!("/proc/{}/cwd", pid);
        std::fs::read_link(&link).ok().map(|p| p.display().to_string())
    }

    pub fn resize(&self, rows: u16, cols: u16) -> Result<()> {
        self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        Ok(())
    }

    pub fn try_wait(&self) -> Option<portable_pty::ExitStatus> {
        self.child.lock().unwrap().try_wait().ok().flatten()
    }

    pub fn kill(&self) {
        let _ = self.child.lock().unwrap().kill();
    }
}
