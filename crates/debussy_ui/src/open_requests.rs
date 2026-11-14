//! [`OpenRequests`]: projects asked for from outside the window, such as by the Finder, carried
//! to it as [`Message::OpenDocument`] (`docs/architecture/startup.md` "Application startup").

use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use iced::Subscription;
use iced::futures::StreamExt;
use iced::futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use iced::futures::stream::{self, BoxStream};

use crate::app::Message;

/// A queue of project paths to open. Paths sent before the window starts wait until it does.
pub struct OpenRequests {
    /// Where [`OpenRequests::sender`]'s senders send.
    sender: UnboundedSender<PathBuf>,
    /// What the window reads.
    receiver: UnboundedReceiver<PathBuf>,
}

impl OpenRequests {
    /// An empty queue.
    #[must_use]
    pub fn new() -> OpenRequests {
        let (sender, receiver) = unbounded();
        OpenRequests { sender, receiver }
    }

    /// A function that asks the window to open the project at a path.
    pub fn sender(&self) -> impl Fn(PathBuf) + 'static {
        let sender = self.sender.clone();
        move |path| {
            // Fails only once the window has closed, when there is nothing left to open into.
            let _ = sender.unbounded_send(path);
        }
    }

    /// The window's end of the queue.
    pub(crate) fn into_inbox(self) -> Inbox {
        Inbox(Arc::new(Mutex::new(Some(self.receiver))))
    }
}

impl Default for OpenRequests {
    fn default() -> OpenRequests {
        OpenRequests::new()
    }
}

/// The window's end of an [`OpenRequests`] queue, taken by the first subscription to read it.
#[derive(Clone)]
pub(crate) struct Inbox(Arc<Mutex<Option<UnboundedReceiver<PathBuf>>>>);

impl Hash for Inbox {
    /// A window has one inbox, so every inbox is the same subscription.
    fn hash<H: Hasher>(&self, state: &mut H) {
        "open requests".hash(state);
    }
}

impl Inbox {
    /// An inbox that never receives anything.
    #[cfg(test)]
    pub(crate) fn empty() -> Inbox {
        Inbox(Arc::new(Mutex::new(None)))
    }

    /// Each path received, as [`Message::OpenDocument`].
    pub(crate) fn subscription(&self) -> Subscription<Message> {
        Subscription::run_with(self.clone(), Inbox::stream)
    }

    /// The paths received; empty if another stream already took them.
    fn stream(&self) -> BoxStream<'static, Message> {
        let receiver = self.0.lock().ok().and_then(|mut r| r.take());
        match receiver {
            Some(receiver) => receiver.map(Message::OpenDocument).boxed(),
            None => stream::empty().boxed(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use iced::futures::executor::block_on;

    #[test]
    fn test_inbox_stream_yields_paths_sent_before_it_starts()
    -> Result<(), Box<dyn std::error::Error>> {
        let requests = OpenRequests::new();
        let send = requests.sender();
        send(PathBuf::from("/a/Song.debussy"));
        let inbox = requests.into_inbox();
        drop(send);
        let messages: Vec<Message> = block_on(inbox.stream().collect());
        assert_eq!(
            messages,
            vec![Message::OpenDocument(PathBuf::from("/a/Song.debussy"))]
        );
        // A second stream gets nothing.
        assert!(block_on(inbox.stream().collect::<Vec<_>>()).is_empty());
        Ok(())
    }
}
