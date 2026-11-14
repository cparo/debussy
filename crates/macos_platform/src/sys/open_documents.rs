//! The Finder's "open documents" Apple Event (`docs/architecture/platform.md` "Documents opened
//! from the Finder", ADR-0132).
//!
//! winit owns the application delegate, so `application:openURLs:` cannot be implemented.
//! Instead, an observer of `NSApplicationWillFinishLaunchingNotification` installs a handler
//! for `kCoreEventClass`/`kAEOpenDocuments` with `NSAppleEventManager`. AppKit posts that
//! notification after installing its own handlers and before it sends the launch's `odoc`
//! event, so the handler replaces AppKit's in time for the first event.

use std::path::PathBuf;
use std::ptr::NonNull;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_foundation::{
    NSAppleEventDescriptor, NSAppleEventManager, NSNotification, NSNotificationCenter, NSObject,
    NSString,
};

/// A four-character code, as Apple Events name classes, events and keywords.
const fn four_cc(code: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*code)
}

/// `kCoreEventClass`.
const CORE_EVENT_CLASS: u32 = four_cc(b"aevt");
/// `kAEOpenDocuments`.
const OPEN_DOCUMENTS: u32 = four_cc(b"odoc");
/// `keyDirectObject`: the event's list of files.
const DIRECT_OBJECT: u32 = four_cc(b"----");
/// The value of AppKit's `NSApplicationWillFinishLaunchingNotification`, named by its string so
/// that this crate need not depend on AppKit's bindings; AppKit's notification names are their
/// own values.
const WILL_FINISH_LAUNCHING: &str = "NSApplicationWillFinishLaunchingNotification";

/// What the handler calls with each opened path.
type Callback = Box<dyn Fn(PathBuf)>;

define_class!(
    // SAFETY:
    // - NSObject has no subclassing requirements.
    // - `Handler` does not implement `Drop`.
    // - It is main-thread only: Apple Events are dispatched on the main thread.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "DebussyOpenDocumentsHandler"]
    #[ivars = Callback]
    struct Handler;

    impl Handler {
        // SAFETY: the selector's signature is the one `NSAppleEventManager` calls an event
        // handler with: `- (void)handleEvent:(NSAppleEventDescriptor *)event
        // withReplyEvent:(NSAppleEventDescriptor *)reply`.
        #[unsafe(method(handleOpenDocuments:withReplyEvent:))]
        fn handle_open_documents(
            &self,
            event: &NSAppleEventDescriptor,
            _reply: &NSAppleEventDescriptor,
        ) {
            for path in paths(event) {
                (self.ivars())(path);
            }
        }
    }
);

impl Handler {
    /// A handler calling `callback`.
    fn new(mtm: MainThreadMarker, callback: Callback) -> Retained<Handler> {
        let this = Handler::alloc(mtm).set_ivars(callback);
        // SAFETY: `init` is NSObject's designated initialiser, called once on a fresh object.
        unsafe { msg_send![super(this), init] }
    }
}

/// The file paths an `odoc` event names; those that are not file URLs are skipped.
fn paths(event: &NSAppleEventDescriptor) -> Vec<PathBuf> {
    // SAFETY: `paramDescriptorForKeyword:` takes an `AEKeyword` (a `u32` four-character code)
    // and returns a possibly nil descriptor, autoreleased; `Retained` retains it.
    let list: Option<Retained<NSAppleEventDescriptor>> =
        unsafe { msg_send![event, paramDescriptorForKeyword: DIRECT_OBJECT] };
    let Some(list) = list else {
        return Vec::new();
    };
    let count = list.numberOfItems();
    if count <= 0 {
        // A single file rather than a list.
        return list
            .fileURLValue()
            .and_then(|u| u.to_file_path())
            .into_iter()
            .collect();
    }
    // A descriptor list is indexed from 1.
    (1..=count)
        .filter_map(|i| list.descriptorAtIndex(i))
        .filter_map(|item| item.fileURLValue())
        .filter_map(|url| url.to_file_path())
        .collect()
}

/// Calls `callback` with each path the Finder asks the app to open, from launch onwards.
/// Must be called on the main thread, shown by `mtm`, before the application finishes
/// launching; the handler and the observer live until the process exits.
pub(crate) fn install(mtm: MainThreadMarker, callback: Callback) {
    // Leaked: `NSAppleEventManager` does not retain its handlers, and this one is needed for
    // the life of the process.
    let handler: &'static Handler = Box::leak(Box::new(Handler::new(mtm, callback)));
    let register = RcBlock::new(move |_: NonNull<NSNotification>| {
        let manager = NSAppleEventManager::sharedAppleEventManager();
        // SAFETY: `handler` lives for the process and answers the selector with the signature
        // the manager expects; the class and event ids are `u32` four-character codes, as
        // `AEEventClass` and `AEEventID` are.
        unsafe {
            let () = msg_send![
                &manager,
                setEventHandler: handler,
                andSelector: sel!(handleOpenDocuments:withReplyEvent:),
                forEventClass: CORE_EVENT_CLASS,
                andEventID: OPEN_DOCUMENTS
            ];
        }
    });
    let name = NSString::from_str(WILL_FINISH_LAUNCHING);
    // SAFETY: with no queue the block runs on the thread posting the notification, which for
    // this one is the main thread, where `handler` may be used; it captures only `handler`.
    let observer = unsafe {
        NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
            Some(&name),
            None,
            None,
            &register,
        )
    };
    // The observer stays registered for the life of the process.
    std::mem::forget(observer);
}
