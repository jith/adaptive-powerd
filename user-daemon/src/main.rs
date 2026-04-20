use std::fs;
use std::time::Duration;
use zbus::Connection;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let conn = Connection::session().await?;

    let proxy = zbus::Proxy::new(
        &conn,
        "org.gnome.Mutter.IdleMonitor",
        "/org/gnome/Mutter/IdleMonitor/Core",
        "org.gnome.Mutter.IdleMonitor",
    )
    .await?;

    let idle_watch: u32 = proxy.call("AddIdleWatch", &(5000u64)).await?;
    let active_watch: u32 = proxy.call("AddUserActiveWatch", &()).await?;

    loop {
        let msg = proxy.receive_signal("WatchFired").await?;
        let id: u32 = msg.body()?;

        let idle = id == idle_watch;

        let path = format!("/run/user/{}/adaptive-powerd.state", unsafe {
            libc::geteuid()
        });

        let _ = fs::write(path, format!("idle={}", idle));

        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
