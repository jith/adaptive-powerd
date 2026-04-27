use anyhow::Result;
use futures_util::StreamExt;
use tracing::{info, warn};
use zbus::{dbus_interface, Connection, Proxy, SignalContext};

struct Powerd;

#[dbus_interface(name = "org.adaptive.Powerd")]
impl Powerd {
    #[dbus_interface(signal)]
    async fn idle_changed(ctxt: &SignalContext<'_>, idle: bool) -> zbus::Result<()>;
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Connect to session bus
    let conn = Connection::session().await?;

    // Register our D-Bus interface
    conn.object_server()
        .at("/org/adaptive/Powerd", Powerd)
        .await?;

    // GNOME IdleMonitor proxy
    let proxy = Proxy::new(
        &conn,
        "org.gnome.Mutter.IdleMonitor",
        "/org/gnome/Mutter/IdleMonitor/Core",
        "org.gnome.Mutter.IdleMonitor",
    )
    .await?;

    // Register idle + active watches
    let idle_watch: u32 = proxy.call("AddIdleWatch", &(5000u64)).await?;
    let active_watch: u32 = proxy.call("AddUserActiveWatch", &()).await?;

    info!("Idle watch ID: {}", idle_watch);
    info!("Active watch ID: {}", active_watch);

    // Subscribe to WatchFired signal
    let mut stream = proxy.receive_signal("WatchFired").await?;

    let mut last_state: Option<bool> = None;

    // Main loop
    while let Some(msg) = stream.next().await {
        // Extract signal body (u32 id)
        let id: u32 = match msg.body() {
            Ok(v) => v,
            Err(e) => {
                warn!("Failed to parse signal body: {}", e);
                continue;
            }
        };

        // Determine idle state
        let idle = if id == idle_watch {
            true
        } else if id == active_watch {
            false
        } else {
            continue;
        };

        // Debounce duplicate state
        if last_state == Some(idle) {
            continue;
        }

        last_state = Some(idle);

        info!("Idle state changed → {}", idle);

        // Emit custom D-Bus signal
        let ctxt = SignalContext::new(&conn, "/org/adaptive/Powerd")?;
        if let Err(e) = Powerd::idle_changed(&ctxt, idle).await {
            warn!("Failed to emit signal: {}", e);
        }
    }

    Ok(())
}
