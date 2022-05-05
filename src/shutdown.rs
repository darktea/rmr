use tokio::sync::broadcast;

#[derive(Debug)]
pub struct Shutdown {
    shutdown: bool,
    notify: broadcast::Receiver<()>,
}

impl Shutdown {
    pub fn new(notify: broadcast::Receiver<()>) -> Shutdown {
        Shutdown {
            shutdown: false,
            notify,
        }
    }

    pub fn is_shutdown(&self) -> bool {
        self.shutdown
    }

    pub async fn recv(&mut self) {
        if self.shutdown {
            return;
        }

        // 当 server 要 shutdown 时，这里会接收到一个 Err(RecvError::Closed) 消息
        let _ = self.notify.recv().await;

        self.shutdown = true;
    }
}
