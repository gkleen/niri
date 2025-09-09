use anyhow::anyhow;
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::sync::{Mutex, mpsc::{channel, Sender, Receiver}};
use std::path::{Path, PathBuf};
use std::thread;
use std::os::unix::net::UnixStream;
use std::io::Write;

lazy_static! {
    static ref UNIX_SOCKETS: Mutex<HashMap<PathBuf, Sender<Vec<u8>>>> = Mutex::new(HashMap::new());
}

pub fn send_unix(path: &str, message: Vec<u8>) -> anyhow::Result<()> {
    let path: PathBuf = subst::substitute(path, &subst::Env)?.into();
    let mut sockets = UNIX_SOCKETS.lock().map_err(|e| anyhow!("{:?}", e))?;

    if let Some(chan) = (*sockets).get(&path) {
        chan.send(message)?;
    } else {
        let (sender, receiver) = channel();
        let path_ref = path.clone();
        let _ = (*sockets).insert(path, sender.clone());
        let _ = thread::Builder::new()
            .name("Unix socket service".to_owned())
            .spawn(move || {
                if let Err(err) = service_unix_socket(&path_ref, receiver) {
                    warn!("error servicing unix socket {:?}: {:?}", path_ref, err);
                }
                if let Err(err) = (|| -> anyhow::Result<()> {
                    let mut sockets = UNIX_SOCKETS.lock().map_err(|e| anyhow!("{:?}", e))?;
                    (*sockets).remove(&path_ref);
                    Ok(())
                })() {
                    warn!("error removing dead unix socket server for {:?}: {:?}", path_ref, err);
                }
            })?;
        sender.send(message)?;
    }
    Ok(())
}

fn service_unix_socket(path: &Path, receiver: Receiver<Vec<u8>>) -> anyhow::Result<()> {
    let mut stream = UnixStream::connect(path)?;
    loop {
        let msg = receiver.recv()?;
        stream.write_all(&msg)?;
        stream.flush()?;
    }
}
