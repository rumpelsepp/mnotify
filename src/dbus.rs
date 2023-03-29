use std::sync::Arc;

use matrix_sdk::ruma::OwnedRoomId;
use zbus::fdo::{Error, Result};
use zbus::{dbus_interface, dbus_proxy, Connection, ConnectionBuilder};

use crate::client::Client;

pub(crate) struct DBusServer {
    client: Arc<Client>,
}

#[dbus_interface(name = "org.rumpelsepp.mnotify1")]
impl DBusServer {
    async fn send(&self, room_id: &str, body: &str, markdown: bool) -> Result<()> {
        let room_id: OwnedRoomId = room_id
            .try_into()
            .map_err(|e| Error::InvalidArgs(format!("{:?}", e)))?;

        self.client
            .send_message(room_id, body, markdown)
            .await
            .map_err(|e| Error::Failed(format!("{:?}", e)))?;
        Ok(())
    }
}

pub(crate) async fn connect(client: Arc<Client>) -> anyhow::Result<Connection> {
    let iface = DBusServer { client };
    let connection = ConnectionBuilder::session()?
        .name("org.rumpelsepp.mnotify")?
        .serve_at("/org/rumpelsepp/mnotify", iface)?
        .build()
        .await?;

    Ok(connection)
}

#[dbus_proxy(
    interface = "org.rumpelsepp.mnotify1",
    default_service = "org.rumpelsepp.mnotify",
    default_path = "/org/rumpelsepp/mnotify"
)]
trait DBusClient {
    fn send(&self, room_id: &str, body: &str, markdown: bool) -> Result<()>;
}
