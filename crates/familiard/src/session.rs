//! Follows whether the user's graphical session is locked, from logind, so
//! button actions can be paused on the lock screen.

use std::sync::mpsc::Sender;

use futures_util::StreamExt;
use tracing::{debug, info};
use zbus::{Connection, OwnedObjectPath, Proxy};

use crate::input::InputMsg;

const LOGIND: &str = "org.freedesktop.login1";

/// Sends [`InputMsg::Locked`] now and whenever the lock state changes. Returns
/// when there is no graphical session to watch.
pub async fn watch_lock(tx: Sender<InputMsg>) -> zbus::Result<()> {
    let conn = Connection::system().await?;
    // SAFETY: getuid takes no arguments and cannot fail.
    let uid = unsafe { libc::getuid() };
    let manager = Proxy::new(&conn, LOGIND, "/org/freedesktop/login1", "org.freedesktop.login1.Manager").await?;
    let user: OwnedObjectPath = manager.call("GetUser", &(uid,)).await?;
    let user = Proxy::new(&conn, LOGIND, user, "org.freedesktop.login1.User").await?;
    let (id, session): (String, OwnedObjectPath) = user.get_property("Display").await?;
    if session.as_str() == "/" {
        info!("no graphical session; button actions are not paused when locked");
        return Ok(());
    }
    debug!(session = %id, "watching lock state");
    let session = Proxy::new(&conn, LOGIND, session, "org.freedesktop.login1.Session").await?;
    let locked: bool = session.get_property("LockedHint").await?;
    let _ = tx.send(InputMsg::Locked(locked));
    let mut changes = session.receive_property_changed::<bool>("LockedHint").await;
    while let Some(change) = changes.next().await {
        if let Ok(locked) = change.get().await
            && tx.send(InputMsg::Locked(locked)).is_err()
        {
            break;
        }
    }
    Ok(())
}
